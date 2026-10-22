//! Writes PNGs of screens of each window class pushed onto the map, with the classic sidebar, at 1280x800 and
//! 1920x1080, to `$SHOT_DIR` for a person to look at. Needs a Vulkan adapter, so it only runs when asked:
//! `cargo test --test it ui_window_class_shots:: -- --ignored`.
use crate::common::fixture;
use bn_client_egui::transport::ScriptedTransport;
use bn_client_egui::{App, Clock, ConnectOptions, EngineModel};
use eframe::egui;
use serde_json::{Value, json};

fn example(file: &str, name: &str) -> Value {
    let path = format!(
        "{}/../core/tests/fixtures/0.2.examples/{file}.json",
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

fn shot(screen: Value, size: egui::Vec2, path: &std::path::Path) {
    let mut subscribe = fixture("subscribe_result");
    let map = subscribe["result"]["interaction"]["stack"][0].clone();
    subscribe["result"]["interaction"]["stack"] = json!([map, screen]);
    let reply = vec![
        subscribe,
        fixture("snapshot_part_0"),
        fixture("snapshot_part_1"),
    ];
    let (t, script) = ScriptedTransport::new();
    let t = t
        .on("bn.hello", vec![fixture("hello_result")])
        .on("bn.subscribe", reply.clone())
        .on("bn.subscribe", reply);
    let clock = Clock::manual();
    let model = EngineModel::open(Box::new(t), ConnectOptions::default()).with_clock(clock.clone());
    let mut h = crate::common::shot_harness(size, App::new(model).with_clock(clock));
    h.run_steps(3);
    // The avatar's status, for the classic sidebar beside the map.
    script.push(fixture("events_41_walk"));
    h.run_steps(6);
    crate::common::settle_windows(&mut h);
    h.render().expect("renders").save(path).unwrap();
}

#[test]
#[ignore = "needs a Vulkan adapter and SHOT_DIR"]
fn window_classes() {
    let dir = std::path::PathBuf::from(std::env::var("SHOT_DIR").expect("SHOT_DIR"));
    std::fs::create_dir_all(&dir).unwrap();
    let screens = [
        ("info", "message log", "message-log"),
        ("character", "character sheet", "character-sheet"),
        ("inventory", "drop selector screen", "inventory-drop"),
        ("pickup", "pickup screen", "pickup"),
        ("aim", "advanced inventory screen", "advanced-inventory"),
        ("crafting", "Crafting screen", "crafting"),
        (
            "editor",
            "map editor with a drawn room and a new mapping",
            "editor",
        ),
        ("overmap", "overmap screen", "overmap"),
        ("generic", "query popup screen", "query-popup"),
        ("look", "look-around screen", "look"),
        ("npctalk", "NPC talk screen", "npc-talk"),
    ];
    for size in [egui::vec2(1280.0, 800.0), egui::vec2(1920.0, 1080.0)] {
        for (file, name, shot_name) in screens {
            let path = dir.join(format!("{shot_name}-{}x{}.png", size.x, size.y));
            shot(example(file, name), size, &path);
        }
    }
}
