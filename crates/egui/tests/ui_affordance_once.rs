#![recursion_limit = "512"]
//! Every list screen of the engine's examples draws each of its affordances as one button at most: a row action
//! runs from the rows, the cancel is one button or the window's close, never a second button beside it.
mod common;
use bn_client_egui::protocol::{Affordance, AffordanceRole, Component, Screen};
use bn_client_egui::{App, EngineModel, markup};
use common::{fixture, session};
use eframe::egui;
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use serde_json::{Value, json};

type H<'a> = Harness<'a, App<EngineModel>>;

/// Every example screen that holds a list, by file and name.
fn list_screens() -> Vec<(String, Value)> {
    let dir = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../core/tests/fixtures/0.2.examples"
    );
    let mut found = vec![];
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        let doc = serde_json::from_str::<Value>(&std::fs::read_to_string(&path).unwrap()).unwrap();
        for ex in doc["valid"].as_array().into_iter().flatten() {
            let lists = serde_json::to_string(&ex["value"]["root"])
                .unwrap()
                .contains(r#""kind":"list""#);
            if ex["def"] == "screen" && lists {
                let name = format!("{}: {}", path.display(), ex["name"]);
                found.push((name, ex["value"].clone()));
            }
        }
    }
    found
}

/// A game in progress with `screen` on top of the map.
fn showing<'a>(screen: Value) -> H<'a> {
    let (t, script) = session();
    let mut h = Harness::builder()
        .with_size(egui::vec2(1600.0, 1000.0))
        .build_ui_state(
            |ui, app: &mut App<EngineModel>| app.show(ui),
            App::new(EngineModel::new(Box::new(t))),
        );
    h.run_steps(3);
    let map = fixture("subscribe_result")["result"]["interaction"]["stack"][0].clone();
    script.push(json!({"jsonrpc": "2.0", "method": "bn.events", "params": {"epoch": "e7f3", "events": [{
        "sequence": "41", "revision": "32", "type": "interaction.changed", "changes": {"interaction": {
        "boundary_id": "b:60",
        "readiness": {"phase": "playing", "game_ready": true, "accepts_interaction_commands": true,
            "accepts_registered_actions": true},
        "actions": [], "interaction": null, "stack": [map, screen]}}}]}}));
    h.run_steps(8);
    h
}

/// The button an affordance is drawn as: its label, run by the first key bound to it.
fn button_label(a: &Affordance) -> (String, Option<String>) {
    (markup::plain(&a.label.shown()), a.keys.first().cloned())
}

/// The actions the rows of the screen's lists run.
fn row_actions(c: &Component, found: &mut Vec<String>) {
    if let Component::List(list) = c {
        found.extend(list.activate.clone());
    }
    c.children().into_iter().for_each(|c| row_actions(c, found));
}

#[test]
fn no_list_screen_draws_a_row_action_or_a_second_cancel_as_a_button() {
    let screens = list_screens();
    assert!(screens.len() >= 10, "{} list screens", screens.len());
    for (name, value) in screens {
        let screen = serde_json::from_value::<Screen>(value.clone()).unwrap();
        let h = showing(value);
        let buttons = h
            .get_all_by_role(egui::accesskit::Role::Button)
            .filter_map(|b| {
                let node = b.accesskit_node();
                let shortcut = node.data().keyboard_shortcut().map(str::to_string);
                node.label().map(|l| (l.to_string(), shortcut))
            })
            .collect::<Vec<_>>();
        let drawn = |a: &Affordance| buttons.iter().filter(|b| **b == button_label(a)).count();
        let mut rows = vec![];
        row_actions(screen.root.as_ref().unwrap(), &mut rows);
        for a in screen.affordances.iter().filter(|a| rows.contains(&a.id)) {
            assert_eq!(
                drawn(a),
                0,
                "{name}: the row action {} in {buttons:?}",
                a.id
            );
        }
        for a in screen
            .affordances
            .iter()
            .filter(|a| a.role == Some(AffordanceRole::Cancel))
        {
            assert!(drawn(a) <= 1, "{name}: {} twice in {buttons:?}", a.id);
        }
    }
}
