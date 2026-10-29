//! The log strip under the vehicle editor's canvas shows its latest step whole, however long the history, instead of
//! leaving it below the strip's edge.
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

/// The vehicle editor after fourteen steps, the last one the step it is at.
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
    let history = find(&mut screen["root"], "vehicle:history").unwrap();
    let row = history["rows"][0].clone();
    history["rows"] = (0..14)
        .map(|n| {
            let mut row = row.clone();
            row["id"] = json!(format!("step:{n}"));
            row["label"] = json!(format!("Install frame ×{n}"));
            row["selected"] = json!(n == 13);
            row
        })
        .collect();
    history["total"] = json!(14);
    screen
}

#[test]
fn the_latest_step_of_a_long_history_is_whole_in_the_log_strip() {
    for size in [egui::vec2(1280.0, 800.0), egui::vec2(1920.0, 1080.0)] {
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
        let heading = h.get_by_label("History").rect();
        let status = h.get_by_label("Front").rect();
        let latest = h
            .query_by_label("Install frame ×13")
            .map(|n| n.rect())
            .unwrap_or_else(|| panic!("at {size:?} the latest step is not drawn"));
        assert!(
            latest.top() >= heading.bottom() && latest.bottom() <= status.top(),
            "at {size:?} the latest step {latest:?} is out of the strip between {heading:?} and {status:?}"
        );
    }
}
