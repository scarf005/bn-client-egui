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

fn harness<'a>(screen: &str) -> (H<'a>, ScriptHandle) {
    harness_with(screen, |_| {})
}

fn harness_with<'a>(screen: &str, edit: impl FnOnce(&mut Value)) -> (H<'a>, ScriptHandle) {
    let text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../core/tests/fixtures/character.json"
    ))
    .unwrap();
    let mut boundary = serde_json::from_str::<Value>(&text).unwrap()[screen].clone();
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

/// Selects the sheet's tab `pane`.
fn open_tab(boundary: &mut Value, pane: &str) {
    let choices = boundary["interaction"]["choices"].as_array_mut().unwrap();
    for c in choices.iter_mut() {
        let id = c["id"].as_str().unwrap().to_string();
        if id.starts_with("character-tab:") {
            c["selected"] = (id == format!("character-tab:{pane}")).into();
        }
    }
}

#[test]
fn the_sheet_lists_the_tabs_the_stat_rows_and_the_speed_breakdown() {
    let (h, _) = harness("sheet");
    assert!(
        h.query_all_by_label("Character information")
            .next()
            .is_some()
    );
    h.get_by_label("STATS");
    h.get_by_label("ENCUMBRANCE AND WARMTH");
    h.get_by_label_contains("Strength:");
    h.get_by_label_contains("Height:");
    h.get_by_label("Base Move Cost:");
    h.get_by_label("-5%");
    h.get_by_label("Base HP: 90");
}

#[test]
fn only_the_selected_tabs_rows_are_listed() {
    let (h, _) = harness_with("sheet", |b| open_tab(b, "skills"));
    h.get_by_label_contains("Melee");
    h.get_by_label("Combat");
    assert!(h.query_by_label_contains("Strength:").is_none());
    assert!(h.query_by_label_contains("Quick").is_none());
}

#[test]
fn a_skill_category_is_a_heading_not_a_row() {
    let (h, _) = harness_with("sheet", |b| open_tab(b, "skills"));
    assert_ne!(
        h.get_by_label("Combat").accesskit_node().role(),
        egui::accesskit::Role::MenuItem
    );
    assert_eq!(
        h.get_by_label_contains("Melee").accesskit_node().role(),
        egui::accesskit::Role::MenuItem
    );
}

#[test]
fn clicking_a_tab_chooses_it_natively() {
    let (mut h, script) = harness("sheet");
    h.get_by_label("TRAITS").click();
    h.run_steps(3);
    assert_eq!(
        operations(&script),
        [json!({"kind":"choose","choice_id":"character-tab:traits"})]
    );
}

#[test]
fn clicking_a_row_moves_the_native_cursor_to_it() {
    let (mut h, script) = harness("sheet");
    h.get_by_label_contains("Dexterity:").click();
    h.run_steps(3);
    assert_eq!(
        operations(&script),
        [json!({"kind":"choose","choice_id":"character-stat:1"})]
    );
}

#[test]
fn the_sheets_own_controls_are_the_menus_choices() {
    let (mut h, script) = harness("sheet");
    h.get_by_label("Change name").click();
    h.run_steps(3);
    assert_eq!(
        operations(&script),
        [json!({"kind":"choose","choice_id":"character-control:CHANGE_NAME"})]
    );
}

#[test]
fn bionics_show_power_and_the_tabs_without_the_key_help_line() {
    let (h, _) = harness("bionics");
    h.get_by_label("Bionic Power: 12/100kJ");
    h.get_by_label("Available Fuel: Gasoline: 40/200");
    assert!(h.query_by_label_contains("Activating  [e]").is_none());
    h.get_by_label("Head: 0/18   Eyes: 1/4");
    h.get_by_label("ACTIVE (2)");
    h.get_by_label("PASSIVE (1)");
    h.get_by_label_contains("Flashlight");
}

#[test]
fn a_bionic_row_prints_its_key_power_and_state() {
    let (h, _) = harness("bionics");
    let row = h.get_by_label_contains("Flashlight");
    let text = row.accesskit_node().label().unwrap();
    assert!(text.starts_with("a "), "{text}");
    assert!(text.contains("0.50 kJ act, OFF"), "{text}");
    assert!(text.trim_end().ends_with("Off"), "{text}");
    // The name is not printed twice: the power is its own column.
    assert_eq!(text.matches("0.50 kJ act").count(), 1, "{text}");
}

#[test]
fn clicking_the_other_bionics_tab_chooses_it() {
    let (mut h, script) = harness("bionics");
    h.get_by_label("PASSIVE (1)").click();
    h.run_steps(3);
    assert_eq!(
        operations(&script),
        [json!({"kind":"action","action_id":"NEXT_TAB"})]
    );
}

#[test]
fn clicking_a_bionic_chooses_it_and_its_actions_are_the_engines() {
    let (mut h, script) = harness("bionics");
    h.get_by_label_contains("Fuel burner").click();
    h.run_steps(3);
    assert_eq!(
        operations(&script),
        [json!({"kind":"choose","choice_id":"bionic:burner"})]
    );
    let (mut h, script) = harness("bionics");
    h.get_by_label_contains("Reassign").click();
    h.run_steps(3);
    assert_eq!(
        operations(&script),
        [json!({"kind":"action","action_id":"REASSIGN"})]
    );
}

#[test]
fn an_action_the_engine_does_not_offer_has_no_button() {
    let (h, _) = harness("bionics");
    assert!(h.query_by_label_contains("Toggle safe fuel").is_none());
    h.get_by_label_contains("Toggle examine");
}

#[test]
fn mutations_list_passive_and_active_traits_side_by_side() {
    let (h, _) = harness("mutations");
    h.get_by_label("Passive:");
    h.get_by_label("Active:");
    h.get_by_label_contains("Night vision");
    let spit = h.get_by_label_contains("Acid spit");
    let text = spit.accesskit_node().label().unwrap();
    assert!(text.contains("Inactive, 5 RU, 10 turns"), "{text}");
    h.get_by_label("Spit acid.");
}

#[test]
fn choosing_a_mutation_or_an_action_row_is_the_native_choice() {
    let (mut h, script) = harness("mutations");
    h.get_by_label_contains("Acid spit").click();
    h.run_steps(3);
    assert_eq!(
        operations(&script),
        [json!({"kind":"choose","choice_id":"mutation:spit"})]
    );
    let (mut h, script) = harness("mutations");
    h.get_by_label("Reassign").click();
    h.run_steps(3);
    assert_eq!(
        operations(&script),
        [json!({"kind":"choose","choice_id":"action:REASSIGN"})]
    );
}
