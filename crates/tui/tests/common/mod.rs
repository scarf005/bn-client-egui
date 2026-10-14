#![allow(dead_code)]
use bn_client_core::protocol::{Component, InvokeArgs};
use bn_client_core::transport::Profile;
use bn_client_core::transport::{ScriptHandle, ScriptedTransport};
use bn_client_core::watchdog::Watchdog;
use bn_client_core::{EngineModel, GameModel};
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

/// Answers hello and subscribe with the fixture session; extra replies can be chained.
pub fn session() -> (ScriptedTransport, ScriptHandle) {
    let (t, h) = ScriptedTransport::new();
    (
        t.on("bn.hello", vec![fixture("hello_result")])
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
            "route_text": "Route of 2 squares.",
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

/// A private profile for live engine tests: English, no animation or autosave, and the CPU compute
/// path, which needs no GPU shaders and does not stall in a headless session. Put `TMPDIR` under
/// `$HOME` to keep the scratch off `/tmp`.
pub fn cpu_profile() -> Profile {
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let id = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("bn-tui-live-{}-{id}", std::process::id()));
    std::fs::create_dir_all(dir.join("config")).unwrap();
    std::fs::write(
        dir.join("config/options.json"),
        r#"[{"name":"USE_LANG","value":"en_US"},{"name":"ANIMATIONS","value":"false"},{"name":"AUTOSAVE","value":"false"},{"name":"COMPUTE_ACCELERATION","value":"cpu"}]"#,
    )
    .unwrap();
    std::fs::write(
        dir.join("config/preload.json"),
        r#"{"compute_acceleration":"cpu"}"#,
    )
    .unwrap();
    Profile {
        userdir: Some(dir.clone()),
        configdir: Some(dir.join("config")),
        ..Profile::default()
    }
}

/// Polls the model until `done`.
pub fn wait_for(model: &mut EngineModel, what: &str, mut done: impl FnMut(&EngineModel) -> bool) {
    let mut watchdog = Watchdog::live();
    while !done(model) {
        assert!(
            !watchdog.expired(model.received()),
            "timed out waiting for {what}: {:?}",
            model.messages()
        );
        model.poll();
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}

/// Runs `affordance` of the main menu on the row `row` (a tab or an entry, by id) once the menu offers it and
/// no command is in flight, then waits for the command to finish.
pub fn main_menu(model: &mut EngineModel, affordance: &str, row: &str) {
    wait_for(model, affordance, |m| {
        !m.busy()
            && m.screens()
                .last()
                .is_some_and(|s| s.affordances.iter().any(|a| a.id == affordance))
    });
    let args = InvokeArgs {
        row: Some(row.into()),
        ..Default::default()
    };
    model
        .invoke_affordance(affordance, args)
        .unwrap_or_else(|e| panic!("{affordance} on {row}: {e:?}"));
    wait_for(model, row, |m| !m.busy());
}

/// The ids of the main menu's tabs, empty while the menu is not on top.
pub fn main_menu_tabs(model: &EngineModel) -> Vec<String> {
    let tabs = model
        .screens()
        .last()
        .and_then(|s| s.root.as_ref()?.find("menu:tabs"));
    match tabs {
        Some(Component::Tabs(tabs)) => tabs.tabs.iter().map(|t| t.id.clone()).collect(),
        _ => vec![],
    }
}
