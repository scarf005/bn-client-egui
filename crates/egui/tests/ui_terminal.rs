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

fn boundary(name: &str) -> Value {
    let text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../core/tests/fixtures/terminal.json"
    ))
    .unwrap();
    serde_json::from_str::<Value>(&text).unwrap()[name].clone()
}

fn harness<'a>(name: &str) -> (H<'a>, ScriptHandle) {
    harness_edited(name, |_| {})
}

fn harness_edited<'a>(name: &str, edit: impl FnOnce(&mut Value)) -> (H<'a>, ScriptHandle) {
    let mut boundary = boundary(name);
    edit(&mut boundary);
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
        "changes": {"interaction": boundary}}]}}),
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
fn a_terminal_query_shows_the_terminal_text_and_its_answers() {
    let (h, _) = harness("query");
    h.get_by_label_contains("Logging into Locked Terminal");
    h.get_by_label_contains("Access denied.");
    h.get_by_label("Yes");
    h.get_by_label("No");
    h.get_by_label("Quit");
    // The terminal's own window, not the generic list with its Cancel button.
    assert!(h.query_by_label("Cancel").is_none());
}

#[test]
fn clicking_a_terminal_answer_chooses_it_through_the_engine() {
    let (mut h, script) = harness("query");
    h.get_by_label("No").click();
    h.run_steps(3);
    assert_eq!(
        operations(&script),
        [json!({"kind":"choose","choice_id":"no"})]
    );
}

#[test]
fn the_extended_description_shows_its_text_and_targets_with_the_current_one_lit() {
    let (h, _) = harness("description");
    h.get_by_label_contains("This is a dirt.");
    h.get_by_label_contains("Loose soil.");
    h.get_by_label("Describe creatures");
}

#[test]
fn a_line_of_two_dashes_is_the_rule_between_description_parts_not_text() {
    let (h, _) = harness_edited("description", |b| {
        b["interaction"]["message"] = json!("This is a dirt.\n--\nLoose soil.")
    });
    h.get_by_label_contains("This is a dirt.");
    h.get_by_label_contains("Loose soil.");
    assert!(h.query_by_label("--").is_none());
}

#[test]
fn clicking_a_description_target_switches_it_through_the_engine() {
    let (mut h, script) = harness("description");
    h.get_by_label("Describe furniture").click();
    h.run_steps(3);
    assert_eq!(
        operations(&script),
        [json!({"kind":"choose","choice_id":"describe:furniture"})]
    );
}
