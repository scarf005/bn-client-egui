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
        .map(|c| {
            (
                c["x"].as_f64().unwrap() as f32,
                c["y"].as_f64().unwrap() as f32,
            )
        })
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
        // The canvas pane, between the tools' column and the right column.
        let tools = h
            .query_all_by_label("Resize workspace column")
            .map(|n| n.rect().center().x)
            .fold(f32::MAX, f32::min);
        let side = h.get_by_label("Parts").rect().left();
        let middle = (tools.min(side).max(room.left()) + side) / 2.0;
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

/// The vehicle editor with a prototype open, its part list and prototype list each a full page of 50 rows.
fn open_with_full_pages() -> Value {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../core/tests/fixtures/0.2.examples/vehicle_editor.json"
    );
    let all = serde_json::from_str::<Value>(&std::fs::read_to_string(path).unwrap()).unwrap();
    let mut screen = all["valid"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["name"] == "vehicle editor with a prototype open on a mount")
        .unwrap()["value"]
        .clone();
    fn find_mut<'a>(value: &'a mut Value, id: &str) -> Option<&'a mut Value> {
        if value["id"] == id {
            return Some(value);
        }
        match value {
            Value::Array(items) => items.iter_mut().find_map(|v| find_mut(v, id)),
            Value::Object(members) => members.values_mut().find_map(|v| find_mut(v, id)),
            _ => None,
        }
    }
    for id in ["vehicle:parts", "vehicle:prototypes"] {
        let list = find_mut(&mut screen["root"], id).unwrap();
        let first = list["rows"][0].clone();
        list["rows"] = (0..50)
            .map(|i| {
                let mut row = first.clone();
                row["id"] = json!(format!("{id}:{i}"));
                row["label"] = json!(format!("{id} row {i}"));
                row["selected"] = json!(false);
                row
            })
            .collect();
        list["total"] = json!(200);
    }
    screen
}

#[test]
fn the_right_column_holds_every_design_tab_and_some_rows_of_each_list_in_view() {
    let screen = open_with_full_pages();
    let tabs = find(&screen["root"], "vehicle:tools").unwrap()["tabs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["label"].as_str().unwrap().to_owned())
        .collect::<Vec<_>>();
    let stack = labels(&screen, "vehicle:stack");
    for size in [egui::vec2(1290.0, 830.0), egui::vec2(1920.0, 1080.0)] {
        let h = open(screen.clone(), size);
        // The column ends above the status line, which the open document's facts start.
        let bottom = h.get_by_label("Mount").rect().top();
        let in_view = |r: egui::Rect| r.top() >= 0.0 && r.bottom() <= bottom + 0.5;
        for tab in &tabs {
            let r = h
                .query_all_by_label(tab)
                .find(|n| n.accesskit_node().role() == egui::accesskit::Role::Tab)
                .map(|n| n.rect());
            assert!(
                r.is_some_and(in_view),
                "at {size:?} the tab {tab:?} at {r:?} is out of view above {bottom}"
            );
        }
        for label in &stack {
            let r = h.get_by_label(label).rect();
            assert!(
                in_view(r),
                "at {size:?} the stack's {label:?} at {r:?} is out of view above {bottom}"
            );
        }
        for id in ["vehicle:parts", "vehicle:prototypes"] {
            let shown = (0..50)
                .filter(|i| {
                    h.query_all_by_label(&format!("{id} row {i}"))
                        .any(|n| in_view(n.rect()) && n.rect().height() > 0.0)
                })
                .count();
            assert!(
                shown >= 3,
                "at {size:?} {id} shows {shown} rows in view above {bottom}"
            );
        }
    }
}

/// The world list's rows run `Edit in place` on a double click, and it is a button beside the list's other
/// actions on a row, `Export design` and `Copy vehicle`, as they are.
#[test]
fn a_lists_activate_is_a_button_beside_its_other_row_actions() {
    let h = open(editor(), egui::vec2(1920.0, 1080.0));
    for label in ["Export design", "Copy vehicle", "Edit in place"] {
        assert!(
            h.query_all_by_label(label)
                .any(|n| n.accesskit_node().role() == egui::accesskit::Role::Button),
            "no {label} button"
        );
    }
    // The history's Go to step, the only action on its rows, has no button: its rows run it.
    assert!(h.query_by_label("Go to step").is_none());
}

/// A part picked far down a mount's long stack, in place of one at its top, is brought into view.
#[test]
fn a_part_picked_far_down_a_long_stack_is_brought_into_view() {
    let picked = |n: usize| {
        let mut screen = open_with_full_pages();
        fn stack_of(value: &mut Value) -> Option<&mut Value> {
            if value["id"] == "vehicle:stack" {
                return Some(value);
            }
            match value {
                Value::Array(items) => items.iter_mut().find_map(stack_of),
                Value::Object(members) => members.values_mut().find_map(stack_of),
                _ => None,
            }
        }
        let stack = stack_of(&mut screen["root"]).unwrap();
        let first = stack["rows"][0].clone();
        stack["rows"] = (0..30)
            .map(|i| {
                let mut row = first.clone();
                row["id"] = json!(format!("part:{i}"));
                row["label"] = json!(format!("stacked part {i}"));
                row["selected"] = json!(i == n);
                row
            })
            .collect();
        stack["total"] = json!(30);
        screen
    };
    let size = egui::vec2(1290.0, 830.0);
    let (mut h, script) = crate::ui_editor_drag::open_sized(picked(0), size);
    crate::common::settle_windows(&mut h);
    crate::ui_editor_drag::restack(&script, 41, vec![picked(25)]);
    h.run_steps(8);
    let bottom = h.get_by_label("Mount").rect().top();
    let r = h.query_by_label("stacked part 25").map(|n| n.rect());
    assert!(
        r.is_some_and(|r| r.top() >= 0.0 && r.bottom() <= bottom + 0.5),
        "the picked part is at {r:?}, above {bottom}"
    );
}
