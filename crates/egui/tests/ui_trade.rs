#![recursion_limit = "512"]
mod common;
use bn_client_egui::transport::ScriptHandle;
use bn_client_egui::{App, Clock, EngineModel};
use common::{fixture, session};
use eframe::egui;
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use serde_json::{Value, json};

type H<'a> = Harness<'a, App<EngineModel>>;

fn harness<'a>() -> (H<'a>, ScriptHandle) {
    let text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../core/tests/fixtures/trade.json"
    ))
    .unwrap();
    let trade = serde_json::from_str::<Value>(&text).unwrap()["trade"].clone();
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
        "changes": {"interaction": trade}}]}}),
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
fn both_parties_show_their_rows_prices_capacity_and_the_balance_line() {
    let (h, _) = harness();
    h.get_by_label("Lena: NPC items");
    h.get_by_label("You: Player items");
    h.get_by_label("Free capacity: 12.00 kg, 8.00 L");
    h.get_by_label("Credit $1.50 | Balance acceptable: yes");
    h.get_by_label("[x] tin can (3)");
    h.get_by_label("[ ] lighter");
    h.get_by_label("$0.50");
    h.get_by_label("$1.00");
}

#[test]
fn a_marked_stack_takes_a_count() {
    let (mut h, script) = harness();
    h.get_by_label("Count of tin can (3)").click();
    h.run_steps(2);
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::A);
    h.run_steps(2);
    h.get_by_label("Count of tin can (3)").type_text("3");
    h.run_steps(2);
    h.key_press(egui::Key::Enter);
    h.run_steps(3);
    assert_eq!(
        operations(&script),
        [json!({"kind":"set_count","choice_id":"trade:n1","count":3})]
    );
}

#[test]
fn clicking_a_row_chooses_it() {
    let (mut h, script) = harness();
    h.get_by_label("[ ] lighter").click();
    h.run_steps(3);
    assert_eq!(
        operations(&script),
        [json!({"kind":"choose","choice_id":"trade:y1"})]
    );
}

#[test]
fn confirm_is_the_native_confirm_action() {
    let (mut h, script) = harness();
    h.get_by_label("Confirm trade").click();
    h.run_steps(3);
    assert_eq!(
        operations(&script),
        [json!({"kind":"action","action_id":"CONFIRM"})]
    );
}

#[test]
fn a_count_beyond_the_stack_is_not_sent() {
    let (mut h, script) = harness();
    h.get_by_label("Count of tin can (3)").click();
    h.run_steps(2);
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::A);
    h.run_steps(2);
    h.get_by_label("Count of tin can (3)").type_text("9");
    h.run_steps(2);
    h.key_press(egui::Key::Enter);
    h.run_steps(3);
    assert!(operations(&script).is_empty());
}
