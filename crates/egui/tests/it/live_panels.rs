//! Against a real engine: the character sheet and inventory keys open the client's panel windows over the game,
//! and the player walks with both open.
//!
//! `BN_ENGINE=<engine> SHOT_DIR=<dir> cargo test --release -p bn-client-egui --test it live_panels:: -- --ignored
//! --nocapture`
use crate::common::live::{choose, dismiss_popups, safe_mode_off, save};
use bn_client_egui::protocol::PanelKind;
use bn_client_egui::transport::game_root;
use bn_client_egui::watchdog::Watchdog;
use bn_client_egui::{App, EngineModel, GameModel};
use eframe::egui;
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
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
    safe_mode_off(&mut h);
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

/// The id of the tab selected in the screen last read for `panel`.
fn selected_tab(h: &H, panel: PanelKind) -> Option<String> {
    let screen = h.state().model.watched_panel(panel)?;
    match screen.root.as_ref()? {
        bn_client_egui::protocol::Component::Tabs(tabs) => {
            tabs.tabs.iter().find(|t| t.selected).map(|t| t.id.clone())
        }
        _ => None,
    }
}

#[test]
#[ignore = "needs BN_ENGINE"]
fn a_closed_character_sheet_reopens_on_the_tab_it_was_left_on() {
    let engine = std::env::var_os("BN_ENGINE").expect("set BN_ENGINE");
    for (width, height) in [(1280.0, 800.0), (1920.0, 1080.0)] {
        let root = game_root(engine.as_ref(), None).expect("game root");
        let dir = std::env::temp_dir().join(format!("bn-panel-tab-{}-{width}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let settings = dir.join("settings.json");
        std::fs::write(&settings, r#"{"tiles":false,"tileset":null}"#).unwrap();
        let transport = crate::common::live::english_transport();
        let app = App::new(EngineModel::new(Box::new(transport)))
            .with_settings(settings)
            .with_root(root);
        let mut h = crate::common::shot_harness(egui::vec2(width, height), app);
        choose(&mut h, "menu:tab", "new_game");
        choose(&mut h, "menu:entry", "new_game:play_now");
        run_until(&mut h, "the map", 600, |h| h.state().model.size().x > 0);
        dismiss_popups(&mut h);
        safe_mode_off(&mut h);
        dismiss_popups(&mut h);
        let tab = |h: &H| selected_tab(h, PanelKind::Character);
        key(&mut h, "@");
        run_until(&mut h, "the character sheet", 60, |h| tab(h).is_some());
        h.get_all_by_label_contains("ENCUMBRANCE")
            .find(|n| n.accesskit_node().role() == egui::accesskit::Role::Tab)
            .expect("the tab")
            .click();
        run_until(&mut h, "the chosen tab", 60, |h| {
            tab(h).as_deref() == Some("encumbrance")
        });
        key(&mut h, "@");
        h.run_steps(5);
        key(&mut h, "@");
        run_until(&mut h, "the reopened sheet", 60, |h| {
            tab(h).as_deref() == Some("encumbrance")
        });
        h.run_steps(5);
        crate::common::settle_windows(&mut h);
        save(&mut h, &format!("sheet-reopened-{}.png", width as u32));
    }
}

#[test]
#[ignore = "needs BN_ENGINE"]
fn the_character_and_inventory_windows_at_150_percent_stay_on_screen() {
    let engine = std::env::var_os("BN_ENGINE").expect("set BN_ENGINE");
    let root = game_root(engine.as_ref(), None).expect("game root");
    let dir = std::env::temp_dir().join(format!("bn-panels-scale-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let settings = dir.join("settings.json");
    std::fs::write(
        &settings,
        r#"{"tiles":false,"tileset":null,"interface_scale":150}"#,
    )
    .unwrap();
    let transport = crate::common::live::english_transport();
    let app = App::new(EngineModel::new(Box::new(transport)))
        .with_settings(settings)
        .with_root(root);
    let mut h = crate::common::shot_harness(egui::vec2(1920.0, 1080.0), app);
    choose(&mut h, "menu:tab", "new_game");
    choose(&mut h, "menu:entry", "new_game:play_now");
    run_until(&mut h, "the map", 600, |h| h.state().model.size().x > 0);
    dismiss_popups(&mut h);
    safe_mode_off(&mut h);
    dismiss_popups(&mut h);
    let read = |panel| move |h: &H| h.state().model.watched_panel(panel).is_some();
    key(&mut h, "@");
    run_until(&mut h, "the sheet", 60, read(PanelKind::Character));
    key(&mut h, "i");
    run_until(&mut h, "the inventory", 60, read(PanelKind::Inventory));
    h.run_steps(5);
    crate::common::settle_windows(&mut h);
    save(&mut h, "scale-150-1920.png");
    // The window is 1920 / 1.5 points wide at 150 %.
    let room = h.ctx.content_rect();
    for panel in [PanelKind::Character, PanelKind::Inventory] {
        let title = title(&h, panel);
        let window = h.get_by_role_and_label(egui::accesskit::Role::Window, &title);
        assert!(
            room.contains_rect(window.rect()),
            "{title}: {:?} in {room:?}",
            window.rect()
        );
    }
}
