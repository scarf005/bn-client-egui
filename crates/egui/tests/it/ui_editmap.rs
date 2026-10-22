//! The debug map editor's screens as a real engine published them (`crates/core/tests/fixtures/editmap/`).
use crate::common::fixture;
use bn_client_egui::transport::{ScriptHandle, ScriptedTransport};
use bn_client_egui::{App, Clock, ConnectOptions, EngineModel};
use eframe::egui;
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use serde_json::Value;

type H<'a> = Harness<'a, App<EngineModel>>;

fn editmap(name: &str) -> Value {
    let path = format!(
        "{}/../core/tests/fixtures/editmap/{name}.json",
        env!("CARGO_MANIFEST_DIR")
    );
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

/// A session at the stack of `capture`, whose further pages of rows the engine answers in turn.
fn harness<'a>(capture: &Value, size: egui::Vec2) -> (H<'a>, ScriptHandle) {
    let mut subscribe = fixture("subscribe_result");
    subscribe["result"]["interaction"]["stack"] = capture["stack"].clone();
    let boundary = subscribe["result"]["interaction"]["boundary_id"].clone();
    let reply = vec![
        subscribe,
        fixture("snapshot_part_0"),
        fixture("snapshot_part_1"),
    ];
    let (t, script) = ScriptedTransport::new();
    let mut t = t
        .on("bn.hello", vec![fixture("hello_result")])
        .on("bn.subscribe", reply.clone())
        .on("bn.subscribe", reply)
        .on("bn.command.submit", vec![fixture("submit_result")]);
    for page in capture["pages"].as_array().into_iter().flatten() {
        let mut page = page.clone();
        page["result"]["boundary_id"] = boundary.clone();
        t = t.on("bn.interaction.choices", vec![page]);
    }
    let clock = Clock::manual();
    let model = EngineModel::open(Box::new(t), ConnectOptions::default()).with_clock(clock.clone());
    let mut harness = Harness::builder().with_size(size).build_ui_state(
        |ui, app: &mut App<EngineModel>| app.show(ui),
        App::new(model).with_clock(clock),
    );
    harness.run_steps(4);
    crate::common::settle_windows(&mut harness);
    (harness, script)
}

/// The rows of the picker drawn now.
fn rows_drawn(h: &H, capture: &Value) -> usize {
    let labels = capture["stack"][2]["root"]["rows"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|r| r["label"].as_str())
        .collect::<Vec<_>>();
    labels
        .iter()
        .filter(|l| h.query_by_label(l).is_some())
        .count()
}

#[test]
fn the_terrain_picker_opens_on_the_terrain_under_the_cursor_past_its_first_page() {
    let capture = editmap("terrain-picker");
    let (mut h, _script) = harness(&capture, egui::vec2(1280.0, 800.0));
    for _ in 0..10 {
        h.run_steps(2);
    }
    h.get_by_label("Filter");
    let row = h
        .query_by_label("white wall (t_wall_w)")
        .expect("the focused row is drawn");
    assert_eq!(
        row.accesskit_node().toggled(),
        Some(egui::accesskit::Toggled::True)
    );
}

#[test]
fn the_furniture_picker_never_shows_fewer_rows_in_a_larger_window() {
    let capture = editmap("furniture-picker");
    let (large, _script) = harness(&capture, egui::vec2(1920.0, 1080.0));
    let (small, _script) = harness(&capture, egui::vec2(1280.0, 800.0));
    let (large, small) = (rows_drawn(&large, &capture), rows_drawn(&small, &capture));
    assert!(
        small > 0 && large >= small,
        "{large} rows at 1920x1080, {small} at 1280x800"
    );
}

/// The engine's next stack, bottom first.
fn restacked(stack: &[Value]) -> Value {
    serde_json::json!({"jsonrpc": "2.0", "method": "bn.events", "params": {"epoch": "e7f3", "events": [{
        "sequence": "41", "revision": "32", "type": "interaction.changed", "changes": {"interaction": {
        "boundary_id": "b:41",
        "readiness": {"phase": "playing", "game_ready": true, "accepts_interaction_commands": true,
            "accepts_registered_actions": true},
        "actions": [], "interaction": null, "stack": stack}}}]}})
}

#[test]
fn closing_a_picker_does_not_move_the_cursor_to_the_pointer_left_where_it_was() {
    let capture = editmap("terrain-picker");
    let (mut h, script) = harness(&capture, egui::vec2(1280.0, 800.0));
    let picker = h.get_by_label("Filter").rect().center();
    h.event(egui::Event::PointerMoved(picker));
    h.run_steps(4);
    let editor = capture["stack"].as_array().unwrap()[..2].to_vec();
    script.push(restacked(&editor));
    h.run_steps(8);
    assert!(h.query_all_by_label("Map editor").count() > 0);
    let moves = script
        .sent()
        .into_iter()
        .map(|m| m["params"]["operation"].clone())
        .filter(|op| op["kind"] == "set_target" || op["affordance_id"] == "target:move")
        .collect::<Vec<_>>();
    assert!(moves.is_empty(), "{moves:?}");
}

#[test]
fn the_terrain_picker_shows_the_rows_after_the_terrain_under_the_cursor() {
    let capture = editmap("terrain-picker");
    let (mut h, _script) = harness(&capture, egui::vec2(1280.0, 800.0));
    for _ in 0..10 {
        h.run_steps(2);
    }
    let next = h.get_by_label("blue wall (t_wall_b)").rect();
    let description = h.get_by_label_contains("Move cost: 0").rect();
    assert!(
        next.bottom() <= description.top(),
        "{next:?} over {description:?}"
    );
}

#[test]
fn tab_in_the_terrain_picker_is_the_engines_and_leaves_the_toolbar_unfocused() {
    let capture = editmap("terrain-picker");
    let (mut h, script) = harness(&capture, egui::vec2(1280.0, 800.0));
    // Nothing has the keyboard, as after a menu of the client closed.
    h.ctx
        .memory_mut(|m| m.focused().map(|id| m.surrender_focus(id)));
    h.key_press(egui::Key::Tab);
    h.run_steps(4);
    let invoked = script
        .sent()
        .into_iter()
        .any(|m| m["params"]["operation"]["affordance_id"] == "EDITMAP_TAB");
    assert!(invoked);
    assert!(!h.get_by_label("Actions").accesskit_node().is_focused());
}
