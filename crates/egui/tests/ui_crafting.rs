mod common;
use bn_client_egui::transport::ScriptHandle;
use bn_client_egui::{App, Clock, EngineModel, GameModel};
use common::{fixture, session};
use eframe::egui;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use serde_json::{Value, json};

type H<'a> = Harness<'a, App<EngineModel>>;

fn crafting(name: &str) -> Value {
    let text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../core/tests/fixtures/crafting.json"
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
    script.push(event(41, screen));
    h.run_steps(4);
    (h, script)
}

fn event(sequence: u32, screen: &str) -> Value {
    json!({"jsonrpc":"2.0","method":"bn.events","params":{"epoch":"e7f3","events":[{
        "sequence": sequence.to_string(), "revision": (sequence - 9).to_string(),
        "type": "interaction.changed", "changes": {"interaction": crafting(screen)}}]}})
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
fn crafting_lists_tabs_subtabs_recipes_and_the_selected_recipes_details() {
    let (mut h, _) = harness("weapon");
    h.get_by_label("Weapons");
    h.get_by_label("Melee");
    h.get_by_label("wooden club");
    h.get_by_label("knife spear");
    h.get_by_label("Search");
    // The first recipe is selected locally and its native text is shown.
    h.get_by_label("Time to complete: 10 m\nTools: none");
    h.get_by_label("fabrication 1");
    h.get_by_label("knife spear").click();
    h.run_steps(3);
    h.get_by_label("Time to complete: 30 m");
    h.get_by_label("You lack the components.");
}

#[test]
fn clicking_a_tab_a_subtab_or_a_recipe_chooses_it_and_a_disabled_recipe_is_inert() {
    for (label, id) in [
        ("Food", "tab:CC_FOOD"),
        ("Ranged", "subtab:CSC_WEAPON_RANGED"),
        ("Craft wooden club", "recipe:aa"),
    ] {
        let (mut h, script) = harness("weapon");
        h.get_by_label(label).click();
        h.run_steps(3);
        assert_eq!(
            operations(&script),
            [json!({"kind":"choose","choice_id":id})]
        );
    }
    let (mut h, script) = harness("weapon");
    h.get_by_label("knife spear").click();
    h.run_steps(2);
    h.key_press(egui::Key::Enter);
    h.run_steps(2);
    assert!(operations(&script).is_empty());
}

#[test]
fn arrows_move_the_local_selection_and_enter_crafts_it() {
    let (mut h, script) = harness("weapon");
    h.key_press(egui::Key::ArrowDown);
    h.run_steps(2);
    h.get_by_label("Time to complete: 30 m");
    h.key_press(egui::Key::ArrowUp);
    h.run_steps(2);
    h.key_press(egui::Key::Enter);
    h.run_steps(2);
    assert_eq!(
        operations(&script),
        [json!({"kind":"choose","choice_id":"recipe:aa"})]
    );
}

#[test]
fn right_arrow_switches_to_the_next_tab() {
    let (mut h, script) = harness("weapon");
    h.key_press(egui::Key::ArrowRight);
    h.run_steps(2);
    assert_eq!(
        operations(&script),
        [json!({"kind":"choose","choice_id":"tab:CC_FOOD"})]
    );
}

#[test]
fn typing_in_the_search_field_and_pressing_enter_submits_the_filter() {
    let (mut h, script) = harness("weapon");
    h.get_by_label("Search").click();
    h.run_steps(2);
    h.get_by_label("Search").type_text("club");
    h.run_steps(2);
    h.key_press(egui::Key::Enter);
    h.run_steps(2);
    assert_eq!(
        operations(&script),
        [json!({"kind":"fill","field_id":"filter","value":"club","submit":true})]
    );
}

#[test]
fn batch_count_crafts_with_set_count() {
    let (mut h, script) = harness("weapon");
    h.get_by_label("Batch count").click();
    h.run_steps(2);
    h.get_by_label("Batch count").type_text("3");
    h.run_steps(2);
    h.get_by_label("Craft batch").click();
    h.run_steps(2);
    assert_eq!(
        operations(&script),
        [json!({"kind":"set_count","choice_id":"recipe:aa","count":3})]
    );
}

#[test]
fn batch_mode_has_no_tabs_or_search_and_escape_closes() {
    let (mut h, script) = harness("batch");
    assert!(h.query_by_label("Search").is_none());
    assert!(h.query_by_label("Weapons").is_none());
    h.get_by_label("wooden club x2");
    h.key_press(egui::Key::Escape);
    h.run_steps(2);
    assert_eq!(operations(&script), [json!({"kind":"cancel"})]);
    assert!(h.state().model.menu().is_some());
}
