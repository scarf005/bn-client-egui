//! Keys held at the map go to the engine as screen-scoped affordances, which it queues behind the running one.
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
        "affordances": [{"id": "RIGHT", "label": "Move East", "keys": ["RIGHT", "l"],
            "enabled": true, "expect": "screen"}]}]);
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

fn submits(script: &ScriptHandle) -> Vec<Value> {
    script
        .sent()
        .into_iter()
        .filter(|m| m["method"] == "bn.command.submit")
        .map(|m| m["params"].clone())
        .collect()
}

/// The key repeats the OS sends while a held key's step is still running.
fn repeat(h: &mut H, key: egui::Key, times: usize) {
    for _ in 0..times {
        h.key_press(key);
        h.run_steps(1);
    }
}

#[test]
fn a_held_arrow_sends_every_repeat_to_the_engine_up_to_the_depth_of_its_queue() {
    let (mut h, script) = walking(8);
    h.run_steps(3);
    repeat(&mut h, egui::Key::ArrowRight, 12);
    let sent = submits(&script);
    assert_eq!(sent.len(), 9, "the running step and eight behind it");
    for command in &sent {
        assert_eq!(command["expect"], json!({"screen_id": "screen:map:e1"}));
        assert_eq!(command["operation"]["affordance_id"], "RIGHT");
    }
}

#[test]
fn a_tap_while_a_command_runs_waits_in_the_engine_not_in_the_client() {
    let (mut h, script) = walking(1);
    h.run_steps(3);
    repeat(&mut h, egui::Key::ArrowRight, 2);
    assert_eq!(submits(&script).len(), 2, "sent at once");
}

#[test]
fn a_key_that_runs_nothing_is_not_sent() {
    let (mut h, script) = walking(8);
    h.run_steps(3);
    h.event(egui::Event::Text("z".into()));
    h.run_steps(2);
    assert!(submits(&script).is_empty());
}
