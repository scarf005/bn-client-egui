#![recursion_limit = "512"]
mod common;
use bn_client_egui::app::CELL;
use bn_client_egui::transport::ScriptHandle;
use bn_client_egui::{App, Clock, EngineModel, GameModel};
use common::{fixture, session};
use eframe::egui::{self, accesskit};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use serde_json::json;
use std::time::Duration;

type H<'a> = Harness<'a, App<EngineModel>>;

fn harness<'a>() -> (H<'a>, ScriptHandle) {
    let (t, h) = session();
    let t = t.on("bn.command.submit", vec![fixture("submit_result")]);
    let clock = Clock::manual();
    let model = EngineModel::new(Box::new(t)).with_clock(clock.clone());
    let harness = Harness::builder()
        .with_size(egui::vec2(1100.0, 800.0))
        .build_ui_state(
            |ui, app: &mut App<EngineModel>| app.show(ui),
            App::new(model).with_clock(clock),
        );
    (harness, h)
}

fn submits(h: &ScriptHandle) -> Vec<serde_json::Value> {
    h.sent()
        .into_iter()
        .filter(|m| m["method"] == "bn.command.submit")
        .collect()
}

#[test]
fn engine_map_renders_and_a_click_submits_a_travel_through_the_protocol() {
    let (mut h, script) = harness();
    h.run_steps(3);
    assert_eq!(script.sent_methods(), ["bn.hello", "bn.subscribe"]);
    let origin = h.get_by_label("Map").rect().min;
    // The avatar is at view (20, 12); click two cells east of it.
    let pos = origin + egui::vec2(22.5 * CELL, 12.5 * CELL);
    h.hover_at(pos);
    h.run_steps(2);
    for pressed in [true, false] {
        h.event(egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        });
        h.run_steps(1);
    }
    h.run_steps(3);
    let sent = submits(&script);
    assert_eq!(sent.len(), 1, "one command in flight at a time");
    assert_eq!(
        sent[0]["params"]["operation"],
        json!({"kind":"travel","pos":{"dim":"","x":5,"y":4,"z":0}})
    );
}

#[test]
fn planned_route_is_announced_and_cleared_by_an_empty_route() {
    let (mut h, script) = harness();
    h.run_steps(3);
    let event = |sequence: &str, route: serde_json::Value| {
        json!({"jsonrpc":"2.0","method":"bn.events","params":{"epoch":"e7f3","events":[{
            "sequence": sequence, "revision": "32", "type": "cells.seen",
            "changes": {"route": route}}]}})
    };
    let at = |x| json!({"dim":"","x":x,"y":4,"z":0});
    script.push(event("41", json!([at(4), at(5)])));
    h.run_steps(3);
    assert_eq!(h.state().model.route().len(), 2);
    let map = h.get_by_label("Map");
    let description = map.accesskit_node().description().unwrap_or_default();
    assert!(
        description.contains("Route of 2 squares to (22, 12)"),
        "{description}"
    );
    script.push(event("42", json!([])));
    h.run_steps(3);
    assert!(h.state().model.route().is_empty());
    let map = h.get_by_label("Map");
    let description = map.accesskit_node().description().unwrap_or_default();
    assert!(!description.contains("Route"), "{description}");
}

#[test]
fn ground_and_inventory_come_from_the_engine_state() {
    let (mut h, _) = harness();
    h.run_steps(3);
    h.get_by_label("Pick up knife_combat");
    h.get_by_label("Drop flashlight");
}

#[test]
fn engine_menu_renders_choices_denials_and_submits_choose_and_cancel() {
    let (mut h, script) = harness();
    h.run_steps(3);
    for name in ["events_41_walk", "events_42_message", "events_43_menu"] {
        script.push(fixture(name));
    }
    h.run_steps(3);
    h.get_by_label("Choose an item");
    h.get_by_label("Too heavy");
    h.get_by_label("tin can").click();
    h.run_steps(2);
    assert!(submits(&script).is_empty(), "disabled choice is inert");
    h.get_by_label("combat knife").click();
    h.run_steps(2);
    let sent = submits(&script);
    assert_eq!(
        sent[0]["params"]["operation"],
        json!({"kind":"choose","choice_id":"c:1"})
    );
    assert_eq!(sent[0]["params"]["expect"]["schema_id"], "s:pickup");
    assert_eq!(h.state().model.menu().unwrap().choices.len(), 2);
}

#[test]
fn engine_menu_cancel_button_submits_cancel() {
    let (mut h, script) = harness();
    h.run_steps(3);
    for name in ["events_41_walk", "events_42_message", "events_43_menu"] {
        script.push(fixture(name));
    }
    h.run_steps(3);
    h.get_by_label("Cancel").click();
    h.run_steps(2);
    assert_eq!(
        submits(&script)[0]["params"]["operation"],
        json!({"kind":"cancel"})
    );
}

#[test]
fn closing_the_menu_event_removes_the_window() {
    let (mut h, script) = harness();
    h.run_steps(3);
    for name in ["events_41_walk", "events_42_message", "events_43_menu"] {
        script.push(fixture(name));
    }
    h.run_steps(3);
    assert!(h.state().model.menu().is_some());
    let mut closed = fixture("events_43_menu");
    let ev = &mut closed["params"]["events"][0];
    ev["sequence"] = "44".into();
    ev["revision"] = "34".into();
    ev["changes"]["interaction"]["interaction"] = serde_json::Value::Null;
    script.push(closed);
    h.run_steps(3);
    assert!(h.state().model.menu().is_none());
    assert!(h.query_by_label("Choose an item").is_none());
}

#[test]
fn action_list_shows_name_and_keys_and_clicking_submits_the_action_id() {
    let (mut h, script) = harness();
    h.run_steps(3);
    h.get_by_label("Move right (RIGHT, l)").click();
    h.run_steps(2);
    assert_eq!(
        submits(&script)[0]["params"]["operation"],
        json!({"kind":"action","action_id":"RIGHT"})
    );
}

#[test]
fn typing_a_game_key_submits_its_action() {
    let (mut h, script) = harness();
    h.run_steps(3);
    h.event(egui::Event::Text("k".into()));
    h.run_steps(2);
    assert_eq!(
        submits(&script)[0]["params"]["operation"],
        json!({"kind":"action","action_id":"UP"})
    );
}

#[test]
fn typed_key_with_no_offered_action_submits_nothing() {
    let (mut h, script) = harness();
    h.run_steps(3);
    h.event(egui::Event::Text("q".into()));
    h.run_steps(2);
    assert!(submits(&script).is_empty(), "q is bound to nothing offered");
}

fn open_menu(h: &mut H, script: &ScriptHandle) {
    h.run_steps(3);
    for name in ["events_41_walk", "events_42_message", "events_43_menu"] {
        script.push(fixture(name));
    }
    h.run_steps(3);
}

fn notice(stage: &str, error: Option<&str>) -> serde_json::Value {
    let mut params = json!({"epoch":"e7f3","command_id":"c:5","stage":stage});
    if let Some(e) = error {
        params["error"] = e.into();
    }
    json!({"jsonrpc":"2.0","method":"bn.command","params":params})
}

fn click_choice(h: &mut H) {
    h.get_by_label("combat knife").click();
    h.run_steps(2);
}

const SETTLE: Duration = Duration::from_millis(200);

#[test]
fn pressed_choice_is_highlighted_and_others_wait_once_the_command_outlasts_a_blink() {
    let (mut h, script) = harness();
    open_menu(&mut h, &script);
    click_choice(&mut h);
    h.run_steps(2);
    h.state().clock().advance(SETTLE);
    h.run_steps(2);
    let pressed = h.get_by_label("combat knife");
    assert_eq!(
        pressed.accesskit_node().toggled(),
        Some(accesskit::Toggled::True)
    );
    assert!(h.get_by_label("Cancel").accesskit_node().is_disabled());
    script.push(notice("completed", None));
    h.run_steps(3);
    assert_ne!(
        h.get_by_label("combat knife")
            .accesskit_node()
            .is_selected(),
        Some(true)
    );
    assert!(!h.get_by_label("Cancel").accesskit_node().is_disabled());
}

#[test]
fn status_line_names_the_stage_and_explains_a_rejection_in_words() {
    let (mut h, script) = harness();
    open_menu(&mut h, &script);
    h.get_by_label("Ready");
    click_choice(&mut h);
    let line = h.get_by_label("combat knife: accepted");
    assert_eq!(
        line.accesskit_node().live(),
        egui::accesskit::Live::Polite,
        "announced to screen readers"
    );
    script.push(notice("executing", None));
    h.run_steps(2);
    h.get_by_label("combat knife: executing");
    script.push(notice("interrupted", Some("not_ready")));
    h.run_steps(2);
    h.get_by_label("combat knife: interrupted: the game is not ready");
}

#[test]
fn status_line_keeps_executing_until_the_boundary_is_reached_then_goes_back_to_ready() {
    let (mut h, script) = harness();
    open_menu(&mut h, &script);
    click_choice(&mut h);
    script.push(notice("executing", None));
    h.run_steps(40);
    h.get_by_label("combat knife: executing");
    assert!(h.state().model.busy(), "still busy without any bn.loading");
    script.push(notice("completed", None));
    h.run_steps(2);
    h.get_by_label("Ready");
    assert!(!h.state().model.busy());
}

#[test]
fn map_click_marks_the_cell_before_the_route_arrives() {
    let (mut h, script) = harness();
    h.run_steps(3);
    let origin = h.get_by_label("Map").rect().min;
    let pos = origin + egui::vec2(22.5 * CELL, 12.5 * CELL);
    h.hover_at(pos);
    h.run_steps(2);
    for pressed in [true, false] {
        h.event(egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        });
        h.run_steps(1);
    }
    h.run_steps(2);
    assert!(h.state().model.route().is_empty(), "no route yet");
    assert_eq!(
        h.state().marker(),
        Some(bn_client_egui::Pos { x: 22, y: 12 })
    );
    h.get_by_label("Travel: accepted");
    script.push(notice("completed", None));
    h.run_steps(3);
    assert_eq!(h.state().marker(), None, "cleared once the command ends");
}

#[test]
fn tabbed_menu_shows_tabs_and_only_the_selected_tabs_entries_and_choosing_a_tab_submits_its_id() {
    let (mut h, script) = harness();
    h.run_steps(3);
    for name in ["events_41_walk", "events_42_message"] {
        script.push(fixture(name));
    }
    let mut menu = fixture("events_43_menu");
    let ev = &mut menu["params"]["events"][0];
    let row = |id: &str, label: &str, selected: bool, pane: Option<&str>| {
        let mut c = json!({"id":id,"label":label,"description":"","denial":"","enabled":true,
            "selectable":true,"selected":selected,"columns":[]});
        if let Some(pane) = pane {
            c["pane_id"] = pane.into();
        }
        c
    };
    ev["changes"]["interaction"]["interaction"]["choices"] = json!([
        row("tab:new_game", "New Game", true, None),
        row("tab:settings", "Settings", false, None),
        row("new_game:tutorial", "Tutorial", false, Some("new_game")),
    ]);
    script.push(menu);
    h.run_steps(3);
    h.get_by_label("New Game");
    h.get_by_label("Tutorial");
    h.get_by_label("Settings").click();
    h.run_steps(2);
    assert_eq!(
        submits(&script)[0]["params"]["operation"],
        json!({"kind":"choose","choice_id":"tab:settings"})
    );
}

fn click_map(h: &mut H, view: (f32, f32)) {
    let origin = h.get_by_label("Map").rect().min;
    let pos = origin + egui::vec2(view.0 * CELL, view.1 * CELL);
    h.hover_at(pos);
    h.run_steps(2);
    for pressed in [true, false] {
        h.event(egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        });
        h.run_steps(1);
    }
}

fn operations(script: &ScriptHandle) -> Vec<serde_json::Value> {
    submits(script)
        .iter()
        .map(|m| m["params"]["operation"].clone())
        .collect()
}

#[test]
fn arrow_key_walks_after_a_button_was_clicked() {
    let (mut h, script) = harness();
    h.run_steps(3);
    h.get_by_label("Move left (LEFT, h)").click();
    h.run_steps(2);
    script.push(notice("completed", None));
    h.run_steps(3);
    h.key_press(egui::Key::ArrowRight);
    h.run_steps(3);
    let ops = operations(&script);
    assert_eq!(ops.len(), 2, "{ops:?}");
    assert_eq!(ops[1], json!({"kind":"action","action_id":"RIGHT"}));
}

#[test]
fn arrow_key_walks_after_the_map_was_clicked() {
    let (mut h, script) = harness();
    h.run_steps(3);
    click_map(&mut h, (22.5, 12.5));
    h.run_steps(2);
    script.push(notice("completed", None));
    h.run_steps(3);
    h.key_press(egui::Key::ArrowUp);
    h.run_steps(3);
    let ops = operations(&script);
    assert_eq!(ops.len(), 2, "{ops:?}");
    assert_eq!(ops[1], json!({"kind":"action","action_id":"UP"}));
}

#[test]
fn arrow_key_in_a_menu_runs_the_action_the_menus_context_binds_to_it() {
    let (mut h, script) = harness();
    open_menu(&mut h, &script);
    h.key_press(egui::Key::ArrowRight);
    h.run_steps(3);
    assert_eq!(
        submits(&script)
            .iter()
            .map(|s| s["params"]["operation"].clone())
            .collect::<Vec<_>>(),
        [json!({"kind":"action","action_id":"RIGHT"})]
    );
}

#[test]
fn actions_panel_does_not_blink_during_a_short_command() {
    let (mut h, _script) = harness();
    h.run_steps(3);
    let before = h.get_by_label("Move right (RIGHT, l)").rect();
    click_map(&mut h, (22.5, 12.5));
    h.run_steps(3);
    assert!(h.state().model.busy());
    let during = h.get_by_label("Move right (RIGHT, l)");
    assert!(!during.accesskit_node().is_disabled(), "no disabled blink");
    assert_eq!(during.rect(), before, "no re-layout");
}

#[test]
fn actions_panel_keeps_its_layout_when_a_long_command_disables_it() {
    let (mut h, _script) = harness();
    h.run_steps(3);
    let before = h.get_by_label("Move right (RIGHT, l)").rect();
    click_map(&mut h, (22.5, 12.5));
    h.run_steps(2);
    h.state().clock().advance(SETTLE);
    h.run_steps(3);
    let during = h.get_by_label("Move right (RIGHT, l)");
    assert!(
        during.accesskit_node().is_disabled(),
        "disabled once it lasts"
    );
    assert_eq!(during.rect(), before, "same size and place");
}

#[test]
fn native_log_lines_show_with_repeat_count_and_the_new_ones_are_announced_together() {
    let (mut h, script) = harness();
    h.run_steps(3);
    script.push(fixture("events_41_walk"));
    for (sequence, id, text, count) in [
        (42, "7", "You open the door.", 1),
        (43, "8", "Bang!", 1),
        (44, "8", "Bang!", 2),
    ] {
        script.push(json!({"jsonrpc": "2.0", "method": "bn.events", "params": {"epoch": "e7f3",
            "events": [{"sequence": sequence.to_string(), "revision": "32", "type": "message.logged",
            "data": {"id": id, "text": text, "kind": "bad", "color": "c_red", "count": count}}]}}));
    }
    h.run_steps(3);
    h.get_by_label("You open the door.");
    let live: Vec<String> = h
        .root()
        .children_recursive()
        .filter(|n| n.accesskit_node().live() == accesskit::Live::Polite)
        .filter_map(|n| n.accesskit_node().value())
        .collect();
    assert!(
        live.iter().any(|v| v == "You open the door. Bang! x2"),
        "{live:?}"
    );
}

fn interrupts(script: &ScriptHandle) -> usize {
    let methods = script.sent_methods();
    methods.iter().filter(|m| *m == "bn.interrupt").count()
}

#[test]
fn escape_interrupts_the_running_command_but_does_nothing_when_idle() {
    let (mut h, script) = harness();
    h.run_steps(3);
    h.key_press(egui::Key::Escape);
    h.run_steps(2);
    assert_eq!(interrupts(&script), 0, "nothing is running");
    h.get_by_label("Move left (LEFT, h)").click();
    h.run_steps(2);
    h.key_press(egui::Key::Escape);
    h.run_steps(2);
    assert_eq!(interrupts(&script), 1);
}

#[test]
fn status_line_offers_interrupt_only_while_a_command_runs() {
    let (mut h, script) = harness();
    h.run_steps(3);
    assert!(h.query_by_label("Interrupt").is_none());
    h.get_by_label("Move left (LEFT, h)").click();
    h.run_steps(2);
    h.get_by_label("Interrupt").click();
    h.run_steps(2);
    assert_eq!(interrupts(&script), 1);
    script.push(notice("completed", None));
    h.run_steps(3);
    assert!(h.query_by_label("Interrupt").is_none());
}

/// An options-like menu: one page tab and a row per editor kind, plus a row without one.
fn settings_menu(script: &ScriptHandle, field: Option<serde_json::Value>) {
    let mut menu = fixture("events_43_menu");
    let ev = &mut menu["params"]["events"][0];
    let row = |id: &str, label: &str, editor: Option<serde_json::Value>, enabled: bool| {
        let mut c = json!({"id":id,"label":label,"description":"","denial":
            if enabled { "" } else { "Prerequisite not met" },"enabled":enabled,
            "selectable":enabled,"selected":false,"columns":[],"pane_id":"page:general"});
        if let Some(editor) = editor {
            c["editor"] = editor;
        }
        c
    };
    let editor =
        |kind: &str, value: &str| json!({"type":kind,"value":value,"values":[],"max_length":-1});
    let mut select = editor("select", "no");
    select["values"] = json!([{"id":"no","label":"No"},{"id":"all","label":"All"}]);
    let mut number = editor("integer", "7");
    number["minimum"] = 0.into();
    number["maximum"] = 60.into();
    ev["changes"]["interaction"]["interaction"]["choices"] = json!([
        {"id":"page:general","label":"General","description":"","denial":"","enabled":true,
         "selectable":true,"selected":true,"columns":[]},
        row("option:SAFEMODE", "Safe mode", Some(editor("bool", "true")), true),
        row("option:DROP_EMPTY", "Drop empty containers", Some(select), true),
        row("option:SAFEMODEPROXIMITY", "Proximity", Some(number), true),
        row("option:LOCKED", "Locked", Some(editor("bool", "false")), false),
        row("option:PLAIN", "Plain row", None, true),
    ]);
    if let Some(field) = field {
        ev["changes"]["interaction"]["interaction"]["field"] = field;
    }
    script.push(menu);
}

fn open_settings(h: &mut H, script: &ScriptHandle, field: Option<serde_json::Value>) {
    h.run_steps(3);
    for name in ["events_41_walk", "events_42_message"] {
        script.push(fixture(name));
    }
    settings_menu(script, field);
    h.run_steps(3);
}

#[test]
fn a_bool_editor_row_is_a_checkbox_and_toggling_it_fills_the_row_with_the_opposite_value() {
    let (mut h, script) = harness();
    open_settings(&mut h, &script, None);
    h.get_by_label("Plain row");
    h.get_by_label("Safe mode").click();
    h.run_steps(2);
    assert_eq!(
        operations(&script),
        [json!({"kind":"fill","field_id":"option:SAFEMODE","value":"false","submit":true})]
    );
}

#[test]
fn a_select_editor_offers_its_values_and_a_disabled_row_stays_inert_with_its_denial() {
    let (mut h, script) = harness();
    open_settings(&mut h, &script, None);
    h.get_by_label("Prerequisite not met");
    h.get_by_label("Locked").click();
    h.run_steps(2);
    assert!(
        operations(&script).is_empty(),
        "a row with an unmet prerequisite is inert"
    );
    h.get_all_by_role(accesskit::Role::ComboBox)
        .find(|node| node.accesskit_node().value().as_deref() == Some("No"))
        .expect("the editor's combo box shows its current value")
        .click();
    h.run_steps(2);
    h.get_by_label("All").click();
    h.run_steps(2);
    assert_eq!(
        operations(&script),
        [json!({"kind":"fill","field_id":"option:DROP_EMPTY","value":"all","submit":true})]
    );
}

#[test]
fn a_key_prompt_fills_its_field_with_the_next_key_and_a_function_key_by_its_name() {
    let (mut h, script) = harness();
    let field = json!({"id":"field:key","label":"New key","description":"","value":"","type":"key",
        "max_length":-1,"printable":false});
    open_settings(&mut h, &script, Some(field));
    h.get_by_label("Press the new key");
    h.event(egui::Event::Text("z".into()));
    h.run_steps(3);
    assert_eq!(
        operations(&script),
        [json!({"kind":"fill","field_id":"field:key","value":"z","submit":true})]
    );
}

mod failures {
    use super::*;
    use bn_client_core::transport::ScriptedTransport;

    fn dying<'a>() -> (H<'a>, ScriptHandle, ScriptHandle) {
        let (t, first) = session();
        let (next, second) = session();
        let mut spare = Some(next);
        let model = EngineModel::new(Box::new(t)).with_respawn(Box::new(move || {
            Ok(Box::new(spare.take().expect("one restart"))
                as Box<dyn bn_client_egui::transport::Transport>)
        }));
        let _: &ScriptedTransport;
        let harness = Harness::builder()
            .with_size(egui::vec2(1100.0, 800.0))
            .build_ui_state(
                |ui, app: &mut App<EngineModel>| app.show(ui),
                App::new(model),
            );
        (harness, first, second)
    }

    #[test]
    fn a_dead_engine_shows_its_stderr_not_a_frozen_map_and_restart_relaunches() {
        let (mut h, first, second) = dying();
        h.run_steps(3);
        h.get_by_label("Map");
        first.close(&["loading data", "Segmentation fault"]);
        h.run_steps(3);
        h.get_by_label("The engine stopped.");
        h.get_by_label("Segmentation fault");
        assert!(h.query_by_label("Map").is_none(), "no frozen map behind it");
        h.get_by_label("Restart").click();
        h.run_steps(4);
        assert_eq!(second.sent_methods()[..2], ["bn.hello", "bn.subscribe"]);
        h.get_by_label("Map");
        assert!(h.query_by_label("Restart").is_none());
    }
}

mod overmap {
    use super::*;

    fn open_overmap<'a>() -> (H<'a>, ScriptHandle) {
        let (mut h, script) = harness();
        h.run_steps(3);
        script.push(common::overmap_event(100));
        h.run_steps(3);
        (h, script)
    }

    #[test]
    fn the_overmap_replaces_the_map_and_names_the_cursor_square() {
        let (h, _script) = open_overmap();
        h.get_by_label("Overmap view");
        h.get_by_label("Overmap (100, 200, 0).");
        assert!(
            h.query_by_label("Map").is_none(),
            "the map is not drawn under it"
        );
        h.get_by_label("  forest");
    }

    #[test]
    fn clicking_a_square_moves_the_cursor_and_clicking_the_cursor_travels() {
        let (mut h, script) = open_overmap();
        let origin = h.get_by_label("Overmap view").rect().min;
        let click = |h: &mut H, col: f32, row: f32| {
            let pos = origin + egui::vec2((col + 0.5) * CELL, (row + 0.5) * CELL);
            h.hover_at(pos);
            h.run_steps(1);
            for pressed in [true, false] {
                h.event(egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                });
                h.run_steps(1);
            }
            h.run_steps(2);
        };
        // Cell (3, 0) is square (101, 199).
        click(&mut h, 3.0, 0.0);
        let ops = operations(&script);
        assert_eq!(
            ops.last().unwrap(),
            &json!({"kind":"set_target","pos":{"dim":"","x":101,"y":199,"z":0}})
        );
    }

    #[test]
    fn escape_leaves_the_overmap_through_the_engines_quit_action() {
        let (mut h, script) = open_overmap();
        h.key_press(egui::Key::Escape);
        h.run_steps(2);
        assert_eq!(
            operations(&script).last().unwrap(),
            &json!({"kind":"action","action_id":"QUIT"})
        );
    }
}
