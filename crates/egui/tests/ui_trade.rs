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
    harness_with(|_| {})
}

fn harness_with<'a>(edit: impl FnOnce(&mut Value)) -> (H<'a>, ScriptHandle) {
    let text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../core/tests/fixtures/trade.json"
    ))
    .unwrap();
    let mut trade = serde_json::from_str::<Value>(&text).unwrap()["trade"].clone();
    edit(&mut trade);
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
fn both_parties_show_their_rows_prices_and_the_balance_line() {
    let (h, _) = harness();
    h.get_by_label("Inventory: Lena");
    h.get_by_label("Inventory: You");
    h.get_by_label("Credit $1.50 | Balance acceptable: yes");
    h.get_by_label("tin can (3)");
    h.get_by_label("lighter");
    h.get_by_label("$0.50");
    h.get_by_label("$1.00");
}

#[test]
fn rows_sit_under_their_category_in_an_amount_weight_volume_table() {
    let (h, _) = harness();
    assert_eq!(h.query_all_by_label("TOOLS").count(), 2);
    assert_eq!(h.query_all_by_label("amt").count(), 2);
    assert_eq!(h.query_all_by_label("unit price").count(), 2);
    // 3 cans of 0.10 kg
    h.get_by_label("0.30");
}

#[test]
fn the_pane_header_shows_carried_against_capacity_not_free_capacity() {
    let (h, _) = harness();
    h.get_by_label("/45.00 kg");
    h.get_by_label("1.17");
    h.get_by_label("/8.00 L");
    assert!(
        h.query_by_label("Free capacity: 12.00 kg, 8.00 L")
            .is_none()
    );
}

#[test]
fn the_header_lists_the_keys_of_the_offered_actions() {
    let (h, _) = harness();
    h.get_by_label("[e] examine item  [TAB] switch panes  [RETURN] confirm trade  [b] autobalance  [;] category select");
}

#[test]
fn category_select_is_the_native_action() {
    let (mut h, script) = harness();
    h.get_by_label("Category select").click();
    h.run_steps(3);
    assert_eq!(
        operations(&script),
        [json!({"kind":"action","action_id":"CATEGORY_SELECTION"})]
    );
}

#[test]
fn a_filter_typed_in_the_other_pane_switches_panes_before_it_filters() {
    let (mut h, script) = harness();
    h.get_by_label("You filter").click();
    h.run_steps(2);
    h.get_by_label("You filter").type_text("lig");
    h.run_steps(2);
    h.key_press(egui::Key::Enter);
    h.run_steps(8);
    assert_eq!(
        operations(&script)[0],
        json!({"kind":"action","action_id":"SWITCH_LISTS"})
    );
}

#[test]
fn a_filter_in_the_pane_under_the_cursor_filters_without_switching() {
    let (mut h, script) = harness();
    h.get_by_label("Lena filter").click();
    h.run_steps(2);
    h.get_by_label("Lena filter").type_text("tin");
    h.run_steps(2);
    h.key_press(egui::Key::Enter);
    h.run_steps(8);
    assert_eq!(
        operations(&script)[0],
        json!({"kind":"action","action_id":"FILTER"})
    );
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
    h.get_by_label("lighter").click();
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

#[test]
fn rows_are_plain_menu_items_named_by_the_item_label() {
    let (h, _) = harness_with(|t| t["interaction"]["choices"][1]["hotkey"] = "b".into());
    let row = h.get_by_label("lighter");
    assert_eq!(row.accesskit_node().role(), egui::accesskit::Role::MenuItem);
    assert!(h.query_by_label("[ ] lighter").is_none());
}

#[test]
fn the_page_indicator_and_category_mode_are_the_engines() {
    let (h, _) = harness();
    h.get_by_label("< Page 1/2 >");
    h.get_by_label("< Page 1/1 >");
    h.get_by_label("Category select OFF");
    let (h, _) = harness_with(|t| t["interaction"]["category_mode"] = true.into());
    h.get_by_label("Category select ON");
}

#[test]
fn an_engine_without_category_mode_shows_no_state() {
    let (h, _) = harness_with(|t| {
        t["interaction"]
            .as_object_mut()
            .unwrap()
            .remove("category_mode");
    });
    assert!(h.query_by_label("Category select OFF").is_none());
    assert!(h.query_by_label("Category select ON").is_none());
}
