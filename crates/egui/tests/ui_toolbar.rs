//! The shortcut bar at the map's left edge: the engine publishes which buttons exist, their labels and keys;
//! a click is an invoke of the map's registered action of that id.
mod common;
use bn_client_egui::transport::{ScriptHandle, ScriptedTransport};
use bn_client_egui::{App, Clock, ConnectOptions, EngineModel};
use common::fixture;
use eframe::egui::{self, accesskit::Role};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use serde_json::{Value, json};

type H<'a> = Harness<'a, App<EngineModel>>;

const SHORTCUTS: [(&str, &str, &str, &str, bool); 4] = [
    ("inventory", "Inventory", "i", "I", true),
    ("craft", "Crafting", "&", "C", true),
    ("pickup", "Pick up", "g", "P", true),
    ("sleep", "Sleep", "$", "Z", false),
];

fn map_screen() -> Value {
    let shortcuts = SHORTCUTS
        .iter()
        .map(|(id, label, key, glyph, enabled)| {
            json!({"affordance_id": id, "label": label, "keys": [key], "icon": id, "glyph": glyph,
                "enabled": enabled})
        })
        .collect::<Vec<_>>();
    let affordances = SHORTCUTS
        .iter()
        .filter(|s| s.4)
        .map(|(id, label, key, ..)| {
            json!({"id": id, "label": label, "keys": [key], "enabled": true, "expect": "screen",
                "args": {}})
        })
        .collect::<Vec<_>>();
    json!({"screen_id": "screen:1", "role": "map",
        "root": {"id": "map", "kind": "map", "affordances": [], "mode": "world"},
        "affordances": affordances, "shortcuts": shortcuts})
}

fn harness<'a>() -> (H<'a>, ScriptHandle) {
    let mut subscribe = fixture("subscribe_result");
    subscribe["result"]["interaction"]["stack"] = json!([map_screen()]);
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
    let model = EngineModel::open(Box::new(t), ConnectOptions::default()).with_clock(clock.clone());
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1100.0, 800.0))
        .build_ui_state(
            |ui, app: &mut App<EngineModel>| app.show(ui),
            App::new(model).with_clock(clock),
        );
    harness.run_steps(4);
    (harness, script)
}

fn operations(script: &ScriptHandle) -> Vec<Value> {
    script
        .sent()
        .into_iter()
        .filter(|m| m["method"] == "bn.command.submit")
        .map(|m| m["params"]["operation"].clone())
        .collect()
}

#[test]
fn every_published_shortcut_is_a_button_named_by_the_engine() {
    let (h, _) = harness();
    for (_, label, ..) in SHORTCUTS {
        h.get_by_role_and_label(Role::Button, label);
    }
}

#[test]
fn clicking_each_enabled_shortcut_invokes_the_registered_action_of_its_id() {
    for (id, label, _, _, enabled) in SHORTCUTS.iter().filter(|s| s.4) {
        let (mut h, script) = harness();
        assert!(enabled);
        h.get_by_role_and_label(Role::Button, label).click();
        h.run_steps(3);
        assert_eq!(
            operations(&script),
            [json!({"kind": "invoke", "affordance_id": id, })],
            "{label}"
        );
    }
}

#[test]
fn a_shortcut_the_engine_cannot_run_now_is_disabled_and_sends_nothing() {
    let (mut h, script) = harness();
    assert!(
        h.get_by_role_and_label(Role::Button, "Sleep")
            .accesskit_node()
            .is_disabled()
    );
    h.get_by_role_and_label(Role::Button, "Sleep").click();
    h.run_steps(3);
    assert!(operations(&script).is_empty());
}

#[test]
fn the_tooltip_of_a_shortcut_names_its_bound_key() {
    let (mut h, _) = harness();
    h.get_by_role_and_label(Role::Button, "Inventory").hover();
    h.run_steps(4);
    h.get_by_label("Inventory [i]");
}
