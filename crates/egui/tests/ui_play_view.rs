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

#[test]
fn the_top_bar_names_the_avatars_activity_and_how_much_of_it_is_done() {
    let (t, script) = session();
    let mut h = Harness::builder()
        .with_size(egui::vec2(1280.0, 800.0))
        .build_ui_state(
            |ui, app: &mut App<EngineModel>| app.show(ui),
            App::new(EngineModel::new(Box::new(t))),
        );
    h.run_steps(3);
    let mut busy = fixture("events_41_walk");
    busy["params"]["events"][0]["changes"]["avatar"]["activity"] =
        serde_json::json!({"id": "ACT_CRAFT", "name": "crafting", "done": 350});
    script.push(busy);
    h.run_steps(4);
    let name = h.get_by_label("crafting").rect();
    let bar = h.get_by_label("35%").rect();
    let top = h.get_by_label("Actions").rect();
    assert!(
        name.max.y <= top.max.y + 4.0 && bar.max.y <= top.max.y + 4.0,
        "{name:?} {bar:?} {top:?}"
    );
    // Over the avatar's square, a thin track filled for the part done.
    let rects = h
        .output()
        .shapes
        .iter()
        .filter_map(|c| match &c.shape {
            egui::Shape::Rect(r) if r.rect.height() == 4.0 => Some(r.clone()),
            _ => None,
        })
        .collect::<Vec<_>>();
    let filled = h.ctx.global_style().visuals.selection.bg_fill;
    let track = rects.iter().find(|r| r.fill != filled).expect("a track");
    let part = rects
        .iter()
        .find(|r| r.fill == filled)
        .expect("a filled part");
    assert_eq!(part.rect.min, track.rect.min);
    assert!(
        (part.rect.width() / track.rect.width() - 0.35).abs() < 0.01,
        "{part:?} {track:?}"
    );
}
