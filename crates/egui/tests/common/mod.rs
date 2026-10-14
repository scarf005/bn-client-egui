#![allow(dead_code)]
use bn_client_egui::transport::{ScriptHandle, ScriptedTransport};
use bn_client_egui::{EngineModel, GameModel};
use serde_json::{Value, json};

pub fn fixture(name: &str) -> Value {
    let text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../core/tests/fixtures/design.json"
    ))
    .unwrap();
    serde_json::from_str::<Value>(&text).unwrap()[name].clone()
}

pub fn subscribe_reply() -> Vec<Value> {
    ["subscribe_result", "snapshot_part_0", "snapshot_part_1"]
        .map(fixture)
        .to_vec()
}

/// Answers hello and subscribe with the fixture session; extra replies can be chained. A second subscribe
/// is answered too: the client subscribes again after telling the engine it draws tiles.
pub fn session() -> (ScriptedTransport, ScriptHandle) {
    let (t, h) = ScriptedTransport::new();
    (
        t.on("bn.hello", vec![fixture("hello_result")])
            .on("bn.subscribe", subscribe_reply())
            .on("bn.subscribe", subscribe_reply()),
        h,
    )
}

/// A model that finished hello, subscribe and the snapshot.
pub fn connected(script: ScriptedTransport) -> EngineModel {
    let mut m = EngineModel::new(Box::new(script));
    m.poll();
    assert!(m.live());
    m
}

/// A game root holding one 4x4 tileset `test` whose sheets are solid colours: `a.png` has sprites 0 to 7
/// (red, green, blue, ... one per cell) and `b.png` sprites 8 and 9. The root is new for every call.
pub fn game_root(tag: &str) -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!("bn-client-tiles-{}-{tag}", std::process::id()));
    let dir = root.join("gfx/test");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("tileset.txt"),
        "# A test tileset\nNAME: test\nVIEW: Test Set\nJSON: tile_config.json\nTILESET: a.png\n",
    )
    .unwrap();
    let sheet = |name: &str, cells: u32| {
        let mut image = image::RgbaImage::new(4 * cells.min(4), 4 * cells.div_ceil(4));
        for (x, y, px) in image.enumerate_pixels_mut() {
            let n = (y / 4) * 4 + x / 4;
            *px = image::Rgba([40 + 25 * n as u8, 255 - 25 * n as u8, 128, 255]);
        }
        image.save(dir.join(name)).unwrap();
    };
    sheet("a.png", 8);
    sheet("b.png", 2);
    let config = serde_json::json!({
        "tile_info": [{"width": 4, "height": 4}],
        "tiles-new": [
            {"file": "a.png", "tiles": [
                {"id": ["t_floor", "t_dirt"], "fg": 0},
                {"id": "t_wall", "fg": 1, "multitile": true, "additional_tiles": [
                    {"id": "edge", "fg": [2, 3]},
                    {"id": "corner", "fg": 4}
                ]},
                {"id": "t_wall_season_winter", "fg": 5},
                {"id": "lighting_hidden", "fg": 4},
                {"id": "lighting_lowlight_dark", "fg": 2},
                {"id": "lighting_lowlight_light", "fg": 3},
                {"id": "grass", "fg": [{"weight": 1, "sprite": 5}, {"weight": 3, "sprite": 6}]},
                {"id": ["mon_a", "mon_b", "mon_zombie"], "fg": 7}
            ]},
            {"file": "b.png", "sprite_offset_x": 1, "sprite_offset_y": -2, "tiles": [
                {"id": "two_sheets", "fg": 8, "bg": 9}
            ]},
            {"file": "missing_fallback.png", "tiles": [], "ascii": []}
        ]
    });
    std::fs::write(dir.join("tile_config.json"), config.to_string()).unwrap();
    root
}

/// The overmap screen the engine publishes: 4x3 squares centred on (cursor_x, 200, 0) with the player two
/// squares west of the cursor, over the map in the stack.
pub fn overmap_event(cursor_x: i32) -> Value {
    let cell = |g: &str, fg: u8| json!([g, fg, 0]);
    let row = |g: &str| json!([cell(g, 7), cell(g, 2), cell("@", 11), cell(g, 1)]);
    let at = |x: i32, y: i32| json!({"dim": "", "x": x, "y": y, "z": 0});
    let map = json!({
        "id": "map:overmap", "kind": "map", "mode": "overmap",
        "affordances": ["overmap:cursor", "CHOOSE_DESTINATION"],
        "target": {"source": at(98, 200), "current": at(cursor_x, 200),
            "minimum": {"dim": "", "x": -1000, "y": -1000, "z": -10},
            "maximum": {"dim": "", "x": 1000, "y": 1000, "z": 10},
            "range": 2000, "distance_metric": "square", "unit": "omt", "status": "", "candidates": []},
        "overmap": {
            "origin": at(cursor_x - 2, 199), "player": at(98, 200), "cols": 4, "rows": 3,
            "cells": [row("F"), row("f"), row("r")],
            "legend": ["LEVEL 0, 100, 200"], "cursor_description": "Overmap forest",
            "route_text": "Route of 2 squares.", "notes_title": "List Notes",
            "notes": [{"pos": at(cursor_x - 1, 200), "text": "cache"}],
            "path": [at(cursor_x + 1, 200), at(cursor_x + 2, 200)]}});
    let toggles = json!({"id": "overmap:toggles", "kind": "panel", "title": "", "stats": [
        {"key": "TOGGLE_MAP_NOTES", "label": "Toggle Map Notes", "value": "On", "tone": "good"}]});
    let screen = json!({
        "screen_id": "screen:om", "role": "screen", "title": "Overmap",
        "root": {"id": "overmap", "kind": "split", "relation": "peers", "children": [map, toggles]},
        "affordances": [
            {"id": "overmap:cursor", "label": "Select", "keys": [], "enabled": true,
                "expect": "boundary", "args": {"pos": {"unit": "overmap", "component_id": "map:overmap"}}},
            {"id": "CHOOSE_DESTINATION", "label": "Travel", "keys": ["_"], "enabled": true,
                "expect": "screen"},
            {"id": "QUIT", "label": "Quit", "keys": ["ESC"], "enabled": true, "role": "cancel",
                "expect": "screen"}]});
    json!({"jsonrpc": "2.0", "method": "bn.events", "params": {"epoch": "e7f3", "events": [{
        "sequence": "41", "revision": "32", "type": "interaction.changed", "changes": {"interaction": {
        "boundary_id": "b:60",
        "readiness": {"phase": "playing", "game_ready": true, "accepts_interaction_commands": true,
            "accepts_registered_actions": true},
        "actions": [{"id": "CHOOSE_DESTINATION", "name": "Travel", "keys": ["_"]},
            {"id": "QUIT", "name": "Quit", "keys": ["ESC"]}],
        "interaction": null,
        "stack": [{"screen_id": "screen:map:e7f3", "role": "map", "root": null, "affordances": []},
            screen]}}}]}})
}

/// The squares of the overmap screen in `event`.
pub fn squares_mut(event: &mut Value) -> &mut Value {
    &mut event["params"]["events"][0]["changes"]["interaction"]["stack"][1]["root"]["children"][0]["overmap"]
}

/// The overmap event for a client that said it draws tiles: every square is `t_floor` but the
/// top-right one, a `forest` that falls back to `t_dirt`.
pub fn with_tiles(mut event: Value) -> Value {
    let rows: Vec<Value> = (0..3)
        .map(|row| {
            let cells: Vec<Value> = (0..4)
                .map(|col| match (col, row) {
                    (3, 0) => json!(["forest", 1, ""]),
                    _ => json!(["t_floor", 0, ""]),
                })
                .collect();
            json!(cells)
        })
        .collect();
    squares_mut(&mut event)["tiles"] =
        json!({"cells": rows, "looks_like": {"forest": ["t_dirt"], "t_floor": []}});
    event
}
