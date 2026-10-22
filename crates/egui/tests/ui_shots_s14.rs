//! Writes PNGs of the distraction, colour, auto pickup and safe mode managers to
//! `$SHOT_DIR` for a person to look at. Needs a Vulkan adapter, so it only runs when asked:
//! `cargo test --test ui_shots_s14 -- --ignored`.
mod common;
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
    common::shot_stack(common::over_map(screen), egui::vec2(1100.0, 700.0), name);
}

#[test]
#[ignore = "needs a Vulkan adapter and SHOT_DIR"]
fn rule_screens() {
    shot(
        example(
            "distractions.json",
            "distraction manager with a distraction switched off",
        ),
        "distractions.png",
    );
    shot(
        example("colors.json", "colour manager with a custom colour set"),
        "colors.png",
    );
    shot(
        example("autopickup.json", "auto pickup manager with two rules"),
        "autopickup.png",
    );
    shot(
        example(
            "safemode.json",
            "safe mode manager with the default ruleset",
        ),
        "safemode.png",
    );
}
