//! Writes PNGs of the auto notes manager to
//! `$SHOT_DIR` for a person to look at. Needs a Vulkan adapter, so it only runs when asked:
//! `cargo test --test it ui_shots_s18:: -- --ignored`.
use eframe::egui;
use serde_json::Value;

fn example(file: &str, name: &str) -> Value {
    let path = format!(
        "{}/../core/tests/fixtures/0.2.examples/{file}",
        env!("CARGO_MANIFEST_DIR")
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
fn auto_notes_screens() {
    shot(
        example(
            "autonote.json",
            "auto notes manager with three discovered map extras",
        ),
        "autonote.png",
    );
    shot(
        example("autonote.json", "auto notes manager before any discovery"),
        "autonote_empty.png",
    );
}
