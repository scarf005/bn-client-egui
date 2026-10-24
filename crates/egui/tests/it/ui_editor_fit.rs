//! A screen that fills the map region and holds more than its height scrolls: the map editor's catalog, under a
//! full-size canvas, is reachable at 1280×800 instead of cut off below the window.
use crate::common::fixture;
use bn_client_egui::transport::ScriptedTransport;
use bn_client_egui::{App, Clock, ConnectOptions, EngineModel};
use eframe::egui;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use serde_json::{Value, json};

type H<'a> = Harness<'a, App<EngineModel>>;

fn find<'a>(value: &'a mut Value, id: &str) -> Option<&'a mut Value> {
    if value["id"] == id {
        return Some(value);
    }
    match value {
        Value::Array(items) => items.iter_mut().find_map(|v| find(v, id)),
        Value::Object(members) => members.values_mut().find_map(|v| find(v, id)),
        _ => None,
    }
}

/// The editor with a nested chunk open, its canvas grown to a whole 24×24 overmap square.
fn editor() -> Value {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../core/tests/fixtures/0.2.examples/editor.json"
    );
    let all = serde_json::from_str::<Value>(&std::fs::read_to_string(path).unwrap()).unwrap();
    let mut screen = all["valid"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["name"] == "map editor with a nested chunk open")
        .unwrap()["value"]
        .clone();
    let canvas = find(&mut screen["root"], "editor:canvas").unwrap();
    canvas["cols"] = json!(24);
    canvas["rows"] = json!(24);
    canvas["cells"] = (0..24)
        .flat_map(|y| (0..24).map(move |x| json!({ "x": x, "y": y, "id": format!("square:{x},{y}"), "glyph": ".", "affordance": "editor:paint", "enabled": true })))
        .collect();
    screen
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
    h.run_steps(6);
    h
}

#[test]
fn the_catalog_under_a_full_canvas_scrolls_into_the_window() {
    let size = egui::vec2(1280.0, 800.0);
    let mut screen = editor();
    let catalog = find(&mut screen["root"], "editor:catalog").unwrap()["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["label"].as_str().unwrap().to_owned())
        .collect::<Vec<_>>();
    let mut h = open(screen, size);
    // The window fills the map region, from under its title bar to the bottom of the screen.
    let close = h.get_by_label("Close window").rect();
    let room = egui::Rect::from_x_y_ranges(0.0..=close.right(), close.bottom()..=size.y);
    let shown = |h: &H| {
        catalog
            .iter()
            .flat_map(|label| {
                h.query_all_by_label(label)
                    .map(|n| n.rect())
                    .collect::<Vec<_>>()
            })
            .any(|r| room.contains_rect(r))
    };
    // The canvas scrolls inside its pane, so the catalog may be in view already; scrolling the window keeps it so.
    let over = egui::pos2(room.center().x, room.bottom() - 60.0);
    for _ in 0..40 {
        h.hover_at(over);
        h.event(egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: egui::vec2(0.0, -200.0),
            modifiers: egui::Modifiers::NONE,
            phase: egui::TouchPhase::Move,
        });
        h.run_steps(2);
    }
    assert!(shown(&h), "a catalog row is inside the window {room:?}");
}

#[test]
fn the_legend_under_a_full_canvas_shows_most_of_a_dozen_symbols_at_once() {
    for size in [egui::vec2(1290.0, 830.0), egui::vec2(1920.0, 1080.0)] {
        let mut screen = editor();
        let legend = find(&mut screen["root"], "editor:legend").unwrap();
        legend["rows"] = (0..12)
            .map(|n| json!({ "id": format!("symbol:{n}"), "label": format!("{n}"), "description": "", "denial": "", "enabled": true, "selectable": true, "selected": false, "columns": [{ "key": "terrain", "label": "terrain", "value": format!("\"t_wall_{n}\"") }] }))
            .collect();
        legend["total"] = json!(12);
        let mut h = open(screen, size);
        let over = egui::pos2(size.x * 0.45, 150.0);
        // Scrolling the window a notch at a time, the most symbols in view at once.
        let mut drawn = 0;
        for _ in 0..60 {
            h.hover_at(over);
            h.event(egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: egui::vec2(0.0, -50.0),
                modifiers: egui::Modifiers::NONE,
                phase: egui::TouchPhase::Move,
            });
            h.run_steps(2);
            let seen = (0..12)
                .filter(|n| {
                    h.query_by_label_contains(&format!("t_wall_{n}\""))
                        .is_some()
                })
                .count();
            drawn = drawn.max(seen);
        }
        assert!(
            drawn >= 8,
            "{drawn} of 12 legend symbols are in view at once at {size:?}"
        );
    }
}
