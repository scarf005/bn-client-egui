#![allow(dead_code)]
use bn_client_core::transport::{ScriptHandle, ScriptedTransport};
use bn_client_core::{EngineModel, GameModel};
use serde_json::Value;

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
