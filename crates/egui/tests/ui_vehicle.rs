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

fn choice(id: &str, label: &str, edit: Value) -> Value {
    let mut row = json!({
        "id": id, "label": label, "description": "", "denial": "", "enabled": true,
        "selectable": true, "selected": false, "columns": []
    });
    row.as_object_mut()
        .unwrap()
        .extend(edit.as_object().unwrap().clone());
    row
}

fn vehicle(mode: &str, edit: impl FnOnce(&mut Value)) -> Value {
    let mut view = json!({
        "name": "Bicycle", "owner": "Ada", "owned": true, "mode": mode,
        "cursor": {"x": 0, "y": 1}, "blocked": false,
        "cells": [
            {"dx": 0, "dy": 0, "glyph": "o", "color": "light_gray"},
            {"dx": 0, "dy": -1, "glyph": "H", "color": "light_gray"}
        ],
        "parts": [{"name": "frame", "open": "[", "close": "]", "health": 62, "broken": false}],
        "stats": ["Safe/Top Speed:   0/ 18 km/h", "", "Mass:    16 kg"]
    });
    edit(&mut view);
    view
}

fn boundary(title: &str, choices: Vec<Value>, view: Value) -> Value {
    let focus = choices.iter().find(|c| c["highlighted"] == true);
    json!({
        "boundary_id": "b:40",
        "readiness": {"phase": "playing", "game_ready": true,
            "accepts_interaction_commands": true, "accepts_registered_actions": true},
        "actions": [
            {"id": "INSTALL", "name": "install", "keys": ["i"]},
            {"id": "QUIT", "name": "quit", "keys": ["ESC"]}
        ],
        "interaction": {
            "schema_id": "s:veh", "kind": "choices", "context": "VEH_INTERACT", "title": title,
            "message": "", "allow_cancel": true, "allow_set_count": false,
            "choices": choices, "choice_total": 0, "vehicle": view,
            "compat": {"focus": {"choice_id": focus.map(|c| c["id"].clone())}, "panes": []}
        }
    })
}

fn main_screen() -> Value {
    let denied = json!({"enabled": false, "denial": "There are no damaged parts on this vehicle."});
    boundary(
        "Bicycle",
        vec![
            choice("action:INSTALL", "install", json!({})),
            choice("action:REPAIR", "repair", denied),
            choice("action:UP", "Move the cursor: up", json!({})),
            choice("action:DOWN", "Move the cursor: down", json!({})),
        ],
        vehicle("main", |_| {}),
    )
}

fn install_screen() -> Value {
    let unmet = json!({"enabled": false, "denial": "The requirements are not met.",
        "highlighted": true, "color": "c_dark_gray"});
    let view = vehicle("install", |v| {
        v["verb"] = json!("Install");
        v["tabs"] = json!({"labels": ["All", "Cargo"], "selected": 0, "filter": ""});
        v["detail"] = json!({
            "name": "wooden board", "description": "A board mounted on the frame.",
            "time": "30 minutes", "time_heading": "Time required:", "can_do": false,
            "groups": [
                {"kind": "skill", "heading": "Skills required:", "empty_text": "NONE"},
                {"kind": "component", "heading": "Components required:", "empty_text": ""},
                {"kind": "tool", "heading": "Tools required:", "empty_text": ""},
                {"kind": "extra", "heading": "Additional requirements:", "empty_text": ""}
            ],
            "stats": [{"label": "Durability", "value": "120"}],
            "requirements": [
                {"kind": "skill", "text": "mechanics 1", "met": true},
                {"kind": "component", "text": "2x plank", "met": true},
                {"kind": "tool", "text": "hammer OR rock", "met": false}
            ]
        });
    });
    boundary(
        "Choose new part to install here:",
        vec![
            choice("tab:0", "All", json!({"selected": true})),
            choice("tab:1", "Cargo", json!({})),
            choice("part:0", "wooden board", unmet),
            choice("part:1", "wheel", json!({})),
        ],
        view,
    )
}

fn harness<'a>(screen: Value) -> (H<'a>, ScriptHandle) {
    let (t, script) = session();
    let t = t.on("bn.command.submit", vec![fixture("submit_result")]);
    let clock = Clock::manual();
    let model = EngineModel::new(Box::new(t)).with_clock(clock.clone());
    let mut h = Harness::builder()
        .with_size(egui::vec2(1300.0, 800.0))
        .build_ui_state(
            |ui, app: &mut App<EngineModel>| app.show(ui),
            App::new(model).with_clock(clock),
        );
    h.run_steps(3);
    script.push(
        json!({"jsonrpc":"2.0","method":"bn.events","params":{"epoch":"e7f3","events":[{
        "sequence": "41", "revision": "32", "type": "interaction.changed",
        "changes": {"interaction": screen}}]}}),
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
fn the_main_screen_shows_the_parts_on_the_cursor_square_and_the_stat_lines() {
    let (h, _) = harness(main_screen());
    h.get_by_label("[frame]");
    h.get_by_label_contains("Safe/Top Speed:");
    h.get_by_label_contains("Mass:    16 kg");
    h.get_by_label_contains("Bicycle (Ada)");
}

#[test]
fn a_denied_action_is_inert_and_an_offered_one_runs_through_the_engine() {
    let (mut h, script) = harness(main_screen());
    h.get_by_label_contains("repair").click();
    h.run_steps(3);
    assert!(operations(&script).is_empty());
    h.get_by_label_contains("install").click();
    h.run_steps(3);
    assert_eq!(
        operations(&script),
        [json!({"kind":"choose","choice_id":"action:INSTALL"})]
    );
}

#[test]
fn the_cursor_arrows_choose_the_engines_move_rows() {
    let (mut h, script) = harness(main_screen());
    h.get_by_label("Move the cursor: up").click();
    h.run_steps(3);
    assert_eq!(
        operations(&script),
        [json!({"kind":"choose","choice_id":"action:UP"})]
    );
}

#[test]
fn the_requirements_of_the_highlighted_part_are_listed_with_their_status() {
    let (h, _) = harness(install_screen());
    h.get_by_label("Time required:");
    h.get_by_label("> 30 minutes");
    h.get_by_label("> mechanics 1");
    h.get_by_label("> 2x plank");
    h.get_by_label("> hammer OR rock");
    h.get_by_label_contains("A board mounted on the frame.");
}

#[test]
fn headings_and_the_task_verb_are_the_engines_words() {
    let mut screen = install_screen();
    let view = &mut screen["interaction"]["vehicle"];
    view["verb"] = json!("설치");
    view["detail"]["time_heading"] = json!("필요한 시간:");
    view["detail"]["groups"][0]["heading"] = json!("필요한 기술:");
    view["detail"]["requirements"] = json!([{"kind": "tool", "text": "망치", "met": true}]);
    view["detail"]["groups"][0]["empty_text"] = json!("없음");
    let (h, _) = harness(screen);
    h.get_by_label("필요한 시간:");
    h.get_by_label("필요한 기술:");
    h.get_by_label("> 없음");
    h.get_by_label("설치 wooden board");
    assert!(h.query_by_label("Tools required:").is_some());
    assert!(h.query_by_label("Components required:").is_none());
}

#[test]
fn the_part_that_lacks_its_requirements_is_not_started_by_its_button() {
    let (mut h, script) = harness(install_screen());
    h.get_by_label("Install wooden board").click();
    h.run_steps(3);
    assert!(operations(&script).is_empty());
}

#[test]
fn clicking_another_part_chooses_it_through_the_engine() {
    let (mut h, script) = harness(install_screen());
    h.get_by_label("wheel").click();
    h.run_steps(3);
    assert_eq!(
        operations(&script),
        [json!({"kind":"choose","choice_id":"part:1"})]
    );
}

#[test]
fn clicking_a_tab_chooses_it_through_the_engine() {
    let (mut h, script) = harness(install_screen());
    h.get_by_label("Cargo").click();
    h.run_steps(3);
    assert_eq!(
        operations(&script),
        [json!({"kind":"choose","choice_id":"tab:1"})]
    );
}
