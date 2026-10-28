//! The vehicle editor's Ctrl keys reach it: undo and redo from the canvas, and Done right after Return set a part's
//! value, once, while a world vehicle is edited in place.
use crate::ui_editor_drag::{H, open_sized, sent};
use eframe::egui;
use egui_kittest::kittest::Queryable;
use serde_json::{Value, json};

/// The vehicle editor editing a world vehicle in place, its redo on, with the settings form of a picked tank part.
fn in_place() -> Value {
    let mut screen = crate::ui_window_class::example(
        "vehicle_editor",
        "vehicle editor editing a world vehicle in place",
    );
    let tools = screen["root"]["children"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|c| c["id"] == "vehicle:tools")
        .unwrap();
    tools["child"] = json!({"kind": "form", "id": "vehicle:part_form", "label": "vehicle tank (60L)",
        "affordances": ["vehicle:part_set"],
        "fields": [{"id": "fuel", "label": "Fuel", "value": "", "type": "text"}]});
    let affordances = screen["affordances"].as_array_mut().unwrap();
    affordances.push(
        json!({"id": "vehicle:part_set", "label": "Value", "keys": [], "enabled": true,
        "args": {"row": {"component_id": "vehicle:part_form"}, "text": {}}, "expect": "boundary"}),
    );
    for a in affordances.iter_mut().filter(|a| a["id"] == "vehicle:redo") {
        a["enabled"] = json!(true);
    }
    screen
}

fn ctrl(h: &mut H, key: egui::Key) {
    h.key_press_modifiers(egui::Modifiers::CTRL, key);
    h.run_steps(4);
}

#[test]
fn ctrl_z_and_ctrl_y_undo_and_redo_in_the_vehicle_editor() {
    let (mut h, script) = open_sized(in_place(), egui::vec2(1290.0, 830.0));
    h.run_steps(6);
    ctrl(&mut h, egui::Key::Z);
    ctrl(&mut h, egui::Key::Y);
    let sent = sent(&script);
    assert!(
        sent.contains(&"vehicle:undo".to_owned()) && sent.contains(&"vehicle:redo".to_owned()),
        "{sent:?}"
    );
}

#[test]
fn ctrl_d_right_after_return_in_a_part_field_finishes_editing_in_place() {
    let (mut h, script) = open_sized(in_place(), egui::vec2(1290.0, 830.0));
    h.run_steps(6);
    h.get_by_role_and_label(egui::accesskit::Role::TextInput, "Fuel")
        .click();
    h.run_steps(4);
    h.event(egui::Event::Text("gasoline".into()));
    h.run_steps(2);
    h.key_press(egui::Key::Enter);
    h.run_steps(4);
    ctrl(&mut h, egui::Key::D);
    // One Return sets the value once, then Done finishes.
    assert_eq!(sent(&script), ["vehicle:part_set", "vehicle:done"]);
}
