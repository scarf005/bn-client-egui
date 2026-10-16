//! A step of the map is one command per key press, as in the native game; once the OS repeats the key it is held
//! input state: the walk starts, key up stops it, and further repeats send nothing.
//! Every other key at the map is a screen-scoped affordance, which the engine queues behind the running one.
mod common;
use bn_client_egui::transport::{ScriptHandle, ScriptedTransport};
use bn_client_egui::{App, EngineModel};
use common::fixture;
use eframe::egui;
use egui_kittest::Harness;
use serde_json::{Value, json};

type H<'a> = Harness<'a, App<EngineModel>>;

/// A session at the map, whose engine holds `queue` commands behind the running one.
fn walking<'a>(queue: u64) -> (H<'a>, ScriptHandle) {
    let mut hello = fixture("hello_result");
    hello["result"]["limits"]["screen_queue"] = json!(queue);
    let mut subscribe = fixture("subscribe_result");
    subscribe["result"]["interaction"]["stack"] = json!([{
        "screen_id": "screen:map:e1", "role": "map",
        "root": {"id": "map:world", "kind": "map", "affordances": [], "mode": "world"},
        "affordances": [
            {"id": "RIGHT", "label": "Move East", "keys": ["RIGHT", "l"],
                "enabled": true, "expect": "screen", "holdable": true},
            {"id": "UP", "label": "Move North", "keys": ["UP", "k"],
                "enabled": true, "expect": "screen", "holdable": true},
            {"id": "pickup", "label": "Pick up", "keys": ["g"], "enabled": true, "expect": "screen"}]}]);
    let reply = vec![
        subscribe,
        fixture("snapshot_part_0"),
        fixture("snapshot_part_1"),
    ];
    let (t, script) = ScriptedTransport::new();
    let t = (0..20).fold(
        t.on("bn.hello", vec![hello])
            .on("bn.subscribe", reply.clone())
            .on("bn.subscribe", reply),
        |t, n| {
            let mut received = fixture("submit_result");
            received["result"]["command_id"] = json!(format!("c:{n}"));
            t.on("bn.command.submit", vec![received])
        },
    );
    let app = App::new(EngineModel::new(Box::new(t)));
    let h = Harness::builder()
        .with_size(egui::vec2(1100.0, 800.0))
        .build_ui_state(|ui, app: &mut App<EngineModel>| app.show(ui), app);
    (h, script)
}

fn sent(script: &ScriptHandle, method: &str) -> Vec<Value> {
    script
        .sent()
        .into_iter()
        .filter(|m| m["method"] == method)
        .map(|m| m["params"].clone())
        .collect()
}

fn key(h: &mut H, key: egui::Key, pressed: bool, repeat: bool) {
    h.event(egui::Event::Key {
        key,
        physical_key: Some(key),
        pressed,
        repeat,
        modifiers: egui::Modifiers::NONE,
    });
    h.run_steps(1);
}

#[test]
fn key_down_takes_one_step_and_no_hold() {
    let (mut h, script) = walking(8);
    h.run_steps(3);
    key(&mut h, egui::Key::ArrowRight, true, false);
    let steps = sent(&script, "bn.command.submit");
    assert_eq!(steps.len(), 1);
    assert_eq!(steps[0]["operation"]["affordance_id"], "RIGHT");
    assert!(sent(&script, "bn.hold").is_empty(), "a press is not a hold");
    key(&mut h, egui::Key::ArrowRight, false, false);
    assert!(sent(&script, "bn.release").is_empty());
    assert_eq!(sent(&script, "bn.command.submit").len(), 1);
}

#[test]
fn a_repeated_key_holds_the_step_and_key_up_releases_it() {
    let (mut h, script) = walking(8);
    h.run_steps(3);
    key(&mut h, egui::Key::ArrowRight, true, false);
    key(&mut h, egui::Key::ArrowRight, true, true);
    let holds = sent(&script, "bn.hold");
    assert_eq!(holds.len(), 1);
    assert_eq!(holds[0]["screen_id"], "screen:map:e1");
    assert_eq!(holds[0]["affordance_id"], "RIGHT");
    assert!(holds[0]["epoch"].is_string());
    assert!(sent(&script, "bn.release").is_empty(), "still down");
    key(&mut h, egui::Key::ArrowRight, false, false);
    assert_eq!(sent(&script, "bn.release").len(), 1);
    assert_eq!(
        sent(&script, "bn.command.submit").len(),
        1,
        "only the press was a command"
    );
}

#[test]
fn key_repeats_send_nothing() {
    let (mut h, script) = walking(8);
    h.run_steps(3);
    key(&mut h, egui::Key::ArrowRight, true, false);
    for _ in 0..12 {
        key(&mut h, egui::Key::ArrowRight, true, true);
    }
    assert_eq!(sent(&script, "bn.hold").len(), 1);
    assert_eq!(sent(&script, "bn.command.submit").len(), 1);
    assert!(sent(&script, "bn.release").is_empty());
}

#[test]
fn a_tap_inside_one_frame_takes_one_step() {
    let (mut h, script) = walking(8);
    h.run_steps(3);
    h.key_press(egui::Key::ArrowRight);
    h.run_steps(2);
    assert_eq!(sent(&script, "bn.command.submit").len(), 1);
    assert!(sent(&script, "bn.hold").is_empty());
}

#[test]
fn a_typed_step_key_takes_one_step() {
    let (mut h, script) = walking(8);
    h.run_steps(3);
    key(&mut h, egui::Key::L, true, false);
    h.event(egui::Event::Text("l".into()));
    h.run_steps(1);
    key(&mut h, egui::Key::L, false, false);
    assert_eq!(sent(&script, "bn.command.submit").len(), 1);
    assert!(sent(&script, "bn.hold").is_empty());
}

#[test]
fn losing_the_keyboard_releases_the_hold() {
    let (mut h, script) = walking(8);
    h.run_steps(3);
    key(&mut h, egui::Key::ArrowRight, true, false);
    key(&mut h, egui::Key::ArrowRight, true, true);
    h.event(egui::Event::WindowFocused(false));
    h.run_steps(2);
    assert_eq!(sent(&script, "bn.release").len(), 1);
}

#[test]
fn a_key_that_is_not_a_step_still_queues_in_the_engine() {
    let (mut h, script) = walking(8);
    h.run_steps(3);
    for _ in 0..12 {
        h.event(egui::Event::Text("g".into()));
        h.run_steps(1);
    }
    let submits = sent(&script, "bn.command.submit");
    assert_eq!(submits.len(), 9, "the running command and eight behind it");
    for command in &submits {
        assert_eq!(command["expect"], json!({"screen_id": "screen:map:e1"}));
        assert_eq!(command["operation"]["affordance_id"], "pickup");
    }
    assert!(sent(&script, "bn.hold").is_empty());
}

#[test]
fn a_key_that_runs_nothing_is_not_sent() {
    let (mut h, script) = walking(8);
    h.run_steps(3);
    h.event(egui::Event::Text("q".into()));
    h.run_steps(2);
    assert!(sent(&script, "bn.command.submit").is_empty());
    assert!(sent(&script, "bn.hold").is_empty());
}

#[test]
fn releasing_the_walk_key_ends_the_walk_while_another_key_stays_down() {
    let (mut h, script) = walking(8);
    h.run_steps(3);
    key(&mut h, egui::Key::ArrowRight, true, false);
    key(&mut h, egui::Key::ArrowRight, true, true);
    key(&mut h, egui::Key::Q, true, false);
    assert!(sent(&script, "bn.release").is_empty(), "still down");
    key(&mut h, egui::Key::ArrowRight, false, false);
    assert_eq!(sent(&script, "bn.release").len(), 1);
}

#[test]
fn releasing_the_later_of_two_pressed_steps_ends_the_walk() {
    let (mut h, script) = walking(8);
    h.run_steps(3);
    key(&mut h, egui::Key::ArrowUp, true, false);
    key(&mut h, egui::Key::ArrowUp, true, true);
    key(&mut h, egui::Key::ArrowRight, true, false);
    key(&mut h, egui::Key::ArrowRight, true, true);
    assert_eq!(
        sent(&script, "bn.hold").len(),
        2,
        "the later key takes the walk"
    );
    key(&mut h, egui::Key::ArrowRight, false, false);
    assert_eq!(
        sent(&script, "bn.release").len(),
        1,
        "the earlier key is still down"
    );
}
