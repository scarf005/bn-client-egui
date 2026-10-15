//! Writes PNGs of the character creation tabs and the world creation pages to `$SHOT_DIR` for a person to
//! look at. Needs a Vulkan adapter, so it only runs when asked: `cargo test --test ui_shots_s13 -- --ignored`.
mod common;
use bn_client_egui::transport::ScriptedTransport;
use bn_client_egui::{App, Clock, ConnectOptions, EngineModel};
use common::fixture;
use eframe::egui;
use egui_kittest::Harness;
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
    let mut subscribe = fixture("subscribe_result");
    subscribe["result"]["interaction"]["stack"] = json!([screen]);
    let reply = vec![
        subscribe,
        fixture("snapshot_part_0"),
        fixture("snapshot_part_1"),
    ];
    let (t, _script) = ScriptedTransport::new();
    let t = t
        .on("bn.hello", vec![fixture("hello_result")])
        .on("bn.subscribe", reply.clone())
        .on("bn.subscribe", reply);
    let clock = Clock::manual();
    let model = EngineModel::open(Box::new(t), ConnectOptions::default()).with_clock(clock.clone());
    let mut h = Harness::builder()
        .with_size(egui::vec2(1100.0, 700.0))
        .wgpu()
        .build_ui_state(
            |ui, app: &mut App<EngineModel>| app.show(ui),
            App::new(model).with_clock(clock),
        );
    h.run_steps(6);
    let dir = std::path::PathBuf::from(std::env::var("SHOT_DIR").expect("SHOT_DIR"));
    std::fs::create_dir_all(&dir).unwrap();
    h.render().expect("renders").save(dir.join(name)).unwrap();
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
