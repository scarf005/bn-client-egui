//! The screens of the 0.2 stack drawn from their components, from the engine's own examples
//! (`crates/core/tests/fixtures/0.2.examples/`): one test per component kind, then the affordances and keys.
use crate::common::fixture;
use bn_client_egui::transport::{ScriptHandle, ScriptedTransport};
use bn_client_egui::{App, Clock, ConnectOptions, EngineModel, GameModel};
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

/// The id of the row of an item whose label ends with `label`: the engine numbers items by session, so a
/// test reads it from the screen.
fn item_row_id(screen: &Value, label: &str) -> String {
    match screen {
        Value::Object(map) => {
            let own = map
                .get("id")
                .and_then(Value::as_str)
                .filter(|id| id.starts_with("item:"));
            let named = map
                .get("label")
                .and_then(Value::as_str)
                .is_some_and(|l| l.ends_with(label));
            match own {
                Some(id) if named => id.to_string(),
                _ => map
                    .values()
                    .map(|v| item_row_id(v, label))
                    .find(|id| !id.is_empty())
                    .unwrap_or_default(),
            }
        }
        Value::Array(list) => list
            .iter()
            .map(|v| item_row_id(v, label))
            .find(|id| !id.is_empty())
            .unwrap_or_default(),
        _ => String::new(),
    }
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
    stacked(vec![screen])
}

/// The same with a whole stack, bottom first; a game is running only when the map is in it.
fn stacked<'a>(stack: Vec<Value>) -> (H<'a>, ScriptHandle) {
    let mut subscribe = fixture("subscribe_result");
    subscribe["result"]["interaction"]["stack"] = json!(stack);
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
    crate::common::settle_windows(&mut harness);
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
    // The rows run the list's choose, so it has no button.
    assert!(h.query_by_label("bn:choose:list:npc").is_none());
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
fn double_clicking_the_row_of_a_single_list_runs_its_activate_affordance_with_the_row() {
    let (mut h, script) = harness(pick_list(vec![]));
    let at = h.get_by_label("b").rect().center();
    crate::common::double_click(&mut h, at);
    h.run_steps(3);
    assert_eq!(
        submits(&script),
        [boundary_invoke("pick", json!({"row": "b"}))]
    );
}

/// Two tabs, the second selected, over a list of one row with a favorite and a buildable column.
fn tabs_over_rows() -> Value {
    json!({"id": "tabs", "kind": "tabs", "affordances": ["bn:choose:tabs"],
        "tabs": [
            {"id": "tab:ALL", "label": "All", "selected": false, "enabled": true},
            {"id": "tab:WALLS", "label": "Walls", "selected": true, "enabled": true},
        ],
        "select": "bn:choose:tabs",
        "child": {"id": "list", "kind": "list", "affordances": ["bn:choose:list"],
            "columns": [
                {"key": "favorite", "label": "Favorite"},
                {"key": "buildable", "label": "Buildable"},
            ],
            "rows": [{"id": "wall", "label": "Build Wooden Wall", "description": "", "denial": "",
                "enabled": true, "selectable": true, "selected": true,
                "columns": [
                    {"key": "favorite", "label": "Favorite", "value": "no"},
                    {"key": "buildable", "label": "Buildable", "value": "yes"},
                ]}],
            "total": 1, "selection": "single", "activate": "bn:choose:list"}})
}

#[test]
fn tabs_select_a_tab_by_its_id_and_show_the_child() {
    let tabs = tabs_over_rows();
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
    let panel = json!({
        "id": "panel:pane-player", "kind": "panel", "title": "You", "subtitle": "Player items",
        "stats": [
            {"key": "weight", "label": "Weight", "value": "1.17", "max": "45.00", "unit": "kg"},
            {"key": "volume", "label": "Volume", "value": "0.03", "max": "15.00", "unit": "L"}
        ],
        "child": {"id": "text:pane-player", "kind": "text", "markup": "Free capacity: 43.83 kg, 14.97 L"}
    });
    let (h, _) = harness(dialog(panel, vec![]));
    h.get_by_label("You");
    h.get_by_label("Player items");
    h.get_by_label("Weight");
    h.get_by_label("1.17/45.00 kg");
    h.get_by_label("Volume");
    h.get_by_label("0.03/15.00 L");
    h.get_by_label("Free capacity: 43.83 kg, 14.97 L");
}

#[test]
fn a_panel_with_no_title_draws_no_heading_line() {
    let subtitle_top = |title: &str| {
        let panel = json!({"id": "panel:one", "kind": "panel", "title": title,
            "subtitle": "Player items"});
        let (h, _) = harness(dialog(panel, vec![]));
        h.get_by_label("Player items").rect().min.y
    };
    assert!(subtitle_top("") < subtitle_top("You"));
}

/// The pickup list as the engine publishes it: a panel with the carry stats over a single list whose
/// rows carry the engine's marks and counts, with a toggle and a count affordance.
fn pickup_screen() -> Value {
    let row = |id: &str, label: &str, glyph: &str, picked: u64| {
        json!({"id": id, "label": label, "description": "", "denial": "", "enabled": true,
            "selectable": true, "selected": picked > 0, "columns": [],
            "mark": {"glyph": glyph, "color": "c_light_blue"},
            "selected_count": picked, "minimum_count": 0, "max": 3})
    };
    let list = json!({"id": "pickup:items", "kind": "list", "columns": [],
        "rows": [row("pickup:a", "3 rocks", "#", 2), row("pickup:b", "stick", "-", 0)],
        "total": 2, "selection": "single", "activate": "pickup:toggle",
        "affordances": ["pickup:count"]});
    let panel = json!({"id": "pickup", "kind": "panel", "title": "Pick up", "subtitle": "ground",
        "stats": [{"key": "weight", "label": "Weight", "value": "2.0", "max": "45.0", "unit": "kg"}],
        "child": list});
    let row_arg = json!({"row": {"component_id": "pickup:items"}});
    dialog(
        panel,
        vec![
            affordance("pickup:toggle", &[], row_arg.clone()),
            affordance("pickup:count", &[], row_arg),
        ],
    )
}

#[test]
fn a_pickup_list_draws_its_stats_and_rows_and_a_double_click_runs_the_toggle_with_the_row() {
    let (mut h, script) = harness(pickup_screen());
    h.get_by_label("Pick up");
    h.get_by_label("Weight");
    h.get_by_label("2.0/45.0 kg");
    h.get_by_label("3 rocks");
    let at = h.get_by_label("stick").rect().center();
    crate::common::double_click(&mut h, at);
    h.run_steps(3);
    assert_eq!(
        submits(&script),
        [boundary_invoke("pickup:toggle", json!({"row": "pickup:b"}))]
    );
}

#[test]
fn a_screen_with_a_dock_starts_at_that_side_of_the_window() {
    let side = |dock: &str| {
        let mut screen = pickup_screen();
        screen["dock"] = dock.into();
        screen["title"] = "Docked".into();
        let map = example("tutorial.json", "map screen");
        let (h, _) = stacked(vec![map, screen]);
        h.get_all_by_role(egui::accesskit::Role::Window)
            .next()
            .unwrap()
            .rect()
    };
    let (left, right) = (side("left"), side("right"));
    // At the room's edge, right of the map's shortcut bar, less the margin the room keeps (the harness adds its
    // own 8 points).
    let edge = bn_client_egui::toolbar::BUTTON;
    assert!(left.left() > edge && left.left() < edge + 20.0, "{left:?}");
    // The sidebar is on the right: the window sits against its inner edge, not the window's.
    assert!(right.right() < 1095.0 && right.right() > 500.0, "{right:?}");
}

#[test]
fn the_title_screen_draws_the_banner_above_its_tabs_and_entries() {
    let title = example("title.json", "title screen with its banner and tabs");
    let (h, _) = harness(title);
    h.get_by_label("Tutorial");
    // The banner is above the tab strip, which is above the entries.
    let top = |label: &str| h.get_by_label(label).rect().top();
    assert!(
        top("Tutorial") > top("New Game"),
        "entries are under their tab"
    );
    let banner = h
        .get_all_by_role(egui::accesskit::Role::Label)
        .find(|n| {
            n.accesskit_node()
                .value()
                .is_some_and(|v| v.contains("Version: build"))
        })
        .expect("the banner text is drawn");
    assert!(
        banner.rect().top() < top("New Game"),
        "the banner is the heading"
    );
}

#[test]
fn the_title_screen_is_only_its_menu_and_a_tab_click_runs_the_tab_with_its_id() {
    let title = example("title.json", "title screen with its banner and tabs");
    let (mut h, script) = harness(title);
    for panel in ["Message log", "Ground", "Inventory", "Map"] {
        assert!(
            h.query_by_label(panel).is_none(),
            "{panel} belongs to a running game"
        );
    }
    h.get_by_label("Credits").click();
    h.run_steps(3);
    assert_eq!(
        submits(&script),
        [boundary_invoke("menu:tab", json!({"row": "credits"}))]
    );
}

#[test]
fn the_action_palette_and_client_settings_open_on_the_title_screen() {
    let title = example("title.json", "title screen with its banner and tabs");
    let (mut h, script) = harness(title);
    let dialog = |h: &H, label: &str| {
        h.query_all_by_label(label)
            .any(|n| n.accesskit_node().role() == egui::accesskit::Role::Dialog)
    };
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::P);
    h.run_steps(3);
    assert!(dialog(&h, "Actions"), "Ctrl+P opens the palette");
    h.key_press(egui::Key::Escape);
    h.run_steps(3);
    assert!(!dialog(&h, "Actions"), "Escape closes it");
    assert!(submits(&script).is_empty(), "{:?}", submits(&script));
    h.get_by_label("Client settings").click();
    h.run_steps(3);
    assert!(h.query_by_label("Reduced motion").is_some());
}

#[test]
fn escape_that_interrupts_a_running_command_does_not_also_cancel_the_screen() {
    let close = json!({"id": "close", "label": "Close", "keys": ["ESC"], "enabled": true,
        "expect": "screen", "role": "cancel"});
    let (mut h, script) = harness(pick_list(vec![close]));
    h.key_press(egui::Key::Enter);
    h.run_steps(3);
    assert_eq!(submits(&script).len(), 1, "the pick runs");
    assert!(h.state().model.busy());
    h.key_press(egui::Key::Escape);
    h.run_steps(3);
    let interrupts = script.sent_methods();
    assert_eq!(
        interrupts.iter().filter(|m| *m == "bn.interrupt").count(),
        1
    );
    assert_eq!(submits(&script).len(), 1, "{:?}", submits(&script));
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
fn a_key_field_takes_a_key_the_screen_binds_as_its_text_and_the_screen_does_not_run_it() {
    let form = example("menu.json", "key field of a keybinding prompt");
    let fill = affordance(
        "bn:fill:field",
        &[],
        json!({"text": {"field_id": "field:key"}}),
    );
    let down = affordance("down", &["DOWN"], json!({}));
    let (mut h, script) = harness(dialog(form, vec![fill, down]));
    h.key_press(egui::Key::ArrowDown);
    h.run_steps(3);
    assert_eq!(
        submits(&script),
        [boundary_invoke("bn:fill:field", json!({"text": "DOWN"}))]
    );
}

#[test]
fn text_draws_its_markup_without_the_tags_and_its_tone() {
    let text =
        json!({"id": "message", "kind": "text", "markup": "You'll owe $12.00", "tone": "bad"});
    let (h, _) = harness(dialog(text, vec![]));
    h.get_by_label("You'll owe $12.00");
    let marked =
        json!({"id": "t", "kind": "text", "markup": "a <color_c_light_green>green</color> word"});
    let (h, _) = harness(dialog(marked, vec![]));
    h.get_by_label("a green word");
}

fn screen_action(id: &str, label: &str, role: Option<&str>) -> Value {
    let mut action =
        json!({"id": id, "label": label, "keys": [], "enabled": true, "expect": "screen"});
    if let Some(role) = role {
        action["role"] = role.into();
    }
    action
}

#[test]
fn a_terminal_draws_its_output_and_its_any_key_button_runs_the_any_key_action() {
    let output = json!({"id": "terminal:output", "kind": "text", "role": "terminal_output",
        "markup": "<color_c_green>Logging into Test Terminal</color>\nLogin successful"});
    let any_key = screen_action("ANY_INPUT", "Press any key", Some("any_key"));
    let (mut h, script) = harness(dialog(output, vec![any_key]));
    assert!(h.get_all_by_role(egui::accesskit::Role::Label).any(|n| {
        n.accesskit_node()
            .value()
            .is_some_and(|v| v.contains("Login successful"))
    }));
    h.get_by_label("Press any key").click();
    h.run_steps(3);
    assert_eq!(submits(&script), [screen_invoke("ANY_INPUT")]);
}

#[test]
fn a_terminal_query_offers_yes_no_and_quit_and_runs_the_one_clicked() {
    let output = json!({"id": "terminal:output", "kind": "text", "role": "terminal_output",
        "markup": "Bypass security?"});
    let answers = vec![
        screen_action("YES", "Yes", None),
        screen_action("NO", "No", None),
        screen_action("QUIT", "Quit", Some("cancel")),
    ];
    let (mut h, script) = harness(dialog(output, answers));
    h.get_by_label("No").click();
    h.run_steps(3);
    assert_eq!(submits(&script), [screen_invoke("NO")]);
}

#[test]
fn the_description_of_a_target_runs_the_action_that_switches_the_target() {
    let panel = json!({"id": "description", "kind": "panel", "title": "Creature",
        "child": {"id": "description:text", "kind": "text", "markup": "This is a zombie."}});
    let switch = screen_action("FURNITURE", "Furniture", None);
    let (mut h, script) = harness(dialog(panel, vec![switch]));
    h.get_by_label("Creature");
    h.get_by_label("This is a zombie.");
    h.get_by_label("Furniture").click();
    h.run_steps(3);
    assert_eq!(submits(&script), [screen_invoke("FURNITURE")]);
}

#[test]
fn grid_draws_its_cells_and_a_cell_with_an_affordance_runs_it_with_the_cell_as_its_row() {
    let screen = example("vehicle.json", "vehicle screen");
    let drawn = &screen["root"]["children"][0]["child"]["children"][0];
    assert_eq!(drawn["kind"], "grid");
    let (h, _) = harness(dialog(drawn.clone(), vec![]));
    h.get_by_label("|");
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
fn the_overmap_screen_draws_its_map_and_the_toggles_it_names() {
    let (h, _) = harness(example("overmap.json", "overmap screen"));
    h.get_by_role_and_label(egui::accesskit::Role::Canvas, "Overmap");
    h.get_by_label_contains("You are here.");
    assert!(h.get_all_by_label_contains("Toggle Map Notes").count() >= 1);
}

#[test]
fn canvas_draws_the_engines_window_run_by_run() {
    let screen = example("canvas.json", "timed mini-game window with color runs");
    let (h, _) = harness(screen);
    assert!(h.get_all_by_label_contains("S N A K E").count() >= 1);
    h.get_by_label("######   *");
}

/// A mini-game window of the engine: `rows` of `(text, fg, bg)` runs on a grid `cols` wide.
fn game(cols: u32, rows: Vec<Value>) -> Value {
    json!({"screen_id": "screen:2", "role": "dialog", "title": "S N A K E",
        "root": {"id": "canvas", "kind": "canvas", "cols": cols, "rows": rows},
        "refresh_ms": 100, "affordances": []})
}

#[test]
fn a_canvas_is_sized_to_the_window_and_keeps_its_grid() {
    let rows = vec![
        json!([["Score: 0", 15, 0]]),
        json!([["ab", 15, 0], ["*", 9, 0]]),
    ];
    let (mut h, _) = harness(game(20, rows));
    let big = h.get_by_label_contains("Score: 0").rect();
    h.set_size(egui::vec2(240.0, 300.0));
    h.run_steps(4);
    let small = h.get_by_label_contains("Score: 0").rect();
    assert!(big.height() > 16.0, "larger than a fixed 14 px font");
    assert!(big.height() > small.height(), "{big:?} vs {small:?}");
    // The widest row spans the grid: 20 columns fit the room the window leaves.
    let row = h.get_by_label_contains("ab").rect();
    assert!(row.width() <= 240.0 * 0.9 + 1.0, "{row:?}");
}

#[test]
fn a_canvas_of_24_rows_is_not_clipped_by_the_dialog() {
    let rows = (0..24)
        .map(|n| json!([[format!("row{n:02}"), 15, 0]]))
        .collect::<Vec<_>>();
    let (h, _) = harness(game(80, rows));
    let last = h.get_by_label_contains("row23").rect();
    let window = h
        .ctx
        .memory(|m| m.area_rect(egui::Id::new(("dialog", "S N A K E"))))
        .unwrap();
    assert!(window.max.y >= last.max.y, "{window:?} under {last:?}");
}

#[test]
fn what_follows_a_canvas_keeps_the_themes_spacing() {
    let mut screen = game(20, vec![json!([["Score: 0", 15, 0]])]);
    screen["affordances"] = json!([affordance("go", &[], json!({}))]);
    let (h, _) = harness(screen);
    let row = h.get_by_label("Score: 0").rect();
    let button = h.get_by_label("go").rect();
    let theme = h.ctx.global_style().spacing.item_spacing.y;
    assert_eq!(button.min.y - row.max.y, theme, "{row:?} then {button:?}");
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
fn down_then_enter_on_a_query_popup_chooses_the_option_the_focus_moved_to() {
    let mut popup = pick_list(vec![
        affordance("LEFT", &["LEFT"], json!({})),
        affordance("CONFIRM", &["RETURN"], json!({})),
    ]);
    popup["affordances"][0]["keys"] = json!([]);
    popup["focus"] = json!({"component_id": "list", "row_id": "a"});
    let (mut h, script) = harness(popup);
    h.key_press(egui::Key::ArrowDown);
    h.run_steps(2);
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
    assert!(disabled(&h, "Go"));
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
        .on("bn.subscribe", crate::common::subscribe_reply());
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

/// A crafting or construction screen: unread tabs over a list whose cursor row has `detail`, and the
/// `select` affordance that moves the engine's cursor to the row the focus moves to.
fn recipe_screen(tabs: &str, list: &str, select: &str) -> Value {
    let rows = ["a", "b", "c"].iter().enumerate().map(|(i, id)| {
        json!({"id": id, "label": format!("Row {id}"), "description": "", "denial": "", "enabled": true,
            "selectable": true, "selected": i == 0, "columns": []})
    });
    let root = json!({"id": tabs, "kind": "tabs", "select": "tab",
    "tabs": [
        {"id": "t1", "label": "Weapons", "selected": true, "enabled": true, "unread": true},
        {"id": "t2", "label": "Tools", "selected": false, "enabled": true},
    ],
    "child": {"id": "body", "kind": "split", "relation": "detail", "active": list, "children": [
        {"id": list, "kind": "list", "columns": [], "rows": rows.collect::<Vec<_>>(),
            "total": 3, "selection": "single", "activate": "go"},
        {"id": "detail", "kind": "panel", "title": "Detail of a", "stats": []},
    ]}});
    let row = json!({"row": {"component_id": list}});
    let mut go = affordance("go", &["RETURN"], row.clone());
    go["label"] = json!("Go");
    let mut pick = affordance(select, &[], row);
    pick["role"] = json!("select");
    dialog(root, vec![go, pick])
}

fn follows_the_focus_with_a_select_invoke(tabs: &str, list: &str, select: &str) {
    let mut screen = recipe_screen(tabs, list, select);
    screen["focus"] = json!({"component_id": list, "row_id": "a"});
    let (mut h, script) = harness(screen);
    h.get_by_label("Weapons •");
    h.get_by_label("Tools");
    h.get_by_label("Detail of a");
    assert!(
        h.query_by_label(select).is_none(),
        "a select affordance is not a button"
    );
    assert!(submits(&script).is_empty());
    h.key_press(egui::Key::ArrowDown);
    h.run_steps(3);
    assert_eq!(
        submits(&script),
        [boundary_invoke(select, json!({"row": "b"}))]
    );
}

#[test]
fn the_crafting_screen_marks_unread_tabs_and_selects_the_row_the_focus_moves_to() {
    follows_the_focus_with_a_select_invoke("crafting:tabs", "crafting:recipes", "craft:select");
}

#[test]
fn the_construction_screen_marks_unread_tabs_and_selects_the_row_the_focus_moves_to() {
    follows_the_focus_with_a_select_invoke(
        "construction:tabs",
        "construction:groups",
        "construct:select",
    );
}

/// A click on a row moves the engine's cursor there, as an arrow key does, and runs nothing else.
#[test]
fn a_click_on_a_recipe_selects_it_without_crafting_it() {
    let (mut h, script) = harness(recipe_screen(
        "crafting:tabs",
        "crafting:recipes",
        "craft:select",
    ));
    h.get_by_label("Row b").click();
    h.run_steps(3);
    assert_eq!(
        submits(&script),
        [boundary_invoke("craft:select", json!({"row": "b"}))]
    );
}

/// A double click on a row runs the list's `activate` on it, as Enter does.
#[test]
fn a_double_click_on_a_recipe_crafts_it() {
    let (mut h, script) = harness(recipe_screen(
        "crafting:tabs",
        "crafting:recipes",
        "craft:select",
    ));
    let at = h.get_by_label("Row b").rect().center();
    crate::common::double_click(&mut h, at);
    h.run_steps(3);
    let sent = submits(&script);
    assert_eq!(
        sent.last(),
        Some(&boundary_invoke("go", json!({"row": "b"}))),
        "{sent:?}"
    );
}

#[test]
fn the_editmap_screen_draws_the_engines_cursor_parts_and_a_click_runs_its_edit_affordance() {
    let map = json!({"id": "editmap:map", "kind": "map", "affordances": ["target:move"],
        "mode": "world", "target": {"source": {"dim": "", "x": 2, "y": 4, "z": 0},
            "current": {"dim": "", "x": 3, "y": 4, "z": 0}, "range": 20, "status": "edit",
            "candidates": [], "parts": [
                {"kind": "terrain", "text": "Grass", "id": "t_grass"},
                {"kind": "field", "text": "Smoke", "id": "fd_smoke", "values": {"intensity": 2}}]}});
    let root = json!({"id": "editmap", "kind": "panel", "title": "Edit map", "child": map});
    let edit = json!({"id": "EDIT_TERRAIN", "label": "Edit terrain", "keys": ["e"],
        "enabled": true, "expect": "screen", "args": {}});
    let (mut h, script) = harness(dialog(root, vec![edit]));
    h.get_by_value("Grass\nSmoke");
    h.get_by_label_contains("Edit terrain").click();
    h.run_steps(3);
    assert_eq!(submits(&script), [screen_invoke("EDIT_TERRAIN")]);
}

#[test]
fn an_option_row_runs_the_options_activate_with_the_option_as_its_row() {
    let screen = example(
        "options.json",
        "options screen with the editor of an integer option",
    );
    let (mut h, script) = harness(screen);
    let at = h.get_by_label("Dangerous pickups").rect().center();
    crate::common::double_click(&mut h, at);
    h.run_steps(3);
    assert_eq!(
        submits(&script),
        [boundary_invoke(
            "options:activate",
            json!({"row": "option:DANGEROUS_PICKUPS"})
        )]
    );
}

#[test]
fn the_editor_of_the_option_under_the_cursor_runs_the_options_set_with_the_option_and_the_text() {
    let screen = example(
        "options.json",
        "options screen with the editor of an integer option",
    );
    let (mut h, script) = harness(screen);
    let field = h
        .get_all_by_role(egui::accesskit::Role::TextInput)
        .find(|n| n.accesskit_node().value().as_deref() == Some("0"))
        .unwrap();
    field.focus();
    field.type_text("5");
    h.run_steps(2);
    h.key_press(egui::Key::Enter);
    h.run_steps(3);
    let sent = submits(&script);
    assert_eq!(sent.len(), 1, "{sent:?}");
    assert_eq!(sent[0]["operation"]["affordance_id"], "options:set");
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
fn a_keybinding_row_runs_the_pick_with_its_action_and_the_screen_says_its_mode() {
    let screen = example("keybindings.json", "keybindings screen in the remove mode");
    let (mut h, script) = harness(screen);
    h.get_by_label("Mode");
    h.get_all_by_label("Remove bindings").next().unwrap();
    let at = h.get_by_label("Move East").rect().center();
    crate::common::double_click(&mut h, at);
    h.run_steps(3);
    assert_eq!(
        submits(&script),
        [boundary_invoke(
            "keybindings:pick",
            json!({"row": "action:RIGHT"})
        )]
    );
}

#[test]
fn a_distraction_row_runs_the_toggle_with_the_distraction_as_its_row() {
    let screen = example("distractions.json", "distraction manager");
    let (mut h, script) = harness(screen);
    let at = h.get_by_label("Pain").rect().center();
    crate::common::double_click(&mut h, at);
    h.run_steps(3);
    assert_eq!(
        submits(&script),
        [boundary_invoke(
            "distractions:toggle",
            json!({"row": "distraction:Pain"})
        )]
    );
}

#[test]
fn a_colour_column_tab_runs_the_column_affordance_by_its_token() {
    let screen = example("colors.json", "colour manager");
    let (mut h, script) = harness(screen);
    h.get_all_by_label("Invert").next().unwrap().click();
    h.run_steps(3);
    assert_eq!(
        submits(&script),
        [boundary_invoke("colors:column", json!({"row": "invert"}))]
    );
}

#[test]
fn a_colour_row_runs_the_activate_affordance_with_the_colour_as_its_row() {
    let screen = example("colors.json", "colour manager");
    let (mut h, script) = harness(screen);
    let at = h.get_by_label("c_black_cyan").rect().center();
    crate::common::double_click(&mut h, at);
    h.run_steps(3);
    assert_eq!(
        submits(&script),
        [boundary_invoke(
            "colors:activate",
            json!({"row": "color:c_black_cyan"})
        )]
    );
}

#[test]
fn a_stat_draws_the_engines_label_and_value_as_parts_of_their_own() {
    let screen = example(
        "autonote.json",
        "auto notes manager with three discovered map extras",
    );
    let (h, _script) = harness(screen);
    let _ = h.get_by_label("Auto notes enabled:");
    let _ = h.get_by_label("True");
}

#[test]
fn an_auto_notes_row_runs_the_toggle_with_the_map_extra_id_as_its_row() {
    let screen = example(
        "autonote.json",
        "auto notes manager with three discovered map extras",
    );
    let (mut h, script) = harness(screen);
    let at = h.get_by_label("Lost Campers").rect().center();
    crate::common::double_click(&mut h, at);
    h.run_steps(3);
    assert_eq!(
        submits(&script),
        [boundary_invoke(
            "autonote:toggle",
            json!({"row": "extra:mx_collegekids"})
        )]
    );
}

#[test]
fn an_auto_pickup_row_runs_the_edit_with_the_rule_serial_as_its_row() {
    let screen = example("autopickup.json", "auto pickup manager with two rules");
    let (mut h, script) = harness(screen);
    let at = h.get_by_label("*axe*").rect().center();
    crate::common::double_click(&mut h, at);
    h.run_steps(3);
    assert_eq!(
        submits(&script),
        [boundary_invoke("autopickup:edit", json!({"row": "rule:2"}))]
    );
}

#[test]
fn an_auto_pickup_tab_runs_the_tab_affordance_by_its_token() {
    let screen = example("autopickup.json", "auto pickup manager with two rules");
    let (mut h, script) = harness(screen);
    h.get_by_label("[<Character>]").click();
    h.run_steps(3);
    assert_eq!(
        submits(&script),
        [boundary_invoke(
            "autopickup:tab",
            json!({"row": "character"})
        )]
    );
}

#[test]
fn a_safe_mode_row_runs_the_edit_with_the_rule_serial_as_its_row() {
    let screen = example("safemode.json", "safe mode manager with a rule");
    let (mut h, script) = harness(screen);
    let at = h.get_by_label("zombie").rect().center();
    crate::common::double_click(&mut h, at);
    h.run_steps(3);
    assert_eq!(
        submits(&script),
        [boundary_invoke("safemode:edit", json!({"row": "rule:1"}))]
    );
}

#[test]
fn a_safe_mode_tab_runs_the_tab_affordance_by_its_token() {
    let screen = example("safemode.json", "safe mode manager with a rule");
    let (mut h, script) = harness(screen);
    h.get_by_label("Character").click();
    h.run_steps(3);
    assert_eq!(
        submits(&script),
        [boundary_invoke("safemode:tab", json!({"row": "character"}))]
    );
}

#[test]
fn a_worn_item_runs_the_armor_screens_pick_with_its_identity() {
    let screen = example("armor.json", "armor layers screen");
    let jeans = item_row_id(&screen, "jeans");
    assert!(jeans.starts_with("item:"), "no row of jeans in the fixture");
    let (mut h, script) = harness(screen);
    let at = h
        .get_all_by_label("||\u{a0}jeans")
        .next()
        .unwrap()
        .rect()
        .center();
    crate::common::double_click(&mut h, at);
    h.run_steps(3);
    assert_eq!(
        submits(&script),
        [boundary_invoke("armor:pick", json!({"row": jeans}))]
    );
}

#[test]
fn a_body_part_tab_runs_the_armor_screens_part_pick_with_its_id() {
    let (mut h, script) = harness(example("armor.json", "armor layers screen"));
    h.get_all_by_label("Torso").next().unwrap().click();
    h.run_steps(3);
    assert_eq!(
        submits(&script),
        [boundary_invoke("armor:part", json!({"row": "part:torso"}))]
    );
}

#[test]
fn an_action_of_the_item_examine_menu_runs_the_menus_run_with_the_action_id() {
    let (mut h, script) = harness(example("armor.json", "item examine menu"));
    // The menu's row, not the registered action's button of the same name under the list.
    let at = h.get_all_by_label("Wield").next().unwrap().rect().center();
    crate::common::double_click(&mut h, at);
    h.run_steps(3);
    assert_eq!(
        submits(&script),
        [boundary_invoke("examine:run", json!({"row": "WIELD"}))]
    );
}

#[test]
fn a_response_runs_the_talk_screens_respond_with_its_data_name() {
    let (mut h, script) = harness(example("npctalk.json", "NPC talk screen"));
    let at = h.get_by_label("What are you doing here?").rect().center();
    crate::common::double_click(&mut h, at);
    h.run_steps(3);
    assert_eq!(
        submits(&script),
        [boundary_invoke(
            "talk:respond",
            json!({"row": "TALK_EVAC_MERCHANT:5"})
        )]
    );
}

#[test]
fn a_special_key_runs_the_talk_screens_special_with_its_fixed_name() {
    let (mut h, script) = harness(example("npctalk.json", "NPC talk screen"));
    let at = h.get_by_label("Yell").rect().center();
    crate::common::double_click(&mut h, at);
    h.run_steps(3);
    assert_eq!(
        submits(&script),
        [boundary_invoke(
            "talk:special:run",
            json!({"row": "special:yell"})
        )]
    );
}

#[test]
fn a_diary_page_runs_the_diaries_page_with_its_serial_id() {
    let (mut h, script) = harness(example("diary.json", "diary screen with two pages"));
    let first = h
        .get_all_by_label("Year 1, Spring, day 1 12:05:00PM")
        .next()
        .expect("a page row");
    let at = first.rect().center();
    crate::common::double_click(&mut h, at);
    h.run_steps(3);
    assert_eq!(
        submits(&script),
        [boundary_invoke("diary:page", json!({"row": "page:1"}))]
    );
}

#[test]
fn the_text_editor_draws_its_text_in_a_text_box() {
    let (mut h, _script) = harness(example("diary.json", "text editor screen"));
    h.run_steps(3);
    assert!(
        h.query_by_role(eframe::egui::accesskit::Role::TextInput)
            .is_some()
    );
}

fn chrome(extra: usize) -> Vec<Value> {
    let mut all = vec![
        screen_action("CONFIRM", "Confirm", Some("confirm")),
        screen_action("QUIT", "Quit", Some("cancel")),
        screen_action("UP", "Move up", Some("navigate")),
        screen_action("HELP_KEYBINDINGS", "Help", Some("help")),
    ];
    all.extend((0..extra).map(|i| screen_action(&format!("A{i}"), &format!("Act{i}"), None)));
    all
}

fn chrome_body() -> Value {
    json!({"id": "t", "kind": "text", "markup": "body"})
}

fn top(h: &H<'_>, label: &str) -> f32 {
    h.get_by_label(label).rect().top()
}

#[test]
fn the_footer_has_the_confirm_and_no_navigation_cancel_or_help_chips() {
    let (mut h, script) = harness(dialog(chrome_body(), chrome(2)));
    h.get_by_label("Act0");
    h.get_by_label("Act1");
    assert!(h.query_by_label("Move up").is_none());
    assert!(h.query_by_label("Quit").is_none());
    assert!(
        top(&h, "Help") < top(&h, "body"),
        "the help is in the title bar, above the body"
    );
    assert!(top(&h, "Confirm") > top(&h, "body"));
    h.get_by_label("Confirm").click();
    h.run_steps(3);
    h.get_by_label("Help").click();
    h.run_steps(3);
    assert_eq!(
        submits(&script),
        [screen_invoke("CONFIRM"), screen_invoke("HELP_KEYBINDINGS")]
    );
}

#[test]
fn a_screen_without_a_title_has_its_cancel_and_help_in_the_footer() {
    let mut screen = dialog(chrome_body(), chrome(2));
    screen.as_object_mut().unwrap().remove("title");
    let (mut h, script) = harness(screen);
    assert!(top(&h, "Help") > top(&h, "body"));
    let at = |label| (top(&h, label), h.get_by_label(label).rect().left());
    assert!(at("Quit") < at("Act0"), "the cancel leads the row");
    assert!(at("Help") > at("Act1"), "the help follows the actions");
    assert!(at("Confirm") > at("Help"), "the confirm ends the row");
    h.get_by_label("Help").click();
    h.run_steps(3);
    h.get_by_label("Quit").click();
    h.run_steps(3);
    assert_eq!(
        submits(&script),
        [screen_invoke("HELP_KEYBINDINGS"), screen_invoke("QUIT")]
    );
}

#[test]
fn the_footer_keeps_one_row_and_puts_the_actions_it_has_no_room_for_in_a_menu() {
    let (mut h, script) = harness(dialog(chrome_body(), chrome(30)));
    let tops = h
        .get_all_by_role(egui::accesskit::Role::Button)
        .filter(|n| {
            n.accesskit_node().label().is_some_and(|l| {
                let act = l
                    .strip_prefix("Act")
                    .is_some_and(|n| n.parse::<u32>().is_ok());
                l == "Confirm" || l == "⋯" || act
            })
        })
        .map(|n| n.rect().top() as i32)
        .collect::<std::collections::HashSet<_>>();
    assert_eq!(tops.len(), 1, "{tops:?}");
    assert!(h.query_by_label("Act29").is_none(), "folded into the menu");
    assert!(
        h.query_by_label("More").is_none(),
        "the client names nothing"
    );
    h.get_by_label("⋯").click();
    h.run_steps(3);
    h.get_by_label("Act29").click();
    h.run_steps(3);
    assert_eq!(submits(&script), [screen_invoke("A29")]);
}

/// A list of `n` rows `{prefix}0`, `{prefix}1`, ... with the id `id`.
fn long_list(id: &str, prefix: &str, n: usize) -> Value {
    let rows = (0..n).map(|i| {
        json!({"id": format!("{prefix}{i}"), "label": format!("{prefix}{i}"), "description": "",
            "denial": "", "enabled": true, "selectable": true, "selected": i == 0, "columns": []})
    });
    json!({"id": id, "kind": "list", "columns": [], "rows": rows.collect::<Vec<_>>(),
        "total": n, "selection": "single", "activate": "pick"})
}

#[test]
fn a_page_key_moves_the_focus_by_the_rows_its_own_list_shows() {
    let stats = (0..12)
        .map(|i| json!({"key": format!("s{i}"), "label": format!("Stat {i}"), "value": "1"}));
    let panel = json!({"id": "side", "kind": "panel", "title": "", "stats": stats.collect::<Vec<_>>(),
        "child": long_list("two", "b", 60)});
    let split = json!({"id": "root", "kind": "split", "relation": "peers",
        "children": [long_list("one", "a", 60), panel]});
    let pick = affordance("pick", &["RETURN"], json!({"row": {"component_id": "one"}}));
    let (mut h, script) = harness(dialog(split, vec![pick]));
    let shown = |h: &H, prefix: &str| {
        (0..60)
            .filter(|i| h.query_by_label(&format!("{prefix}{i}")).is_some())
            .count()
    };
    let (one, two) = (shown(&h, "a"), shown(&h, "b"));
    assert!(
        one > two + 2,
        "the panel's stats leave its list fewer rows: {one} vs {two}"
    );
    h.key_press(egui::Key::PageDown);
    h.run_steps(2);
    h.key_press(egui::Key::Enter);
    h.run_steps(3);
    let sent = submits(&script);
    let row = sent[0]["operation"]["args"]["row"]
        .as_str()
        .unwrap()
        .to_owned();
    let moved = row[1..].parse::<usize>().unwrap();
    // The rows drawn include the ones cut at the top and bottom edges, which a page does not count.
    assert!(
        (one - 2..=one).contains(&moved),
        "moved {moved} rows; the list shows {one}, the other {two}"
    );
}

/// The stack with `screen` over the map, as event `sequence`.
fn stack_event(sequence: u32, screen: Value) -> Value {
    json!({"jsonrpc": "2.0", "method": "bn.events", "params": {"epoch": "e7f3", "events": [{
        "sequence": sequence.to_string(), "revision": (sequence - 9).to_string(),
        "type": "interaction.changed", "changes": {"interaction": {
        "boundary_id": format!("b:{sequence}"),
        "readiness": {"phase": "playing", "game_ready": true, "accepts_interaction_commands": true,
            "accepts_registered_actions": true},
        "actions": [], "interaction": null,
        "stack": [{"screen_id": "screen:map:e7f3", "role": "map", "root": null, "affordances": []},
            screen]}}}]}})
}

#[test]
fn the_typing_state_of_closed_prompts_is_not_kept() {
    let prompt = |n: u32| {
        let form = json!({"id": "form:f", "kind": "form", "affordances": ["fill", "submit"],
            "fields": [{"id": "field:t", "label": "Filter", "value": "", "type": "text"}],
            "submit": "submit"});
        let text = json!({"text": {"field_id": "field:t"}});
        let mut submit = affordance("submit", &[], text.clone());
        submit["role"] = json!("confirm");
        json!({"screen_id": format!("screen:{n}"), "role": "dialog", "title": "Filter", "root": form,
            "affordances": [affordance("fill", &[], text), submit]})
    };
    let (mut h, script) = harness(prompt(0));
    let mut kept = vec![];
    for n in 1..20 {
        script.push(stack_event(40 + n, prompt(n)));
        h.run_steps(3);
        h.get_by_role(egui::accesskit::Role::TextInput);
        kept.push(h.ctx.data(|d| d.len()));
    }
    assert_eq!(kept.first(), kept.last(), "{kept:?}");
}

#[test]
fn a_multiline_field_holding_a_long_text_stays_a_few_lines_tall_and_as_wide_as_the_form() {
    let long = "{\"rows\":[\"########################\"]}".repeat(60);
    let form = json!({"id": "form:f", "kind": "form", "affordances": ["fill"],
        "fields": [{"id": "field:t", "label": "Mapgen", "value": long, "type": "text", "multiline": true},
            {"id": "field:u", "label": "After", "value": "", "type": "text"}],
        "submit": "fill"});
    let text = json!({"text": {"field_id": "field:t"}});
    let (mut h, _script) = harness(dialog(form, vec![affordance("fill", &[], text)]));
    h.run_steps(5);
    let rect = h.get_by_label("Mapgen").rect();
    let after = h.get_by_label("After").rect();
    let window = h.ctx.content_rect();
    // The next field follows within a few lines, and the box is as wide as the window's room allows.
    assert!(
        after.top() - rect.top() < window.height() / 3.0,
        "{rect:?} {after:?}"
    );
    let field = h
        .get_all_by_role(egui::accesskit::Role::TextInput)
        .next()
        .unwrap()
        .rect();
    assert!(
        field.width() > window.width() / 2.0,
        "{field:?} in {window:?}"
    );
}

#[test]
fn hovering_an_off_tab_gives_the_engines_reason() {
    let mut screen = recipe_screen("tabs", "list", "pick");
    screen["root"]["tabs"][1]["enabled"] = json!(false);
    screen["root"]["tabs"][1]["denial"] = json!("Nothing to make here.");
    let (mut h, _script) = harness(screen);
    assert!(h.query_by_label("Nothing to make here.").is_none());
    h.get_by_label("Tools").hover();
    h.run_steps(6);
    h.get_by_label("Nothing to make here.");
}

#[test]
fn a_typed_digit_of_a_counting_list_runs_its_digit_and_the_digits_are_no_buttons() {
    let mut digit = screen_action("inventory:digit:6", "6", Some("digit"));
    digit["keys"] = json!(["6"]);
    let mut right = screen_action("RIGHT", "Mark", Some("navigate"));
    right["keys"] = json!(["6"]);
    let (mut h, script) = harness(dialog(chrome_body(), vec![digit, right]));
    assert!(h.query_by_label("6").is_none());
    h.event(egui::Event::Text("6".into()));
    h.run_steps(3);
    assert_eq!(submits(&script), [screen_invoke("inventory:digit:6")]);
}

/// The room between the last row wholly above the footer and the footer, in rows, for a full screen whose
/// list of 60 rows has `own` affordances.
fn rows_left_above_the_footer(own: Vec<Value>) -> f32 {
    let rows = (0..60)
        .map(|i| {
            json!({"id": format!("r:{i}"), "label": format!("Row {i}"), "description": "",
            "denial": "", "enabled": true, "selectable": true, "selected": false, "columns": []})
        })
        .collect::<Vec<_>>();
    let names = own.iter().map(|a| a["id"].clone()).collect::<Vec<_>>();
    let list = json!({"id": "parts", "kind": "list", "columns": [], "rows": rows,
        "total": 60, "selection": "single", "affordances": names});
    let leave = json!({"id": "RENAME", "label": "Leave", "keys": ["e"], "enabled": true,
        "expect": "screen"});
    let mut affordances = own;
    affordances.push(leave);
    let screen = json!({"screen_id": "screen:9", "role": "screen", "title": "Test", "root": list,
        "affordances": affordances});
    let (mut h, _) = harness(screen);
    let row = h.get_by_label("Row 0").rect().height();
    // At the end of the list its last row sits on the room the list keeps under itself.
    h.hover_at(h.get_by_label("Row 0").rect().center());
    h.run_steps(2);
    for _ in 0..40 {
        h.event(egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Page,
            delta: egui::vec2(0.0, -1.0),
            modifiers: egui::Modifiers::NONE,
            phase: egui::TouchPhase::Move,
        });
        h.run_steps(1);
    }
    h.run_steps(40);
    let footer = h.get_by_label_contains("Leave").rect().top();
    let last = h.get_by_label("Row 59").rect().bottom();
    (footer - last) / row
}

#[test]
fn a_list_with_no_descriptions_or_buttons_of_its_own_reaches_down_to_the_footer() {
    let left = rows_left_above_the_footer(vec![]);
    assert!(left < 1.5, "{left} rows of room are left");
    // A row's own picking affordance draws no button under the list.
    let pick = json!({"id": "pick", "label": "Pick", "keys": [], "enabled": true, "expect": "boundary",
        "role": "select", "args": {"row": {"component_id": "parts"}}});
    let left = rows_left_above_the_footer(vec![pick]);
    assert!(
        left < 2.0,
        "{left} rows of room are left beside a picking affordance"
    );
}

#[test]
fn wrapped_peer_lists_share_the_height_so_every_line_shows_above_the_footer() {
    let list = |id: &str| {
        let rows = (0..60)
            .map(|i| {
                json!({"id": format!("{id}:{i}"), "label": format!("{id} row with a label long enough that three of them do not fit one line, number {i}"),
                "description": "", "denial": "", "enabled": true, "selectable": true, "selected": false,
                "columns": []})
            })
            .collect::<Vec<_>>();
        json!({"id": id, "kind": "list", "columns": [], "rows": rows, "total": 60, "selection": "single"})
    };
    let root = json!({"id": "lists", "kind": "split", "relation": "peers",
        "children": [list("Alpha"), list("Beta"), list("Gamma")]});
    let leave = json!({"id": "RENAME", "label": "Leave", "keys": ["e"], "enabled": true,
        "expect": "screen"});
    let screen = json!({"screen_id": "screen:9", "role": "screen", "title": "Test", "root": root,
        "affordances": [leave]});
    let (mut h, _) = harness(screen);
    bn_client_egui::kit::tokens::apply(&h.ctx);
    bn_client_egui::fonts::install_bundled(&h.ctx);
    crate::common::settle_windows(&mut h);
    let footer = h.get_by_label_contains("Leave").rect().top();
    for id in ["Alpha", "Beta", "Gamma"] {
        let first = h
            .get_by_label(&format!(
                "{id} row with a label long enough that three of them do not fit one line, number 0"
            ))
            .rect();
        assert!(
            first.bottom() <= footer,
            "{id}'s first row ends at {} under the footer at {footer}",
            first.bottom()
        );
    }
}

#[test]
fn a_grid_takes_the_room_of_its_cells_and_cursor_not_of_its_empty_edges() {
    let grid = json!({"id": "grid", "kind": "grid", "cols": 17, "rows": 9, "cursor": {"x": 6, "y": 4},
        "cells": [
            {"x": 5, "y": 4, "glyph": "A", "enabled": true},
            {"x": 7, "y": 5, "glyph": "B", "enabled": true}]});
    // Docked at a side.
    let mut screen = dialog(grid, vec![]);
    screen["dock"] = json!("left");
    let (h, _) = harness(screen);
    let a = h.get_by_label("A").rect();
    let b = h.get_by_label("B").rect();
    assert!((b.left() - a.left() - 2.0 * a.width()).abs() < 1.0);
}

#[test]
fn peer_panes_too_wide_for_one_line_go_on_to_the_next_instead_of_scrolling_sideways() {
    // The construction screen as the engine sent it, on a stage whose facts do not fit one line.
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../core/tests/fixtures/construction-stage.json"
    );
    let screen = serde_json::from_str::<Value>(&std::fs::read_to_string(path).unwrap()).unwrap();
    let (mut h, _) = harness(screen);
    // In the client's own fonts, whose words are as wide as a player sees them.
    bn_client_egui::kit::tokens::apply(&h.ctx);
    bn_client_egui::fonts::install_bundled(&h.ctx);
    crate::common::settle_windows(&mut h);
    let window = h.get_by_role(egui::accesskit::Role::Window).rect();
    let last = h.get_by_label_contains("10 adobe bricks").rect();
    assert!(
        last.right() <= window.right(),
        "the last pane ends at {} past the window's {}",
        last.right(),
        window.right()
    );
    let fabrication = h.get_by_label_contains("Fabrication").rect();
    let one_line = h.get_by_label_contains("10 adobe bricks").rect().height();
    assert!(
        fabrication.height() <= one_line + 0.5,
        "a word broke across lines"
    );
}

#[test]
fn a_pane_is_as_wide_as_its_heading_and_its_rows_beside_their_scroll_bar() {
    // The construction screen as the engine sent it, on a stage that needs no tools.
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../core/tests/fixtures/construction-stage.json"
    );
    let mut screen =
        serde_json::from_str::<Value>(&std::fs::read_to_string(path).unwrap()).unwrap();
    fn tools(value: &mut Value) -> Option<&mut Value> {
        let own = value["id"]
            .as_str()
            .is_some_and(|id| id.ends_with(":tools"));
        match value {
            _ if own => Some(value),
            Value::Object(map) => map.values_mut().find_map(tools),
            Value::Array(list) => list.iter_mut().find_map(tools),
            _ => None,
        }
    }
    let tools = tools(&mut screen).unwrap();
    tools["rows"] = json!([]);
    tools["total"] = json!(0);
    let (mut h, _) = harness(screen);
    bn_client_egui::kit::tokens::apply(&h.ctx);
    bn_client_egui::fonts::install_bundled(&h.ctx);
    crate::common::settle_windows(&mut h);
    let one_line = h.get_by_label_contains("10 adobe bricks").rect().height();
    for label in ["Tools required:", "Fabrication"] {
        let rect = h.get_by_label_contains(label).rect();
        assert!(
            rect.height() <= one_line + 0.5,
            "{label} broke across lines: {rect:?}"
        );
    }
}

#[test]
fn a_grid_with_room_to_spare_draws_its_cells_larger() {
    let grid = json!({"id": "grid", "kind": "grid", "cols": 3, "rows": 2, "cells": [
        {"x": 0, "y": 0, "glyph": "A", "enabled": true},
        {"x": 2, "y": 1, "glyph": "B", "enabled": true}]});
    let parts = rows_list(&["frame", "roof"]);
    let root =
        json!({"id": "vehicle", "kind": "split", "relation": "peers", "children": [grid, parts]});
    let screen = json!({"screen_id": "screen:9", "role": "screen", "title": "Test", "root": root,
        "affordances": []});
    let (h, _) = harness(screen);
    let a = h.get_by_label("A").rect();
    let b = h.get_by_label("B").rect();
    assert!(a.width() > 40.0, "a cell {} wide", a.width());
    assert!((b.left() - a.left() - 2.0 * a.width()).abs() < 1.0);
    assert!((b.top() - a.top() - a.height()).abs() < 1.0);
}

#[test]
fn a_query_popup_draws_its_confirm_once_and_its_rows_run_the_choose() {
    let (h, _) = harness(example("generic.json", "query popup screen"));
    // The list's choose runs from its rows; the registered confirm is the one button that does the same.
    let confirms = h.query_all_by_label("Confirm Choice").collect::<Vec<_>>();
    assert_eq!(confirms.len(), 1);
    assert_eq!(
        confirms[0].accesskit_node().data().keyboard_shortcut(),
        Some("RETURN")
    );
}

#[test]
fn a_query_popup_offers_each_choice_once_on_its_row_under_its_prompt() {
    let (h, _) = harness(example("generic.json", "query popup screen"));
    // The rows name the registered YES and NO, so neither is a footer button again.
    for label in ["Yes", "No"] {
        assert_eq!(h.query_all_by_label(label).count(), 1, "{label}");
    }
    let prompt = h.get_by_label("Save and quit? (Case Sensitive)").rect();
    let yes = h.get_by_label("Yes").rect();
    assert!(prompt.bottom() <= yes.top(), "{prompt:?} beside {yes:?}");
    assert!(
        prompt.left() <= yes.left() + 1.0,
        "{prompt:?} right of {yes:?}"
    );
}

#[test]
fn a_toggle_of_the_native_layout_is_no_button() {
    let (h, _) = harness(example("info.json", "message log"));
    for label in ["Toggle wide display", "Toggle full height display"] {
        assert!(h.query_by_label_contains(label).is_none(), "{label}");
    }
}

#[test]
fn the_look_screen_draws_no_button_for_the_move_its_map_clicks_make() {
    let (h, _) = harness(example("look.json", "look-around screen"));
    assert_eq!(h.query_all_by_label("Select").count(), 0);
}

#[test]
fn an_any_key_popup_says_its_message_once_and_asks_for_the_key_only_by_its_button() {
    let mut form = example("menu.json", "key field of a keybinding prompt");
    form["fields"][0]["label"] = json!("");
    let message = json!({"id": "message", "kind": "text", "markup": "You have nothing to wear."});
    let root = json!({"id": "split:root", "kind": "split", "relation": "peers", "children": [message, form]});
    let fill = affordance(
        "bn:fill:field",
        &[],
        json!({"text": {"field_id": "field:key"}}),
    );
    let any = json!({"id": "bn:any_key", "label": "Press any key", "keys": [], "enabled": true,
        "role": "any_key", "expect": "screen"});
    let (h, _) = harness(dialog(root, vec![fill, any]));
    assert_eq!(h.get_all_by_label("You have nothing to wear.").count(), 1);
    h.get_by_label("Press any key");
    assert!(h.query_by_label("Press the new key").is_none());
}

#[test]
fn the_text_a_started_affordance_asks_for_takes_the_typing_without_a_click_first() {
    let ask = affordance("name", &[], json!({"text": {}}));
    let (mut h, script) = harness(dialog(rows_list(&["a", "b"]), vec![ask]));
    h.get_by_label("name").click();
    h.run_steps(3);
    h.event(egui::Event::Text("car".into()));
    h.run_steps(3);
    h.key_press(egui::Key::Enter);
    h.run_steps(3);
    assert_eq!(
        submits(&script),
        [boundary_invoke("name", json!({"text": "car"}))],
        "the box of the question has the focus when it opens"
    );
}
