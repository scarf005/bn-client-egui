//! The vehicle editor around its canvas: at 1280×800 and 1920×1080 the vehicle's mounts are tile-sized squares in the
//! middle of the canvas pane, every part of the stack reads whole, the zoom gesture over the canvas scales its mounts,
//! and a mount names its parts on hover.
use crate::common::{fixture, legible};
use bn_client_egui::transport::ScriptedTransport;
use bn_client_egui::{App, Clock, ConnectOptions, EngineModel};
use eframe::egui::{self, epaint};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use serde_json::{Value, json};

type H<'a> = Harness<'a, App<EngineModel>>;

fn find<'a>(value: &'a Value, id: &str) -> Option<&'a Value> {
    if value["id"] == id {
        return Some(value);
    }
    match value {
        Value::Array(items) => items.iter().find_map(|v| find(v, id)),
        Value::Object(members) => members.values().find_map(|v| find(v, id)),
        _ => None,
    }
}

/// The vehicle editor editing a spawned bicycle in place, its mount's part stack shown.
fn editor() -> Value {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../core/tests/fixtures/0.2.examples/vehicle_editor.json"
    );
    let all = serde_json::from_str::<Value>(&std::fs::read_to_string(path).unwrap()).unwrap();
    all["valid"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["name"] == "vehicle editor editing a world vehicle in place")
        .unwrap()["value"]
        .clone()
}

fn open<'a>(screen: Value, size: egui::Vec2) -> H<'a> {
    let mut subscribe = fixture("subscribe_result");
    subscribe["result"]["interaction"]["stack"] = json!([screen]);
    let reply = vec![
        subscribe,
        fixture("snapshot_part_0"),
        fixture("snapshot_part_1"),
    ];
    let (t, _) = ScriptedTransport::new();
    let t = t
        .on("bn.hello", vec![fixture("hello_result")])
        .on("bn.subscribe", reply.clone())
        .on("bn.subscribe", reply);
    let clock = Clock::manual();
    let model = EngineModel::open(Box::new(t), ConnectOptions::default()).with_clock(clock.clone());
    let mut h = Harness::builder().with_size(size).build_ui_state(
        |ui, app: &mut App<EngineModel>| app.show(ui),
        App::new(model).with_clock(clock),
    );
    h.run_steps(8);
    h
}

/// The labels of the rows of the list `id` in `screen`.
fn labels(screen: &Value, id: &str) -> Vec<String> {
    find(&screen["root"], id).unwrap()["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["label"].as_str().unwrap().to_owned())
        .collect()
}

/// The texts painted in the last frame.
fn painted(h: &H) -> Vec<String> {
    fn walk(shape: &egui::Shape, out: &mut Vec<String>) {
        match shape {
            egui::Shape::Text(text) => out.push(text.galley.text().to_owned()),
            egui::Shape::Vec(shapes) => shapes.iter().for_each(|s| walk(s, out)),
            _ => {}
        }
    }
    let mut out = Vec::new();
    h.output()
        .shapes
        .iter()
        .for_each(|s: &epaint::ClippedShape| walk(&s.shape, &mut out));
    out
}

/// The cell of the mount that holds the parts `label` names, on screen.
fn mount(h: &H, label: &str) -> egui::Rect {
    h.query_all_by_label(label)
        .find(|n| n.accesskit_node().role() == egui::accesskit::Role::Button)
        .unwrap()
        .rect()
}

#[test]
fn the_mounts_are_tile_sized_in_the_middle_of_the_canvas_and_the_stack_reads_whole() {
    let screen = editor();
    let cells = find(&screen["root"], "vehicle:grid").unwrap();
    let (cols, rows) = (
        cells["cols"].as_f64().unwrap() as f32,
        cells["rows"].as_f64().unwrap() as f32,
    );
    let named = cells["cells"]
        .as_array()
        .unwrap()
        .iter()
        .find_map(|c| c["label"].as_str().filter(|l| l.contains(", ")))
        .unwrap()
        .to_owned();
    let at = cells["cells"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["label"].as_str() == Some(named.as_str()))
        .map(|c| (c["x"].as_f64().unwrap() as f32, c["y"].as_f64().unwrap() as f32))
        .unwrap();
    let stack = labels(&screen, "vehicle:stack");
    assert!(!stack.is_empty());
    for size in [egui::vec2(1280.0, 800.0), egui::vec2(1920.0, 1080.0)] {
        let h = open(screen.clone(), size);
        let square = mount(&h, &named);
        assert!(
            (20.0..=64.0).contains(&square.width()),
            "at {size:?} a mount is {} wide",
            square.width()
        );
        // The grid's corner, from the mount's place in it.
        let corner = square.min - egui::vec2(at.0, at.1) * square.width();
        let grid = egui::Rect::from_min_size(corner, egui::vec2(cols, rows) * square.width());
        let room = legible::room(&h);
        let side = h.get_by_label("Parts").rect().left();
        let middle = (room.left() + side) / 2.0;
        assert!(
            (grid.center().x - middle).abs() < 40.0,
            "at {size:?} the grid {grid:?} is off the middle {middle} of the canvas pane"
        );
        assert!(
            grid.center().y > room.center().y - 120.0,
            "at {size:?} the grid {grid:?} hugs the top of {room:?}"
        );
        let shown = painted(&h);
        for label in &stack {
            assert!(
                shown.iter().any(|t| t == label),
                "at {size:?} the stack's {label:?} is not painted whole"
            );
        }
        let faults = legible::legible(&h, &screen);
        assert!(faults.is_empty(), "at {size:?}: {faults:?}");
    }
}

#[test]
fn the_zoom_gesture_scales_the_canvas_and_a_mount_names_its_parts() {
    let screen = editor();
    let named = find(&screen["root"], "vehicle:grid").unwrap()["cells"]
        .as_array()
        .unwrap()
        .iter()
        .find_map(|c| c["label"].as_str().filter(|l| l.contains(", ")))
        .unwrap()
        .to_owned();
    let mut h = open(screen, egui::vec2(1920.0, 1080.0));
    let before = mount(&h, &named);
    h.hover_at(before.center());
    h.run_steps(30);
    assert!(
        h.query_all_by_label(&named).count() > 1,
        "hovering a mount shows its parts"
    );
    h.event(egui::Event::Zoom(1.5));
    h.run_steps(4);
    let after = mount(&h, &named);
    assert!(
        after.width() > before.width() * 1.4,
        "{before:?} zoomed to {after:?}"
    );
}
