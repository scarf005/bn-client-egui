mod common;
use bn_client_egui::app::CELL;
use bn_client_egui::transport::ScriptHandle;
use bn_client_egui::{App, EngineModel, GameModel};
use common::{fixture, session};
use eframe::egui;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use serde_json::json;

type H<'a> = Harness<'a, App<EngineModel>>;

fn harness<'a>() -> (H<'a>, ScriptHandle) {
    let (t, h) = session();
    let t = t.on("bn.command.submit", vec![fixture("submit_result")]);
    let model = EngineModel::new(Box::new(t));
    let harness = Harness::builder()
        .with_size(egui::vec2(1100.0, 800.0))
        .build_ui_state(
            |ui, app: &mut App<EngineModel>| app.show(ui),
            App::new(model),
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
fn engine_map_renders_and_a_click_submits_a_move_through_the_protocol() {
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
    assert_eq!(sent.len(), 1, "one step in flight at a time");
    assert_eq!(
        sent[0]["params"]["operation"],
        json!({"kind":"action","action_id":"RIGHT"})
    );
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
    h.get_by_label("Pick up");
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
