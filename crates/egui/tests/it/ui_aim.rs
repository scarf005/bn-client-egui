//! The aim screen the engine publishes: its ways of aiming are buttons over their chances that fire with them, and
//! its creatures are rows that move the cursor, which keep their places as the cursor moves.
use crate::common::{fixture, iso_root, open_client_settings, settle_windows};
use crate::ui_iso3d;
use crate::ui_screen_placement::{example, playing};
use bn_client_egui::transport::ScriptHandle;
use bn_client_egui::{App, EngineModel, GameModel};
use eframe::egui;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use glam::Vec2;
use serde_json::{Value, json};

type H<'a> = Harness<'a, App<EngineModel>>;

/// The look screen as an aim screen: two ways of aiming with their chances, two creatures in sight with the one
/// the cursor is on (`current`) first, as the engine lists them.
fn aim_screen(h: &H, current: usize) -> Value {
    let (origin, me) = (h.state().model.origin(), h.state().model.player());
    let at = |dx: i32, dy: i32| json!({"dim": "", "x": origin.x + me.x + dx, "y": origin.y + me.y + dy, "z": 0});
    let mut screen = example("look", "look-around screen");
    screen["kind"] = json!("target");
    let stat = |key: &str, label: &str, value: &str, unit: &str| json!({"key": key, "label": label, "value": value, "unit": unit});
    screen["root"]["stats"] = json!([
        stat("range", "Range", "3", ""),
        stat("aim:FIRE", "Current Aim", "75", "Moves"),
        stat("aim:FIRE:Great", "Great", "2", "%"),
        stat("aim:AIMED_SHOT", "Regular Aim", "201", "Moves"),
        stat("aim:AIMED_SHOT:Great", "Great", "13", "%"),
    ]);
    let creatures = [("the zombie", at(3, 0)), ("Don Honeycutt", at(-2, -1))];
    let mut candidates = creatures
        .iter()
        .enumerate()
        .map(|(n, (label, position))| {
            json!({"id": format!("target:{n}"), "label": label, "position": position, "creature": true,
                "description": ""})
        })
        .collect::<Vec<_>>();
    candidates.swap(0, current);
    let target = &mut screen["root"]["child"]["target"];
    target["current"] = creatures[current].1.clone();
    target["source"] = at(0, 0);
    target["candidates"] = json!(candidates);
    let key = |id: &str, label: &str| json!({"id": id, "label": label, "keys": [], "enabled": true, "expect": "screen"});
    let affordances = screen["affordances"].as_array_mut().unwrap();
    affordances.push(key("FIRE", "Fire Weapon"));
    affordances.push(key("AIMED_SHOT", "Aimed Shot"));
    screen
}

/// Publishes `screen` over the map as the next change of the interaction.
fn publish(script: &ScriptHandle, step: u64, screen: Value) {
    let map = fixture("subscribe_result")["result"]["interaction"]["stack"][0].clone();
    script.push(
        json!({"jsonrpc": "2.0", "method": "bn.events", "params": {"epoch": "e7f3", "events": [{
        "sequence": (42 + step).to_string(), "revision": (33 + step).to_string(),
        "type": "interaction.changed", "changes": {"interaction": {
        "boundary_id": "b:60",
        "readiness": {"phase": "playing", "game_ready": true, "accepts_interaction_commands": true,
            "accepts_registered_actions": true},
        "actions": [], "interaction": null, "stack": [map, screen]}}}]}}),
    );
}

/// What the client invoked, as the engine received it.
fn invoked(script: &ScriptHandle) -> Vec<Value> {
    let sent = script.sent().into_iter();
    sent.map(|m| m["params"]["operation"].clone())
        .filter(|op| op["affordance_id"].is_string())
        .collect()
}

#[test]
fn a_way_of_aiming_is_a_button_that_fires_with_it() {
    let (mut h, script) = playing(egui::vec2(1280.0, 800.0), None);
    publish(&script, 0, aim_screen(&h, 0));
    h.run_steps(6);
    settle_windows(&mut h);
    assert!(h.query_by_label("Great 13 %").is_some());
    h.get_by_label("Regular Aim").click();
    h.run_steps(4);
    let aimed = invoked(&script);
    assert!(
        aimed.iter().any(|op| op["affordance_id"] == "AIMED_SHOT"),
        "{aimed:?}"
    );
}

#[test]
fn a_creature_row_moves_the_cursor_to_it_and_the_rows_keep_their_places() {
    let (mut h, script) = playing(egui::vec2(1280.0, 800.0), None);
    publish(&script, 0, aim_screen(&h, 0));
    h.run_steps(6);
    settle_windows(&mut h);
    let rows = |h: &H| {
        (
            h.get_by_label("the zombie").rect().top(),
            h.get_by_label("Don Honeycutt").rect().top(),
        )
    };
    let first = rows(&h);
    publish(&script, 1, aim_screen(&h, 1));
    h.run_steps(6);
    settle_windows(&mut h);
    assert_eq!(rows(&h), first, "the rows swapped as the cursor moved");
    h.get_by_label("the zombie").click();
    h.run_steps(4);
    let moved = invoked(&script);
    let to = moved
        .iter()
        .find(|op| op["affordance_id"] == "target:move")
        .unwrap_or_else(|| panic!("{moved:?}"));
    assert_eq!(
        to["args"]["pos"]["x"].as_i64(),
        aim_screen(&h, 0)["root"]["child"]["target"]["current"]["x"].as_i64()
    );
}

#[test]
fn the_first_person_eye_turns_to_the_creature_the_aim_cursor_is_on() {
    let root = iso_root("aim-turn");
    let (mut h, script) = ui_iso3d::harness(&root, &root.join("settings.json"), true);
    ui_iso3d::settle(&mut h);
    open_client_settings(&mut h);
    h.get_by_label("First person").click();
    h.run_steps(3);
    h.get_by_label("Close window").click();
    h.run_steps(3);
    // The zombie is three squares east, Don Honeycutt two west and one north.
    for (step, (n, (dx, dy))) in [(0, (3.0, 0.0)), (1, (-2.0, -1.0))].into_iter().enumerate() {
        publish(&script, step as u64, aim_screen(&h, n));
        h.run_steps(6);
        let yaw = h.state().camera().yaw;
        let ahead = Vec2::new(-yaw.cos(), -yaw.sin());
        let off = ahead.angle_to(Vec2::new(dx, dy));
        assert!(off.abs() < 0.05, "the eye is {off} rad off ({dx}, {dy})");
    }
}
