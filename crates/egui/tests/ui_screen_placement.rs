#![recursion_limit = "512"]
//! Where windows sit: a full screen fills the map's room and leaves the sidebar, a dialog is centred and kept on
//! screen, a screen that puts a cursor on the map keeps off the cursor and the avatar.
mod common;
use bn_client_egui::app::CELL;
use bn_client_egui::transport::ScriptHandle;
use bn_client_egui::{App, EngineModel, GameModel};
use common::{fixture, session};
use eframe::egui;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use serde_json::{Value, json};

type H<'a> = Harness<'a, App<EngineModel>>;

fn example(file: &str, name: &str) -> Value {
    let path = format!(
        "{}/../core/tests/fixtures/0.2.examples/{file}.json",
        env!("CARGO_MANIFEST_DIR")
    );
    let all = serde_json::from_str::<Value>(&std::fs::read_to_string(path).unwrap()).unwrap();
    all["valid"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["name"] == name)
        .unwrap_or_else(|| panic!("no example {name}"))["value"]
        .clone()
}

/// A game in progress in a window of `size`, with `screen` on top of the map.
fn playing<'a>(size: egui::Vec2, screen: Option<Value>) -> (H<'a>, ScriptHandle) {
    let (t, script) = session();
    let mut h = Harness::builder().with_size(size).build_ui_state(
        |ui, app: &mut App<EngineModel>| app.show(ui),
        App::new(EngineModel::new(Box::new(t))),
    );
    h.run_steps(3);
    script.push(fixture("events_41_walk"));
    h.run_steps(4);
    if let Some(screen) = screen {
        let map =
            json!({"screen_id": "screen:map", "role": "map", "root": null, "affordances": []});
        script.push(json!({"jsonrpc": "2.0", "method": "bn.events", "params": {"epoch": "e7f3", "events": [{
            "sequence": "42", "revision": "33", "type": "interaction.changed", "changes": {"interaction": {
            "boundary_id": "b:60",
            "readiness": {"phase": "playing", "game_ready": true, "accepts_interaction_commands": true,
                "accepts_registered_actions": true},
            "actions": [], "interaction": null, "stack": [map, screen]}}}]}}));
        h.run_steps(6);
    }
    (h, script)
}

/// The window of the screen the engine names `kind`.
fn window_of(h: &H, kind: &str) -> egui::Rect {
    let id = egui::Id::new(("dialog", format!("screen:{kind}")));
    h.ctx
        .memory(|m| m.area_rect(id))
        .unwrap_or_else(|| panic!("no window {kind}"))
}

fn screen_rect(size: egui::Vec2) -> egui::Rect {
    egui::Rect::from_min_size(egui::Pos2::ZERO, size)
}

#[test]
fn a_full_screen_fills_the_map_room_and_leaves_the_sidebar_visible() {
    let size = egui::vec2(1280.0, 800.0);
    let (h, _) = playing(size, Some(example("crafting", "Crafting screen")));
    let window = window_of(&h, "crafting");
    let map = h.get_by_label("Map").rect();
    assert!(window.min.y <= 40.0, "{window:?}");
    assert!(window.max.y <= size.y, "{window:?}");
    let ground = h.get_by_label("Ground").rect();
    assert!(
        window.max.x <= ground.min.x,
        "{window:?} over the sidebar at {ground:?}"
    );
    assert!(
        window.width() >= map.width() - 40.0,
        "{window:?} vs {map:?}"
    );
}

#[test]
fn a_dialog_is_centred_in_the_map_room_and_wholly_on_screen() {
    let size = egui::vec2(1280.0, 800.0);
    let (h, _) = playing(size, Some(example("info", "message log")));
    let window = window_of(&h, "messages");
    assert!(screen_rect(size).contains_rect(window), "{window:?}");
    let map = h.get_by_label("Map").rect();
    assert!(
        (window.center().x - map.center().x).abs() < 40.0,
        "{window:?}"
    );
}

#[test]
fn the_look_screen_covers_neither_the_cursor_nor_the_avatar() {
    let size = egui::vec2(1280.0, 800.0);
    let (h, _) = playing(size, Some(example("look", "look-around screen")));
    let window = window_of(&h, "look");
    let map = h.get_by_label("Map").rect();
    let model = &h.state().model;
    let cells = model.focus().into_iter().chain([model.player()]);
    for at in cells {
        let center = map.min + egui::vec2(at.x as f32 + 0.5, at.y as f32 + 0.5) * CELL;
        assert!(!window.contains(center), "{window:?} covers {at:?}");
    }
}

#[test]
fn a_wide_dialog_is_as_wide_as_its_content_and_stays_inside_the_map_room() {
    let size = egui::vec2(1280.0, 800.0);
    let (h, _) = playing(size, Some(example("character", "character sheet")));
    let window = window_of(&h, "character_sheet");
    let map = h.get_by_label("Map").rect();
    assert!(window.width() > 640.0, "{window:?}");
    assert!(screen_rect(size).contains_rect(window), "{window:?}");
    assert!(
        (window.center().x - map.center().x).abs() < 40.0,
        "{window:?}"
    );
}
