//! A header row of the vehicle editor's parts list is a heading over the rows after it, not a part to pick.
use crate::ui_window_class::example;
use eframe::egui;
use egui_kittest::kittest::{NodeT, Queryable};
use serde_json::{Value, json};

fn parts(value: &mut Value) -> Option<&mut Vec<Value>> {
    if value["id"] == "vehicle:parts" {
        return value["rows"].as_array_mut();
    }
    match value {
        Value::Array(items) => items.iter_mut().find_map(parts),
        Value::Object(members) => members.values_mut().find_map(parts),
        _ => None,
    }
}

#[test]
fn a_header_row_is_drawn_as_a_heading_that_takes_no_click() {
    let mut screen = example(
        "vehicle_editor",
        "vehicle editor with a prototype open on a mount",
    );
    let rows = parts(&mut screen["root"]).expect("the parts list");
    rows.insert(
        0,
        json!({"id": "header:frames", "label": "Frames and walls", "description": "", "denial": "",
            "enabled": false, "selectable": false, "selected": false, "header": true, "columns": []}),
    );
    let (app, script) = crate::common::staged(crate::common::over_map(screen));
    let mut h = crate::common::themed_harness(egui::vec2(1920.0, 1080.0), app);
    h.run_steps(7);
    crate::common::settle_windows(&mut h);
    let heading = h.get_by_label("Frames and walls");
    assert_ne!(
        heading.accesskit_node().role(),
        egui::accesskit::Role::Button,
        "a heading is no row to press"
    );
    let before = script.sent().len();
    heading.click();
    h.run_steps(4);
    assert_eq!(
        script.sent().len(),
        before,
        "nothing was sent for the click"
    );
}
