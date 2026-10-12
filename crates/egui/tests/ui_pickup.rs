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

fn pickup() -> Value {
    let text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../core/tests/fixtures/pickup.json"
    ))
    .unwrap();
    serde_json::from_str::<Value>(&text).unwrap()["pickup"].clone()
}

fn harness<'a>() -> (H<'a>, ScriptHandle) {
    harness_with(pickup())
}

fn harness_with<'a>(pickup: Value) -> (H<'a>, ScriptHandle) {
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
        "changes": {"interaction": pickup}}]}}),
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
    h.get_by_label("lighter (17/25)");
    h.get_by_label("tin can");
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
    h.get_by_label("tin can").click();
    h.run_steps(3);
    assert_eq!(
        operations(&script),
        [json!({"kind":"choose","choice_id":"pickup:b"})]
    );
}

#[test]
fn the_native_list_has_no_confirm_button_because_enter_confirms() {
    let (h, _) = harness();
    assert!(h.query_by_label("Confirm").is_none());
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

#[test]
fn the_rows_do_not_move_with_the_length_of_the_hovered_description() {
    let mut menu = pickup();
    let long = "A long description that wraps over many lines of the dialog. ".repeat(12);
    menu["interaction"]["choices"][0]["description"] = long.into();
    let (mut h, _) = harness_with(menu);
    let can = h.get_by_label("tin can").rect();
    for row in ["lighter (17/25)", "tin can"] {
        let at = h.get_by_label(row).rect().center();
        h.hover_at(at);
        h.run_steps(3);
        assert_eq!(h.get_by_label("tin can").rect(), can, "{row}");
    }
}

#[test]
fn an_arrow_key_keeps_the_focus_on_the_focused_row() {
    let (mut h, _) = harness();
    h.key_press(egui::Key::Tab);
    h.run_steps(3);
    let focused = |h: &H| {
        h.get_all_by_role(egui::accesskit::Role::MenuItem)
            .position(|n| n.accesskit_node().is_focused())
    };
    let before = focused(&h);
    assert!(before.is_some(), "Tab focuses a menu item");
    h.key_press(egui::Key::ArrowRight);
    h.run_steps(3);
    assert_eq!(focused(&h), before);
}

#[test]
fn enter_on_a_focused_row_confirms_like_the_native_key_instead_of_unmarking_it() {
    let (mut h, script) = harness();
    h.key_press(egui::Key::Tab);
    h.run_steps(3);
    h.key_press(egui::Key::Enter);
    h.run_steps(3);
    assert_eq!(
        operations(&script),
        [json!({"kind":"action","action_id":"CONFIRM"})]
    );
}

#[test]
fn a_stolen_row_keeps_its_mark_and_the_right_position_anchors_the_list_at_the_right_edge() {
    let mut b = pickup();
    b["interaction"]["choices"][0]["columns"] = json!([
        {"label": "Name markup", "value": "<color_light_red>!</color> 2 lighter"},
        {"label": "Row color", "value": "c_light_green"}
    ]);
    b["interaction"]["compat"] = json!({"panes": [{
        "id": "p", "label": "Pick up", "role": "pickup", "area_id": "right",
        "area_label": "Pick up", "area_description": "", "filter": "", "storage_kind": ""
    }]});
    let (h, _) = harness_with(b);
    assert!(h.get_by_label("lighter (17/25)").rect().min.x > 400.0);
}
