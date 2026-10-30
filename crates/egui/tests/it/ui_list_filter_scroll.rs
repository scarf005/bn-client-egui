//! A list whose filter text changes shows its first rows again, as a search box's results start at the top.
use crate::common::fixture;
use bn_client_egui::transport::ScriptHandle;
use bn_client_egui::{App, EngineModel};
use eframe::egui;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use serde_json::{Value, json};

type H<'a> = Harness<'a, App<EngineModel>>;

/// The vehicle editor with a prototype open, its parts list holding `count` rows that match `filter`.
fn editor(filter: &str, count: usize) -> Value {
    let path = format!(
        "{}/../core/tests/fixtures/0.2.examples/vehicle_editor.json",
        env!("CARGO_MANIFEST_DIR")
    );
    let all = serde_json::from_str::<Value>(&std::fs::read_to_string(path).unwrap()).unwrap();
    let mut screen = all["valid"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["name"] == "vehicle editor with a prototype open on a mount")
        .unwrap()["value"]
        .clone();
    fn parts(c: &mut Value) -> Option<&mut Value> {
        if c["kind"] == "list" && c["id"] == "vehicle:parts" {
            return Some(c);
        }
        match c {
            Value::Object(m) => m.values_mut().find_map(parts),
            Value::Array(a) => a.iter_mut().find_map(parts),
            _ => None,
        }
    }
    let list = parts(&mut screen).unwrap();
    let row = list["rows"][0].clone();
    list["rows"] = (0..count)
        .map(|i| {
            let mut r = row.clone();
            r["id"] = format!("part:{filter}{i}").into();
            r["label"] = format!("{filter} part {i}").into();
            r["selected"] = false.into();
            r
        })
        .collect();
    list["total"] = count.into();
    list["filter"] = filter.into();
    screen
}

fn push(script: &ScriptHandle, sequence: u32, screen: Value) {
    let map = fixture("subscribe_result")["result"]["interaction"]["stack"][0].clone();
    script.push(
        json!({"jsonrpc": "2.0", "method": "bn.events", "params": {"epoch": "e7f3", "events": [{
        "sequence": sequence.to_string(), "revision": (sequence - 9).to_string(),
        "type": "interaction.changed", "changes": {"interaction": {
        "boundary_id": format!("b:{sequence}"),
        "readiness": {"phase": "playing", "game_ready": true, "accepts_interaction_commands": true,
            "accepts_registered_actions": true},
        "actions": [], "interaction": null, "stack": [map, screen]}}}]}}),
    );
}

fn open<'a>(screen: Value) -> (H<'a>, ScriptHandle) {
    let reply = vec![
        fixture("subscribe_result"),
        fixture("snapshot_part_0"),
        fixture("snapshot_part_1"),
    ];
    let (t, script) = bn_client_egui::transport::ScriptedTransport::new();
    let t = t
        .on("bn.hello", vec![fixture("hello_result")])
        .on("bn.subscribe", reply.clone())
        .on("bn.subscribe", reply);
    let clock = bn_client_egui::Clock::manual();
    let model = EngineModel::open(Box::new(t), bn_client_egui::ConnectOptions::default())
        .with_clock(clock.clone());
    let app = App::new(model).with_clock(clock);
    let mut h = Harness::builder()
        .with_size(egui::vec2(1280.0, 800.0))
        .build_ui_state(|ui, app: &mut App<EngineModel>| app.show(ui), app);
    bn_client_egui::kit::tokens::apply(&h.ctx);
    bn_client_egui::fonts::install_bundled(&h.ctx);
    h.run_steps(4);
    push(&script, 41, screen);
    h.run_steps(8);
    (h, script)
}

fn top_of(h: &H, label: &str) -> f32 {
    h.get_by_label_contains(label).rect().top()
}

#[test]
fn a_new_filter_text_brings_the_list_back_to_its_first_rows() {
    let (mut h, script) = open(editor("a", 60));
    let first = top_of(&h, "a part 0");
    let over = h.get_by_label_contains("a part 3").rect().center();
    for _ in 0..4 {
        h.hover_at(over);
        h.run_steps(2);
        h.event(egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: egui::vec2(0.0, -200.0),
            modifiers: egui::Modifiers::NONE,
            phase: egui::TouchPhase::Move,
        });
        h.run_steps(4);
    }
    assert!(
        h.query_by_label_contains("a part 0").is_none(),
        "the wheel scrolls the first rows out of sight"
    );
    push(&script, 42, editor("ab", 60));
    h.run_steps(8);
    assert!(
        (top_of(&h, "ab part 0") - first).abs() < 1.0,
        "the rows of the new filter start at the top"
    );
}
