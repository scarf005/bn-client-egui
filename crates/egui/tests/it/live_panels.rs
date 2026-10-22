//! Against a real engine: the character sheet and inventory keys open the client's panel windows over the game,
//! and the player walks with both open.
//!
//! `BN_ENGINE=<engine> SHOT_DIR=<dir> cargo test --release -p bn-client-egui --test it live_panels:: -- --ignored
//! --nocapture`
use crate::common::live::{choose, dismiss_popups, save};
use bn_client_egui::protocol::PanelKind;
use bn_client_egui::transport::game_root;
use bn_client_egui::watchdog::Watchdog;
use bn_client_egui::{App, EngineModel, GameModel};
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

/// The title of the screen last read for `panel`, which its window is titled with.
fn title(h: &H, panel: PanelKind) -> String {
    let screen = h.state().model.watched_panel(panel).expect("a screen read");
    let title = screen.title.as_ref().map(|t| t.shown()).unwrap_or_default();
    bn_client_egui::markup::plain(&title)
}

/// The window titled `title` is shown.
fn window(h: &H, title: &str) {
    h.get_by_role_and_label(egui::accesskit::Role::Window, title);
}

#[test]
#[ignore = "needs BN_ENGINE"]
fn the_player_walks_with_the_character_and_inventory_windows_open() {
    let engine = std::env::var_os("BN_ENGINE").expect("set BN_ENGINE");
    let root = game_root(engine.as_ref(), None).expect("game root");
    let dir = std::env::temp_dir().join(format!("bn-panels-{}", std::process::id()));
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
    // A start with a zombie in view refuses steps until safe mode is off, as natively.
    key(&mut h, "!");
    run_until(&mut h, "safe mode", 120, |h| !h.state().model.busy());
    dismiss_popups(&mut h);

    let read = |panel| move |h: &H| h.state().model.watched_panel(panel).is_some();
    key(&mut h, "@");
    run_until(
        &mut h,
        "the character sheet",
        60,
        read(PanelKind::Character),
    );
    key(&mut h, "i");
    run_until(&mut h, "the inventory", 60, read(PanelKind::Inventory));
    h.run_steps(5);
    crate::common::settle_windows(&mut h);
    assert!(
        h.state()
            .model
            .screens()
            .last()
            .is_some_and(|s| s.kind.as_deref() == Some("map")),
        "the engine's own screens stay closed"
    );
    save(&mut h, "panels-open.png");
    let (sheet, inventory) = (
        title(&h, PanelKind::Character),
        title(&h, PanelKind::Inventory),
    );
    window(&h, &sheet);
    window(&h, &inventory);
    let square = |h: &H| (h.state().model.player(), h.state().model.origin());
    let before = square(&h);
    // Steps with the windows open, turning where the way is blocked.
    for way in ["l", "j", "h", "k", "y", "u", "b", "n"] {
        key(&mut h, way);
        run_until(&mut h, "the step", 60, |h| !h.state().model.busy());
        dismiss_popups(&mut h);
        h.run_steps(3);
        if square(&h) != before {
            break;
        }
    }
    assert_ne!(square(&h), before, "the player moved with the windows open");
    h.run_steps(5);
    save(&mut h, "panels-walked.png");
    window(&h, &sheet);
    window(&h, &inventory);
}
