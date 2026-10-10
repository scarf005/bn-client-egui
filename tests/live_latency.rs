//! Per-step and start-up latency against a real engine:
//! `BN_ENGINE=... cargo test --release --test live_latency -- --ignored --nocapture`
use bn_client_egui::transport::StdioTransport;
use bn_client_egui::{EngineModel, GameModel, Pos};
use std::sync::{Mutex, OnceLock, mpsc};
use std::time::{Duration, Instant};

static WAKE: OnceLock<Mutex<mpsc::Receiver<()>>> = OnceLock::new();

fn pump(model: &mut EngineModel, what: &str, done: impl Fn(&EngineModel) -> bool) -> Instant {
    let wake = WAKE.get().unwrap();
    let deadline = Instant::now() + Duration::from_secs(120);
    loop {
        model.poll();
        if done(model) {
            return Instant::now();
        }
        assert!(
            Instant::now() < deadline,
            "timeout: {what}: {:?}",
            model.messages()
        );
        // Block on the transport's waker, as the UI's repaint request does.
        let _ = wake.lock().unwrap().recv_timeout(Duration::from_millis(50));
    }
}

fn choose(model: &mut EngineModel, label: &str) -> Instant {
    pump(model, label, |m| {
        !m.busy()
            && m.menu()
                .is_some_and(|x| x.choices.iter().any(|c| c.label.contains(label)))
    });
    let id = model
        .menu()
        .unwrap()
        .choices
        .iter()
        .find(|c| c.label.contains(label))
        .unwrap()
        .id
        .clone();
    let t = Instant::now();
    model.choose(&id);
    t
}

fn ms(a: Instant, b: Instant) -> f64 {
    b.saturating_duration_since(a).as_secs_f64() * 1000.0
}

#[test]
#[ignore = "requires BN_ENGINE pointing at an engine binary"]
fn measure_startup_and_steps() {
    let engine = std::env::var_os("BN_ENGINE").expect("set BN_ENGINE");
    let t0 = Instant::now();
    let transport = StdioTransport::spawn(engine.as_ref()).unwrap();
    let mut model = EngineModel::new(Box::new(transport));
    let (tx, rx) = mpsc::channel();
    let tx = Mutex::new(tx);
    model.set_waker(std::sync::Arc::new(move || {
        let _ = tx.lock().unwrap().send(());
    }));
    WAKE.set(Mutex::new(rx)).ok();
    pump(&mut model, "menu", |m| m.menu().is_some());
    let trace = model.take_trace();
    let at = |name: &str| {
        trace
            .iter()
            .find(|(_, w)| *w == name)
            .map(|(t, _)| *t)
            .unwrap()
    };
    println!("start -> hello response   {:8.0} ms", ms(t0, at("hello")));
    println!(
        "hello -> snapshot         {:8.0} ms",
        ms(at("hello"), at("subscribed"))
    );
    let t_menu = Instant::now();
    println!(
        "subscribed -> main menu   {:8.0} ms",
        ms(at("subscribed"), t_menu)
    );
    let t = choose(&mut model, "New Game");
    let t_sub = pump(&mut model, "world menu", |m| {
        !m.busy()
            && m.menu()
                .is_some_and(|x| x.choices.iter().any(|c| c.label.contains("Tutorial")))
    });
    println!("New Game -> world menu    {:8.0} ms", ms(t, t_sub));
    let t = choose(&mut model, "Tutorial");
    let t_map = pump(&mut model, "map", |m| m.size().x > 0);
    println!("Tutorial -> first map     {:8.0} ms", ms(t, t_map));
    println!("total start -> map        {:8.0} ms", ms(t0, t_map));
    pump(&mut model, "idle", |m| !m.busy());
    model.take_trace();

    // Walk east, then back west, 10 steps in total, as fast as the protocol allows.
    let mut steps = vec![];
    for i in 0..10 {
        pump(&mut model, "idle", |m| !m.busy());
        for _ in 0..20 {
            let Some(menu) = model.menu() else { break };
            println!(
                "dismissing {:?}: {}",
                menu.title,
                menu.message.lines().next().unwrap_or("")
            );
            match menu.choices.iter().find(|c| c.enabled) {
                Some(c) if !menu.can_cancel => model.choose(&c.id.clone()),
                _ => model.cancel_menu(),
            }
            pump(&mut model, "idle", |m| !m.busy());
            std::thread::sleep(Duration::from_millis(300));
            model.poll();
        }
        model.take_trace();
        let start = model.player();
        let dx = if i < 5 { 1 } else { -1 };
        let target = Pos {
            x: start.x + dx,
            y: start.y,
        };
        assert!(
            model.step_toward(target),
            "step {i}: {:?}",
            model.messages()
        );
        pump(&mut model, "step", |m| m.player() != start && !m.busy());
        steps.push(model.take_trace());
    }
    let mut rows: Vec<[f64; 6]> = vec![];
    let mut prev_done: Option<Instant> = None;
    for tr in &steps {
        let at = |name: &str| {
            tr.iter()
                .filter(|(_, w)| *w == name)
                .map(|(t, _)| *t)
                .next_back()
        };
        let (Some(sub), Some(rec), Some(exe), Some(done), Some(ev)) = (
            at("submit"),
            at("received"),
            at("executing"),
            at("completed"),
            at("events"),
        ) else {
            println!(
                "incomplete trace: {:?}",
                tr.iter().map(|x| x.1).collect::<Vec<_>>()
            );
            continue;
        };
        let gap = prev_done.map_or(0.0, |p| ms(p, sub));
        prev_done = Some(done.max(ev));
        rows.push([
            ms(sub, rec),
            ms(rec, exe),
            ms(exe, done),
            ms(exe, ev),
            ms(done, ev.max(done)),
            gap,
        ]);
    }
    println!(
        "{:>6} {:>10} {:>10} {:>10} {:>11} {:>12} {:>14}",
        "step",
        "sub>recv",
        "recv>exec",
        "exec>done",
        "exec>events",
        "done>events+",
        "prev>next sub"
    );
    for (i, r) in rows.iter().enumerate() {
        println!(
            "{:>6} {:>10.1} {:>10.1} {:>10.1} {:>11.1} {:>12.1} {:>14.1}",
            i, r[0], r[1], r[2], r[3], r[4], r[5]
        );
    }
    let n = rows.len().max(1) as f64;
    let avg: Vec<f64> = (0..6)
        .map(|c| rows.iter().map(|r| r[c]).sum::<f64>() / n)
        .collect();
    let max: Vec<f64> = (0..6)
        .map(|c| rows.iter().map(|r| r[c]).fold(0.0, f64::max))
        .collect();
    println!(
        "{:>6} {:>10.1} {:>10.1} {:>10.1} {:>11.1} {:>12.1} {:>14.1}",
        "avg", avg[0], avg[1], avg[2], avg[3], avg[4], avg[5]
    );
    println!(
        "{:>6} {:>10.1} {:>10.1} {:>10.1} {:>11.1} {:>12.1} {:>14.1}",
        "max", max[0], max[1], max[2], max[3], max[4], max[5]
    );
}
