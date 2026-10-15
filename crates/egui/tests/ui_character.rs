//! The character sheet, the bionics list, the mutation list and the spell menu drawn from their components,
//! from the engine's own examples (`crates/core/tests/fixtures/0.2.examples/character.json`): what the
//! screens show, and that a click on a tab or a row runs the affordance the engine named, with the row's id.
mod common;
use bn_client_egui::transport::{ScriptHandle, ScriptedTransport};
use bn_client_egui::{App, Clock, ConnectOptions, EngineModel};
use common::fixture;
use eframe::egui;
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use serde_json::Value;

type H<'a> = Harness<'a, App<EngineModel>>;

fn example(name: &str) -> Value {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../core/tests/fixtures/0.2.examples/character.json"
    );
    let all = serde_json::from_str::<Value>(&std::fs::read_to_string(path).unwrap()).unwrap();
    all["valid"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["name"] == name)
        .unwrap_or_else(|| panic!("no example {name}"))["value"]
        .clone()
}

fn open<'a>(screen: Value) -> (H<'a>, ScriptHandle) {
    let mut subscribe = fixture("subscribe_result");
    subscribe["result"]["interaction"]["stack"] = serde_json::json!([screen]);
    let reply = vec![
        subscribe,
        fixture("snapshot_part_0"),
        fixture("snapshot_part_1"),
    ];
    let (t, script) = ScriptedTransport::new();
    let t = t
        .on("bn.hello", vec![fixture("hello_result")])
        .on("bn.subscribe", reply.clone())
        .on("bn.subscribe", reply)
        .on("bn.command.submit", vec![fixture("submit_result")]);
    let clock = Clock::manual();
    let model = EngineModel::open(Box::new(t), ConnectOptions::default()).with_clock(clock.clone());
    let mut h = Harness::builder()
        .with_size(egui::vec2(1100.0, 800.0))
        .build_ui_state(
            |ui, app: &mut App<EngineModel>| app.show(ui),
            App::new(model).with_clock(clock),
        );
    h.run_steps(4);
    (h, script)
}

/// The operations the client submitted, in order.
fn operations(script: &ScriptHandle) -> Vec<Value> {
    script
        .sent()
        .into_iter()
        .filter(|m| m["method"] == "bn.command.submit")
        .map(|m| m["params"]["operation"].clone())
        .collect()
}

/// The first node whose label contains `text`: a name may show twice, as a tab and as a title.
fn first<'a>(h: &'a H<'a>, text: &'a str) -> egui_kittest::Node<'a> {
    h.query_all_by_label_contains(text)
        .next()
        .unwrap_or_else(|| panic!("nothing shows {text}"))
}

/// The component of `id` in the tree under `node`.
fn find<'a>(node: &'a Value, id: &str) -> Option<&'a Value> {
    if node["id"] == id {
        return Some(node);
    }
    node["children"]
        .as_array()
        .into_iter()
        .flatten()
        .chain(node.get("child"))
        .find_map(|child| find(child, id))
}

#[test]
fn the_sheet_draws_its_tabs_the_rows_of_the_selected_one_and_the_speed_breakdown() {
    let screen = example("character sheet");
    let (h, _) = open(screen.clone());
    for tab in find(&screen["root"], "character:tabs").unwrap()["tabs"]
        .as_array()
        .unwrap()
    {
        first(&h, tab["label"].as_str().unwrap());
    }
    let rows = find(&screen["root"], "character:stats:rows").unwrap()["rows"]
        .as_array()
        .unwrap()
        .clone();
    first(&h, rows[0]["label"].as_str().unwrap());
    let speed = find(&screen["root"], "character:speed").unwrap();
    first(&h, speed["stats"][0]["label"].as_str().unwrap());
}

#[test]
fn a_click_on_a_sheet_tab_runs_the_tab_affordance_with_the_tabs_own_id() {
    let screen = example("character sheet");
    let (mut h, script) = open(screen.clone());
    let tabs = find(&screen["root"], "character:tabs").unwrap();
    let skills = tabs["tabs"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["id"] == "skills")
        .unwrap();
    first(&h, skills["label"].as_str().unwrap()).click();
    h.run_steps(3);
    let last = operations(&script).pop().expect("a command");
    assert_eq!(last["kind"], "invoke");
    assert_eq!(last["affordance_id"], tabs["select"]);
    assert_eq!(last["args"]["row"], "skills");
}

#[test]
fn the_bionics_list_draws_its_rows_by_identity_and_the_power() {
    let screen = example("bionics screen");
    let (h, script) = open(screen.clone());
    let list = find(&screen["root"], "bionics:list").unwrap();
    let row = &list["rows"][0];
    assert!(row["id"].as_str().unwrap().starts_with("bionic:"));
    let status = find(&screen["root"], "bionics:status").unwrap();
    first(&h, status["stats"][0]["label"].as_str().unwrap());
    first(&h, row["label"].as_str().unwrap());
    assert!(operations(&script).is_empty(), "drawing sends nothing");
}

#[test]
fn the_mutation_list_draws_both_columns_and_a_click_picks_the_row_by_identity() {
    let screen = example("mutation screen");
    let (mut h, script) = open(screen.clone());
    let passive = &find(&screen["root"], "mutations:passive:rows").unwrap()["rows"][0];
    let active = &find(&screen["root"], "mutations:active:rows").unwrap()["rows"][0];
    first(&h, passive["label"].as_str().unwrap());
    first(&h, active["label"].as_str().unwrap()).click();
    h.run_steps(3);
    let last = operations(&script).pop().expect("a command");
    assert_eq!(last["affordance_id"], "bn:activate:mutations:pick:active");
    assert_eq!(last["args"]["row"], active["id"]);
}

#[test]
fn a_click_on_a_spell_chooses_it_by_the_spells_identity() {
    let screen = example("spell menu");
    let (mut h, script) = open(screen.clone());
    let list = find(&screen["root"], "list:choices").unwrap();
    let spell = list["rows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["id"].as_str().unwrap().starts_with("spell:"))
        .unwrap();
    first(&h, spell["label"].as_str().unwrap()).click();
    h.run_steps(3);
    let last = operations(&script).pop().expect("a command");
    assert_eq!(last["affordance_id"], list["activate"]);
    assert_eq!(last["args"]["row"], spell["id"]);
}

#[test]
fn the_spell_list_draws_every_spell_row_and_the_engines_own_key_rows() {
    let screen = example("spell menu");
    let (h, script) = open(screen.clone());
    let list = find(&screen["root"], "list:choices").unwrap();
    let rows = list["rows"].as_array().unwrap();
    assert!(
        rows.iter()
            .any(|r| r["id"].as_str().unwrap().starts_with("spell:"))
    );
    for row in rows
        .iter()
        .filter(|r| !r["header"].as_bool().unwrap_or(false))
        .take(8)
    {
        first(&h, row["label"].as_str().unwrap());
    }
    assert!(operations(&script).is_empty(), "drawing sends nothing");
}

/// The invoke the last click sent, with its affordance id and row.
fn last_invoke(script: &ScriptHandle) -> Value {
    operations(script).pop().expect("a command")
}

/// `screen` without the affordance `id`: an action the engine does not offer.
fn without(mut screen: Value, id: &str) -> Value {
    let kept = screen["affordances"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|a| a["id"] != id)
        .cloned()
        .collect::<Vec<_>>();
    screen["affordances"] = Value::Array(kept);
    screen
}

#[test]
fn the_sheet_marks_one_tab_selected_and_draws_the_rows_of_every_pane() {
    let screen = example("character sheet");
    let (h, _) = open(screen.clone());
    let tabs = find(&screen["root"], "character:tabs").unwrap()["tabs"]
        .as_array()
        .unwrap()
        .clone();
    assert_eq!(tabs.iter().filter(|t| t["selected"] == true).count(), 1);
    // Like the native sheet, every pane is on screen at once; the selected one is the focused split child.
    let panes = find(&screen["root"], "character:panes").unwrap();
    let selected = tabs.iter().find(|t| t["selected"] == true).unwrap();
    assert_eq!(
        panes["active"],
        format!("character:{}", selected["id"].as_str().unwrap())
    );
    for pane in ["character:stats:rows", "character:encumbrance:rows"] {
        let rows = find(&screen["root"], pane).unwrap()["rows"]
            .as_array()
            .unwrap();
        first(&h, rows[0]["label"].as_str().unwrap());
    }
}

#[test]
fn a_skill_category_is_a_heading_that_sends_nothing_and_a_skill_is_a_row() {
    let screen = example("character sheet");
    let (mut h, script) = open(screen.clone());
    let rows = find(&screen["root"], "character:skills:rows").unwrap()["rows"]
        .as_array()
        .unwrap()
        .clone();
    let heading = rows.iter().find(|r| r["header"] == true).unwrap();
    let skill = rows
        .iter()
        .find(|r| r["id"].as_str().unwrap().starts_with("skill:"))
        .unwrap();
    assert_ne!(
        first(&h, heading["label"].as_str().unwrap())
            .accesskit_node()
            .role(),
        first(&h, skill["label"].as_str().unwrap())
            .accesskit_node()
            .role()
    );
    first(&h, heading["label"].as_str().unwrap()).click();
    h.run_steps(3);
    assert!(operations(&script).is_empty(), "a heading is not chosen");
    first(&h, skill["label"].as_str().unwrap()).click();
    h.run_steps(3);
    let last = last_invoke(&script);
    assert_eq!(last["affordance_id"], "bn:activate:character:focus:skills");
    assert_eq!(last["args"]["row"], skill["id"]);
}

#[test]
fn a_click_on_a_stat_row_moves_the_engines_cursor_to_it_by_the_rows_id() {
    let screen = example("character sheet");
    let (mut h, script) = open(screen.clone());
    let rows = find(&screen["root"], "character:stats:rows").unwrap()["rows"]
        .as_array()
        .unwrap()
        .clone();
    first(&h, rows[1]["label"].as_str().unwrap()).click();
    h.run_steps(3);
    let last = last_invoke(&script);
    assert_eq!(last["affordance_id"], "bn:activate:character:focus:stats");
    assert_eq!(last["args"]["row"], rows[1]["id"]);
}

#[test]
fn the_sheets_own_controls_are_the_engines_affordances_run_by_id() {
    let screen = example("character sheet");
    let (mut h, script) = open(screen);
    first(&h, "Change name").click();
    h.run_steps(3);
    assert_eq!(last_invoke(&script)["affordance_id"], "CHANGE_NAME");
}

#[test]
fn bionics_draw_the_power_the_tabs_the_slots_and_a_row_with_its_key_power_and_state() {
    let screen = example("bionics screen");
    let (h, _) = open(screen.clone());
    let status = find(&screen["root"], "bionics:status").unwrap();
    first(&h, status["stats"][0]["label"].as_str().unwrap());
    for tab in find(&screen["root"], "bionics:tabs").unwrap()["tabs"]
        .as_array()
        .unwrap()
    {
        first(&h, tab["label"].as_str().unwrap());
    }
    first(&h, "Head");
    first(&h, "0/18");
    let row = &find(&screen["root"], "bionics:list").unwrap()["rows"][0];
    // The key is painted beside the label, not part of the row's accessible name.
    assert!(!row["hotkey"].as_str().unwrap().is_empty());
    first(&h, row["label"].as_str().unwrap());
    for column in row["columns"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["value"] != "")
    {
        let value = column["value"].as_str().unwrap();
        assert!(
            h.query_all_by_label_contains(value).next().is_some(),
            "{value}"
        );
    }
}

#[test]
fn a_click_on_the_other_bionics_tab_runs_the_tab_affordance_with_that_tabs_id() {
    let screen = example("bionics screen");
    let (mut h, script) = open(screen);
    first(&h, "PASSIVE").click();
    h.run_steps(3);
    let last = last_invoke(&script);
    assert_eq!(last["affordance_id"], "bionics:tab");
    assert_eq!(last["args"]["row"], "passive");
}

#[test]
fn the_bionics_actions_are_the_engines_and_a_disabled_row_is_not_chosen() {
    let screen = example("bionics screen");
    let (mut h, script) = open(screen.clone());
    let row = &find(&screen["root"], "bionics:list").unwrap()["rows"][0];
    assert_eq!(row["enabled"], false);
    first(&h, row["label"].as_str().unwrap()).click();
    h.run_steps(3);
    assert!(operations(&script).is_empty(), "the engine denied the row");
    first(&h, "Reassign invlet").click();
    h.run_steps(3);
    assert_eq!(last_invoke(&script)["affordance_id"], "REASSIGN");
}

#[test]
fn an_action_the_engine_does_not_offer_has_no_button() {
    let screen = example("bionics screen");
    let (h, _) = open(without(screen, "REASSIGN"));
    assert!(h.query_by_label_contains("Reassign invlet").is_none());
    first(&h, "Toggle activate/examine");
}

#[test]
fn the_mutation_list_shows_each_mutations_state_column_beside_the_other_column() {
    let screen = example("mutation screen");
    let (h, _) = open(screen.clone());
    for id in ["mutations:passive", "mutations:active"] {
        first(
            &h,
            find(&screen["root"], id).unwrap()["title"]["text"]
                .as_str()
                .unwrap_or("Active"),
        );
    }
    let active = &find(&screen["root"], "mutations:active:rows").unwrap()["rows"][0];
    let state = active["columns"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["key"] == "state")
        .unwrap();
    assert!(state["value"].as_str().unwrap().contains("RU"));
}

#[test]
fn a_mutation_action_button_runs_the_engines_affordance() {
    let screen = example("mutation screen");
    let (mut h, script) = open(screen);
    first(&h, "Reassign").click();
    h.run_steps(3);
    assert_eq!(last_invoke(&script)["affordance_id"], "REASSIGN");
}

#[test]
fn a_spell_row_shows_its_hotkey_and_the_cursor_spells_detail_text() {
    let screen = example("spell menu");
    let (h, _) = open(screen.clone());
    let list = find(&screen["root"], "list:choices").unwrap();
    let spell = list["rows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["id"].as_str().unwrap().starts_with("spell:"))
        .unwrap();
    // The key is painted beside the label, not part of the row's accessible name.
    assert!(!spell["hotkey"].as_str().unwrap().is_empty());
    first(&h, spell["label"].as_str().unwrap());
    first(&h, "Poisons the target.");
}

#[test]
fn a_key_row_of_the_spell_menu_is_chosen_by_its_own_row_id() {
    let screen = example("spell menu");
    let (mut h, script) = open(screen.clone());
    let list = find(&screen["root"], "list:choices").unwrap();
    let key = list["rows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["id"].as_str().unwrap().starts_with("action:key:"))
        .unwrap();
    first(&h, key["label"].as_str().unwrap()).click();
    h.run_steps(3);
    let last = last_invoke(&script);
    assert_eq!(last["affordance_id"], list["activate"]);
    assert_eq!(last["args"]["row"], key["id"]);
}
