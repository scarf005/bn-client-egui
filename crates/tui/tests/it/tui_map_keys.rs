//! Keys held at the map go to the engine as screen-scoped affordances, which it queues behind the running one.
use crate::common::fixture;
use bn_client_core::transport::{ScriptHandle, ScriptedTransport};
use bn_client_core::{EngineModel, GameModel};
use bn_client_tui::TuiApp;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use serde_json::{Value, json};

/// A session at the map, whose engine holds eight commands behind the running one.
fn walking() -> (TuiApp<EngineModel>, ScriptHandle) {
    let mut hello = fixture("hello_result");
    hello["result"]["limits"]["screen_queue"] = json!(8);
    let mut subscribe = fixture("subscribe_result");
    subscribe["result"]["interaction"]["stack"] = json!([{
        "screen_id": "screen:map:e1", "role": "map",
        "root": {"id": "map:world", "kind": "map", "affordances": [], "mode": "world"},
        "affordances": [{"id": "RIGHT", "label": "Move East", "keys": ["RIGHT", "l"],
            "enabled": true, "expect": "screen"}]}]);
    let (t, script) = ScriptedTransport::new();
    let t = (0..20).fold(
        t.on("bn.hello", vec![hello]).on(
            "bn.subscribe",
            vec![
                subscribe,
                fixture("snapshot_part_0"),
                fixture("snapshot_part_1"),
            ],
        ),
        |t, n| {
            let mut received = fixture("submit_result");
            received["result"]["command_id"] = json!(format!("c:{n}"));
            t.on("bn.command.submit", vec![received])
        },
    );
    let mut model = EngineModel::new(Box::new(t));
    model.poll();
    (TuiApp::new(model), script)
}

fn submits(script: &ScriptHandle) -> Vec<Value> {
    script
        .sent()
        .into_iter()
        .filter(|m| m["method"] == "bn.command.submit")
        .map(|m| m["params"].clone())
        .collect()
}

#[test]
fn a_held_arrow_sends_every_repeat_to_the_engine_up_to_the_depth_of_its_queue() {
    let (mut app, script) = walking();
    for _ in 0..12 {
        app.handle_key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE));
    }
    let sent = submits(&script);
    assert_eq!(sent.len(), 9, "the running step and eight behind it");
    for command in &sent {
        assert_eq!(command["expect"], json!({"screen_id": "screen:map:e1"}));
        assert_eq!(command["operation"]["affordance_id"], "RIGHT");
    }
}
