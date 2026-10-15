#![recursion_limit = "512"]
//! What the play view shows above the map: only the top bar, with the client's own choices in the settings
//! window; the cursor's square and the command stage are said to screen readers, not drawn.
mod common;
use bn_client_egui::{App, EngineModel};
use common::{fixture, session};
use eframe::egui;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;

type H<'a> = Harness<'a, App<EngineModel>>;

/// A game in progress in a window of `size`.
fn playing<'a>(size: egui::Vec2) -> H<'a> {
    let (t, script) = session();
    let mut h = Harness::builder().with_size(size).build_ui_state(
        |ui, app: &mut App<EngineModel>| app.show(ui),
        App::new(EngineModel::new(Box::new(t))),
    );
    h.run_steps(3);
    script.push(fixture("events_41_walk"));
    h.run_steps(4);
    h
}

#[test]
fn nothing_but_the_top_bar_is_above_the_map() {
    for size in [egui::vec2(1280.0, 800.0), egui::vec2(1920.0, 1080.0)] {
        let h = playing(size);
        let map = h.get_by_label("Map").rect();
        let bar = h.get_by_label("Actions").rect();
        assert!(
            map.min.y - bar.max.y <= 8.0,
            "{map:?} under {bar:?} at {size:?}"
        );
        for chrome in ["Tiles", "ASCII", "Tileset", "Reduced motion"] {
            assert!(h.query_by_label_contains(chrome).is_none(), "{chrome}");
        }
    }
}

#[test]
fn the_cursors_square_is_said_to_screen_readers_and_takes_no_room() {
    let h = playing(egui::vec2(1280.0, 800.0));
    let said = h.get_by_label_contains("Player at").rect();
    assert!(said.width() < 1.0 && said.height() < 1.0, "{said:?}");
}

#[test]
fn the_view_mode_tileset_and_motion_choices_are_in_the_client_settings_window() {
    let mut h = playing(egui::vec2(1280.0, 800.0));
    h.get_by_label("Client settings").click();
    h.run_steps(3);
    for choice in ["Tiles", "ASCII", "Reduced motion"] {
        assert!(h.query_by_label(choice).is_some(), "{choice}");
    }
}
