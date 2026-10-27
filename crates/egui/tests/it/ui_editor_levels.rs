//! The map editor's canvas outlines each block of squares its `block` names, and draws the squares marked `faded`
//! (another level seen through open air) at reduced opacity while they stay paintable like the others.
use crate::common::fixture;
use bn_client_egui::transport::{ScriptHandle, ScriptedTransport};
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

/// The editor with a 12×12 canvas; `block` and `faded` squares as given.
fn editor(block: Option<(u32, u32)>, faded: &[(u32, u32)]) -> Value {
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
    canvas["cols"] = json!(12);
    canvas["rows"] = json!(12);
    canvas["block"] = block.map_or(Value::Null, |(x, y)| json!({ "x": x, "y": y }));
    canvas["cells"] = (0..12)
        .flat_map(|y| (0..12).map(move |x| (x, y)))
        .map(|(x, y)| {
            let mut cell = json!({ "x": x, "y": y, "id": format!("square:{x},{y}"), "glyph": ".", "affordance": "editor:paint", "enabled": true });
            if faded.contains(&(x, y)) {
                cell["faded"] = json!(true);
            }
            cell
        })
        .collect();
    screen
}

fn open<'a>(screen: Value) -> (H<'a>, ScriptHandle) {
    let mut subscribe = fixture("subscribe_result");
    subscribe["result"]["interaction"]["stack"] = json!([screen]);
    let reply = vec![
        subscribe,
        fixture("snapshot_part_0"),
        fixture("snapshot_part_1"),
    ];
    let (mut t, script) = ScriptedTransport::new();
    t = t
        .on("bn.hello", vec![fixture("hello_result")])
        .on("bn.subscribe", reply.clone())
        .on("bn.subscribe", reply);
    for _ in 0..4 {
        t = t.on(
            "bn.command.submit",
            vec![fixture("submit_result"), fixture("command_completed")],
        );
    }
    let clock = Clock::manual();
    let model = EngineModel::open(Box::new(t), ConnectOptions::default()).with_clock(clock.clone());
    let mut h = Harness::builder()
        .with_size(egui::vec2(1280.0, 1600.0))
        .build_ui_state(
            |ui, app: &mut App<EngineModel>| app.show(ui),
            App::new(model).with_clock(clock),
        );
    h.run_steps(6);
    (h, script)
}

/// The rect of the square at `x`,`y`: the squares are drawn in rows from the canvas's top left.
fn square(h: &H, x: u32, y: u32) -> egui::Rect {
    let first = h
        .get_all_by_label(".")
        .map(|n| n.rect())
        // A canvas square is as wide as it is tall; a list row naming `.` is wider.
        .filter(|r| (r.width() - r.height()).abs() < 1.0)
        .min_by(|a, b| (a.min.y, a.min.x).partial_cmp(&(b.min.y, b.min.x)).unwrap())
        .unwrap();
    first.translate(egui::vec2(
        x as f32 * first.width(),
        y as f32 * first.height(),
    ))
}

fn flat(shape: &egui::Shape, into: &mut Vec<egui::Shape>) {
    match shape {
        egui::Shape::Vec(items) => items.iter().for_each(|s| flat(s, into)),
        other => into.push(other.clone()),
    }
}

fn shapes(h: &H) -> Vec<egui::Shape> {
    let mut all = vec![];
    h.output()
        .shapes
        .iter()
        .for_each(|c| flat(&c.shape, &mut all));
    all
}

/// The x of each vertical line and the y of each horizontal line drawn over the squares.
fn rules(h: &H) -> (Vec<f32>, Vec<f32>) {
    let canvas = square(h, 0, 0).union(square(h, 11, 11)).expand(1.0);
    let (mut xs, mut ys) = (vec![], vec![]);
    for shape in shapes(h) {
        if let egui::Shape::LineSegment { points: [a, b], .. } = shape {
            if !(canvas.contains(a) && canvas.contains(b)) {
                continue;
            }
            if a.x == b.x && (a.y - b.y).abs() > 20.0 {
                xs.push(a.x);
            } else if a.y == b.y && (a.x - b.x).abs() > 20.0 {
                ys.push(a.y);
            }
        }
    }
    (xs, ys)
}

/// The opacity the glyph of the square at `x`,`y` is drawn at.
fn opacity(h: &H, x: u32, y: u32) -> f32 {
    let at = square(h, x, y);
    shapes(h)
        .iter()
        .find_map(|s| match s {
            egui::Shape::Text(t)
                if t.galley.text() == "." && at.contains(t.pos + egui::vec2(1.0, 1.0)) =>
            {
                Some(t.fallback_color.a() as f32 / 255.0)
            }
            _ => None,
        })
        .unwrap_or_else(|| panic!("no glyph drawn in {at:?}"))
}

#[test]
fn a_canvas_with_a_block_draws_a_line_on_every_block_edge() {
    let (h, _) = open(editor(Some((6, 6)), &[]));
    let (xs, ys) = rules(&h);
    let (left, top) = (square(&h, 0, 0).left(), square(&h, 0, 0).top());
    for edge in [left, square(&h, 6, 0).left(), square(&h, 11, 0).right()] {
        assert!(
            xs.iter().any(|x| (x - edge).abs() < 1.0),
            "no vertical line at {edge} in {xs:?}"
        );
    }
    for edge in [top, square(&h, 0, 6).top(), square(&h, 0, 11).bottom()] {
        assert!(
            ys.iter().any(|y| (y - edge).abs() < 1.0),
            "no horizontal line at {edge} in {ys:?}"
        );
    }
    let inside = square(&h, 3, 0).left();
    assert!(
        xs.iter().all(|x| (x - inside).abs() > 1.0),
        "a line inside a block at {inside}"
    );
}

#[test]
fn a_canvas_without_a_block_draws_no_lines() {
    let (h, _) = open(editor(None, &[]));
    let (xs, ys) = rules(&h);
    assert!(xs.is_empty() && ys.is_empty(), "{xs:?} {ys:?}");
}

#[test]
fn a_faded_square_is_drawn_at_under_half_the_opacity_of_the_rest() {
    let (h, _) = open(editor(Some((6, 6)), &[(2, 1)]));
    let full = opacity(&h, 3, 1);
    assert_eq!(opacity(&h, 2, 2), full);
    let ratio = opacity(&h, 2, 1) / full;
    assert!((0.3..0.6).contains(&ratio), "faded at {ratio} of {full}");
}

#[test]
fn a_faded_square_still_takes_a_click_like_the_others() {
    let (mut h, script) = open(editor(Some((6, 6)), &[(2, 1)]));
    h.hover_at(square(&h, 2, 1).center());
    h.run_steps(2);
    h.get_all_by_label(".").for_each(drop);
    h.event(egui::Event::PointerButton {
        pos: square(&h, 2, 1).center(),
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers: egui::Modifiers::NONE,
    });
    h.run_steps(2);
    h.event(egui::Event::PointerButton {
        pos: square(&h, 2, 1).center(),
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: egui::Modifiers::NONE,
    });
    h.run_steps(4);
    let rows = script
        .sent()
        .into_iter()
        .filter(|m| m["method"] == "bn.command.submit")
        .map(|m| m["params"]["operation"].clone())
        .filter(|o| o["affordance_id"] == "editor:paint")
        .map(|o| o["args"]["row"].as_str().unwrap().to_owned())
        .collect::<Vec<_>>();
    assert!(rows.contains(&"square:2,1".to_owned()), "{rows:?}");
}

fn sent(script: &ScriptHandle) -> Vec<String> {
    script
        .sent()
        .into_iter()
        .filter(|m| m["method"] == "bn.command.submit")
        .map(|m| {
            m["params"]["operation"]["affordance_id"]
                .as_str()
                .unwrap()
                .to_owned()
        })
        .collect()
}

fn type_key(h: &mut H, key: &str) {
    h.event(egui::Event::Text(key.into()));
    h.run_steps(4);
}

#[test]
fn the_level_keys_run_the_level_affordances_and_a_disabled_one_shows_its_denial() {
    let mut screen = editor(Some((6, 6)), &[]);
    let up = screen["affordances"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|a| a["id"] == "editor:level_up")
        .unwrap();
    up["enabled"] = json!(true);
    up["denial"] = Value::Null;
    let (mut h, script) = open(screen);
    type_key(&mut h, "<");
    assert_eq!(sent(&script), ["editor:level_up"]);
    type_key(&mut h, ">");
    assert_eq!(
        sent(&script),
        ["editor:level_up"],
        "a disabled affordance is not sent"
    );
    h.get_by_label_contains("No level below in this file.");
}
