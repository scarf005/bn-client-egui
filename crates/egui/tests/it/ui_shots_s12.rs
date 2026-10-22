//! Writes PNGs of the message log, help, morale, mission list, faction manager and scores to
//! `$SHOT_DIR` for a person to look at. Needs a Vulkan adapter, so it only runs when asked:
//! `cargo test --test it ui_shots_s12:: -- --ignored`.
use eframe::egui;
use serde_json::Value;

fn example(name: &str) -> Value {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../core/tests/fixtures/0.2.examples/info.json"
    );
    let all = serde_json::from_str::<Value>(&std::fs::read_to_string(path).unwrap()).unwrap();
    all["valid"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["name"] == name)
        .unwrap_or_else(|| panic!("no example {name}"))["value"]
        .clone()
}

fn shot(screen: Value, name: &str) {
    crate::common::shot_stack(
        crate::common::over_map(screen),
        egui::vec2(1100.0, 700.0),
        name,
    );
}

#[test]
#[ignore = "needs a Vulkan adapter and SHOT_DIR"]
fn info_screens() {
    for (name, file) in [
        ("message log", "messages.png"),
        ("help topics", "help.png"),
        ("morale", "morale.png"),
        ("mission list", "missions.png"),
        ("faction manager", "factions.png"),
        ("scores", "scores.png"),
    ] {
        shot(example(name), file);
    }
}
