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
                {"id": "fd_fire", "animated": true,
                    "fg": [{"weight": 2, "sprite": 1}, {"weight": 3, "sprite": 2}]},
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

/// A game root holding one isometric tileset `iso` (sprites 0 and 1 are the ground, 2 the wall, 3 the person and
/// 4 the table; `t_mound` is a tall sprite that is not joined to its neighbours), drawn like Ultica_iso's sheets: 48 x 24 tiles, ground in
/// 48 x 36 sprites, walls in 48 x 120 sprites, creatures and furniture in 48 x 72 sprites, trees in 200 x 240
/// sprites. `flat.png` has a light and a dark ground (the light with a red mark at its north corner), `tall.png` a
/// wall with a white top, red left face and blue right face, `upright.png` a yellow creature and a brown table, and
/// `giant.png` a tree (5) and its ground (6), a dark diamond in the tile's columns of the bottom rows. The root is
/// new for every call.
pub fn iso_root(tag: &str) -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!("bn-client-iso-{}-{tag}", std::process::id()));
    let dir = root.join("gfx/iso");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("tileset.txt"),
        "NAME: iso\nVIEW: Iso Set\nJSON: tile_config.json\nTILESET: flat.png\n",
    )
    .unwrap();
    // Whether (x, y) of a sprite `height` tall lies on the diamond in its lowest 24 rows.
    let on_diamond = |x: u32, y: u32, height: u32| {
        let (dx, dy) = (
            (x as f32 + 0.5 - 24.0).abs() / 24.0,
            (y as f32 + 0.5 - (height as f32 - 12.0)).abs() / 12.0,
        );
        dx + dy <= 1.0
    };
    let mut flat = image::RgbaImage::new(96, 36);
    for (x, y, px) in flat.enumerate_pixels_mut() {
        let (sprite, x) = (x / 48, x % 48);
        let mark = x.abs_diff(24) < 3 && (14..18).contains(&y);
        if on_diamond(x, y, 36) {
            *px = match (sprite, mark) {
                (0, true) => image::Rgba([220, 40, 40, 255]),
                (0, false) => image::Rgba([120, 200, 90, 255]),
                _ => image::Rgba([90, 70, 50, 255]),
            };
        }
    }
    flat.save(dir.join("flat.png")).unwrap();
    // The wall: a white top diamond over a red left face and a blue right face, banded every 16 rows.
    let mut tall = image::RgbaImage::new(48, 120);
    for (x, y, px) in tall.enumerate_pixels_mut() {
        let left = x < 24;
        let slope = if left { x / 2 } else { (47 - x) / 2 };
        let face_top = 12 + slope;
        let band = ((y.saturating_sub(face_top)) / 16) % 2 == 0;
        if on_diamond(x, y, 24) {
            *px = image::Rgba([240, 240, 240, 255]);
        } else if y >= face_top && y < face_top + 96 {
            *px = match (left, band) {
                (true, true) => image::Rgba([200, 50, 50, 255]),
                (true, false) => image::Rgba([150, 30, 30, 255]),
                (false, true) => image::Rgba([60, 90, 210, 255]),
                (false, false) => image::Rgba([30, 50, 150, 255]),
            };
        }
    }
    tall.save(dir.join("tall.png")).unwrap();
    // Upright things: a yellow person and a brown table standing on the tile's diamond.
    let mut upright = image::RgbaImage::new(96, 72);
    for (x, y, px) in upright.enumerate_pixels_mut() {
        let (sprite, x) = (x / 48, x % 48);
        let person = (16..32).contains(&x) && (14..62).contains(&y);
        let table = (6..42).contains(&x) && (36..60).contains(&y);
        if (sprite == 0 && person) || (sprite == 1 && table) {
            *px = if sprite == 0 {
                image::Rgba([240, 220, 40, 255])
            } else {
                image::Rgba([140, 90, 40, 255])
            };
        }
    }
    upright.save(dir.join("upright.png")).unwrap();
    let mut giant = image::RgbaImage::new(400, 240);
    for (x, y, px) in giant.enumerate_pixels_mut() {
        let (sprite, x) = (x / 200, x % 200);
        let tree = ((96..104).contains(&x) && (150..228).contains(&y))
            || ((40..160).contains(&x) && (30..150).contains(&y));
        let ground = (76..124).contains(&x) && on_diamond(x - 76, y, 240);
        if (sprite == 0 && tree) || (sprite == 1 && ground) {
            *px = if sprite == 0 {
                image::Rgba([40, 120, 40, 255])
            } else {
                image::Rgba([60, 90, 40, 255])
            };
        }
    }
    giant.save(dir.join("giant.png")).unwrap();
    let config = serde_json::json!({
        "tile_info": [{"width": 48, "height": 24, "iso": true, "zlevel_height": 108, "pixelscale": 1}],
        "tiles-new": [
            {"file": "flat.png", "sprite_width": 48, "sprite_height": 36, "sprite_offset_y": -12, "tiles": [
                {"id": ["t_floor", "t_dirt"], "fg": 0},
                {"id": "t_grass", "fg": 1}
            ]},
            {"file": "tall.png", "sprite_width": 48, "sprite_height": 120, "sprite_offset_y": -96, "tiles": [
                {"id": "t_wall", "fg": 2, "multitile": true, "additional_tiles": [
                    {"id": "center", "fg": 2}
                ]},
                {"id": "t_mound", "fg": 2}
            ]},
            {"file": "upright.png", "sprite_width": 48, "sprite_height": 72, "sprite_offset_y": -48, "tiles": [
                {"id": ["mon_a", "mon_zombie"], "fg": 3},
                {"id": "f_table", "fg": 4}
            ]},
            {"file": "giant.png", "sprite_width": 200, "sprite_height": 240, "sprite_offset_x": -76,
                "sprite_offset_y": -216, "tiles": [{"id": "t_tree", "fg": 5, "bg": 6}]}
        ]
    });
    std::fs::write(dir.join("tile_config.json"), config.to_string()).unwrap();
    root
}

/// A game root holding a 32 x 32 tileset `flat` drawn from above, in `gfx/flat`: grass, a shrub whose top rows
/// are clear, a wall and a vehicle frame on grass as wide and tall as the tile, a flower inside it that a person (`mon_a`)
/// is drawn as too, a wall with a hole of 4 x 4 pixels in its middle (`t_wall_holed`), a wall over the top half of
/// the square only (`t_wall_half`), and, twice as tall, a tree whose ground is a sprite of its own with grass in its
/// bottom half.
pub fn flat_root(tag: &str) -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!("bn-client-flat-{}-{tag}", std::process::id()));
    let dir = root.join("gfx/flat");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("tileset.txt"),
        "NAME: flat\nVIEW: Flat Set\nJSON: tile_config.json\nTILESET: top.png\n",
    )
    .unwrap();
    let grass = image::Rgba([90, 170, 60, 255]);
    let mut top = image::RgbaImage::new(224, 32);
    for (x, y, px) in top.enumerate_pixels_mut() {
        let (sprite, x) = (x / 32, x % 32);
        let shrub = (6..26).contains(&x) && (8..28).contains(&y);
        let flower = (12..20).contains(&x) && (12..20).contains(&y);
        *px = match sprite {
            0 => grass,
            1 if shrub => image::Rgba([30, 90, 30, 255]),
            2 => image::Rgba([150, 150, 150, 255]),
            3 => image::Rgba([60, 90, 210, 255]),
            4 if flower => image::Rgba([200, 60, 220, 255]),
            5 if !(14..18).contains(&x) || !(14..18).contains(&y) => {
                image::Rgba([120, 120, 120, 255])
            }
            6 if y < 16 => image::Rgba([170, 170, 170, 255]),
            _ => image::Rgba([0, 0, 0, 0]),
        };
    }
    top.save(dir.join("top.png")).unwrap();
    let mut tall = image::RgbaImage::new(64, 64);
    for (x, y, px) in tall.enumerate_pixels_mut() {
        let (sprite, x) = (x / 32, x % 32);
        match sprite {
            0 if (12..20).contains(&x) && y >= 8 => *px = image::Rgba([110, 70, 30, 255]),
            1 if y >= 32 => *px = grass,
            _ => {}
        }
    }
    tall.save(dir.join("tall.png")).unwrap();
    let config = serde_json::json!({
        "tile_info": [{"width": 32, "height": 32, "pixelscale": 1}],
        "tiles-new": [
            {"file": "top.png", "tiles": [
                {"id": "t_grass", "fg": 0},
                {"id": "t_dirt", "fg": 0, "multitile": true, "additional_tiles": [{"id": "center", "fg": 0}]},
                {"id": "t_shrub", "fg": 1, "bg": 0},
                {"id": "t_wall", "fg": 2, "multitile": true, "additional_tiles": [{"id": "center", "fg": 2}]},
                {"id": "vp_frame", "fg": 3, "bg": 0},
                {"id": ["f_flower", "mon_a"], "fg": 4},
                {"id": "t_wall_holed", "fg": 5},
                {"id": "t_wall_half", "fg": 6}
            ]},
            {"file": "tall.png", "sprite_width": 32, "sprite_height": 64, "sprite_offset_y": -32, "tiles": [
                {"id": "t_tree", "fg": 7, "bg": 8}
            ]}
        ]
    });
    std::fs::write(dir.join("tile_config.json"), config.to_string()).unwrap();
    root
}

/// Opens the client settings window, where the view mode, tileset and motion choices are, unless it is open.
pub fn open_client_settings<S>(h: &mut egui_kittest::Harness<'_, S>) {
    use egui_kittest::kittest::Queryable;
    if h.query_by_label("Reduced motion").is_none() {
        h.get_by_label("Client settings").click();
        h.run_steps(3);
    }
}

/// A double click at `at` for the next frame: both clicks in that one frame, where kittest's `click` takes a
/// frame for each event and two of them are further apart than egui's double-click delay.
pub fn double_click<S>(h: &mut egui_kittest::Harness<'_, S>, at: eframe::egui::Pos2) {
    use eframe::egui::{Event, Modifiers, PointerButton};
    let events = &mut h.input_mut().events;
    events.push(Event::PointerMoved(at));
    for pressed in [true, false, true, false] {
        events.push(Event::PointerButton {
            pos: at,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        });
    }
}

/// Runs frames until no window moves: a window smaller than the room grows to its size over a few frames, and a
/// click aimed before that lands on a neighbour.
pub fn settle_windows<S>(h: &mut egui_kittest::Harness<'_, S>) {
    use egui_kittest::kittest::Queryable;
    let windows = |h: &egui_kittest::Harness<'_, S>| {
        h.get_all_by_role(eframe::egui::accesskit::Role::Window)
            .map(|n| n.rect())
            .collect::<Vec<_>>()
    };
    for _ in 0..60 {
        let before = windows(h);
        h.run_steps(1);
        if windows(h) == before {
            return;
        }
    }
}

/// The app on a GPU harness of `size` in the client's own theme and bundled fonts, as the client starts.
pub fn shot_harness<'a>(
    size: eframe::egui::Vec2,
    app: bn_client_egui::App<EngineModel>,
) -> egui_kittest::Harness<'a, bn_client_egui::App<EngineModel>> {
    let h = egui_kittest::Harness::builder()
        .with_size(size)
        .wgpu()
        .build_ui_state(
            |ui, app: &mut bn_client_egui::App<EngineModel>| app.show(ui),
            app,
        );
    bn_client_egui::theme::apply(&h.ctx);
    bn_client_egui::fonts::install_bundled(&h.ctx);
    h
}
