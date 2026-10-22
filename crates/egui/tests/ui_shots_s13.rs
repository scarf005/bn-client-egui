//! Writes PNGs of the character creation tabs and the world creation pages to `$SHOT_DIR` for a person to
//! look at. Needs a Vulkan adapter, so it only runs when asked: `cargo test --test ui_shots_s13 -- --ignored`.
mod common;
use eframe::egui;
use serde_json::{Value, json};

fn example(name: &str) -> Value {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../core/tests/fixtures/0.2.examples/newgame.json"
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
    common::shot_stack(json!([screen]), egui::vec2(1100.0, 700.0), name);
}

#[test]
#[ignore = "needs a Vulkan adapter and SHOT_DIR"]
fn new_game_screens() {
    for tab in [
        "points",
        "scenario",
        "profession",
        "stats",
        "traits",
        "bionics",
        "skills",
        "overview",
    ] {
        shot(
            example(&format!("character creation on the {tab} page")),
            &format!("newchar_{tab}.png"),
        );
    }
    shot(
        example("world creation on the mods page"),
        "worldgen_mods.png",
    );
    shot(
        example("world creation on the finalize page"),
        "worldgen_confirm.png",
    );
}
