//! The screens of the 0.2 stack drawn from their components, from the engine's own examples
//! (`crates/core/tests/fixtures/0.2.examples/`): one test per component kind, then the affordances and keys.
mod common;
use bn_client_egui::transport::{ScriptHandle, ScriptedTransport};
use bn_client_egui::{App, Clock, ConnectOptions, EngineModel};
use common::fixture;
use eframe::egui;
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use serde_json::{Value, json};

type H<'a> = Harness<'a, App<EngineModel>>;

/// The value of the example `name` in `file` of the vendored 0.2 examples.
fn example(file: &str, name: &str) -> Value {
    let path = format!(
        "{}/../core/tests/fixtures/0.2.examples/{file}",
        env!("CARGO_MANIFEST_DIR")
    );
    let all = serde_json::from_str::<Value>(&std::fs::read_to_string(path).unwrap()).unwrap();
    let found = all["valid"].as_array().unwrap().iter();
    found
        .into_iter()
        .find(|e| e["name"] == name)
        .unwrap_or_else(|| panic!("no example {name}"))["value"]
        .clone()
}

fn affordance(id: &str, keys: &[&str], args: Value) -> Value {
    json!({"id": id, "label": id, "keys": keys, "enabled": true, "expect": "boundary", "args": args})
}

/// A dialog over the map with `root`, in a 0.2 session whose subscribe answer carries it.
fn dialog(root: Value, affordances: Vec<Value>) -> Value {
    json!({"screen_id": "screen:9", "role": "dialog", "title": "Test", "root": root,
        "affordances": affordances})
}

fn harness<'a>(screen: Value) -> (H<'a>, ScriptHandle) {
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
    let options = ConnectOptions::default();
    let model = EngineModel::open(Box::new(t), options).with_clock(clock.clone());
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1100.0, 800.0))
        .build_ui_state(
            |ui, app: &mut App<EngineModel>| app.show(ui),
            App::new(model).with_clock(clock),
        );
    harness.run_steps(4);
    (harness, script)
}

fn submits(script: &ScriptHandle) -> Vec<Value> {
    script
        .sent()
        .into_iter()
        .filter(|m| m["method"] == "bn.command.submit")
        .map(|m| m["params"].clone())
        .collect()
}

fn disabled(h: &H, label: &str) -> bool {
    h.get_by_label(label).accesskit_node().is_disabled()
}

/// A command for an affordance expecting the boundary the session is at.
fn boundary_invoke(id: &str, args: Value) -> Value {
    json!({"epoch": "e7f3", "expect": {"boundary_id": "b:12", "revision": "31"},
        "operation": {"kind": "invoke", "affordance_id": id, "args": args}})
}

fn screen_invoke(id: &str) -> Value {
    json!({"epoch": "e7f3", "expect": {"screen_id": "screen:9"},
        "operation": {"kind": "invoke", "affordance_id": id, "args": {}}})
}

fn rows_list(ids: &[&str]) -> Value {
    let rows = ids.iter().enumerate().map(|(i, id)| {
        json!({"id": id, "label": id, "description": "", "denial": "", "enabled": true,
            "selectable": true, "selected": i == 0, "columns": []})
    });
    json!({"id": "list", "kind": "list", "columns": [], "rows": rows.collect::<Vec<_>>(),
        "total": ids.len(), "selection": "single", "activate": "pick"})
}

fn pick_list(extra: Vec<Value>) -> Value {
    let pick = affordance(
        "pick",
        &["RETURN"],
        json!({"row": {"component_id": "list"}}),
    );
    dialog(rows_list(&["a", "b", "c"]), [vec![pick], extra].concat())
}

#[test]
fn list_draws_header_rows_columns_values_and_a_rows_own_denial() {
    let mut list = example(
        "interaction.json",
        "list with category header rows and a row that allows only some affordances",
    );
    list["affordances"]
        .as_array_mut()
        .unwrap()
        .push(json!("bn:set_count:list:npc"));
    let choose = affordance(
        "bn:choose:list:npc",
        &[],
        json!({"row": {"component_id": "list:npc"}}),
    );
    let count = affordance(
        "bn:set_count:list:npc",
        &[],
        json!({"row": {"component_id": "list:npc"}}),
    );
    let (mut h, _) = harness(dialog(list, vec![choose, count]));
    h.get_by_label("TOOLS");
    h.get_by_label("Price");
    h.get_by_label("$12.00");
    h.get_by_label("hammer").click();
    h.run_steps(3);
    assert!(
        disabled(&h, "bn:set_count:list:npc"),
        "the row turns this one off"
    );
    assert!(
        disabled(&h, "bn:choose:list:npc"),
        "the row lists only the other affordance"
    );
}

#[test]
fn list_with_more_rows_than_held_asks_for_the_next_page_of_its_component() {
    let list = example(
        "interaction.json",
        "list with category header rows and a row that allows only some affordances",
    );
    let (mut h, script) = harness(dialog(list, vec![]));
    h.run_steps(2);
    let asked = script
        .sent()
        .into_iter()
        .filter(|m| m["method"] == "bn.interaction.choices")
        .map(|m| m["params"].clone())
        .collect::<Vec<_>>();
    assert_eq!(
        asked,
        [
            json!({"epoch": "e7f3", "boundary_id": "b:12", "component_id": "list:npc",
            "offset": 2, "limit": 200})
        ],
        "once, after the 2 held rows"
    );
}

#[test]
fn clicking_the_row_of_a_single_list_runs_its_activate_affordance_with_the_row() {
    let (mut h, script) = harness(pick_list(vec![]));
    h.get_by_label("b").click();
    h.run_steps(3);
    assert_eq!(
        submits(&script),
        [boundary_invoke("pick", json!({"row": "b"}))]
    );
}

#[test]
fn tabs_select_a_tab_by_its_id_and_show_the_child() {
    let tabs = example(
        "construction.json",
        "construction menu: tabs over the buildable rows",
    );
    let select = affordance(
        "bn:choose:tabs",
        &[],
        json!({"row": {"component_id": "tabs"}}),
    );
    let (mut h, script) = harness(dialog(tabs, vec![select]));
    h.get_by_label("Build Wooden Wall");
    h.get_by_label("Favorite");
    h.get_by_label("All").click();
    h.run_steps(3);
    assert_eq!(
        submits(&script),
        [boundary_invoke("bn:choose:tabs", json!({"row": "tab:ALL"}))]
    );
}

#[test]
fn split_draws_every_child_side_by_side() {
    let screen = example("menu.json", "options page with each editor type");
    let (h, _) = harness(screen);
    for label in ["Safe mode proximity distance", "Drop empty containers"] {
        let drawn = h.get_all_by_label(label).count();
        assert!(
            drawn >= 2,
            "{label}: the list row and the form field, not {drawn}"
        );
    }
    h.get_by_label("Value");
}

#[test]
fn panel_shows_title_subtitle_and_stats_with_their_maximum_and_unit() {
    let panel = example("trade.json", "trade pane with carried against capacity");
    let (h, _) = harness(dialog(panel, vec![]));
    h.get_by_label("You");
    h.get_by_label("Player items");
    h.get_by_label("Weight: 1.17/45.00 kg");
    h.get_by_label("Volume: 0.03/15.00 L");
    h.get_by_label("Free capacity: 43.83 kg, 14.97 L");
}

#[test]
fn form_field_set_by_the_player_runs_the_forms_affordance_with_the_field_and_text() {
    let screen = example("menu.json", "options page with each editor type");
    let (mut h, script) = harness(screen);
    let field = h
        .get_all_by_role(egui::accesskit::Role::TextInput)
        .find(|n| n.accesskit_node().value().as_deref() == Some("7"))
        .unwrap();
    field.focus();
    field.type_text("5");
    h.run_steps(2);
    h.key_press(egui::Key::Enter);
    h.run_steps(3);
    let sent = submits(&script);
    assert_eq!(sent.len(), 1, "{sent:?}");
    assert_eq!(sent[0]["operation"]["affordance_id"], "bn:submit:editors");
    assert_eq!(
        sent[0]["operation"]["args"]["row"],
        "option:SAFEMODEPROXIMITY"
    );
    assert!(
        sent[0]["operation"]["args"]["text"]
            .as_str()
            .unwrap()
            .contains('5')
    );
}

#[test]
fn key_field_takes_the_next_key_as_the_text_of_its_fill() {
    let form = example("menu.json", "key field of a keybinding prompt");
    let fill = affordance(
        "bn:fill:field",
        &[],
        json!({"text": {"field_id": "field:key"}}),
    );
    let (mut h, script) = harness(dialog(form, vec![fill]));
    h.get_by_label("New key for Move East");
    h.event(egui::Event::Text("x".into()));
    h.run_steps(3);
    assert_eq!(
        submits(&script),
        [boundary_invoke("bn:fill:field", json!({"text": "x"}))],
        "the key is the field's text and no screen key"
    );
}

#[test]
fn text_draws_its_markup_without_the_tags_and_its_tone() {
    let text = example("trade.json", "trade balance in the trader's favour");
    let (h, _) = harness(dialog(text, vec![]));
    h.get_by_label("You'll owe $12.00");
    let marked =
        json!({"id": "t", "kind": "text", "markup": "a <color_c_light_green>green</color> word"});
    let (h, _) = harness(dialog(marked, vec![]));
    h.get_by_label("a green word");
}

#[test]
fn grid_draws_its_cells_and_a_cell_with_an_affordance_runs_it_with_the_cell_as_its_row() {
    let panel = example(
        "vehicle.json",
        "vehicle install list with the highlighted part's requirements",
    );
    let (h, _) = harness(dialog(panel, vec![]));
    h.get_by_label("H");
    h.get_by_label("o");
    let grid = json!({"id": "grid", "kind": "grid", "cols": 2, "rows": 1, "cells": [
        {"x": 0, "y": 0, "id": "cell:0", "glyph": "A", "label": "Alpha", "affordance": "go", "enabled": true},
        {"x": 1, "y": 0, "id": "cell:1", "glyph": "B", "label": "Beta", "affordance": "go", "enabled": false,
            "denial": "Blocked."}]});
    let go = affordance("go", &[], json!({"row": {"component_id": "grid"}}));
    let (mut h, script) = harness(dialog(grid, vec![go]));
    assert!(disabled(&h, "Beta"));
    h.get_by_label("Alpha").click();
    h.run_steps(3);
    assert_eq!(
        submits(&script),
        [boundary_invoke("go", json!({"row": "cell:0"}))]
    );
}

#[test]
fn map_component_keeps_its_place_on_the_screen() {
    let map = example("target.json", "overmap map with the cursor described");
    let (h, _) = harness(dialog(map, vec![]));
    h.get_by_label("Overmap");
}

#[test]
fn canvas_draws_the_engines_window_run_by_run() {
    let screen = example("canvas.json", "timed mini-game window with color runs");
    let (h, _) = harness(screen);
    assert!(h.get_all_by_label_contains("S N A K E").count() >= 1);
    h.get_by_label("######   *");
}

#[test]
fn a_key_the_screen_binds_runs_its_affordance_before_the_client_moves_the_focus() {
    let down = json!({"id": "DOWN", "label": "Down", "keys": ["DOWN"], "enabled": true, "expect": "screen"});
    let (mut h, script) = harness(pick_list(vec![down]));
    h.key_press(egui::Key::ArrowDown);
    h.run_steps(3);
    assert_eq!(submits(&script), [screen_invoke("DOWN")]);
}

#[test]
fn an_unbound_arrow_moves_the_focus_locally_and_the_confirm_key_takes_the_focused_row() {
    let (mut h, script) = harness(pick_list(vec![]));
    h.key_press(egui::Key::ArrowDown);
    h.run_steps(2);
    assert!(
        submits(&script).is_empty(),
        "moving the focus sends nothing"
    );
    h.key_press(egui::Key::Enter);
    h.run_steps(3);
    assert_eq!(
        submits(&script),
        [boundary_invoke("pick", json!({"row": "b"}))]
    );
}

#[test]
fn the_focus_stops_at_the_last_row() {
    let (mut h, script) = harness(pick_list(vec![]));
    for _ in 0..5 {
        h.key_press(egui::Key::ArrowDown);
        h.run_steps(1);
    }
    h.key_press(egui::Key::Enter);
    h.run_steps(3);
    assert_eq!(
        submits(&script),
        [boundary_invoke("pick", json!({"row": "c"}))]
    );
}

#[test]
fn a_key_only_a_disabled_affordance_binds_shows_its_denial_and_sends_nothing() {
    let off = json!({"id": "go", "label": "Go", "keys": ["g"], "enabled": false,
        "denial": "Nothing to go to.", "expect": "screen"});
    let (mut h, script) = harness(pick_list(vec![off]));
    assert!(disabled(&h, "Go [g]"));
    h.event(egui::Event::Text("g".into()));
    h.run_steps(3);
    h.get_by_label("Nothing to go to.");
    assert!(submits(&script).is_empty());
}

#[test]
fn the_any_key_affordance_takes_a_key_nothing_binds() {
    let any = json!({"id": "dismiss", "label": "Dismiss", "keys": [], "enabled": true,
        "role": "any_key", "expect": "screen"});
    let (mut h, script) = harness(dialog(
        json!({"id": "message", "kind": "text", "markup": "Done."}),
        vec![any],
    ));
    h.event(egui::Event::Text("q".into()));
    h.run_steps(3);
    assert_eq!(submits(&script), [screen_invoke("dismiss")]);
}

#[test]
fn an_affordance_asking_for_a_count_collects_it_before_it_goes_out() {
    let ask = affordance(
        "drop",
        &[],
        json!({"row": {"component_id": "list"}, "count": {"min": 1, "max": 9}}),
    );
    let (mut h, script) = harness(pick_list(vec![ask]));
    h.get_by_label("drop").click();
    h.run_steps(3);
    assert!(
        submits(&script).is_empty(),
        "the count is still to be typed"
    );
    h.event(egui::Event::Text("3".into()));
    h.run_steps(2);
    h.key_press(egui::Key::Enter);
    h.run_steps(3);
    assert_eq!(
        submits(&script),
        [boundary_invoke("drop", json!({"row": "a", "count": 3}))]
    );
}

#[test]
fn a_count_outside_the_shapes_bounds_is_held_to_them() {
    let ask = affordance(
        "drop",
        &[],
        json!({"row": {"component_id": "list"}, "count": {"min": 1, "max": 9}}),
    );
    let (mut h, script) = harness(pick_list(vec![ask]));
    h.get_by_label("drop").click();
    h.run_steps(3);
    for digit in ["4", "0"] {
        h.event(egui::Event::Text(digit.into()));
        h.run_steps(2);
    }
    h.key_press(egui::Key::Enter);
    h.run_steps(3);
    assert_eq!(
        submits(&script),
        [boundary_invoke("drop", json!({"row": "a", "count": 9}))]
    );
}

#[test]
fn without_a_stack_the_screen_is_the_0_1_menu() {
    let (t, _) = ScriptedTransport::new();
    let t = t
        .on("bn.hello", vec![fixture("hello_result")])
        .on("bn.subscribe", common::subscribe_reply());
    let model = EngineModel::new(Box::new(t));
    let mut h = Harness::builder()
        .with_size(egui::vec2(1100.0, 800.0))
        .build_ui_state(
            |ui, app: &mut App<EngineModel>| app.show(ui),
            App::new(model),
        );
    h.run_steps(4);
    h.get_by_label("Map");
}

#[test]
fn return_on_the_focused_grid_cell_runs_the_affordance_the_cell_names() {
    let grid = json!({"id": "grid", "kind": "grid", "cols": 2, "rows": 1, "cursor": {"x": 0, "y": 0},
        "cells": [
            {"x": 0, "y": 0, "id": "cell:0", "glyph": "A", "affordance": "go", "enabled": true},
            {"x": 1, "y": 0, "id": "cell:1", "glyph": "B", "affordance": "go", "enabled": true}]});
    let go = affordance("go", &[], json!({"row": {"component_id": "grid"}}));
    let (mut h, script) = harness(dialog(grid, vec![go]));
    h.key_press(egui::Key::ArrowRight);
    h.run_steps(2);
    assert!(
        submits(&script).is_empty(),
        "moving the cursor sends nothing"
    );
    h.key_press(egui::Key::Enter);
    h.run_steps(3);
    assert_eq!(
        submits(&script),
        [boundary_invoke("go", json!({"row": "cell:0"}))]
    );
}
