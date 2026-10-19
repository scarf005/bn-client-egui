//! The debug map editor's screens as a real engine published them (`crates/core/tests/fixtures/editmap/`).
mod common;
use bn_client_egui::transport::{ScriptHandle, ScriptedTransport};
use bn_client_egui::{App, Clock, ConnectOptions, EngineModel};
use common::fixture;
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
    common::settle_windows(&mut harness);
    (harness, script)
}

#[test]
fn the_terrain_picker_opens_on_the_terrain_under_the_cursor_past_its_first_page() {
    let capture = editmap("terrain-picker");
    let (mut h, _script) = harness(&capture, egui::vec2(1280.0, 800.0));
    for _ in 0..10 {
        h.run_steps(2);
    }
    h.get_by_label("Filter [f]");
    let row = h
        .query_by_label("white wall (t_wall_w)")
        .expect("the focused row is drawn");
    assert_eq!(
        row.accesskit_node().toggled(),
        Some(egui::accesskit::Toggled::True)
    );
}
