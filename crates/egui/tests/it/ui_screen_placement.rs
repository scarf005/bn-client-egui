//! Where windows sit: a full screen fills the map's room and leaves the sidebar, a dialog is centred and kept on
//! screen, a screen that puts a cursor on the map keeps off the cursor and the avatar.
use crate::common::{fixture, session};
use bn_client_egui::app::CELL;
use bn_client_egui::transport::ScriptHandle;
use bn_client_egui::{App, EngineModel, GameModel};
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
    playing_with(size, screen, None)
}

/// `playing` with the client's settings kept in `settings`.
fn playing_with<'a>(
    size: egui::Vec2,
    screen: Option<Value>,
    settings: Option<&std::path::Path>,
) -> (H<'a>, ScriptHandle) {
    let (t, script) = session();
    let app = App::new(EngineModel::new(Box::new(t)));
    let app = match settings {
        Some(path) => app.with_settings(path.into()),
        None => app,
    };
    let mut h = Harness::builder()
        .with_size(size)
        .build_ui_state(|ui, app: &mut App<EngineModel>| app.show(ui), app);
    h.run_steps(3);
    script.push(fixture("events_41_walk"));
    h.run_steps(4);
    if let Some(screen) = screen {
        let map = fixture("subscribe_result")["result"]["interaction"]["stack"][0].clone();
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
    let (h, _) = playing(size, Some(example("overmap", "overmap screen")));
    let window = window_of(&h, "overmap");
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

/// The look screen with the engine's cursor `dx` squares east of the avatar and `parts` of what is on its square,
/// published as the next change of the interaction.
fn look_moved(h: &H, script: &ScriptHandle, step: u64, dx: i64, parts: usize) {
    let (origin, me) = (h.state().model.origin(), h.state().model.player());
    let mut screen = example("look", "look-around screen");
    let target = &mut screen["root"]["child"]["target"];
    for (square, at) in [("source", 0), ("current", dx)] {
        target[square]["x"] = json!(i64::from(origin.x + me.x) + at);
        target[square]["y"] = json!(origin.y + me.y);
    }
    let all = target["parts"].as_array().unwrap().clone();
    target["parts"] = json!(all.into_iter().take(parts).collect::<Vec<_>>());
    let map = fixture("subscribe_result")["result"]["interaction"]["stack"][0].clone();
    script.push(
        json!({"jsonrpc": "2.0", "method": "bn.events", "params": {"epoch": "e7f3", "events": [{
        "sequence": (43 + step).to_string(), "revision": (34 + step).to_string(),
        "type": "interaction.changed", "changes": {"interaction": {
        "boundary_id": "b:60",
        "readiness": {"phase": "playing", "game_ready": true, "accepts_interaction_commands": true,
            "accepts_registered_actions": true},
        "actions": [], "interaction": null, "stack": [map, screen]}}}]}}),
    );
}

#[test]
fn the_window_of_a_cursor_screen_keeps_its_place_and_size_as_the_cursor_and_its_content_change() {
    let size = egui::vec2(1280.0, 800.0);
    let (mut h, script) = playing(size, Some(example("look", "look-around screen")));
    crate::common::settle_windows(&mut h);
    let first = window_of(&h, "look");
    let steps = [(8, 10), (-8, 1), (8, 0), (-8, 10), (2, 3)];
    for (step, (dx, parts)) in steps.into_iter().enumerate() {
        look_moved(&h, &script, step as u64, dx, parts);
        for frame in 0..8 {
            h.run_steps(1);
            assert_eq!(
                window_of(&h, "look"),
                first,
                "cursor {dx} east with {parts} lines, frame {frame}"
            );
        }
    }
}

#[test]
fn a_wide_dialog_takes_its_whole_column_and_stays_inside_the_map_room() {
    let size = egui::vec2(1280.0, 800.0);
    let (h, _) = playing(size, Some(example("character", "character sheet")));
    let window = window_of(&h, "character_sheet");
    let map = h.get_by_label("Map").rect();
    // A dialog's column is 60% of the map room inside its margins, at most 720.
    assert!(
        window.width() >= 0.6 * (map.width() - 16.0) - 8.0,
        "{window:?}"
    );
    assert!(screen_rect(size).contains_rect(window), "{window:?}");
    assert!(
        (window.center().x - map.center().x).abs() < 40.0,
        "{window:?}"
    );
}

#[test]
fn a_window_restored_where_a_last_run_left_it_lies_wholly_inside_the_map_room() {
    let size = egui::vec2(1920.0, 1080.0);
    let file = crate::common::game_root("placement-restored").join("settings.json");
    let windows = json!({"windows": {"screen:uilist": {"pos": [460.0, 0.0], "size": [361.0, 1019.0],
        "collapsed": false}}});
    std::fs::write(&file, windows.to_string()).unwrap();
    let screen = example("generic", "filterable list screen");
    let (h, _) = playing_with(size, Some(screen), Some(&file));
    let window = window_of(&h, "uilist");
    let map = h.get_by_label("Map").rect();
    assert!(map.contains_rect(window), "{window:?} outside {map:?}");
    let toolbar = h.get_by_label("Client settings").rect();
    assert!(
        window.top() >= toolbar.bottom(),
        "{window:?} over {toolbar:?}"
    );
}

#[test]
fn a_short_dialog_is_as_tall_as_its_content() {
    let size = egui::vec2(1920.0, 1080.0);
    let (mut h, _) = playing(size, Some(example("generic", "query popup screen")));
    crate::common::settle_windows(&mut h);
    let window = window_of(&h, "popup");
    // The footer's "No", under the list's row of the same name.
    let last = h
        .get_all_by_label("No")
        .map(|n| n.rect())
        .max_by(|a, b| a.bottom().total_cmp(&b.bottom()))
        .unwrap();
    assert!(
        window.bottom() - last.bottom() < 40.0,
        "{window:?} runs past its last row at {last:?}"
    );
}

/// The crafting screen the engine published for a new character's weapon recipes: the recipe list beside the
/// detail of the one under the cursor, whose description is empty.
fn crafting_weapons() -> Value {
    let path = format!(
        "{}/../core/tests/fixtures/crafting-weapons.json",
        env!("CARGO_MANIFEST_DIR")
    );
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

/// The panes of a split fill the window's room, so a pane that drew wider than its share widened the window,
/// and with it the next frame's room, every frame.
#[test]
fn a_window_left_narrower_than_its_room_keeps_its_width_frame_after_frame() {
    for (screen, kept) in [(3000.0, 2000.0), (1920.0, 1200.0)] {
        let dir =
            std::env::temp_dir().join(format!("bn-client-kept-{}-{kept}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let settings = dir.join("settings.json");
        let windows =
            json!({"screen:crafting": {"pos": [60, 60], "size": [kept, 600], "collapsed": false}});
        std::fs::write(
            &settings,
            json!({"tiles": true, "windows": windows}).to_string(),
        )
        .unwrap();
        let size = egui::vec2(screen, 1000.0);
        let (mut h, _) = playing_with(size, Some(crafting_weapons()), Some(&settings));
        // In the client's own look, whose scroll bars take room.
        bn_client_egui::kit::tokens::apply(&h.ctx);
        bn_client_egui::fonts::install_bundled(&h.ctx);
        let widths = (0..40)
            .map(|_| {
                h.run_steps(1);
                window_of(&h, "crafting").width()
            })
            .collect::<Vec<_>>();
        assert!(widths.iter().all(|w| (w - kept).abs() < 0.5), "{widths:?}");
    }
}
