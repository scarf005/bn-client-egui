#![recursion_limit = "512"]
//! The map zoom is the client's: the keys the engine binds to `zoom_in` and `zoom_out`, and the wheel over the map.
mod common;
use bn_client_egui::transport::{ScriptHandle, ScriptedTransport};
use bn_client_egui::{App, EngineModel};
use common::{fixture, subscribe_reply};
use eframe::egui;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use serde_json::json;

type H<'a> = Harness<'a, App<EngineModel>>;

/// A window whose engine binds `zoom_in` to `+` and `zoom_out` to `-`, as the second key of each.
fn zooming<'a>(settings: Option<std::path::PathBuf>) -> (H<'a>, ScriptHandle) {
    let mut reply = subscribe_reply();
    let actions = &mut reply[0]["result"]["interaction"]["actions"];
    actions.as_array_mut().unwrap().extend([
        json!({"id":"zoom_in","name":"Zoom in","keys":["z","+"]}),
        json!({"id":"zoom_out","name":"Zoom out","keys":["Z","-"]}),
    ]);
    let (t, script) = ScriptedTransport::new();
    let t = t
        .on("bn.hello", vec![fixture("hello_result")])
        .on("bn.subscribe", reply.clone())
        .on("bn.subscribe", reply);
    let mut app = App::new(EngineModel::new(Box::new(t)));
    if let Some(path) = settings {
        app = app.with_settings(path);
    }
    let mut h = Harness::builder()
        .with_size(egui::vec2(1100.0, 800.0))
        .build_ui_state(|ui, app: &mut App<EngineModel>| app.show(ui), app);
    h.run_steps(4);
    (h, script)
}

fn type_text(h: &mut H, text: &str) {
    h.event(egui::Event::Text(text.into()));
    h.run_steps(3);
}

fn sent_actions(script: &ScriptHandle) -> usize {
    script
        .sent()
        .iter()
        .filter(|m| m["method"] == "bn.command.submit")
        .count()
}

fn wheel(h: &mut H, dy: f32) {
    let at = h.get_by_label("Map").rect().center();
    h.hover_at(at);
    h.run_steps(2);
    h.event(egui::Event::MouseWheel {
        unit: egui::MouseWheelUnit::Line,
        delta: egui::vec2(0.0, dy),
        phase: egui::TouchPhase::Move,
        modifiers: egui::Modifiers::NONE,
    });
    h.run_steps(3);
}

#[test]
fn the_keys_bound_to_zoom_in_and_out_zoom_the_map_and_reach_no_engine_action() {
    let (mut h, script) = zooming(None);
    let before = h.state().display().zoom();
    type_text(&mut h, "z");
    let bigger = h.state().display().zoom();
    assert!(bigger > before, "{before} -> {bigger}");
    type_text(&mut h, "-");
    type_text(&mut h, "-");
    assert!(h.state().display().zoom() < before);
    assert_eq!(sent_actions(&script), 0);
}

#[test]
fn a_key_the_engine_binds_to_no_zoom_action_does_not_zoom() {
    let (mut h, _script) = zooming(None);
    let before = h.state().display().zoom();
    type_text(&mut h, "q");
    assert_eq!(h.state().display().zoom(), before);
}

#[test]
fn the_wheel_over_the_map_zooms_it_and_is_bounded() {
    let (mut h, _script) = zooming(None);
    let before = h.state().display().zoom();
    wheel(&mut h, 1.0);
    assert!(h.state().display().zoom() > before);
    for _ in 0..40 {
        wheel(&mut h, 1.0);
    }
    let most = h.state().display().zoom();
    wheel(&mut h, 1.0);
    assert_eq!(h.state().display().zoom(), most, "zoom in stops");
    for _ in 0..80 {
        wheel(&mut h, -1.0);
    }
    let least = h.state().display().zoom();
    assert!(least < before && least > 0.0);
    wheel(&mut h, -1.0);
    assert_eq!(h.state().display().zoom(), least, "zoom out stops");
}

#[test]
fn a_zoomed_out_map_asks_the_engine_for_a_larger_viewport() {
    let (mut h, script) = zooming(None);
    let cols = |script: &ScriptHandle| {
        script
            .sent()
            .iter()
            .rev()
            .find(|m| m["method"] == "bn.viewport")
            .map(|m| m["params"]["cols"].as_u64().unwrap())
    };
    let settle = |h: &mut H| {
        std::thread::sleep(std::time::Duration::from_millis(300));
        h.run_steps(10);
    };
    settle(&mut h);
    let before = cols(&script);
    for _ in 0..3 {
        type_text(&mut h, "Z");
    }
    settle(&mut h);
    assert!(cols(&script) > before, "{before:?} -> {:?}", cols(&script));
}

#[test]
fn the_zoom_survives_a_restart() {
    let dir = common::game_root("zoom-settings");
    let path = dir.join("settings.json");
    let (mut h, _script) = zooming(Some(path.clone()));
    type_text(&mut h, "z");
    let kept = h.state().display().zoom();
    drop(h);
    let (h, _script) = zooming(Some(path));
    assert_eq!(h.state().display().zoom(), kept);
}
