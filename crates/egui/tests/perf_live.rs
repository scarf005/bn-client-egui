//! Frame cost against a real engine, driven like eframe drives the app: a frame runs when the app asked for one.
//!
//! `BN_ENGINE=<engine> BN_PERF_SCENARIO=tutorial|town BN_PERF_TILES=0|1 cargo test --release -p bn-client-egui --test perf_live -- --ignored --nocapture`
//! With `--features profile` the scope timings of all frames (egui's own included) are listed too.
//! Measures the CPU time of one frame (UI pass, tessellation, accessibility tree); GPU work is not included.
mod common;
use bn_client_egui::interaction::InvokeError;
use bn_client_egui::protocol::InvokeArgs;
use bn_client_egui::transport::game_root;
use bn_client_egui::watchdog::Watchdog;
use bn_client_egui::{App, EngineModel, GameModel, MenuModel};
use cpu_time::{ProcessTime, ThreadTime};
use eframe::egui;
use egui_kittest::Harness;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// How long the key is held, in seconds.
const HOLD_SECS: u64 = 10;

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

fn choose(h: &mut H, clock: &Clockwork, affordance: &str, row: &str) -> Frames {
    let offered = |h: &H| {
        h.state()
            .model
            .screens()
            .last()
            .is_some_and(|s| s.affordances.iter().any(|a| a.id == affordance))
    };
    let mut frames = Frames::default();
    // An idle engine may still owe answers to what the app's own frames asked; the model refuses until then.
    loop {
        frames.merge(run(h, clock, 120, row, |h| {
            offered(h) && !h.state().model.busy()
        }));
        let args = InvokeArgs {
            row: Some(row.into()),
            ..Default::default()
        };
        match h.state_mut().model.invoke_affordance(affordance, args) {
            Err(InvokeError::Busy) => frame(h, &mut frames),
            sent => {
                sent.unwrap_or_else(|e| panic!("{affordance} on {row}: {e:?}"));
                return frames;
            }
        }
    }
}

/// The tutorial teaches with pop-ups; each closes with its only choice.
fn dismiss_popups(h: &mut H, clock: &Clockwork) {
    while let Some(menu) = h.state().model.menu() {
        let id = menu.choices.first().map(|c| c.id.clone());
        match id {
            Some(id) if !menu.can_cancel => h.state_mut().model.choose(&id),
            _ => h.state_mut().model.cancel_menu(),
        }
        run(h, clock, 180, "popup", |h| !h.state().model.busy());
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
    let transport = common::live::english_transport();
    let model = EngineModel::new(Box::new(transport));
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
    warmup.merge(choose(&mut h, &clock, "menu:tab", "new_game"));
    let (entry, wait) = match scenario.as_str() {
        "town" => ("new_game:play_now", 600),
        _ => ("new_game:tutorial", 120),
    };
    warmup.merge(choose(&mut h, &clock, "menu:entry", entry));
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
    run(&mut h, &clock, 180, "quiet", |h| !h.state().model.busy());
    let _ = run_for(&mut h, &clock, 2);

    #[cfg(feature = "profile")]
    let view = puffin::GlobalFrameView::default();
    let idle = run_for(&mut h, &clock, 8);
    report(&format!("{scenario} tiles={tiles} idle"), &idle, 8.0);
    #[cfg(feature = "profile")]
    bn_client_egui::profile::scopes("idle", &view);

    // A town start often has a zombie in view, and the game's safe mode refuses every step until the player
    // turns it off ('!'), as natively; the stepping below measures steps, not that refusal.
    h.input_mut().events.push(egui::Event::Text("!".into()));
    run(&mut h, &clock, 180, "safe mode off", |h| {
        !h.state().model.busy()
    });
    let _ = run_for(&mut h, &clock, 1);

    let mut steps = Frames::default();
    let mut latencies = vec![];
    let mut attempt = 0;
    let mut held = "l";
    while latencies.len() < 8 {
        attempt += 1;
        assert!(
            attempt < 60,
            "the avatar never moved: menu {:?}, native log {:?}",
            h.state().model.menu().map(|m| m.title),
            h.state()
                .model
                .native_log()
                .iter()
                .rev()
                .take(3)
                .map(|l| &l.text)
                .collect::<Vec<_>>()
        );
        let before = square(&h);
        // Alternating a direction and its opposite keeps the avatar in place; a blocked way tries the next pair.
        let key = STEPS[(attempt / 2) % 4 * 2 + attempt % 2].0;
        let sent = Instant::now();
        h.input_mut().events.push(egui::Event::Text(key.into()));
        h.ctx.request_repaint();
        let f = run(&mut h, &clock, 180, "a step", |h| {
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
        run(&mut h, &clock, 180, "quiet", |h| !h.state().model.busy());
        let _ = run_for(&mut h, &clock, 1);
    }
    // A held key, as the OS delivers it: one key-down, repeats every 40 ms (X's default 25 Hz), one key-up. The engine
    // stops at a wall and the player lets go and presses the other way.
    let mut hold = Frames::default();
    let (mut moves, mut last, mut moved_at) = (0, square(&h), Instant::now());
    let (start, mut next_repeat) = (Instant::now(), Instant::now());
    let mut down = held;
    key_event(&mut h, down, true, false);
    while start.elapsed() < Duration::from_secs(HOLD_SECS) {
        if Instant::now() >= next_repeat {
            if moved_at.elapsed() > Duration::from_millis(1500) {
                key_event(&mut h, down, false, false);
                down = opposite(down);
                key_event(&mut h, down, true, false);
                moved_at = Instant::now();
            } else {
                key_event(&mut h, down, true, true);
            }
            next_repeat += Duration::from_millis(40);
        }
        if clock.wait(next_repeat) {
            frame(&mut h, &mut hold);
        }
        if square(&h) != last {
            (moves, last, moved_at) = (moves + 1, square(&h), Instant::now());
        }
    }
    key_event(&mut h, down, false, false);
    let released = moves;
    // The steps already on the way land first; the walk is over once none has landed for a second.
    let (mut quiet_since, settle_by) = (Instant::now(), Instant::now() + Duration::from_secs(10));
    while quiet_since.elapsed() < Duration::from_secs(1) || h.state().model.busy() {
        assert!(
            Instant::now() < settle_by,
            "the walk goes on after the key was released"
        );
        if clock.wait(quiet_since + Duration::from_secs(1)) {
            frame(&mut h, &mut hold);
        }
        if square(&h) != last {
            (moves, last, quiet_since) = (moves + 1, square(&h), Instant::now());
        }
    }
    println!(
        "PERF {scenario} tiles={tiles} held key: {released} steps in {HOLD_SECS} s = {:.2} steps/s, {} more after release, load={}",
        released as f64 / HOLD_SECS as f64,
        moves - released,
        load()
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
    bn_client_egui::profile::scopes("all", &view);
}

/// The step keys by pairs of opposite directions, with the key the OS reports for each.
const STEPS: [(&str, egui::Key); 8] = [
    ("l", egui::Key::L),
    ("h", egui::Key::H),
    ("j", egui::Key::J),
    ("k", egui::Key::K),
    ("u", egui::Key::U),
    ("b", egui::Key::B),
    ("y", egui::Key::Y),
    ("n", egui::Key::N),
];

fn opposite(text: &str) -> &'static str {
    let at = STEPS.iter().position(|s| s.0 == text).unwrap();
    STEPS[at ^ 1].0
}

/// What the OS sends for a key: the key event, and the character it types when it goes down or repeats.
fn key_event(h: &mut H, text: &str, pressed: bool, repeat: bool) {
    let key = STEPS.iter().find(|s| s.0 == text).unwrap().1;
    let events = &mut h.input_mut().events;
    events.push(egui::Event::Key {
        key,
        physical_key: Some(key),
        pressed,
        repeat,
        modifiers: egui::Modifiers::NONE,
    });
    if pressed {
        events.push(egui::Event::Text(text.into()));
    }
    h.ctx.request_repaint();
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
