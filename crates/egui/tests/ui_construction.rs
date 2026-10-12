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

fn construction(name: &str) -> Value {
    let text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../core/tests/fixtures/construction.json"
    ))
    .unwrap();
    serde_json::from_str::<Value>(&text).unwrap()[name].clone()
}

fn harness<'a>(screen: &str) -> (H<'a>, ScriptHandle) {
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
        "sequence": "41", "revision": "32",
        "type": "interaction.changed", "changes": {"interaction": construction(screen)}}]}}),
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
fn construction_lists_tabs_rows_and_the_selected_constructions_requirements() {
    let (mut h, _) = harness("walls");
    h.get_by_label("Walls");
    h.get_by_label("Wooden Wall");
    h.get_by_label("Brick Wall");
    h.get_by_label("Search");
    h.get_by_label("Construct Wooden Wall");
    h.get_by_label(
        "Required skills: fabrication (1)\nRequires: floor\nTime to complete: 1 h\n\
         Components required:\n  2x4 (10)",
    );
    h.get_by_label("Brick Wall").click();
    h.run_steps(3);
    h.get_by_label(
        "No skills required.\nTime to complete: 2 h\nComponents required:\n  brick (30)",
    );
}

#[test]
fn clicking_a_tab_chooses_it_and_building_an_unbuildable_row_is_inert() {
    let (mut h, script) = harness("walls");
    h.get_by_label("Furniture").click();
    h.run_steps(3);
    assert_eq!(
        operations(&script),
        [json!({"kind":"choose","choice_id":"tab:FURN"})]
    );
    let (mut h, script) = harness("walls");
    h.get_by_label("Brick Wall").click();
    h.run_steps(2);
    h.key_press(egui::Key::Enter);
    h.run_steps(2);
    assert!(operations(&script).is_empty());
}

#[test]
fn enter_builds_the_selected_row_and_right_arrow_switches_tab() {
    let (mut h, script) = harness("walls");
    h.key_press(egui::Key::Enter);
    h.run_steps(2);
    assert_eq!(
        operations(&script),
        [json!({"kind":"choose","choice_id":"construction:aa"})]
    );
    let (mut h, script) = harness("walls");
    h.key_press(egui::Key::ArrowRight);
    h.run_steps(2);
    assert_eq!(
        operations(&script),
        [json!({"kind":"choose","choice_id":"tab:FURN"})]
    );
}

#[test]
fn a_search_shows_one_tab_and_enter_in_the_field_submits_the_filter() {
    let (mut h, _) = harness("searched");
    h.get_by_label("Searched");
    h.get_by_label("Brick Wall");
    assert!(h.query_by_label("Walls").is_none());
    let (mut h, script) = harness("walls");
    h.get_by_label("Search").click();
    h.run_steps(2);
    h.get_by_label("Search").type_text("brick");
    h.run_steps(2);
    h.key_press(egui::Key::Enter);
    h.run_steps(2);
    assert_eq!(
        operations(&script),
        [json!({"kind":"fill","field_id":"filter","value":"brick","submit":true})]
    );
}
