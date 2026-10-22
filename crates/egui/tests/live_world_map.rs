//! Against a real engine: the map key opens the client's map window over the game, and the player walks with it
//! open.
//!
//! `BN_ENGINE=<engine> SHOT_DIR=<dir> cargo test --release -p bn-client-egui --test live_world_map -- --ignored
//! --nocapture`
mod common;
use bn_client_egui::transport::game_root;
use bn_client_egui::watchdog::Watchdog;
use bn_client_egui::{App, EngineModel, GameModel};
use common::live::{choose, dismiss_popups, save};
use eframe::egui;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use std::time::Duration;

type H<'a> = Harness<'a, App<EngineModel>>;

fn run_until(h: &mut H, what: &str, secs: u64, mut done: impl FnMut(&H) -> bool) {
    let mut watchdog = Watchdog::new(Duration::from_secs(secs), Duration::from_secs(900));
    while !done(h) {
        assert!(
            !watchdog.expired(h.state().model.received()),
            "timed out: {what}"
        );
        h.run_steps(1);
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn key(h: &mut H, text: &str) {
    h.event(egui::Event::Text(text.into()));
    h.run_steps(2);
}

#[test]
#[ignore = "needs BN_ENGINE"]
fn the_player_walks_with_the_map_window_open() {
    let engine = std::env::var_os("BN_ENGINE").expect("set BN_ENGINE");
    let root = game_root(engine.as_ref(), None).expect("game root");
    let dir = std::env::temp_dir().join(format!("bn-world-map-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let settings = dir.join("settings.json");
    std::fs::write(&settings, r#"{"tiles":false,"tileset":null}"#).unwrap();
    let transport = common::live::english_transport();
    let app = App::new(EngineModel::new(Box::new(transport)))
        .with_settings(settings)
        .with_root(root);
    let mut h = common::shot_harness(egui::vec2(1280.0, 800.0), app);
    choose(&mut h, "menu:tab", "new_game");
    choose(&mut h, "menu:entry", "new_game:play_now");
    run_until(&mut h, "the map", 600, |h| h.state().model.size().x > 0);
    dismiss_popups(&mut h);
    // A start with a zombie in view refuses steps until safe mode is off, as natively.
    key(&mut h, "!");
    run_until(&mut h, "safe mode", 120, |h| !h.state().model.busy());
    dismiss_popups(&mut h);

    key(&mut h, "m");
    run_until(&mut h, "the map squares", 60, |h| {
        h.state().model.watched_overmap().is_some()
    });
    h.run_steps(5);
    assert!(
        h.state()
            .model
            .screens()
            .last()
            .is_some_and(|s| s.kind.as_deref() == Some("map"))
    );
    h.get_by_label("Travel and notes");
    save(&mut h, "map-open.png");
    let square = |h: &H| (h.state().model.player(), h.state().model.origin());
    let before = square(&h);
    // Steps with the window open, turning where the way is blocked.
    for way in ["l", "j", "h", "k", "y", "u", "b", "n"] {
        key(&mut h, way);
        run_until(&mut h, "the step", 60, |h| !h.state().model.busy());
        dismiss_popups(&mut h);
        h.run_steps(3);
        if square(&h) != before {
            break;
        }
    }
    assert_ne!(square(&h), before, "the player moved with the map open");
    h.run_steps(5);
    h.get_by_label("Travel and notes");
    save(&mut h, "map-walked.png");
}
