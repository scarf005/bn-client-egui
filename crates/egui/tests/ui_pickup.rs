#![recursion_limit = "512"]
mod common;
use bn_client_egui::transport::ScriptHandle;
use bn_client_egui::{App, Clock, EngineModel};
use common::{fixture, session};
use eframe::egui;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use serde_json::{Value, json};

type H<'a> = Harness<'a, App<EngineModel>>;

fn pickup() -> Value {
    let text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../core/tests/fixtures/pickup.json"
    ))
    .unwrap();
    serde_json::from_str::<Value>(&text).unwrap()["pickup"].clone()
}

fn harness<'a>() -> (H<'a>, ScriptHandle) {
    let (t, script) = session();
    let t = t.on("bn.command.submit", vec![fixture("submit_result")]);
    let clock = Clock::manual();
    let model = EngineModel::new(Box::new(t)).with_clock(clock.clone());
    let mut h = Harness::builder()
        .with_size(egui::vec2(1100.0, 800.0))
        .build_ui_state(
            |ui, app: &mut App<EngineModel>| app.show(ui),
            App::new(model).with_clock(clock),
        );
    h.run_steps(3);
    script.push(
        json!({"jsonrpc":"2.0","method":"bn.events","params":{"epoch":"e7f3","events":[{
        "sequence": "41", "revision": "32", "type": "interaction.changed",
        "changes": {"interaction": pickup()}}]}}),
    );
    h.run_steps(4);
    (h, script)
}

fn operations(script: &ScriptHandle) -> Vec<Value> {
    script
        .sent()
        .into_iter()
        .filter(|m| m["method"] == "bn.command.submit")
        .map(|m| m["params"]["operation"].clone())
        .collect()
}

#[test]
fn the_list_shows_marks_the_predicted_carry_header_and_the_cursor_rows_description() {
    let (h, _) = harness();
    h.get_by_label("[ ] lighter (17/25)");
    h.get_by_label("[x] tin can");
    h.get_by_label("PICK Wgt 0.2/45.0  Vol 0.30/0.00");
    // The native cursor row is the tin can: its description is shown, not the first row's.
    h.get_by_label_contains("A battered tin can.");
    assert!(
        h.query_by_label_contains("disposable plastic lighter")
            .is_none()
    );
}

#[test]
fn clicking_a_row_toggles_its_mark_through_the_engine() {
    let (mut h, script) = harness();
    h.get_by_label("[x] tin can").click();
    h.run_steps(3);
    assert_eq!(
        operations(&script),
        [json!({"kind":"choose","choice_id":"pickup:b"})]
    );
}

#[test]
fn confirm_is_the_menus_own_button_and_runs_the_confirm_action() {
    let (mut h, script) = harness();
    h.get_by_label("Confirm").click();
    h.run_steps(3);
    assert_eq!(
        operations(&script),
        [json!({"kind":"action","action_id":"CONFIRM"})]
    );
}

#[test]
fn a_marked_stack_takes_a_count_up_to_what_is_there() {
    let (mut h, script) = harness();
    h.get_by_label("Count of tin can").click();
    h.run_steps(2);
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::A);
    h.run_steps(2);
    h.get_by_label("Count of tin can").type_text("3");
    h.run_steps(2);
    h.key_press(egui::Key::Enter);
    h.run_steps(3);
    assert_eq!(
        operations(&script),
        [json!({"kind":"set_count","choice_id":"pickup:b","count":3})]
    );
}

#[test]
fn a_count_beyond_the_stack_is_not_sent() {
    let (mut h, script) = harness();
    h.get_by_label("Count of tin can").click();
    h.run_steps(2);
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::A);
    h.run_steps(2);
    h.get_by_label("Count of tin can").type_text("9");
    h.run_steps(2);
    h.key_press(egui::Key::Enter);
    h.run_steps(3);
    assert!(operations(&script).is_empty());
}

#[test]
fn an_unmarked_row_has_no_count_box() {
    let (h, _) = harness();
    assert!(h.query_by_label("Count of lighter (17/25)").is_none());
}
