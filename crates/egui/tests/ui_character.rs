//! The character sheet, the bionics list, the mutation list and the spell menu drawn from their components,
//! from the engine's own examples (`crates/core/tests/fixtures/0.2.examples/character.json`): what the
//! screens show, and that a click on a tab or a row runs the affordance the engine named, with the row's id.
mod common;
use bn_client_egui::transport::{ScriptHandle, ScriptedTransport};
use bn_client_egui::{App, Clock, ConnectOptions, EngineModel};
use common::fixture;
use eframe::egui;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
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
    assert_eq!(last["affordance_id"], "mutations:pick:active");
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
