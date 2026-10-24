//! The vehicle editor's Part tab: its form is a column beside the prototypes and the vehicle, at the laptop
//! width too, instead of making the whole screen stack so the form is under the grid and out of view.
use crate::common::fixture;
use bn_client_egui::transport::ScriptedTransport;
use bn_client_egui::{App, Clock, ConnectOptions, EngineModel};
use eframe::egui;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use serde_json::{Value, json};

fn find<'a>(value: &'a mut Value, id: &str) -> Option<&'a mut Value> {
    if value["id"] == id {
        return Some(value);
    }
    match value {
        Value::Array(items) => items.iter_mut().find_map(|v| find(v, id)),
        Value::Object(members) => members.values_mut().find_map(|v| find(v, id)),
        _ => None,
    }
}

/// The editor with a prototype open on a mount and the Part tab holding a tank's fuel form.
fn editor() -> Value {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../core/tests/fixtures/0.2.examples/vehicle_editor.json"
    );
    let all = serde_json::from_str::<Value>(&std::fs::read_to_string(path).unwrap()).unwrap();
    let mut screen = all["valid"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["name"] == "vehicle editor with a prototype open on a mount")
        .unwrap()["value"]
        .clone();
    let tools = find(&mut screen["root"], "vehicle:tools").unwrap();
    for tab in tools["tabs"].as_array_mut().unwrap() {
        tab["selected"] = json!(tab["id"] == "tool:part");
    }
    tools["child"] = json!({
        "id": "vehicle:part_form", "kind": "form",
        "fields": [{ "id": "fuel", "label": "Fuel", "type": "text", "value": "gasoline" }]
    });
    // Prototypes named as the game's are, and a car's grid ten squares wide.
    let list = find(&mut screen["root"], "vehicle:prototypes").unwrap();
    for row in list["rows"].as_array_mut().unwrap() {
        row["label"] = json!("Mechanized Infantry Carrier");
        row["columns"] =
            json!([{ "key": "id", "label": "ID", "value": "commercial_airliner_cargo" }]);
    }
    let grid = find(&mut screen["root"], "vehicle:grid").unwrap();
    grid["cols"] = json!(10);
    grid["rows"] = json!(6);
    grid["cells"] = (0..6)
        .flat_map(|y| (0..10).map(move |x| json!({ "x": x, "y": y, "id": format!("mount:{x},{y}"), "glyph": ".", "affordance": "vehicle:mount", "enabled": true })))
        .collect();
    screen
}

#[test]
fn the_part_tab_form_stays_beside_the_vehicle_in_a_laptop_window() {
    for size in [egui::vec2(940.0, 830.0), egui::vec2(1290.0, 830.0)] {
        let mut subscribe = fixture("subscribe_result");
        subscribe["result"]["interaction"]["stack"] = json!([editor()]);
        let reply = vec![
            subscribe,
            fixture("snapshot_part_0"),
            fixture("snapshot_part_1"),
        ];
        let (t, _) = ScriptedTransport::new();
        let t = t
            .on("bn.hello", vec![fixture("hello_result")])
            .on("bn.subscribe", reply.clone())
            .on("bn.subscribe", reply);
        let clock = Clock::manual();
        let model =
            EngineModel::open(Box::new(t), ConnectOptions::default()).with_clock(clock.clone());
        let mut h = Harness::builder().with_size(size).build_ui_state(
            |ui, app: &mut App<EngineModel>| app.show(ui),
            App::new(model).with_clock(clock),
        );
        h.run_steps(8);
        let fuel = h.get_by_label("Fuel").rect();
        assert!(
            fuel.top() < 200.0,
            "the form ({fuel:?}) is in the first line of the editor at {size:?}"
        );
    }
}
