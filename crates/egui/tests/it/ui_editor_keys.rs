//! The map editor's keys: a tool tab's hotkey chooses the tool as a click does, the keys of an affordance run it,
//! none of them while a text field has the typing, and the affordances only a drag runs have no button.
use crate::ui_editor_drag::{H, editor, open, sent};
use eframe::egui;
use egui_kittest::kittest::Queryable;
use serde_json::{Value, json};

/// The editor whose affordance `id` is on.
fn enabled(id: &str) -> Value {
    let mut screen = editor(Some("trail"));
    let on = screen["affordances"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|a| a["id"] == id);
    on.unwrap()["enabled"] = json!(true);
    screen
}

/// Types `text`, which the editor reads as a key when no field has the typing.
fn type_text(h: &mut H, text: &str) {
    h.event(egui::Event::Text(text.into()));
    h.run_steps(4);
}

fn tool_rows(script: &bn_client_egui::transport::ScriptHandle) -> Vec<String> {
    script
        .sent()
        .into_iter()
        .filter(|m| m["method"] == "bn.command.submit")
        .map(|m| m["params"]["operation"].clone())
        .filter(|o| o["affordance_id"] == "editor:tool")
        .map(|o| o["args"]["row"].as_str().unwrap().to_owned())
        .collect()
}

#[test]
fn a_tab_hotkey_runs_the_tabs_select_on_that_tab() {
    let (mut h, script) = open(editor(Some("trail")));
    type_text(&mut h, "l");
    type_text(&mut h, "S");
    assert_eq!(tool_rows(&script), ["tool:line", "tool:select"]);
}

#[test]
fn a_tab_hotkey_is_not_a_key_while_a_text_field_has_the_typing() {
    let (mut h, script) = open(editor(Some("trail")));
    h.get_all_by_role(egui::accesskit::Role::TextInput)
        .next()
        .unwrap()
        .click();
    h.run_steps(4);
    type_text(&mut h, "l");
    assert_eq!(tool_rows(&script), Vec::<String>::new());
}

#[test]
fn an_affordance_key_is_not_a_key_while_a_text_field_has_the_typing() {
    let (mut h, script) = open(enabled("editor:rotate"));
    h.get_all_by_role(egui::accesskit::Role::TextInput)
        .next()
        .unwrap()
        .click();
    h.run_steps(4);
    type_text(&mut h, "Z");
    assert!(!sent(&script).contains(&"editor:rotate".to_owned()));
}

#[test]
fn an_affordance_key_runs_the_affordance() {
    let (mut h, script) = open(enabled("editor:rotate"));
    type_text(&mut h, "Z");
    assert_eq!(sent(&script), ["editor:rotate"]);
}

/// The editor whose canvas also drags regions as `editor:placement_span`.
fn with_region_drag() -> Value {
    let mut screen = editor(Some("trail"));
    let mut stack = vec![&mut screen];
    while let Some(v) = stack.pop() {
        match v {
            Value::Object(m) => {
                if m.get("id") == Some(&json!("editor:canvas")) {
                    m.insert("region_drag".into(), json!("editor:placement_span"));
                }
                stack.extend(m.values_mut());
            }
            Value::Array(a) => stack.extend(a.iter_mut()),
            _ => {}
        }
    }
    screen["affordances"].as_array_mut().unwrap().push(json!({
        "id": "editor:placement_span", "label": "Move placement", "enabled": true,
        "keys": [], "expect": "screen"
    }));
    screen
}

#[test]
fn an_affordance_only_a_drag_runs_has_no_button() {
    let (mut h, _) = open(with_region_drag());
    h.get_by_label("⋯").click();
    h.run_steps(4);
    // The overflow menu lists the affordances no drag runs.
    assert!(h.query_by_label_contains("Redo").is_some());
    for label in ["Begin stroke", "End stroke", "Move placement"] {
        assert!(h.query_by_label_contains(label).is_none(), "{label}");
    }
}
