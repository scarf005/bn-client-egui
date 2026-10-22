//! Writes PNGs of the generic screens (query popup, filterable list, filter prompt, switches, refused rows) to
//! `$SHOT_DIR` for a person to look at. Needs a Vulkan adapter, so it only runs when asked:
//! `cargo test --test it ui_shots_s0:: -- --ignored`.
use eframe::egui;
use serde_json::{Value, json};

fn example(name: &str) -> Value {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../core/tests/fixtures/0.2.examples/generic.json"
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

fn refused(id: &str, label: &str, denial: &str) -> Value {
    json!({"id": id, "label": label, "description": "", "denial": denial, "enabled": false,
        "selectable": true, "selected": false, "columns": []})
}

#[test]
#[ignore = "needs a Vulkan adapter and SHOT_DIR"]
fn generic_screens() {
    shot(example("query popup screen"), "query-popup.png");
    shot(example("filterable list screen"), "filterable-list.png");
    shot(example("filter prompt before typing"), "filter-prompt.png");
    shot(
        example("filter prompt after typing"),
        "filter-prompt-typed.png",
    );
    shot(example("list of switches screen"), "switches.png");
    let screen = json!({"screen_id": "screen:9", "role": "dialog", "title": "Options", "affordances": [],
        "root": {"id": "list", "kind": "list", "columns": [], "total": 3, "selection": "single",
            "rows": [refused("b", "Adjacent", "Needs auto pickup"),
                refused("c", "Weight limit", "Needs auto pickup"),
                {"id": "d", "label": "Safe mode", "description": "", "denial": "", "enabled": true,
                    "selectable": true, "selected": true, "columns": []}]}});
    shot(screen, "refused-rows.png");
}
