//! Writes PNGs of the character sheet, the bionics list, the mutation list and the spell menu to
//! `$SHOT_DIR` for a person to look at. Needs a Vulkan adapter, so it only runs when asked:
//! `cargo test --test ui_shots_s9 -- --ignored`.
mod common;
use eframe::egui;
use serde_json::Value;

fn example(name: &str) -> Value {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../core/tests/fixtures/0.2.examples/character.json"
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
fn character_screens() {
    for (name, file) in [
        ("character sheet", "character-sheet.png"),
        ("bionics screen", "bionics.png"),
        ("mutation screen", "mutations.png"),
        ("spell menu", "spells.png"),
    ] {
        shot(example(name), file);
    }
}
