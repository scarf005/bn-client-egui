//! Writes PNGs of the sidebar laid out by the game's panel options, with the compass and a sound with its
//! direction, to `$SHOT_DIR` for a person to look at.
//! Needs a Vulkan adapter, so it only runs when asked: `cargo test --test it ui_shots_c:: -- --ignored`.
use crate::common::{fixture, session};
use bn_client_egui::{App, EngineModel};
use eframe::egui;
use serde_json::{Value, json};

fn example(name: &str) -> Value {
    let text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../core/tests/fixtures/0.2.examples/world.json"
    ))
    .unwrap();
    let all: Value = serde_json::from_str(&text).unwrap();
    let mut found = all["valid"].as_array().unwrap().iter();
    found.find(|e| e["name"] == name).unwrap()["value"].clone()
}

fn panel(id: &str, label: &str, enabled: bool) -> Value {
    json!({"id": id, "label": label, "enabled": enabled})
}

fn shot(name: &str, side: &str, width: u32, panels: &[Value]) {
    let (t, script) = session();
    let mut h = crate::common::shot_harness(
        egui::vec2(1280.0, 800.0),
        App::new(EngineModel::new(Box::new(t))),
    );
    h.run_steps(3);
    let mut event = fixture("events_41_walk");
    let mut avatar = example("avatar with creatures on the compass");
    avatar["sidebar"]["layout"] =
        json!({"id": "classic", "side": side, "width": width, "panels": panels});
    event["params"]["events"][0]["changes"]["avatar"] = avatar;
    script.push(event);
    for (n, data) in [
        json!({"text": "a loud explosion", "volume": 74, "logged": false, "distance": 12,
            "direction": {"id": "southeast", "name": "southeast"}}),
        json!({"text": "Tick.", "volume": 52, "logged": false, "distance": 3}),
    ]
    .into_iter()
    .enumerate()
    {
        script.push(
            json!({"jsonrpc": "2.0", "method": "bn.events", "params": {"epoch": "e7f3", "events": [{
            "sequence": (42 + n).to_string(), "revision": "32", "type": "sound.heard", "data": data}]}}),
        );
    }
    h.run_steps(6);
    let dir = std::path::PathBuf::from(std::env::var("SHOT_DIR").expect("SHOT_DIR"));
    std::fs::create_dir_all(&dir).unwrap();
    h.render().expect("renders").save(dir.join(name)).unwrap();
}

#[test]
#[ignore = "needs a Vulkan adapter and SHOT_DIR"]
fn shots() {
    shot(
        "right-44.png",
        "right",
        44,
        &[
            panel("Health", "Health", true),
            panel("Location", "Location", true),
            panel("Weather", "Weather", true),
            panel("Time", "Time", true),
            panel("Weapon", "Weapon", false),
            panel("Compass", "Compass", true),
            panel("Log", "Log", true),
        ],
    );
    shot(
        "left-32-compass-first.png",
        "left",
        32,
        &[
            panel("Compass", "Kompass", true),
            panel("Weapon", "Weapon", true),
            panel("Health", "Health", true),
            panel("Log", "Log", true),
        ],
    );
    shot(
        "labels-44.png",
        "right",
        44,
        &[
            panel("Limbs", "Limbs", true),
            panel("Movement", "Movement", true),
            panel("Stats", "Stats", true),
            panel("Needs", "Needs", true),
            panel("Location", "Location", true),
            panel("Weapon", "Weapon", true),
            panel("Armor", "Armor", true),
            panel("Compass", "Compass", true),
            panel("Log", "Log", true),
        ],
    );
}
