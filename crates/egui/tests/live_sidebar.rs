//! Against a real engine: a held walk keeps every frame of the side panels as the engine sent them: the
//! sidebar's rows never drop out for a frame between two of the engine's updates, and the shortcut bar's buttons
//! stay as enabled as the engine has them while a step is on its way.
//!
//! `BN_ENGINE=<engine> cargo test --release -p bn-client-egui --test live_sidebar -- --ignored --nocapture`
//! (`SHOT_DIR=<dir>` also saves each frame's sidebar.)
use bn_client_egui::interaction::InvokeError;
use bn_client_egui::protocol::InvokeArgs;
use bn_client_egui::transport::{StdioTransport, game_root};
use bn_client_egui::watchdog::Watchdog;
use bn_client_egui::{App, EngineModel, GameModel, MenuModel};
use eframe::egui;
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable, by};
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

/// Runs frames for `secs`, closing the pop-ups the game opens meanwhile.
fn settle(h: &mut H, secs: u64) {
    let end = Instant::now() + Duration::from_secs(secs);
    while Instant::now() < end || h.state().model.busy() || h.state().model.menu().is_some() {
        h.step();
        if let Some(menu) = h.state().model.menu() {
            let id = menu.choices.first().map(|c| c.id.clone());
            match id {
                Some(id) if !menu.can_cancel => h.state_mut().model.choose(&id),
                _ => h.state_mut().model.cancel_menu(),
            }
        }
        std::thread::sleep(Duration::from_millis(2));
    }
}

/// A town game at 1280x720 in ASCII with the safe mode off, idle.
fn start<'a>() -> H<'a> {
    let engine = std::env::var_os("BN_ENGINE").expect("set BN_ENGINE");
    let root = game_root(engine.as_ref(), None).expect("game root");
    let dir = std::env::temp_dir().join(format!("bn-sidebar-{}", std::process::id()));
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
        .wgpu()
        .build_ui_state(|ui, app: &mut App<EngineModel>| app.show(ui), app);
    bn_client_egui::fonts::install(&h.ctx, &root);
    choose(&mut h, "menu:tab", "new_game");
    choose(&mut h, "menu:entry", "new_game:play_now");
    until(&mut h, 600, "the map", |h| h.state().model.size().x > 0);
    until(&mut h, 300, "the settled game", |h| {
        let app = h.state();
        app.model.menu().is_none() && !app.model.busy() && !app.display().loading()
    });
    settle(&mut h, 2);
    h.input_mut().events.push(egui::Event::Text("!".into()));
    settle(&mut h, 2);
    until(&mut h, 180, "safe mode off", |h| !h.state().model.busy());
    h
}

/// What one frame shows of the side panels: where the sidebar is and the text of every label in it, top to
/// bottom, and the shortcut bar's buttons drawn otherwise than the engine has them enabled.
#[derive(Clone, Debug, PartialEq)]
struct Shown {
    /// Engine messages received before the frame.
    received: u64,
    rect: Option<egui::Rect>,
    labels: Vec<String>,
    mismatched: Vec<String>,
}

fn panel_rect(h: &H, name: &str) -> Option<egui::Rect> {
    egui::containers::panel::PanelState::load(&h.ctx, egui::Id::new(name)).map(|s| s.outer_rect)
}

fn shown(h: &H) -> Shown {
    let name = h
        .state()
        .model
        .sidebar_layout()
        .map_or("sidebar".to_string(), |l| {
            format!("sidebar-{:?}-{}", l.side, l.width)
        });
    let rect = panel_rect(h, &name);
    let within = |rect: Option<egui::Rect>, n: &egui_kittest::kittest::AccessKitNode<'_>| {
        rect.zip(n.bounding_box())
            .is_some_and(|(r, b)| r.contains(egui::pos2(b.x0 as f32 + 1.0, b.y0 as f32 + 1.0)))
    };
    let labels = h
        .query_all(
            by().role(egui::accesskit::Role::Label)
                .predicate(|n| within(rect, n)),
        )
        .filter_map(|n| n.accesskit_node().value())
        .collect();
    let bar = panel_rect(h, "shortcut-bar");
    let enabled = h
        .state()
        .model
        .shortcuts()
        .into_iter()
        .map(|s| (bn_client_egui::markup::plain(&s.label.shown()), s.enabled))
        .collect::<Vec<_>>();
    let mismatched = h
        .query_all(
            by().role(egui::accesskit::Role::Button)
                .predicate(|n| within(bar, n)),
        )
        .filter_map(|n| {
            let node = n.accesskit_node();
            let label = node.label()?;
            let on = enabled.iter().find(|(l, _)| *l == label)?.1;
            (node.is_disabled() == on).then_some(label)
        })
        .collect();
    Shown {
        received: h.state().model.received(),
        rect,
        labels,
        mismatched,
    }
}

/// The key `key` pressed down, or let up.
fn key_event(h: &mut H, key: egui::Key, pressed: bool, repeat: bool) {
    h.input_mut().events.push(egui::Event::Key {
        key,
        physical_key: Some(key),
        pressed,
        repeat,
        modifiers: egui::Modifiers::NONE,
    });
}

fn square(h: &H) -> (i32, i32) {
    let m = &h.state().model;
    (m.player().x + m.origin().x, m.player().y + m.origin().y)
}

#[test]
#[ignore = "requires BN_ENGINE pointing at an engine binary"]
fn a_held_walk_keeps_every_frame_of_the_side_panels_as_the_engine_sent_them() {
    let mut h = start();
    let shots = std::env::var_os("SHOT_DIR").map(std::path::PathBuf::from);
    if let Some(dir) = &shots {
        std::fs::create_dir_all(dir).unwrap();
    }
    let mut frames: Vec<Shown> = vec![shown(&h)];
    let mut steps = 0;
    for (text, key) in [
        ("l", egui::Key::L),
        ("j", egui::Key::J),
        ("h", egui::Key::H),
        ("k", egui::Key::K),
    ] {
        if steps >= 10 {
            break;
        }
        let mut at = square(&h);
        key_event(&mut h, key, true, false);
        h.input_mut().events.push(egui::Event::Text(text.into()));
        let up = Instant::now() + Duration::from_secs(4);
        let mut next = Instant::now() + Duration::from_millis(500);
        while Instant::now() < up && steps < 12 {
            if Instant::now() >= next {
                key_event(&mut h, key, true, true);
                h.input_mut().events.push(egui::Event::Text(text.into()));
                next += Duration::from_millis(33);
            }
            h.step();
            if square(&h) != at {
                at = square(&h);
                steps += 1;
            }
            if let Some(dir) = &shots {
                let image = h.render().expect("renders");
                image
                    .save(dir.join(format!("frame-{:04}.png", frames.len())))
                    .unwrap();
            }
            frames.push(shown(&h));
            std::thread::sleep(Duration::from_millis(2));
        }
        key_event(&mut h, key, false, false);
        settle(&mut h, 1);
        frames.push(shown(&h));
    }
    println!("LIVE steps {steps}, frames {}", frames.len());
    assert!(steps >= 10, "no open way for ten steps: {steps}");
    // A label gone for a frame while the frames around it keep it, with no engine message on one side of
    // that frame, is the client's flicker, not the engine's news.
    let mut blinks = vec![];
    for (i, w) in frames.windows(2).enumerate() {
        if w[0].rect != w[1].rect {
            println!(
                "LIVE frame {}: rect {:?} -> {:?}",
                i + 1,
                w[0].rect,
                w[1].rect
            );
        }
    }
    for (i, w) in frames.windows(3).enumerate() {
        let lost = w[0]
            .labels
            .iter()
            .filter(|l| !w[1].labels.contains(l) && w[2].labels.contains(l))
            .cloned()
            .collect::<Vec<_>>();
        let quiet = w[0].received == w[1].received || w[1].received == w[2].received;
        let moved = w[0].rect == w[2].rect && w[1].rect != w[0].rect;
        if (!lost.is_empty() && quiet) || moved {
            blinks.push((i + 1, lost, w[1].rect));
        }
    }
    for b in &blinks {
        println!("LIVE blink at frame {}: {:?}", b.0, b);
    }
    let mismatched = mismatched(&frames);
    assert!(blinks.is_empty(), "{} frames flickered", blinks.len());
    assert!(
        mismatched.is_empty(),
        "{} frames drew shortcuts otherwise than the engine has them",
        mismatched.len()
    );
}

/// The frames that drew a shortcut otherwise than the engine has it, with how many.
fn mismatched(frames: &[Shown]) -> Vec<(usize, usize)> {
    let mismatched = frames
        .iter()
        .enumerate()
        .filter(|(_, f)| !f.mismatched.is_empty())
        .map(|(i, f)| (i, f.mismatched.len()))
        .collect::<Vec<_>>();
    println!(
        "LIVE frames with shortcuts drawn otherwise than the engine has them: {} of {}: {mismatched:?}",
        mismatched.len(),
        frames.len()
    );
    mismatched
}

#[test]
#[ignore = "requires BN_ENGINE pointing at an engine binary"]
fn the_shortcut_bar_follows_the_engine_into_look_aim_throw_and_its_own_screens_and_back() {
    let mut h = start();
    let mut frames = vec![shown(&h)];
    let label = |h: &H, id: &str| {
        let shortcuts = h.state().model.shortcuts();
        let shortcut = shortcuts.iter().find(|s| s.affordance_id == id);
        bn_client_egui::markup::plain(&shortcut.expect(id).label.shown())
    };
    // The keys of look, aim and throw, then the bar's own buttons to the inventory screen and the wait menu.
    for key in ["x", "f", "t", "inventory", "wait"] {
        let before = h.state().model.screens().len();
        match key.len() {
            1 => h.input_mut().events.push(egui::Event::Text(key.into())),
            _ => h.get_by_label(&label(&h, key)).click(),
        }
        let end = Instant::now() + Duration::from_secs(3);
        while Instant::now() < end {
            h.step();
            frames.push(shown(&h));
            std::thread::sleep(Duration::from_millis(2));
        }
        println!(
            "LIVE {key}: screens {before} -> {}, menu {:?}",
            h.state().model.screens().len(),
            h.state().model.menu().map(|m| m.context)
        );
        h.key_press(egui::Key::Escape);
        settle(&mut h, 2);
        frames.push(shown(&h));
    }
    assert!(mismatched(&frames).is_empty());
}
