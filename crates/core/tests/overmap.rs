#![recursion_limit = "512"]
mod common;
use bn_client_core::protocol::{Component, InvokeArgs, MapComponent, OvermapWire, WorldPos};
use bn_client_core::{EngineModel, GameModel, MenuModel};
use common::{connected, session};
use serde_json::{Value, json};

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

fn open() -> (EngineModel, bn_client_core::transport::ScriptHandle) {
    let (t, h) = session();
    let mut m = connected(t.on("bn.command.submit", vec![common::fixture("submit_result")]));
    h.push(overmap_event(100));
    m.poll();
    (m, h)
}

/// The overmap map of the screen on top.
fn map_of(m: &EngineModel) -> MapComponent {
    let root = m
        .screens()
        .last()
        .and_then(|s| s.root.as_ref())
        .expect("a screen with components");
    match root.find("map:overmap") {
        Some(Component::Map(map)) => map.clone(),
        other => panic!("no overmap map: {other:?}"),
    }
}

fn squares_of(m: &EngineModel) -> OvermapWire {
    map_of(m).overmap.expect("the squares")
}

fn at(x: i32, y: i32, z: i32) -> WorldPos {
    WorldPos {
        dim: String::new(),
        x,
        y,
        z,
    }
}

#[test]
fn the_overmap_screen_is_a_map_component_with_squares_cursor_and_notes() {
    let (m, _h) = open();
    let screen = m.screens().last().unwrap();
    assert_eq!(
        screen.title.as_ref().map(|t| t.shown()).as_deref(),
        Some("Overmap")
    );
    let map = map_of(&m);
    assert_eq!(map.target.as_ref().unwrap()["current"]["x"], 100);
    let om = squares_of(&m);
    assert_eq!((om.cols, om.rows), (4, 3));
    assert_eq!(om.cells[0][2], ("@".to_string(), 11, 0));
    assert_eq!(
        om.square(2, 1),
        at(100, 200, 0),
        "cell (2, 1) is the cursor"
    );
    assert_eq!(om.cell_of(&at(100, 200, 0)), Some((2, 1)));
    assert_eq!(om.cell_of(&at(100, 200, 1)), None, "another level");
    assert_eq!(om.cell_of(&at(104, 200, 0)), None, "beyond the columns");
    assert_eq!(om.legend, ["LEVEL 0, 100, 200"]);
    assert_eq!(om.cursor_description, "Overmap forest");
    assert_eq!(om.route_text.as_deref(), Some("Route of 2 squares."));
    assert_eq!(
        (om.cursor_here.as_deref(), om.cursor_note.as_deref()),
        (None, None)
    );
    assert_eq!(om.note(&at(99, 200, 0)), Some("cache"));
    assert_eq!(om.path, [at(101, 200, 0), at(102, 200, 0)]);
    assert!(m.menu().is_none(), "the overmap is not a list menu");
    assert!(m.focus().is_none(), "its cursor is not a map square");
}

#[test]
fn the_status_and_the_notes_are_the_engines_lines_in_its_order() {
    let (m, _h) = open();
    let om = squares_of(&m);
    assert_eq!(om.status(), ["Overmap forest", "Route of 2 squares."]);
    assert_eq!(om.notes_title, "List Notes");
    assert_eq!(om.note_lines(), ["(99, 200, 0) cache"]);
    let mut event = overmap_event(100);
    let squares = squares_mut(&mut event);
    squares["cursor_here"] = "You are here.".into();
    squares["cursor_note"] = "Note: cache".into();
    squares["notes"] = json!([]);
    let (t, h) = session();
    let mut m = connected(t);
    h.push(event);
    m.poll();
    let om = squares_of(&m);
    assert_eq!(
        om.status(),
        [
            "You are here.",
            "Overmap forest",
            "Note: cache",
            "Route of 2 squares."
        ]
    );
    assert!(om.note_lines().is_empty());
}

#[test]
fn labels_and_the_mission_target_are_the_engines() {
    let (m, _h) = open();
    let om = squares_of(&m);
    assert!(om.labels.is_empty() && om.mission_target.is_none());
    let mut event = overmap_event(100);
    let squares = squares_mut(&mut event);
    squares["labels"] = json!([{"pos": {"dim":"","x":101,"y":199,"z":0}, "text": "Mill",
        "foreground": 14, "background": 0}]);
    squares["mission_target"] = json!({"dim":"","x":500,"y":200,"z":0});
    let (t, h) = session();
    let mut m = connected(t);
    h.push(event);
    m.poll();
    let om = squares_of(&m);
    assert_eq!(om.labels.len(), 1);
    assert_eq!(
        (om.labels[0].text.as_str(), om.labels[0].foreground),
        ("Mill", 14)
    );
    assert_eq!(om.mission_target, Some(at(500, 200, 0)));
}

#[test]
fn moving_the_cursor_invokes_its_affordance_with_an_overmap_square() {
    let (mut m, h) = open();
    let args = InvokeArgs {
        pos: Some(at(103, 198, 0)),
        ..InvokeArgs::default()
    };
    assert!(m.invoke_affordance("overmap:cursor", args).is_ok());
    let sent = h
        .sent()
        .into_iter()
        .rfind(|s| s["method"] == "bn.command.submit")
        .expect("a command was submitted");
    assert_eq!(
        sent["params"]["operation"],
        json!({"kind":"invoke","affordance_id":"overmap:cursor",
            "args":{"pos":{"dim":"","x":103,"y":198,"z":0}}})
    );
    assert!(
        sent["params"]["expect"].get("boundary_id").is_some(),
        "the cursor is boundary-scoped"
    );
}

#[test]
fn leaving_the_overmap_invokes_the_registered_quit() {
    let (mut m, h) = open();
    assert!(m.invoke_affordance("QUIT", InvokeArgs::default()).is_ok());
    let sent = h
        .sent()
        .into_iter()
        .rfind(|s| s["method"] == "bn.command.submit")
        .unwrap();
    assert_eq!(
        sent["params"]["operation"],
        json!({"kind":"invoke","affordance_id":"QUIT","args":{}})
    );
}

#[test]
fn the_tile_ids_come_with_the_squares_only_for_a_client_that_draws_tiles() {
    let (m, _h) = open();
    let om = squares_of(&m);
    assert!(om.tile(0, 0).is_none(), "no tiles unless asked for");

    let (t, h) = session();
    let mut tiled = EngineModel::connect(Box::new(t), true);
    tiled.poll();
    h.push(with_tiles(overmap_event(100)));
    tiled.poll();
    let om = squares_of(&tiled);
    let forest = om.tile(3, 0).unwrap();
    assert_eq!((forest.id, forest.rotation), ("forest", 1));
    assert_eq!(forest.fallbacks, ["t_dirt"]);
    assert_eq!(om.tile(0, 0).unwrap().id, "t_floor");
    assert!(om.tile(4, 0).is_none());
}

#[test]
fn hello_says_tiles_only_for_a_client_that_draws_them() {
    let hello_of = |tiles: bool| {
        let (t, h) = session();
        let _ = EngineModel::connect(Box::new(t), tiles);
        h.sent()[0]["params"].clone()
    };
    assert_eq!(hello_of(true)["tiles"], true);
    assert!(hello_of(false).get("tiles").is_none());
}

mod switching {
    use super::*;
    use bn_client_core::Clock;
    use std::time::Duration;

    fn hellos(h: &bn_client_core::transport::ScriptHandle) -> Vec<Value> {
        h.sent()
            .into_iter()
            .filter(|m| m["method"] == "bn.hello")
            .map(|m| m["params"].clone())
            .collect()
    }

    #[test]
    fn switching_tiles_on_sends_a_hello_a_same_size_viewport_and_a_new_subscription_once_idle() {
        let (t, h) = session();
        let mut m = connected(t);
        m.set_tiles(false);
        m.poll();
        assert_eq!(hellos(&h).len(), 1, "no change, nothing sent");
        m.set_tiles(true);
        m.poll();
        let sent = hellos(&h);
        assert_eq!(sent.len(), 2);
        assert_eq!(sent[1]["tiles"], true);
        assert!(
            sent[1].get("viewport").is_none(),
            "only the preference changes"
        );
        // A repeated hello ends the engine's stream, so the snapshot after the viewport restarts it.
        let methods = h.sent_methods();
        assert_eq!(
            &methods[methods.len() - 3..],
            ["bn.hello", "bn.viewport", "bn.subscribe"]
        );
        m.poll();
        assert_eq!(hellos(&h).len(), 2, "told once");
        m.set_tiles(false);
        m.poll();
        assert!(
            hellos(&h)[2].get("tiles").is_none(),
            "switching back says so"
        );
    }

    #[test]
    fn the_ids_of_the_tileset_tints_are_sent_in_a_hello_once_they_change() {
        let (t, h) = session();
        let mut m = connected(t);
        let ids = ["hair_black".to_string(), "jeans".to_string()];
        m.set_tint_ids(&[]);
        m.poll();
        assert_eq!(hellos(&h).len(), 1, "no tints, nothing sent");
        m.set_tint_ids(&ids);
        m.poll();
        let sent = hellos(&h);
        assert_eq!(sent.len(), 2);
        assert_eq!(sent[1]["tint_ids"], json!(["hair_black", "jeans"]));
        m.set_tint_ids(&ids);
        m.poll();
        assert_eq!(hellos(&h).len(), 2, "told once");
        m.set_tint_ids(&[]);
        m.poll();
        assert!(hellos(&h)[2].get("tint_ids").is_none());
    }

    #[test]
    fn the_preference_waits_while_a_command_runs() {
        let (t, h) = session();
        let mut m = connected(t.on("bn.command.submit", vec![common::fixture("submit_result")]));
        assert!(m.invoke_action("RIGHT"));
        m.set_tiles(true);
        m.poll();
        assert_eq!(hellos(&h).len(), 1);
        h.push(json!({"jsonrpc":"2.0","method":"bn.command","params":{
            "epoch":"e7f3","command_id":"c:5","stage":"completed"}}));
        m.poll();
        assert_eq!(hellos(&h).len(), 2);
    }

    #[test]
    fn a_refused_hello_is_tried_again_after_a_moment() {
        let (t, h) = session();
        let clock = Clock::manual();
        let refuse = json!({"jsonrpc":"2.0","id":null,"error":{"code":1000,"message":"x","data":{"kind":"command_busy"}}});
        let mut m =
            EngineModel::new(Box::new(t.on("bn.hello", vec![refuse]))).with_clock(clock.clone());
        m.poll();
        m.set_tiles(true);
        m.poll();
        m.poll();
        assert_eq!(hellos(&h).len(), 2, "refused");
        m.poll();
        assert_eq!(hellos(&h).len(), 2, "not at once");
        clock.advance(Duration::from_secs(2));
        m.poll();
        assert_eq!(hellos(&h).len(), 3);
    }
}

#[test]
fn a_notes_colour_tags_are_not_part_of_its_words() {
    let mut event = overmap_event(100);
    let squares = squares_mut(&mut event);
    squares["notes"] = json!([{"pos": {"dim": "", "x": 99, "y": 200, "z": 0},
        "text": "<color_c_light_blue>Pond</color>"}]);
    squares["cursor_note"] = "Note: <color_c_light_blue>Pond</color>".into();
    let (t, h) = session();
    let mut m = connected(t);
    h.push(event);
    m.poll();
    let om = squares_of(&m);
    assert_eq!(om.note(&at(99, 200, 0)).as_deref(), Some("Pond"));
    assert_eq!(om.note_lines(), ["(99, 200, 0) Pond"]);
    assert!(
        om.status().iter().any(|l| *l == "Note: Pond"),
        "{:?}",
        om.status()
    );
}
