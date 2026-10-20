//! Against a real engine: every toolbar button's screen and every manager of the main menu opens, keeps its
//! window size while it stays open, and closes back to the map.
//!
//! `BN_ENGINE=<engine> SHOT_DIR=<dir> cargo test --release -p bn-client-egui --test live_screen_sweep --
//! --ignored --nocapture`
mod common;
use bn_client_egui::interaction::InvokeError;
use bn_client_egui::protocol::InvokeArgs;
use bn_client_egui::transport::{StdioTransport, game_root};
use bn_client_egui::watchdog::Watchdog;
use bn_client_egui::{App, EngineModel, GameModel, MenuModel};
use eframe::egui;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use std::time::Duration;

type H<'a> = Harness<'a, App<EngineModel>>;

fn run_until(h: &mut H, what: &str, secs: u64, mut done: impl FnMut(&H) -> bool) -> bool {
    let mut watchdog = Watchdog::new(Duration::from_secs(secs), Duration::from_secs(900));
    while !done(h) {
        if watchdog.expired(h.state().model.received()) {
            eprintln!("timed out: {what}");
            return false;
        }
        h.run_steps(1);
        std::thread::sleep(Duration::from_millis(10));
    }
    true
}

fn choose(h: &mut H, affordance: &str, row: &str) {
    let offered = |h: &H| {
        h.state()
            .model
            .screens()
            .last()
            .is_some_and(|s| s.affordances.iter().any(|a| a.id == affordance))
    };
    loop {
        assert!(run_until(h, row, 180, |h| offered(h) && !h.state().model.busy()));
        let args = InvokeArgs {
            row: Some(row.into()),
            ..Default::default()
        };
        match h.state_mut().model.invoke_affordance(affordance, args) {
            Err(InvokeError::Busy) => h.run_steps(1),
            sent => return sent.unwrap_or_else(|e| panic!("{affordance} on {row}: {e:?}")),
        }
    }
}

fn on_map(h: &H) -> bool {
    let m = &h.state().model;
    !m.busy()
        && m.menu().is_none()
        && m.screens()
            .last()
            .is_some_and(|s| s.kind.as_deref() == Some("map"))
}

/// Closes what is open over the map: a screen by its cancel, a 0.1 menu by its only choice or its cancel.
fn back_to_map(h: &mut H) -> bool {
    for _ in 0..8 {
        if on_map(h) {
            return true;
        }
        let model = &h.state().model;
        let cancel = model
            .screens()
            .last()
            .and_then(|s| {
                s.affordances.iter().find(|a| {
                    a.enabled && a.role == Some(bn_client_egui::protocol::AffordanceRole::Cancel)
                })
            })
            .map(|a| a.id.clone());
        let menu = model
            .menu()
            .map(|m| (m.can_cancel, m.choices.first().map(|c| c.id.clone())));
        match (cancel, menu) {
            (Some(id), _) => {
                let _ = h
                    .state_mut()
                    .model
                    .invoke_affordance(&id, InvokeArgs::default());
            }
            (None, Some((false, Some(id)))) => h.state_mut().model.choose(&id),
            (None, Some(_)) => h.state_mut().model.cancel_menu(),
            (None, None) => h.event(egui::Event::Key {
                key: egui::Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }),
        }
        run_until(h, "closing", 60, |h| !h.state().model.busy());
        h.run_steps(5);
    }
    on_map(h)
}

fn windows(h: &H) -> Vec<egui::Rect> {
    h.get_all_by_role(egui::accesskit::Role::Window)
        .map(|n| n.rect())
        .collect()
}

/// Lets the window `name` opened settle, checks that it keeps its size, saves a picture of it and closes it.
fn measure(h: &mut H, name: &str, faults: &mut Vec<String>) {
    let opened = run_until(h, name, 60, |h| !on_map(h) && !h.state().model.busy());
    if !opened {
        eprintln!("{name}: no window");
        return;
    }
    h.run_steps(20);
    let first = windows(h);
    h.run_steps(120);
    let later = windows(h);
    let kind = h
        .state()
        .model
        .screens()
        .last()
        .and_then(|s| s.kind.clone())
        .unwrap_or_default();
    if let Some(dir) = std::env::var_os("SHOT_DIR") {
        std::fs::create_dir_all(&dir).unwrap();
        let image = h.render().expect("renders");
        image
            .save(std::path::Path::new(&dir).join(format!("{}.png", name.replace(' ', "_"))))
            .unwrap();
    }
    let grew = first
        .iter()
        .zip(&later)
        .any(|(a, b)| (a.width() - b.width()).abs() > 1.0 || (a.height() - b.height()).abs() > 1.0);
    eprintln!("{name} ({kind}): {first:?} -> {later:?}");
    if grew || first.len() != later.len() {
        faults.push(format!(
            "{name} ({kind}) changed size: {first:?} -> {later:?}"
        ));
    }
    if !back_to_map(h) {
        faults.push(format!("{name} ({kind}) did not close to the map"));
    }
}

#[test]
#[ignore = "needs BN_ENGINE"]
fn every_toolbar_and_manager_screen_keeps_its_size_and_closes_to_the_map() {
    let engine = std::env::var_os("BN_ENGINE").expect("set BN_ENGINE");
    let root = game_root(engine.as_ref(), None).expect("game root");
    let dir = std::env::temp_dir().join(format!("bn-sweep-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let settings = dir.join("settings.json");
    std::fs::write(&settings, r#"{"tiles":false,"tileset":null}"#).unwrap();
    let transport = StdioTransport::spawn_fresh(engine.as_ref()).expect("engine starts");
    let app = App::new(EngineModel::new(Box::new(transport)))
        .with_settings(settings)
        .with_root(root);
    let mut h = common::shot_harness(egui::vec2(1280.0, 800.0), app);
    choose(&mut h, "menu:tab", "new_game");
    choose(&mut h, "menu:entry", "new_game:play_now");
    assert!(run_until(&mut h, "the map", 600, |h| h
        .state()
        .model
        .size()
        .x
        > 0));
    assert!(back_to_map(&mut h), "the start leaves no window open");
    let shortcuts = h
        .state()
        .model
        .shortcuts()
        .into_iter()
        .filter(|s| s.enabled)
        .map(|s| s.affordance_id)
        .collect::<Vec<_>>();
    assert!(!shortcuts.is_empty());
    let mut faults = vec![];
    for id in shortcuts {
        assert!(back_to_map(&mut h), "back on the map before {id}");
        h.state_mut().model.invoke_action(&id);
        measure(&mut h, &id, &mut faults);
    }
    // The main menu's managers, which have no button of their own.
    for entry in [
        "View Help",
        "Display keybindings menu",
        "Autopickup manager",
        "Autonotes manager",
        "Safe Mode manager",
        "Distractions manager",
        "Color manager",
        "Active World Mods",
        "Action Menu",
    ] {
        assert!(back_to_map(&mut h), "back on the map before {entry}");
        h.state_mut().model.invoke_action("main_menu");
        assert!(run_until(&mut h, "the main menu", 60, |h| {
            h.state().model.menu().is_some() && !h.state().model.busy()
        }));
        let row = h
            .state()
            .model
            .menu()
            .and_then(|m| m.choices.into_iter().find(|c| c.label == entry))
            .map(|c| c.id);
        let Some(row) = row else {
            faults.push(format!("the main menu has no {entry}"));
            continue;
        };
        h.state_mut().model.choose(&row);
        run_until(&mut h, entry, 60, |h| !h.state().model.busy());
        measure(&mut h, entry, &mut faults);
    }
    assert!(faults.is_empty(), "{faults:#?}");
}
