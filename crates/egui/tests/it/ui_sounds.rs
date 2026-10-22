use crate::common::{fixture, session};
use bn_client_egui::transport::ScriptHandle;
use bn_client_egui::{App, EngineModel};
use eframe::egui;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use serde_json::{Value, json};
use std::time::Duration;

type H<'a> = Harness<'a, App<EngineModel>>;

fn harness<'a>() -> (H<'a>, ScriptHandle) {
    let (t, h) = session();
    let clock = bn_client_core::Clock::manual();
    let model = EngineModel::new(Box::new(t)).with_clock(clock.clone());
    let harness = Harness::builder()
        .with_size(egui::vec2(1100.0, 800.0))
        .build_ui_state(
            |ui, app: &mut App<EngineModel>| app.show(ui),
            App::new(model).with_clock(clock),
        );
    (harness, h)
}

fn heard(sequence: u32, text: &str, logged: bool) -> Value {
    json!({"jsonrpc": "2.0", "method": "bn.events", "params": {"epoch": "e7f3", "events": [{
        "sequence": sequence.to_string(), "revision": "32", "type": "sound.heard",
        "data": {"text": text, "volume": 52, "logged": logged}}]}})
}

#[test]
fn unlogged_sounds_are_grouped_and_leave_after_their_lifetime() {
    let (mut h, script) = harness();
    h.run_steps(3);
    script.push(fixture("events_41_walk"));
    script.push(heard(42, "gargling grunting", false));
    script.push(heard(43, "gargling grunting", false));
    script.push(heard(44, "a door slamming", false));
    script.push(heard(45, "Zoe saying hello", true));
    h.run_steps(4);
    h.get_by_label("gargling grunting ×2");
    h.get_by_label("a door slamming");
    assert!(h.query_by_label("Zoe saying hello").is_none());
    h.state().clock().advance(Duration::from_secs(13));
    h.run_steps(3);
    assert!(h.query_by_label("a door slamming").is_none());
    assert!(h.query_by_label("gargling grunting ×2").is_none());
}
