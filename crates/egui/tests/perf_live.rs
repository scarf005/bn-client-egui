//! Frame cost against a real engine, driven like eframe drives the app: a frame runs when the app asked for one.
//!
//! `BN_ENGINE=<engine> BN_PERF_SCENARIO=tutorial|town BN_PERF_TILES=0|1 cargo test --release -p bn-client-egui --test perf_live -- --ignored --nocapture`
//! With `--features profile` the scope timings of all frames (egui's own included) are listed too.
//! Measures the CPU time of one frame (UI pass, tessellation, accessibility tree); GPU work is not included.
use bn_client_egui::transport::{StdioTransport, game_root};
use bn_client_egui::watchdog::Watchdog;
use bn_client_egui::{App, EngineModel, GameModel, keys::KeyMap};
use cpu_time::{ProcessTime, ThreadTime};
use eframe::egui;
use egui_kittest::Harness;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

type H<'a> = Harness<'a, App<EngineModel>>;

#[derive(Default)]
struct Frames {
    cpu_ms: Vec<f64>,
    uploads: usize,
    causes: std::collections::BTreeMap<String, usize>,
}

impl Frames {
    fn merge(&mut self, other: Frames) {
        self.cpu_ms.extend(other.cpu_ms);
        self.uploads += other.uploads;
        for (k, n) in other.causes {
            *self.causes.entry(k).or_default() += n;
        }
    }

    fn percentile(&self, p: f64) -> f64 {
        let mut v = self.cpu_ms.clone();
        v.sort_by(f64::total_cmp);
        v.get(((v.len() as f64 - 1.0) * p).round() as usize)
            .copied()
            .unwrap_or(0.0)
    }
}

fn load() -> String {
    std::fs::read_to_string("/proc/loadavg")
        .map(|s| s.split_whitespace().take(3).collect::<Vec<_>>().join(" "))
        .unwrap_or_default()
}

/// Repaint requests the way eframe schedules them: the earliest asked-for instant wins.
struct Clockwork {
    next: Arc<Mutex<Option<Instant>>>,
}

impl Clockwork {
    fn install(ctx: &egui::Context) -> Self {
        let next = Arc::new(Mutex::new(Some(Instant::now())));
        let slot = next.clone();
        ctx.set_request_repaint_callback(move |info| {
            let at = Instant::now() + info.delay;
            if std::env::var_os("BN_PERF_TRACE").is_some() {
                eprintln!(
                    "REPAINT +{:?} {:?}",
                    info.delay, info.current_cumulative_pass_nr
                );
            }
            let mut slot = slot.lock().unwrap();
            *slot = Some(slot.map_or(at, |old| old.min(at)));
        });
        Self { next }
    }

    /// Waits for the next requested frame, at most `until`; false when none came.
    fn wait(&self, until: Instant) -> bool {
        loop {
            let due = *self.next.lock().unwrap();
            let now = Instant::now();
            match due {
                Some(at) if at <= now => {
                    *self.next.lock().unwrap() = None;
                    return true;
                }
                _ if now >= until => return false,
                _ => std::thread::sleep(Duration::from_micros(500)),
            }
        }
    }
}

fn frame(h: &mut H, frames: &mut Frames) {
    let start = ThreadTime::now();
    h.step();
    let out = h.output();
    frames.uploads += out.textures_delta.set.len();
    let ppp = out.pixels_per_point;
    let _ = h.ctx.tessellate(out.shapes.clone(), ppp);
    if std::env::var_os("BN_PERF_TRACE").is_some() {
        eprintln!(
            "FRAME causes {:?}",
            h.ctx
                .repaint_causes()
                .iter()
                .map(|c| format!("{}:{}", c.file.rsplit('/').next().unwrap(), c.line))
                .collect::<Vec<_>>()
        );
    }
    for c in h.ctx.repaint_causes() {
        *frames
            .causes
            .entry(format!("{}:{} {}", c.file, c.line, c.reason))
            .or_default() += 1;
    }
    frames.cpu_ms.push(start.elapsed().as_secs_f64() * 1e3);
    #[cfg(feature = "profile")]
    puffin::GlobalProfiler::lock().new_frame();
}

/// Runs the frames the app asks for until `done`; returns what they cost.
fn run(
    h: &mut H,
    clock: &Clockwork,
    secs: u64,
    what: &str,
    mut done: impl FnMut(&H) -> bool,
) -> Frames {
    let mut frames = Frames::default();
    let mut watchdog = Watchdog::new(Duration::from_secs(secs), Duration::from_secs(600));
    while !done(h) {
        assert!(
            !watchdog.expired(h.state().model.received()),
            "timed out: {what}; {:?}; {:?}; busy {}; {:?}",
            h.state().model.command(),
            h.state().model.menu().map(|m| m.title),
            h.state().model.busy(),
            h.state()
                .model
                .messages()
                .iter()
                .rev()
                .take(3)
                .collect::<Vec<_>>()
        );
        if clock.wait(Instant::now() + Duration::from_millis(20)) {
            frame(h, &mut frames);
        }
    }
    frames
}

fn square(h: &H) -> (i32, i32) {
    let m = &h.state().model;
    (m.player().x + m.origin().x, m.player().y + m.origin().y)
}

fn choose(h: &mut H, clock: &Clockwork, label: &str) -> Frames {
    let found = |h: &H| {
        h.state()
            .model
            .menu()
            .is_some_and(|m| m.choices.iter().any(|c| c.label.contains(label)))
    };
    let frames = run(h, clock, 120, label, |h| {
        found(h) && !h.state().model.busy()
    });
    let id = h
        .state()
        .model
        .menu()
        .unwrap()
        .choices
        .iter()
        .find(|c| c.label.contains(label))
        .unwrap()
        .id
        .clone();
    h.state_mut().model.choose(&id);
    frames
}

/// The tutorial teaches with pop-ups; each closes with its only choice.
fn dismiss_popups(h: &mut H, clock: &Clockwork) {
    while let Some(menu) = h.state().model.menu() {
        let id = menu.choices.first().map(|c| c.id.clone());
        match id {
            Some(id) if !menu.can_cancel => h.state_mut().model.choose(&id),
            _ => h.state_mut().model.cancel_menu(),
        }
        run(h, clock, 60, "popup", |h| !h.state().model.busy());
        let _ = run_for(h, clock, 1);
    }
}

fn report(phase: &str, f: &Frames, secs: f64) {
    println!(
        "PERF {phase}: frames={} rate={:.1}/s cpu_ms p50={:.2} p95={:.2} max={:.2} uploads={} load={}",
        f.cpu_ms.len(),
        f.cpu_ms.len() as f64 / secs,
        f.percentile(0.5),
        f.percentile(0.95),
        f.percentile(1.0),
        f.uploads,
        load()
    );
    let mut causes = f.causes.iter().collect::<Vec<_>>();
    causes.sort_by_key(|(_, n)| std::cmp::Reverse(**n));
    for (cause, n) in causes.into_iter().take(4) {
        println!(
            "PERF   cause {n:6} {}",
            cause.rsplit("src/").next().unwrap_or(cause)
        );
    }
}

#[test]
#[ignore = "requires BN_ENGINE pointing at an engine binary"]
fn frame_cost_idle_and_while_stepping() {
    let engine = std::env::var_os("BN_ENGINE").expect("set BN_ENGINE");
    let scenario = std::env::var("BN_PERF_SCENARIO").unwrap_or("tutorial".into());
    let tiles = std::env::var("BN_PERF_TILES").is_ok_and(|v| v == "1");
    let size = std::env::var("BN_PERF_SIZE")
        .ok()
        .and_then(|v| {
            v.split_once('x')
                .map(|(w, h)| egui::vec2(w.parse().unwrap(), h.parse().unwrap()))
        })
        .unwrap_or(egui::vec2(1300.0, 800.0));
    let root = game_root(engine.as_ref(), None).expect("game root");
    let dir = std::env::temp_dir().join(format!("bn-perf-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let settings = dir.join("settings.json");
    std::fs::write(&settings, format!(r#"{{"tiles":{tiles},"tileset":null}}"#)).unwrap();
    let transport = StdioTransport::spawn_fresh(engine.as_ref()).expect("engine starts");
    let model = EngineModel::new(Box::new(transport)).with_keys(KeyMap::load(&root));
    let app = App::new(model)
        .with_settings(settings)
        .with_root(root.clone());
    let mut h = Harness::builder()
        .with_size(size)
        .with_step_dt(1.0 / 60.0)
        .build_ui_state(|ui, app: &mut App<EngineModel>| app.show(ui), app);
    // The harness' default limit is below a wide sheet; a real GPU allows far more.
    h.input_mut().max_texture_side = Some(16384);
    bn_client_egui::fonts::install(&h.ctx, &root);
    let clock = Clockwork::install(&h.ctx);
    #[cfg(feature = "profile")]
    puffin::set_scopes_on(true);

    let started = Instant::now();
    let process = ProcessTime::now();
    let mut warmup = Frames::default();
    warmup.merge(choose(&mut h, &clock, "New Game"));
    let (label, wait) = match scenario.as_str() {
        "town" => ("Play Now", 600),
        _ => ("Tutorial", 120),
    };
    warmup.merge(choose(&mut h, &clock, label));
    warmup.merge(run(&mut h, &clock, wait, "the map", |h| {
        h.state().model.size().x > 0
    }));
    // Pop-ups and the tileset arriving: costs of their own, reported apart from the steady state.
    warmup.merge(run(&mut h, &clock, 300, "the settled game", |h| {
        let app = h.state();
        app.model.menu().is_none() && !app.model.busy() && !app.display().loading()
    }));
    let secs = started.elapsed().as_secs_f64();
    println!(
        "PERF {scenario} tiles={tiles} warmup: {secs:.0} s wall, client process cpu {:.1} s (frames on the UI thread {:.1} s)",
        process.elapsed().as_secs_f64(),
        warmup.cpu_ms.iter().sum::<f64>() / 1e3
    );
    report(&format!("{scenario} tiles={tiles} warmup"), &warmup, secs);
    dismiss_popups(&mut h, &clock);
    run(&mut h, &clock, 60, "quiet", |h| !h.state().model.busy());
    let _ = run_for(&mut h, &clock, 2);

    #[cfg(feature = "profile")]
    let view = puffin::GlobalFrameView::default();
    let idle = run_for(&mut h, &clock, 8);
    report(&format!("{scenario} tiles={tiles} idle"), &idle, 8.0);
    #[cfg(feature = "profile")]
    scopes("idle", &view);

    let mut steps = Frames::default();
    let mut latencies = vec![];
    let mut attempt = 0;
    let mut held = "l";
    while latencies.len() < 8 {
        attempt += 1;
        assert!(attempt < 60, "the avatar never moved");
        let before = square(&h);
        // Alternating east and west keeps the avatar in place; a blocked way tries the next direction.
        let key = ["l", "h", "j", "k"][(attempt / 2) % 4 / 2 * 2 + attempt % 2];
        let sent = Instant::now();
        h.input_mut().events.push(egui::Event::Text(key.into()));
        h.ctx.request_repaint();
        let f = run(&mut h, &clock, 60, "a step", |h| {
            square(h) != before
                || (sent.elapsed() > Duration::from_millis(500) && !h.state().model.busy())
        });
        if square(&h) != before {
            held = key;
            latencies.push(sent.elapsed().as_secs_f64() * 1e3);
            steps.cpu_ms.extend(f.cpu_ms);
            steps.uploads += f.uploads;
            for (k, n) in f.causes {
                *steps.causes.entry(k).or_default() += n;
            }
        }
        dismiss_popups(&mut h, &clock);
        run(&mut h, &clock, 60, "quiet", |h| !h.state().model.busy());
        let _ = run_for(&mut h, &clock, 1);
    }
    // A held key: the OS repeats it every 33 ms; the avatar turns round when a wall stops it.
    let mut hold = Frames::default();
    let (mut moves, mut last, mut moved_at) = (0, square(&h), Instant::now());
    let (start, mut next_repeat) = (Instant::now(), Instant::now());
    while start.elapsed() < Duration::from_secs(6) {
        if Instant::now() >= next_repeat {
            if moved_at.elapsed() > Duration::from_millis(1500) {
                held = if held == "l" { "h" } else { "l" };
                moved_at = Instant::now();
            }
            h.input_mut().events.push(egui::Event::Text(held.into()));
            h.ctx.request_repaint();
            next_repeat += Duration::from_millis(33);
        }
        if clock.wait(next_repeat) {
            frame(&mut h, &mut hold);
        }
        if square(&h) != last {
            (moves, last, moved_at) = (moves + 1, square(&h), Instant::now());
        }
    }
    let released = moves;
    let end = Instant::now() + Duration::from_secs(2);
    while Instant::now() < end || h.state().model.busy() {
        if clock.wait(end) {
            frame(&mut h, &mut hold);
        }
        if square(&h) != last {
            (moves, last) = (moves + 1, square(&h));
        }
    }
    println!(
        "PERF {scenario} tiles={tiles} held key: {released} steps in 6 s = {:.2} steps/s, {} more after release, load={}",
        released as f64 / 6.0,
        moves - released,
        load()
    );
    // The step in flight and the two queued ones may still land; anything more would walk on after release.
    assert!(
        moves - released <= 3,
        "{} steps after the key was released",
        moves - released
    );
    println!("PERF repaint causes while stepping: {:#?}", steps.causes);
    latencies.sort_by(f64::total_cmp);
    report(&format!("{scenario} tiles={tiles} stepping"), &steps, 1.0);
    println!(
        "PERF {scenario} tiles={tiles} step latency ms: p50={:.0} min={:.0} max={:.0} load={}",
        latencies[latencies.len() / 2],
        latencies[0],
        latencies[latencies.len() - 1],
        load()
    );
    #[cfg(feature = "profile")]
    scopes("all", &view);
}

fn run_for(h: &mut H, clock: &Clockwork, secs: u64) -> Frames {
    let end = Instant::now() + Duration::from_secs(secs);
    let mut frames = Frames::default();
    while Instant::now() < end {
        if clock.wait(end) {
            frame(h, &mut frames);
        }
    }
    frames
}

/// Inclusive time per scope name, per recorded frame, largest first.
#[cfg(feature = "profile")]
fn scopes(phase: &str, view: &puffin::GlobalFrameView) {
    use std::collections::HashMap;
    fn walk(
        names: &puffin::ScopeCollection,
        stream: &puffin::Stream,
        reader: puffin::Reader<'_>,
        total: &mut HashMap<String, (u64, u64)>,
    ) {
        for scope in reader.flatten() {
            let name = names
                .fetch_by_id(&scope.id)
                .map_or("?".to_string(), |d| d.name().to_string());
            let e = total.entry(name).or_default();
            e.0 += scope.record.duration_ns as u64;
            e.1 += 1;
            if let Ok(children) = puffin::Reader::with_offset(stream, scope.child_begin_position) {
                walk(names, stream, children, total);
            }
        }
    }
    let mut total = HashMap::new();
    let guard = view.lock();
    let frames = guard.all_uniq().cloned().collect::<Vec<_>>();
    for data in &frames {
        let Ok(unpacked) = data.unpacked();
        for info in unpacked.thread_streams.values() {
            walk(
                guard.scope_collection(),
                &info.stream,
                puffin::Reader::from_start(&info.stream),
                &mut total,
            );
        }
    }
    let mut rows = total.into_iter().collect::<Vec<_>>();
    rows.sort_by_key(|(_, (ns, _))| std::cmp::Reverse(*ns));
    let n = frames.len().max(1) as f64;
    println!(
        "PERF scopes {phase}: {} frames; inclusive ms per frame, calls per frame",
        frames.len()
    );
    for (name, (ns, calls)) in rows.into_iter().take(22) {
        println!(
            "PERF   {:9.3} ms {:9.1} calls  {name}",
            ns as f64 / 1e6 / n,
            calls as f64 / n
        );
    }
}
