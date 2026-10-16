//! Against a real engine: a long click-travel keeps the avatar on one screen square (the view follows it every
//! frame), and a step reaches the screen without a fixed wait, with or without a shift of the map under it.
//!
//! `BN_ENGINE=<engine> cargo test --release -p bn-client-egui --test live_travel -- --ignored --nocapture`
use bn_client_egui::interaction::InvokeError;
use bn_client_egui::protocol::InvokeArgs;
use bn_client_egui::transport::{StdioTransport, game_root};
use bn_client_egui::watchdog::Watchdog;
use bn_client_egui::{App, EngineModel, GameModel, Pos};
use eframe::egui;
use egui_kittest::Harness;
use std::time::{Duration, Instant};

type H<'a> = Harness<'a, App<EngineModel>>;

fn frame(h: &mut H, watchdog: &mut Watchdog, what: &str) {
    assert!(
        !watchdog.expired(h.state().model.received()),
        "timed out: {what}"
    );
    h.step();
    std::thread::sleep(Duration::from_millis(2));
}

fn until(h: &mut H, secs: u64, what: &str, mut done: impl FnMut(&H) -> bool) {
    let mut watchdog = Watchdog::new(Duration::from_secs(secs), Duration::from_secs(600));
    while !done(h) {
        frame(h, &mut watchdog, what);
    }
}

fn choose(h: &mut H, affordance: &str, row: &str) {
    loop {
        until(h, 120, row, |h| {
            let m = &h.state().model;
            !m.busy()
                && m.screens()
                    .last()
                    .is_some_and(|s| s.affordances.iter().any(|a| a.id == affordance))
        });
        let args = InvokeArgs {
            row: Some(row.into()),
            ..Default::default()
        };
        match h.state_mut().model.invoke_affordance(affordance, args) {
            Err(InvokeError::Busy) => frame(
                h,
                &mut Watchdog::new(Duration::from_secs(60), Duration::from_secs(600)),
                row,
            ),
            sent => return sent.unwrap_or_else(|e| panic!("{affordance} on {row}: {e:?}")),
        }
    }
}

fn square(h: &H) -> (i32, i32) {
    let m = &h.state().model;
    (m.player().x + m.origin().x, m.player().y + m.origin().y)
}

/// A town game at 1280x720 in ASCII with the safe mode off, idle.
fn start<'a>() -> H<'a> {
    let engine = std::env::var_os("BN_ENGINE").expect("set BN_ENGINE");
    let root = game_root(engine.as_ref(), None).expect("game root");
    let dir = std::env::temp_dir().join(format!("bn-travel-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let settings = dir.join("settings.json");
    std::fs::write(&settings, r#"{"tiles":false,"tileset":null}"#).unwrap();
    let transport = StdioTransport::spawn_fresh(engine.as_ref()).expect("engine starts");
    let app = App::new(EngineModel::new(Box::new(transport)))
        .with_settings(settings)
        .with_root(root.clone());
    let mut h = Harness::builder()
        .with_size(egui::vec2(1280.0, 720.0))
        .with_step_dt(1.0 / 60.0)
        .build_ui_state(|ui, app: &mut App<EngineModel>| app.show(ui), app);
    h.input_mut().max_texture_side = Some(16384);
    bn_client_egui::fonts::install(&h.ctx, &root);

    choose(&mut h, "menu:tab", "new_game");
    choose(&mut h, "menu:entry", "new_game:play_now");
    until(&mut h, 600, "the map", |h| h.state().model.size().x > 0);
    until(&mut h, 300, "the settled game", |h| {
        let app = h.state();
        app.model.menu().is_none() && !app.model.busy() && !app.display().loading()
    });
    // A town start often has a zombie in view, and the game's safe mode refuses every step until the player
    // turns it off ('!'), as natively.
    settle(&mut h, 2);
    h.input_mut().events.push(egui::Event::Text("!".into()));
    settle(&mut h, 2);
    until(&mut h, 180, "safe mode off", |h| !h.state().model.busy());
    h
}

/// Runs frames for `secs`, closing the pop-ups the game opens meanwhile.
fn settle(h: &mut H, secs: u64) {
    let end = Instant::now() + Duration::from_secs(secs);
    while Instant::now() < end || h.state().model.busy() || h.state().model.menu().is_some() {
        h.step();
        close_popup(h);
        std::thread::sleep(Duration::from_millis(2));
    }
}

/// Closes the pop-up on top, with its only choice or by cancelling it.
fn close_popup(h: &mut H) {
    if let Some(menu) = h.state().model.menu() {
        let id = menu.choices.first().map(|c| c.id.clone());
        match id {
            Some(id) if !menu.can_cancel => h.state_mut().model.choose(&id),
            _ => h.state_mut().model.cancel_menu(),
        }
    }
}

type Walk = Vec<((i32, i32), Pos, Pos)>;

/// Clicks far known squares until one has a route (the way may be blocked, or the start a cellar), then walks it
/// with a second click; the avatar, view origin and screen square of every frame of the walk. The first click
/// plans the route.
fn walk_far(h: &mut H) -> Option<Walk> {
    let size = h.state().model.size();
    let seat = h.state().model.player();
    let reach = |p: Pos| (p.x - seat.x).abs().max((p.y - seat.y).abs());
    let mut candidates = (0..size.y)
        .flat_map(|y| (0..size.x).map(move |x| Pos { x, y }))
        .filter(|&p| (14..=25).contains(&reach(p)))
        .filter(|&p| !h.state().model.cell(p).id.contains("wall"))
        .collect::<Vec<_>>();
    candidates.sort_by_key(|&p| std::cmp::Reverse(reach(p)));
    let (mut routes, mut walks) = (0, 0);
    for target in candidates.into_iter().step_by(3).take(120) {
        settle(h, 0);
        let from = square(h);
        h.state_mut().model.travel(target);
        until(h, 60, "the click", |h| !h.state().model.busy());
        if h.state().model.route().last() != Some(&target) {
            continue;
        }
        routes += 1;
        h.state_mut().model.travel(target);
        let mut log = vec![];
        let mut watchdog = Watchdog::new(Duration::from_secs(180), Duration::from_secs(600));
        while h.state().model.busy() {
            frame(h, &mut watchdog, "walk");
            let m = &h.state().model;
            log.push((square(h), m.origin(), m.player()));
        }
        walks += 1;
        // An interrupted walk (a zombie in sight) is no long walk: another target is tried.
        let to = log.last().map_or(from, |l| l.0);
        if (to.0 - from.0).abs().max((to.1 - from.1).abs()) >= 14 {
            return Some(log);
        }
    }
    println!("TRAVEL no far walk: {routes} targets had a route, {walks} walks were cut short");
    None
}

#[test]
#[ignore = "requires BN_ENGINE pointing at an engine binary"]
fn travel_keeps_the_avatar_on_one_screen_square() {
    // The start is random; a cellar or a crowded street has no far route, so another game is tried.
    let log = (0..8)
        .find_map(|_| walk_far(&mut start()))
        .expect("no start had a far route");
    let first = log[0].0;
    let seats: std::collections::BTreeSet<_> = log.iter().map(|(_, _, p)| (p.x, p.y)).collect();
    let jumps = log
        .windows(2)
        .filter(|w| w[0].1 != w[1].1 && w[0].0 == w[1].0)
        .count();
    println!(
        "TRAVEL frames {} avatar moved {:?} -> {:?} screen squares seen {:?} origin moves without the avatar {jumps}",
        log.len(),
        first,
        log.last().map(|l| l.0),
        seats
    );
    for (i, (abs, origin, player)) in log.iter().enumerate() {
        println!(
            "TRAVEL {i:4} avatar {abs:?} origin {:?} screen {:?}",
            (origin.x, origin.y),
            (player.x, player.y)
        );
    }
    assert!(
        log.len() > 1 && first != log.last().unwrap().0,
        "the avatar never walked"
    );
    assert_eq!(
        seats.len(),
        1,
        "the avatar jumped across the screen during the walk: {seats:?}"
    );
    assert_eq!(jumps, 0, "the view moved while the avatar stood still");
}

fn ms(from: Instant, to: Instant) -> f64 {
    (to.duration_since(from).as_secs_f64() - from.duration_since(to).as_secs_f64()) * 1e3
}

/// What the last key did: whether the avatar moved, and whether the step crossed into another submap, which
/// shifts the game's map under the avatar.
#[derive(Clone, Copy)]
struct Step {
    moved: bool,
    shifted: bool,
}

/// Presses one key per step until `count` steps moved the avatar and crossed a submap as `shifts` asks; the
/// milliseconds of each from the key to the request queued, to the first events, to the square on screen, and the
/// total. A game where 40 keys in a row give no such step (a zombie holds the avatar, or a cellar keeps it inside one
/// submap) is left for a new one.
fn measure(count: usize, shifts: bool, mut key: impl FnMut(Step) -> &'static str) -> Vec<[f64; 4]> {
    let mut rows = vec![];
    for _ in 0..6 {
        let mut h = start();
        let (mut last, mut idle) = (
            Step {
                moved: true,
                shifted: false,
            },
            0,
        );
        let _ = h.state_mut().model.take_trace();
        while rows.len() < count && idle < 40 {
            idle += 1;
            let before = square(&h);
            let sent = Instant::now();
            h.input_mut()
                .events
                .push(egui::Event::Text(key(last).into()));
            h.ctx.request_repaint();
            let mut watchdog = Watchdog::new(Duration::from_secs(60), Duration::from_secs(600));
            let applied = loop {
                h.step();
                if h.state().model.menu().is_some() {
                    break None;
                }
                if square(&h) != before {
                    break Some(Instant::now());
                }
                if sent.elapsed() > Duration::from_millis(700) && !h.state().model.busy() {
                    break None;
                }
                assert!(
                    !watchdog.expired(h.state().model.received()),
                    "timed out: a step"
                );
                std::thread::sleep(Duration::from_micros(500));
            };
            let after = square(&h);
            last = Step {
                moved: applied.is_some(),
                shifted: before.0.div_euclid(12) != after.0.div_euclid(12)
                    || before.1.div_euclid(12) != after.1.div_euclid(12),
            };
            settle(&mut h, 0);
            let trace = h.state_mut().model.take_trace();
            let at = |what: &str| trace.iter().find(|(_, w)| *w == what).map(|(t, _)| *t);
            if let (Some(applied), Some(sent_at), true) =
                (applied, at("hold").or(at("submit")), last.shifted == shifts)
            {
                let events = trace
                    .iter()
                    .find(|(t, w)| *w == "events" && *t >= sent_at)
                    .map(|(t, _)| *t)
                    .unwrap_or(applied);
                rows.push([
                    ms(sent, sent_at),
                    ms(sent_at, events),
                    ms(events, applied),
                    ms(sent, applied),
                ]);
                idle = 0;
            }
            let end = Instant::now() + Duration::from_millis(100);
            while Instant::now() < end {
                h.step();
                std::thread::sleep(Duration::from_millis(2));
            }
            let _ = h.state_mut().model.take_trace();
        }
        if rows.len() >= count {
            return rows;
        }
    }
    panic!("only {} steps in six games", rows.len());
}

/// Prints the medians and every row, and fails when the client's own share of a step (the key to the request, the
/// events to the screen) holds a wait: the engine's turn in between is what it is.
fn report(what: &str, rows: &[[f64; 4]]) {
    let column = |c: usize, q: usize| {
        let mut v = rows.iter().map(|r| r[c]).collect::<Vec<_>>();
        v.sort_by(f64::total_cmp);
        v[(v.len() * q / 100).min(v.len() - 1)]
    };
    println!(
        "LATENCY {what} ms median of {}: key->request queued {:.1}, request->first events {:.1}, events->applied {:.1}, key->applied {:.1} (p90 {:.1})",
        rows.len(),
        column(0, 50),
        column(1, 50),
        column(2, 50),
        column(3, 50),
        column(3, 90)
    );
    for r in rows {
        println!(
            "LATENCY row {:.1} {:.1} {:.1} {:.1}",
            r[0], r[1], r[2], r[3]
        );
    }
    assert!(
        column(0, 50) < 20.0 && column(2, 50) < 100.0,
        "the client waits in a {what} step"
    );
}

const KEYS: [&str; 4] = ["l", "h", "j", "k"];

#[test]
#[ignore = "requires BN_ENGINE pointing at an engine binary"]
fn walking_step_reaches_the_screen_without_a_fixed_wait() {
    // A direction and its opposite alternate, so the avatar stays near its place; a blocked way tries the next pair.
    let mut heading = 0;
    let rows = measure(24, false, |last| {
        heading = if last.moved {
            heading ^ 1
        } else {
            (heading + 2) % 4
        };
        KEYS[heading]
    });
    report("plain", &rows);
}

#[test]
#[ignore = "requires BN_ENGINE pointing at an engine binary"]
fn shifting_step_reaches_the_screen_without_a_fixed_wait() {
    // One direction until the avatar crosses into another submap, then back and forth across that edge: every
    // step shifts the map under it. A blocked way tries the next direction.
    let mut heading = 0;
    let rows = measure(24, true, |last| {
        heading = match (last.moved, last.shifted) {
            (true, true) => heading ^ 1,
            (true, false) => heading,
            (false, _) => (heading + 1) % 4,
        };
        KEYS[heading]
    });
    report("shifting", &rows);
}
