//! The sidebar follows the game's panel options: side, width, the order and enabled set of the panels,
//! the engine's panel labels, the compass and the direction of a heard sound.
mod common;
use bn_client_egui::transport::ScriptHandle;
use bn_client_egui::{App, EngineModel, GameModel};
use common::{fixture, session};
use eframe::egui;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use serde_json::{Value, json};

type H<'a> = Harness<'a, App<EngineModel>>;

fn example(name: &str) -> Value {
    let text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../core/tests/fixtures/0.2.examples/world.json"
    ))
    .unwrap();
    let all: Value = serde_json::from_str(&text).unwrap();
    let mut found = all["valid"].as_array().unwrap().iter();
    found.find(|e| e["name"] == name).unwrap()["value"].clone()
}

fn panel(id: &str, enabled: bool, height: Option<u32>) -> Value {
    let mut p = json!({"id": id, "label": format!("<{id}>"), "enabled": enabled});
    if let Some(rows) = height {
        p["height"] = json!(rows);
    }
    p
}

/// A window whose avatar has a sidebar in `side` of `width` columns holding `panels`, and the compass.
fn open<'a>(side: &str, width: u32, panels: &[Value]) -> (H<'a>, ScriptHandle) {
    let (t, script) = session();
    let mut h = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_ui_state(
            |ui, app: &mut App<EngineModel>| app.show(ui),
            App::new(EngineModel::new(Box::new(t))),
        );
    h.run_steps(3);
    let mut event = fixture("events_41_walk");
    let mut avatar = example("avatar with creatures on the compass");
    avatar["sidebar"]["layout"] =
        json!({"id": "labels", "side": side, "width": width, "panels": panels});
    event["params"]["events"][0]["changes"]["avatar"] = avatar;
    script.push(event);
    h.run_steps(5);
    (h, script)
}

fn shown<'a>(side: &str, width: u32, panels: &[Value]) -> H<'a> {
    open(side, width, panels).0
}

fn all_panels() -> Vec<Value> {
    vec![
        panel("Health", true, Some(7)),
        panel("Location", true, Some(1)),
        panel("Compass", true, Some(8)),
        panel("Weapon", false, Some(1)),
        panel("Log", true, None),
    ]
}

#[test]
fn the_sidebar_sits_on_the_side_the_game_options_name() {
    let right = shown("right", 44, &all_panels());
    let left = shown("left", 44, &all_panels());
    let middle = 700.0;
    assert!(right.get_by_label("<Health>").rect().min.x > middle);
    assert!(left.get_by_label("<Health>").rect().max.x < middle);
}

#[test]
fn the_width_follows_the_native_column_count() {
    let narrow = shown("right", 32, &all_panels());
    let wide = shown("right", 60, &all_panels());
    let left_of = |h: &H| h.get_by_label("<Health>").rect().min.x;
    assert!(
        left_of(&wide) < left_of(&narrow) - 100.0,
        "{} vs {}",
        left_of(&wide),
        left_of(&narrow)
    );
}

#[test]
fn the_panels_come_in_the_games_order_under_its_labels_and_a_disabled_one_is_missing() {
    let h = shown("right", 44, &all_panels());
    let top = |label: &str| h.get_by_label(label).rect().min.y;
    assert!(top("<Health>") < top("<Location>"));
    assert!(top("<Location>") < top("<Compass>"));
    assert!(h.query_by_label("<Weapon>").is_none());
    let mut reversed = all_panels();
    reversed.swap(0, 2);
    let h = shown("right", 44, &reversed);
    assert!(h.get_by_label("<Compass>").rect().min.y < h.get_by_label("<Health>").rect().min.y);
}

#[test]
fn a_panel_the_client_cannot_draw_is_not_invented() {
    let mut panels = all_panels();
    panels.push(panel("Armor", true, Some(5)));
    let h = shown("right", 44, &panels);
    assert!(h.query_by_label("<Armor>").is_none());
}

#[test]
fn the_compass_lists_each_direction_and_its_creatures_nearest_first() {
    let h = shown("right", 44, &all_panels());
    let top = |label: &str| h.get_by_label(label).rect().min.y;
    assert!(top("East:") > top("<Compass>"));
    assert!(top("zombie 4") > top("East:"));
    assert!(top("Rae 9") > top("zombie 4"));
    assert!(top("center") > top("Rae 9"));
    assert!(top("bat 1") > top("center"));
}

#[test]
fn the_message_log_follows_its_panel_under_its_label() {
    let h = shown("right", 44, &all_panels());
    assert!(h.query_by_label("<Log>").is_some());
    assert!(h.query_by_label("Message log").is_none());
    let mut off = all_panels();
    off[4]["enabled"] = json!(false);
    assert!(shown("right", 44, &off).query_by_label("<Log>").is_none());
}

/// The item lists are the ones the engine names beside the map, under its headings and with its verbs; a
/// sidebar without the game's panel options has no heading the engine names.
#[test]
fn the_item_lists_carry_the_engines_headings_and_verbs() {
    let (t, script) = session();
    let mut h = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_ui_state(
            |ui, app: &mut App<EngineModel>| app.show(ui),
            App::new(EngineModel::new(Box::new(t))),
        );
    h.run_steps(3);
    let mut avatar = example("avatar with creatures on the compass");
    avatar["sidebar"].as_object_mut().unwrap().remove("layout");
    avatar["inventory"] = fixture("subscribe_result")["result"]["avatar"]["inventory"].clone();
    let mut interaction = fixture("subscribe_result")["result"]["interaction"].clone();
    interaction["stack"][0]["root"]["item_lists"] = json!([
        {"id": "avatar.ground", "label": "바닥", "verb": "줍기"},
        {"id": "avatar.inventory", "label": "소지품", "verb": "버리기"}
    ]);
    let mut event = fixture("events_41_walk");
    event["params"]["events"][0]["changes"]["avatar"] = avatar;
    event["params"]["events"][0]["changes"]["interaction"] = interaction;
    script.push(event);
    h.run_steps(5);
    for shown in ["바닥", "소지품", "버리기 flashlight"] {
        assert!(h.query_by_label(shown).is_some(), "{shown}");
    }
    for client in [
        "Ground",
        "Inventory",
        "Drop flashlight",
        "Status",
        "Message log",
    ] {
        assert!(h.query_by_label(client).is_none(), "{client}");
    }
}

#[test]
fn a_heard_sound_shows_the_engines_direction_name_and_distance() {
    let (mut h, script) = open("right", 44, &all_panels());
    let heard = |seq: u32, data: Value| {
        json!({"jsonrpc": "2.0", "method": "bn.events", "params": {"epoch": "e7f3", "events": [{
            "sequence": seq.to_string(), "revision": "32", "type": "sound.heard", "data": data}]}})
    };
    script.push(heard(
        42,
        json!({"text": "a loud explosion", "volume": 74, "logged": false, "distance": 12,
            "direction": {"id": "southeast", "name": "southeast"}}),
    ));
    script.push(heard(
        43,
        json!({"text": "Tick.", "volume": 52, "logged": false, "distance": 3}),
    ));
    h.run_steps(4);
    h.get_by_label("a loud explosion (southeast, 12)");
    h.get_by_label("Tick. (3)");
}

#[test]
fn the_clients_own_results_show_in_the_status_line_when_the_log_panel_is_off() {
    let mut off = all_panels();
    off[4]["enabled"] = json!(false);
    let mut h = shown("right", 44, &off);
    h.state_mut().model.say("Nothing to pick up.".into());
    h.run_steps(3);
    h.get_by_label("Nothing to pick up.");
}
