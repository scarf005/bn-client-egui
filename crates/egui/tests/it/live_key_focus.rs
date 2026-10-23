//! Against a real engine: arrows in the focused inventory window move its cursor and not the player, and `>` in
//! the focused map window changes its level and climbs no stairs.
//!
//! `BN_ENGINE=<engine> TMPDIR=<dir> SHOT_DIR=<dir> cargo test --release -p bn-client-egui --test it live_key_focus:: --
//! --ignored --nocapture`
use crate::common::live::{choose, dismiss_popups, safe_mode_off, save};
use bn_client_egui::protocol::PanelKind;
use bn_client_egui::transport::game_root;
use bn_client_egui::watchdog::Watchdog;
use bn_client_egui::{App, EngineModel, GameModel};
use eframe::egui;
use egui_kittest::Harness;
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

/// Steps and lets the engine answer.
fn press(h: &mut H, key: egui::Key) {
    h.key_press(key);
    for _ in 0..12 {
        h.run_steps(1);
        std::thread::sleep(Duration::from_millis(25));
    }
}

/// The row the engine's cursor is on in the inventory window's last read.
fn cursor(h: &H) -> Option<String> {
    let screen = h.state().model.watched_panel(PanelKind::Inventory)?;
    screen.focus.as_ref()?.row_id.clone()
}

/// A new game on the map, safe mode off, in a client of 1600 by 900.
fn playing<'a>() -> H<'a> {
    let engine = std::env::var_os("BN_ENGINE").expect("set BN_ENGINE");
    let root = game_root(engine.as_ref(), None).expect("game root");
    let dir = std::env::temp_dir().join(format!("bn-key-focus-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let settings = dir.join("settings.json");
    std::fs::write(&settings, r#"{"tiles":false,"tileset":null}"#).unwrap();
    let transport = crate::common::live::english_transport();
    let app = App::new(EngineModel::new(Box::new(transport)))
        .with_settings(settings)
        .with_root(root);
    let mut h = crate::common::shot_harness(egui::vec2(1600.0, 900.0), app);
    choose(&mut h, "menu:tab", "new_game");
    choose(&mut h, "menu:entry", "new_game:play_now");
    run_until(&mut h, "the map", 600, |h| h.state().model.size().x > 0);
    dismiss_popups(&mut h);
    safe_mode_off(&mut h);
    dismiss_popups(&mut h);
    h
}

/// Where the player stands in the world: the view scrolls, so the square in the view is not it.
fn square(h: &H) -> bn_client_egui::Pos {
    let (player, origin) = (h.state().model.player(), h.state().model.origin());
    bn_client_egui::Pos {
        x: origin.x + player.x,
        y: origin.y + player.y,
    }
}

#[test]
#[ignore = "needs BN_ENGINE"]
fn down_in_the_focused_inventory_window_moves_its_cursor_and_not_the_player() {
    let mut h = playing();
    key(&mut h, "i");
    run_until(&mut h, "the inventory", 60, |h| {
        h.state()
            .model
            .watched_panel(PanelKind::Inventory)
            .is_some()
    });
    h.run_steps(5);
    crate::common::settle_windows(&mut h);
    let (before, cursor_before) = (square(&h), cursor(&h));
    save(&mut h, "inventory-before-down.png");
    press(&mut h, egui::Key::ArrowDown);
    press(&mut h, egui::Key::ArrowDown);
    save(&mut h, "inventory-after-down.png");
    let (after, cursor_after) = (square(&h), cursor(&h));
    eprintln!(
        "inventory Down: player {before:?} -> {after:?}; cursor {cursor_before:?} -> {cursor_after:?}"
    );
    assert_eq!(
        before, after,
        "the player stayed with the inventory focused"
    );
    assert_ne!(cursor_before, cursor_after, "the inventory cursor moved");
}

#[test]
#[ignore = "needs BN_ENGINE"]
fn the_level_keys_in_the_focused_map_window_change_its_level_and_climb_no_stairs() {
    let mut h = playing();
    key(&mut h, "m");
    run_until(&mut h, "the map squares", 60, |h| {
        h.state().model.watched_overmap().is_some()
    });
    h.run_steps(5);
    let level = |h: &H| h.state().model.watched_overmap().map(|o| o.origin.z);
    let (z_before, at_before) = (level(&h), square(&h));
    save(&mut h, "map-before-level.png");
    key(&mut h, ">");
    for _ in 0..12 {
        h.run_steps(1);
        std::thread::sleep(Duration::from_millis(25));
    }
    run_until(&mut h, "the command", 60, |h| !h.state().model.busy());
    dismiss_popups(&mut h);
    h.run_steps(5);
    save(&mut h, "map-after-level.png");
    let (z_after, at_after) = (level(&h), square(&h));
    eprintln!("map >: level {z_before:?} -> {z_after:?}; player {at_before:?} -> {at_after:?}");
    assert_eq!(at_before, at_after, "no stairs were taken");
    assert_ne!(z_before, z_after, "the map window changed its level");
}
