//! The character creation and the world creation drawn from their components, from the engine's own
//! examples (`crates/core/tests/fixtures/0.2.examples/newgame.json`): what the tabs and lists show, and that a
//! click on a tab or a row runs the affordance the engine named, with the tab's or the row's id.
use crate::common::fixture;
use bn_client_egui::transport::{ScriptHandle, ScriptedTransport};
use bn_client_egui::{App, Clock, ConnectOptions, EngineModel};
use eframe::egui;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use serde_json::{Value, json};

type H<'a> = Harness<'a, App<EngineModel>>;

fn example(name: &str) -> Value {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../core/tests/fixtures/0.2.examples/newgame.json"
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
    subscribe["result"]["interaction"]["stack"] = json!([screen]);
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

/// The invoke the last click sent.
fn last_invoke(script: &ScriptHandle) -> Value {
    script
        .sent()
        .into_iter()
        .filter(|m| m["method"] == "bn.command.submit")
        .map(|m| m["params"]["operation"].clone())
        .next_back()
        .expect("a command")
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
fn the_character_creation_draws_every_tab_and_a_click_on_one_runs_the_tab_affordance_with_its_id() {
    let screen = example("character creation on the traits page");
    let (mut h, script) = open(screen.clone());
    let tabs = find(&screen["root"], "newchar:tabs").unwrap();
    for tab in tabs["tabs"].as_array().unwrap() {
        first(&h, tab["label"].as_str().unwrap());
    }
    let skills = tabs["tabs"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["id"] == "skills")
        .unwrap();
    first(&h, skills["label"].as_str().unwrap()).click();
    h.run_steps(3);
    let last = last_invoke(&script);
    assert_eq!(last["kind"], "invoke");
    assert_eq!(last["affordance_id"], tabs["select"]);
    assert_eq!(last["args"]["row"], "skills");
}

#[test]
fn a_double_click_on_a_trait_picks_it_by_the_traits_identity() {
    let screen = example("character creation on the traits page");
    let (mut h, script) = open(screen.clone());
    let list = find(&screen["root"], "newchar:traits:advantage:rows").unwrap();
    // Not the row under the cursor: its name shows again in the cost line.
    let row = list["rows"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["enabled"] != false)
        .nth(1)
        .unwrap();
    assert!(row["id"].as_str().unwrap().starts_with("trait:"));
    let at = first(&h, row["label"].as_str().unwrap()).rect().center();
    crate::common::double_click(&mut h, at);
    h.run_steps(3);
    let last = last_invoke(&script);
    assert_eq!(last["affordance_id"], list["activate"]);
    assert_eq!(last["args"]["row"], row["id"]);
}

#[test]
fn the_profession_list_draws_its_rows_and_a_double_click_picks_one_by_identity() {
    let screen = example("character creation on the profession page");
    let (mut h, script) = open(screen.clone());
    let list = find(&screen["root"], "newchar:profession:list:rows").unwrap();
    // Not the row under the cursor: its name shows again in the cost line.
    let row = &list["rows"][1];
    assert!(row["id"].as_str().unwrap().starts_with("profession:"));
    let at = first(&h, row["label"].as_str().unwrap()).rect().center();
    crate::common::double_click(&mut h, at);
    h.run_steps(3);
    let last = last_invoke(&script);
    assert_eq!(last["affordance_id"], "bn:activate:newchar:profession:pick");
    assert_eq!(last["args"]["row"], row["id"]);
}

#[test]
fn the_world_creation_draws_its_tabs_and_a_click_on_one_runs_the_tab_affordance_with_its_id() {
    let screen = example("world creation on the mods page");
    let (mut h, script) = open(screen.clone());
    let tabs = find(&screen["root"], "worldgen:tabs").unwrap();
    for tab in tabs["tabs"].as_array().unwrap() {
        first(&h, tab["label"].as_str().unwrap());
    }
    let confirm = tabs["tabs"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["id"] == "confirm")
        .unwrap();
    first(&h, confirm["label"].as_str().unwrap()).click();
    h.run_steps(3);
    let last = last_invoke(&script);
    assert_eq!(last["affordance_id"], tabs["select"]);
    assert_eq!(last["args"]["row"], "confirm");
}

#[test]
fn the_world_name_is_a_field_the_engine_offers_to_fill() {
    let screen = example("world creation on the finalize page");
    let (h, _) = open(screen.clone());
    let form = find(&screen["root"], "worldgen:confirm:name").unwrap();
    let field = &form["fields"][0];
    assert_eq!(field["id"], "name");
    first(&h, field["label"].as_str().unwrap());
}

#[test]
fn a_click_on_the_text_of_a_creation_screen_sends_nothing() {
    let screen = example("character creation on the traits page");
    let (mut h, script) = open(screen.clone());
    let panel = find(&screen["root"], "newchar:traits:notes").unwrap();
    // The first note is the points the player has left, in colour tags; the shown text is plain.
    assert_eq!(panel["stats"][0]["key"], "points_left");
    first(&h, "Points left").click();
    h.run_steps(3);
    assert!(
        script
            .sent()
            .iter()
            .all(|m| m["method"] != "bn.command.submit")
    );
}
