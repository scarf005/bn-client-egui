#![allow(dead_code)]
use bn_client_core::transport::{ScriptHandle, ScriptedTransport};
use bn_client_core::{EngineModel, GameModel};
use serde_json::{Value, json};

pub fn fixture(name: &str) -> Value {
    let text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../core/tests/fixtures/design-1.0.json"
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

/// A 4x3 overmap centred on (100, 200, 0) with the player two squares west of the cursor.
pub fn overmap_event(cursor_x: i32) -> Value {
    let cell = |g: &str, fg: u8| json!([g, fg, 0]);
    let row = |g: &str| json!([cell(g, 7), cell(g, 2), cell("@", 11), cell(g, 1)]);
    json!({"jsonrpc":"2.0","method":"bn.events","params":{"epoch":"e7f3","events":[{
    "sequence":"41","revision":"32","type":"interaction.changed","changes":{"interaction":{
    "boundary_id":"b:60",
    "readiness":{"phase":"playing","game_ready":true,"accepts_interaction_commands":true,"accepts_registered_actions":true},
    "actions":[{"id":"CHOOSE_DESTINATION","name":"Travel","keys":["_"]},{"id":"QUIT","name":"Quit","keys":["ESC"]}],
    "interaction":{
        "schema_id":format!("s:om{cursor_x}"),"kind":"target","context":"OVERMAP","title":"Overmap",
        "message":"","allow_cancel":true,"allow_set_count":false,"choices":[],"choice_total":0,
        "target":{"source":{"dim":"","x":98,"y":200,"z":0},"current":{"dim":"","x":cursor_x,"y":200,"z":0},
            "range":100,"distance_metric":"square","unit":"omt","status":"","candidates":[]},
        "overmap":{
            "origin":{"dim":"","x":cursor_x - 2,"y":199,"z":0},
            "player":{"dim":"","x":98,"y":200,"z":0},
            "cols":4,"rows":3,
            "cells":[row("F"), row("f"), row("r")],
            "legend":["Overmap","  forest"],
            "notes":[{"pos":{"dim":"","x":cursor_x - 1,"y":200,"z":0},"text":"cache"}]
        },
        "compat":{"panes":[]}
    }}}}]}})
}
