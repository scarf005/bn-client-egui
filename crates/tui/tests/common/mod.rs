#![allow(dead_code)]
use bn_client_core::transport::Profile;
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

/// A 4x3 overmap centred on (cursor_x, 200, 0) with the player two squares west of the cursor.
pub fn overmap_event(cursor_x: i32) -> Value {
    let cell = |g: &str, fg: u8| format!(r#"["{g}",{fg},0]"#);
    let row = |g: &str| {
        format!(
            "[{},{},{},{}]",
            cell(g, 7),
            cell(g, 2),
            cell("@", 11),
            cell(g, 1)
        )
    };
    let text = format!(
        r#"{{"jsonrpc":"2.0","method":"bn.events","params":{{"epoch":"e7f3","events":[{{
        "sequence":"41","revision":"32","type":"interaction.changed","changes":{{"interaction":{{
        "boundary_id":"b:60",
        "readiness":{{"phase":"playing","game_ready":true,"accepts_interaction_commands":true,"accepts_registered_actions":true}},
        "actions":[{{"id":"CHOOSE_DESTINATION","name":"Travel","keys":["_"]}},{{"id":"QUIT","name":"Quit","keys":["ESC"]}}],
        "interaction":{{
            "schema_id":"s:om{cursor_x}","kind":"target","context":"OVERMAP","title":"Overmap",
            "message":"","allow_cancel":true,"allow_set_count":false,"choices":[],"choice_total":0,
            "target":{{"source":{{"dim":"","x":98,"y":200,"z":0}},"current":{{"dim":"","x":{cursor_x},"y":200,"z":0}},
                "range":100,"distance_metric":"square","unit":"omt","status":"","candidates":[]}},
            "overmap":{{
                "origin":{{"dim":"","x":{origin_x},"y":199,"z":0}},
                "player":{{"dim":"","x":98,"y":200,"z":0}},
                "cols":4,"rows":3,
                "cells":[{rows}],
                "legend":["Overmap","  forest"],
                "notes":[{{"pos":{{"dim":"","x":{note_x},"y":200,"z":0}},"text":"cache"}}],
                "path":[{{"dim":"","x":{p1},"y":200,"z":0}},{{"dim":"","x":{p2},"y":200,"z":0}}]
            }},
            "compat":{{"panes":[]}}
        }}}}}}}}]}}}}"#,
        origin_x = cursor_x - 2,
        rows = [row("F"), row("f"), row("r")].join(","),
        note_x = cursor_x - 1,
        p1 = cursor_x + 1,
        p2 = cursor_x + 2,
    );
    serde_json::from_str(&text).unwrap()
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
    event["params"]["events"][0]["changes"]["interaction"]["interaction"]["overmap"]["tiles"] =
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
