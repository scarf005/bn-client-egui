//! The keyboard goes first to the window that has it: arrows move a window's own cursor and `<` `>` change the
//! map window's level, and only the keys a window does not use go on to the game. A window that opens takes the
//! keyboard, a click in the map gives it back.
use crate::common::{fixture, panel_view};
use crate::ui_panels::{key, operations, panel_reads, serve, serve_with, window};
use bn_client_egui::transport::ScriptHandle;
use bn_client_egui::{App, Clock, EngineModel};
use eframe::egui;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use serde_json::{Value, json};

type H<'a> = Harness<'a, App<EngineModel>>;

/// A game offering the character sheet (`@`), the inventory (`i`), the map (`m`), the stairs (`<` and `>`) and
/// the steps, whose map answers every read with a 3 by 3 overmap.
pub(crate) fn game<'a>() -> (H<'a>, ScriptHandle) {
    game_on_level(0)
}

/// `game`, its map answering every read with the squares of level `z`.
fn game_on_level<'a>(z: i64) -> (H<'a>, ScriptHandle) {
    game_in(egui::vec2(1600.0, 1000.0), z)
}

/// `game_on_level(z)` in a window of `size`.
pub(crate) fn game_in<'a>(size: egui::Vec2, z: i64) -> (H<'a>, ScriptHandle) {
    let mut subscribed = fixture("subscribe_result");
    subscribed["result"]["interaction"]["actions"]
        .as_array_mut()
        .unwrap()
        .extend([
            json!({"id": "player_data", "name": "View Player Info", "keys": ["@"]}),
            json!({"id": "inventory", "name": "Open Inventory", "keys": ["i"]}),
            json!({"id": "map", "name": "View Map", "keys": ["m"]}),
            json!({"id": "LEVEL_UP", "name": "Ascend Stairs", "keys": ["<"]}),
            json!({"id": "LEVEL_DOWN", "name": "Descend Stairs", "keys": [">"]}),
        ]);
    let reply = vec![
        subscribed,
        fixture("snapshot_part_0"),
        fixture("snapshot_part_1"),
    ];
    let square = json!([".", 2, 0]);
    let row = json!([square, square, square]);
    let squares = |z: i64| {
        json!({"jsonrpc": "2.0", "id": 0, "result": {
            "origin": {"dim": "", "x": 0, "y": 0, "z": z}, "player": {"dim": "", "x": 1, "y": 1, "z": z},
            "cols": 3, "rows": 3, "cells": [row, row, row], "legend": [],
            "cursor_description": "", "notes": [], "path": []}})
    };
    let (t, script) = bn_client_egui::transport::ScriptedTransport::new();
    let mut t = t
        .on("bn.hello", vec![fixture("hello_result")])
        .on("bn.subscribe", reply.clone())
        .on("bn.subscribe", reply)
        .on("bn.command.submit", vec![fixture("submit_result")]);
    for _ in 0..40 {
        t = t.on("bn.overmap.cells", vec![squares(z)]);
    }
    let clock = Clock::manual();
    let model = EngineModel::new(Box::new(t)).with_clock(clock.clone());
    let mut h = Harness::builder().with_size(size).build_ui_state(
        |ui, app: &mut App<EngineModel>| app.show(ui),
        App::new(model).with_clock(clock),
    );
    h.run_steps(3);
    (h, script)
}

/// The `bn.overmap.cells` reads so far, as the `min` corner each asked for.
fn overmap_reads(script: &ScriptHandle) -> Vec<Value> {
    script
        .sent()
        .into_iter()
        .filter(|m| m["method"] == "bn.overmap.cells")
        .map(|m| m["params"]["min"].clone())
        .collect()
}

fn press(h: &mut H, key: egui::Key) {
    h.key_press(key);
    h.run_steps(3);
}

/// The inventory of the fixtures with a second item under the first.
fn two_items(panel: &str) -> Value {
    let mut view = panel_view(panel);
    if panel == "inventory" {
        let rows = &mut view["root"]["child"]["children"][0]["child"]["rows"];
        let mut second = rows[1].clone();
        second["id"] = json!("item:3");
        second["label"] = json!("stick (1)");
        rows.as_array_mut().unwrap().push(second);
    }
    view
}

#[test]
fn down_in_the_inventory_window_moves_its_cursor_and_not_the_player() {
    let (mut h, script) = game();
    let mut answered = 0;
    key(&mut h, "i");
    serve_with(&mut h, &script, &mut answered, &two_items);
    press(&mut h, egui::Key::ArrowDown);
    press(&mut h, egui::Key::ArrowDown);
    serve_with(&mut h, &script, &mut answered, &two_items);
    assert_eq!(
        panel_reads(&script).pop().unwrap()["params"]["row"],
        "item:3",
        "the second Down is on the second item"
    );
    assert!(operations(&script).is_empty(), "no key reached the game");
}

#[test]
fn up_in_the_character_window_moves_its_cursor_and_not_the_player() {
    let (mut h, script) = game();
    let mut answered = 0;
    key(&mut h, "@");
    serve(&mut h, &script, &mut answered);
    press(&mut h, egui::Key::ArrowUp);
    serve(&mut h, &script, &mut answered);
    assert!(operations(&script).is_empty(), "no key reached the game");
    assert!(
        panel_reads(&script).last().unwrap()["params"]["row"].is_string(),
        "the sheet is read again with the row the cursor moved to"
    );
}

#[test]
fn level_keys_in_the_map_window_change_its_level_and_climb_no_stairs() {
    let (mut h, script) = game();
    key(&mut h, "m");
    h.run_steps(4);
    let z = |script: &ScriptHandle| overmap_reads(script).last().unwrap()["z"].clone();
    let first = z(&script);
    key(&mut h, ">");
    h.run_steps(4);
    assert_eq!(
        z(&script),
        json!(first.as_i64().unwrap() - 1),
        "one level down"
    );
    key(&mut h, "<");
    key(&mut h, "<");
    h.run_steps(4);
    assert_eq!(
        z(&script),
        json!(first.as_i64().unwrap() + 1),
        "two levels up"
    );
    assert!(operations(&script).is_empty(), "no stairs were taken");
}

#[test]
fn the_map_window_names_the_level_of_the_squares_it_shows() {
    let (mut h, _) = game_on_level(3);
    key(&mut h, "m");
    h.run_steps(4);
    h.get_by_label("Level 3");
}

#[test]
fn arrows_in_the_map_window_pan_it_and_move_no_player() {
    let (mut h, script) = game();
    key(&mut h, "m");
    h.run_steps(4);
    let x = |script: &ScriptHandle| overmap_reads(script).last().unwrap()["x"].as_i64().unwrap();
    let first = x(&script);
    press(&mut h, egui::Key::ArrowRight);
    h.run_steps(4);
    assert_eq!(x(&script), first + 1);
    assert!(operations(&script).is_empty(), "no step was taken");
}

#[test]
fn a_key_the_focused_window_does_not_use_goes_to_the_game() {
    let (mut h, script) = game();
    let mut answered = 0;
    key(&mut h, "i");
    serve(&mut h, &script, &mut answered);
    key(&mut h, "k");
    assert_eq!(
        operations(&script),
        [json!({"kind": "action", "action_id": "UP"})]
    );
}

#[test]
fn a_click_on_the_map_gives_the_arrows_back_to_the_game() {
    let (mut h, script) = game();
    let mut answered = 0;
    key(&mut h, "i");
    serve(&mut h, &script, &mut answered);
    // A press on the map, which a travel would only follow once released.
    let at = h.get_by_label("Map").rect().center();
    h.event(egui::Event::PointerMoved(at));
    h.event(egui::Event::PointerButton {
        pos: at,
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers: egui::Modifiers::NONE,
    });
    h.run_steps(3);
    press(&mut h, egui::Key::ArrowUp);
    assert_eq!(
        operations(&script),
        [json!({"kind": "action", "action_id": "UP"})]
    );
}

#[test]
fn a_click_in_a_window_takes_the_arrows_from_the_window_opened_before_it() {
    let (mut h, script) = game();
    let mut answered = 0;
    key(&mut h, "@");
    serve(&mut h, &script, &mut answered);
    key(&mut h, "i");
    serve(&mut h, &script, &mut answered);
    h.get_by_label("Dexterity:").click();
    h.run_steps(3);
    serve(&mut h, &script, &mut answered);
    let reads = panel_reads(&script).len();
    press(&mut h, egui::Key::ArrowDown);
    serve(&mut h, &script, &mut answered);
    let again = &panel_reads(&script)[reads..];
    assert!(
        !again.is_empty() && again.iter().all(|r| r["params"]["panel"] == "character"),
        "the sheet has the arrows, not the inventory opened after it: {again:?}"
    );
    assert_eq!(operations(&script), Vec::<Value>::new());
}

#[test]
fn escape_closes_the_focused_window_and_sends_nothing() {
    let (mut h, script) = game();
    let mut answered = 0;
    key(&mut h, "i");
    serve(&mut h, &script, &mut answered);
    window(&h, "Inventory");
    h.key_press(egui::Key::Escape);
    h.run_steps(3);
    assert!(
        h.query_by_role_and_label(egui::accesskit::Role::Window, "Inventory")
            .is_none()
    );
    assert!(operations(&script).is_empty());
}

#[test]
fn arrows_in_the_client_settings_window_move_no_player() {
    let (mut h, script) = game();
    h.get_by_label("Client settings").click();
    h.run_steps(4);
    press(&mut h, egui::Key::ArrowDown);
    press(&mut h, egui::Key::ArrowUp);
    assert!(operations(&script).is_empty(), "no key reached the game");
}

#[test]
fn every_navigation_key_of_every_client_window_stays_with_the_window() {
    let keys = [
        egui::Key::ArrowUp,
        egui::Key::ArrowDown,
        egui::Key::ArrowLeft,
        egui::Key::ArrowRight,
        egui::Key::PageUp,
        egui::Key::PageDown,
        egui::Key::Home,
        egui::Key::End,
    ];
    for window in ["@", "i", "m", "Client settings"] {
        let (mut h, script) = game();
        let mut answered = 0;
        match window {
            "Client settings" => h.get_by_label(window).click(),
            _ => key(&mut h, window),
        }
        serve_with(&mut h, &script, &mut answered, &two_items);
        for k in keys {
            press(&mut h, k);
        }
        serve_with(&mut h, &script, &mut answered, &two_items);
        assert!(
            operations(&script).is_empty(),
            "a navigation key of the {window:?} window reached the game"
        );
    }
}

/// The engine's yes or no popup opened over the map, as an event.
fn open_a_popup(h: &mut H, script: &ScriptHandle) {
    open_screen(h, script, "query popup screen");
}

/// The engine's example screen `name` opened over the map, as an event.
fn open_screen(h: &mut H, script: &ScriptHandle, name: &str) {
    script.push(fixture("events_41_walk"));
    h.run_steps(4);
    let map = fixture("subscribe_result")["result"]["interaction"]["stack"][0].clone();
    let popup = crate::ui_window_class::example("generic", name);
    script.push(json!({"jsonrpc": "2.0", "method": "bn.events", "params": {"epoch": "e7f3", "events": [{
        "sequence": "42", "revision": "33", "type": "interaction.changed", "changes": {"interaction": {
        "boundary_id": "b:60",
        "readiness": {"phase": "playing", "game_ready": true, "accepts_interaction_commands": true,
            "accepts_registered_actions": true},
        "actions": [], "interaction": null, "stack": [map, popup]}}}]}}));
    h.run_steps(6);
}

/// A press on the title bar of the window titled `title`.
fn click_window(h: &mut H, title: &str) {
    let window = h
        .get_by_role_and_label(egui::accesskit::Role::Window, title)
        .rect();
    let at = window.left_top() + egui::vec2(40.0, 12.0);
    h.event(egui::Event::PointerMoved(at));
    h.event(egui::Event::PointerButton {
        pos: at,
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers: egui::Modifiers::NONE,
    });
    h.run_steps(3);
}

#[test]
fn down_in_the_clicked_inventory_window_moves_its_cursor_while_an_engine_screen_is_open() {
    let (mut h, script) = game();
    let mut answered = 0;
    key(&mut h, "i");
    serve_with(&mut h, &script, &mut answered, &two_items);
    open_a_popup(&mut h, &script);
    click_window(&mut h, "Inventory");
    let sent = operations(&script).len();
    press(&mut h, egui::Key::ArrowDown);
    press(&mut h, egui::Key::ArrowDown);
    serve_with(&mut h, &script, &mut answered, &two_items);
    assert_eq!(
        panel_reads(&script).pop().unwrap()["params"]["row"],
        "item:3",
        "the second Down is on the second item"
    );
    assert_eq!(operations(&script).len(), sent, "no key reached the screen");
}

#[test]
fn an_engine_screen_that_opens_takes_the_keys_from_a_client_window() {
    let (mut h, script) = game();
    let mut answered = 0;
    key(&mut h, "i");
    serve_with(&mut h, &script, &mut answered, &two_items);
    open_a_popup(&mut h, &script);
    press(&mut h, egui::Key::ArrowDown);
    serve_with(&mut h, &script, &mut answered, &two_items);
    assert!(
        panel_reads(&script)
            .iter()
            .all(|r| r["params"]["row"].is_null()),
        "the inventory window did not move its cursor"
    );
}

#[test]
fn the_keys_of_the_client_settings_window_reach_no_engine_screen_under_it() {
    let (mut h, script) = game();
    open_a_popup(&mut h, &script);
    h.get_by_label("Client settings").click();
    h.run_steps(4);
    let sent = operations(&script).len();
    for k in [
        egui::Key::ArrowDown,
        egui::Key::ArrowUp,
        egui::Key::Tab,
        egui::Key::Enter,
    ] {
        press(&mut h, k);
    }
    assert_eq!(operations(&script).len(), sent, "no key reached the screen");
}

#[test]
fn the_search_box_of_a_screen_keeps_the_focus_it_takes_while_the_client_settings_window_is_open() {
    let (mut h, script) = game();
    open_screen(&mut h, &script, "filterable list screen");
    h.get_by_label("Client settings").click();
    h.run_steps(4);
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::F);
    h.run_steps(6);
    assert!(h.ctx.text_edit_focused(), "the search box has the focus");
}

#[test]
fn the_client_settings_window_opens_again_where_the_player_moved_it() {
    let (mut h, _script) = game();
    let window = |h: &H| {
        h.get_by_role_and_label(egui::accesskit::Role::Window, "Client settings")
            .rect()
    };
    h.get_by_role_and_label(egui::accesskit::Role::Button, "Client settings")
        .click();
    h.run_steps(4);
    let from = window(&h).left_top() + egui::vec2(40.0, 12.0);
    let to = from + egui::vec2(-60.0, 40.0);
    h.hover_at(from);
    h.run_steps(2);
    for (at, pressed) in [(from, true), (to, false)] {
        h.hover_at(at);
        h.run_steps(2);
        h.event(egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        });
        h.run_steps(2);
    }
    let moved = window(&h);
    h.get_by_role_and_label(egui::accesskit::Role::Button, "Client settings")
        .click();
    h.run_steps(4);
    h.get_by_role_and_label(egui::accesskit::Role::Button, "Client settings")
        .click();
    h.run_steps(4);
    assert_eq!(window(&h), moved);
}
