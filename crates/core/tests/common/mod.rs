#![allow(dead_code)]
use bn_client_core::transport::{ScriptHandle, ScriptedTransport};
use bn_client_core::{EngineModel, GameModel};
use serde_json::Value;

pub fn fixture(name: &str) -> Value {
    let text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/design-0.1.json"
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

/// One step of the avatar by the engine's move action for the direction `(dx, dy)`.
pub fn step(model: &mut EngineModel, dx: i32, dy: i32) -> bool {
    let id = match (dx, dy) {
        (0, -1) => "UP",
        (1, -1) => "RIGHTUP",
        (1, 0) => "RIGHT",
        (1, 1) => "RIGHTDOWN",
        (0, 1) => "DOWN",
        (-1, 1) => "LEFTDOWN",
        (-1, 0) => "LEFT",
        _ => "LEFTUP",
    };
    model.invoke_action(id)
}
