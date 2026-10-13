//! Writes PNGs of the map with its action palette to `$SHOT_DIR` for a person to look at. The map is the
//! engine's own example (`tutorial.json`), its actions the affordances of that screen. Needs a Vulkan
//! adapter, so it only runs when asked: `cargo test --test ui_shots_s1 -- --ignored`.
mod common;
use bn_client_egui::transport::ScriptedTransport;
use bn_client_egui::{App, Clock, ConnectOptions, EngineModel};
use common::fixture;
use eframe::egui;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use serde_json::{Value, json};

fn map_screen() -> Value {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../core/tests/fixtures/0.2.examples/tutorial.json"
    );
    let all = serde_json::from_str::<Value>(&std::fs::read_to_string(path).unwrap()).unwrap();
    all["valid"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["name"] == "map screen")
        .expect("the map screen example")["value"]
        .clone()
}

fn shot(h: &mut Harness<'_, App<EngineModel>>, name: &str) {
    let dir = std::path::PathBuf::from(std::env::var("SHOT_DIR").expect("SHOT_DIR"));
    std::fs::create_dir_all(&dir).unwrap();
    h.render().expect("renders").save(dir.join(name)).unwrap();
}

#[test]
#[ignore = "needs a Vulkan adapter and SHOT_DIR"]
fn the_map_with_its_action_palette() {
    let screen = map_screen();
    let actions = screen["affordances"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|a| a["expect"] == "screen")
        .map(|a| json!({"id": a["id"], "name": a["label"], "keys": a["keys"]}))
        .collect::<Vec<_>>();
    let mut subscribe = fixture("subscribe_result");
    subscribe["result"]["interaction"]["stack"] = json!([screen]);
    subscribe["result"]["interaction"]["actions"] = json!(actions);
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
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::P);
    h.run_steps(4);
    shot(&mut h, "palette.png");
    h.get_by_label("Search actions").focus();
    h.run_steps(2);
    h.get_by_label("Search actions").type_text("option");
    h.run_steps(3);
    shot(&mut h, "palette-options.png");
}
