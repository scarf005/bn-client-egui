//! A drag over the map editor's canvas runs its paint affordance on the squares it crosses: every one for a trail,
//! the press and the release for a span, and nothing for a canvas that does not ask for drags. Ctrl and the wheel
//! zoom the canvas and a middle-button drag pans it.
use crate::common::fixture;
use bn_client_egui::transport::{ScriptHandle, ScriptedTransport};
use bn_client_egui::{App, Clock, ConnectOptions, EngineModel};
use eframe::egui;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use serde_json::{Value, json};

type H<'a> = Harness<'a, App<EngineModel>>;

/// The editor with a 12×12 canvas that drags as `drag`.
fn editor(drag: Option<&str>) -> Value {
    editor_of(drag, 12)
}

/// The editor with a `side`×`side` canvas that drags as `drag`.
fn editor_of(drag: Option<&str>, side: u32) -> Value {
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
    canvas["cols"] = json!(side);
    canvas["rows"] = json!(side);
    canvas["drag"] = json!(drag);
    let strokes = drag == Some("trail");
    canvas["drag_begin"] = json!(strokes.then_some("editor:stroke_begin"));
    canvas["drag_end"] = json!(strokes.then_some("editor:stroke_end"));
    canvas["cells"] = (0..side)
        .flat_map(|y| (0..side).map(move |x| json!({ "x": x, "y": y, "id": format!("square:{x},{y}"), "glyph": ".", "affordance": "editor:paint", "enabled": true })))
        .collect();
    screen
}

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

fn open<'a>(screen: Value) -> (H<'a>, ScriptHandle) {
    open_sized(screen, egui::vec2(1280.0, 1600.0))
}

fn open_sized<'a>(screen: Value, size: egui::Vec2) -> (H<'a>, ScriptHandle) {
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
    for _ in 0..16 {
        t = t.on(
            "bn.command.submit",
            vec![fixture("submit_result"), fixture("command_completed")],
        );
    }
    let clock = Clock::manual();
    let model = EngineModel::open(Box::new(t), ConnectOptions::default()).with_clock(clock.clone());
    let mut h = Harness::builder().with_size(size).build_ui_state(
        |ui, app: &mut App<EngineModel>| app.show(ui),
        App::new(model).with_clock(clock),
    );
    h.run_steps(6);
    (h, script)
}

/// The rows of the paint invokes sent so far.
fn painted(script: &ScriptHandle) -> Vec<String> {
    script
        .sent()
        .into_iter()
        .filter(|m| m["method"] == "bn.command.submit")
        .map(|m| m["params"]["operation"].clone())
        .filter(|o| o["affordance_id"] == "editor:paint")
        .map(|o| o["args"]["row"].as_str().unwrap().to_owned())
        .collect()
}

/// The affordances of the invokes sent so far, in order.
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

/// A square's centre: the squares are drawn in rows from the canvas's top left.
fn square(h: &H, x: u32, y: u32) -> egui::Pos2 {
    let first = h
        .get_all_by_label(".")
        .map(|n| n.rect())
        .min_by(|a, b| (a.min.y, a.min.x).partial_cmp(&(b.min.y, b.min.x)).unwrap())
        .unwrap();
    let cell = first.width();
    first.center() + egui::vec2(x as f32 * cell, y as f32 * cell)
}

fn button(h: &mut H, at: egui::Pos2, pressed: bool) {
    h.event(egui::Event::PointerButton {
        pos: at,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    });
    h.run_steps(2);
}

/// Presses on `from`, moves through `via`, and releases on the last square.
fn drag(h: &mut H, from: (u32, u32), via: &[(u32, u32)]) {
    let at = square(h, from.0, from.1);
    h.hover_at(at);
    h.run_steps(2);
    button(h, at, true);
    let mut last = at;
    for &(x, y) in via {
        last = square(h, x, y);
        h.hover_at(last);
        h.run_steps(2);
    }
    button(h, last, false);
    h.run_steps(4);
}

#[test]
fn a_trail_paints_every_square_the_pointer_crosses() {
    let (mut h, script) = open(editor(Some("trail")));
    drag(&mut h, (1, 2), &[(2, 2), (4, 2), (4, 3)]);
    assert_eq!(
        painted(&script),
        [
            "square:1,2",
            "square:2,2",
            "square:3,2",
            "square:4,2",
            "square:4,3"
        ]
    );
}

#[test]
fn a_trail_runs_the_grids_begin_and_end_affordances_around_its_squares() {
    let (mut h, script) = open(editor(Some("trail")));
    drag(&mut h, (1, 2), &[(3, 2)]);
    assert_eq!(
        sent(&script),
        [
            "editor:stroke_begin",
            "editor:paint",
            "editor:paint",
            "editor:paint",
            "editor:stroke_end"
        ]
    );
}

#[test]
fn a_span_sends_no_begin_or_end_without_the_grid_naming_them() {
    let (mut h, script) = open(editor(Some("span")));
    drag(&mut h, (2, 2), &[(5, 2)]);
    assert_eq!(sent(&script), ["editor:paint", "editor:paint"]);
}

#[test]
fn a_span_paints_the_press_and_the_release_squares_only() {
    let (mut h, script) = open(editor(Some("span")));
    drag(&mut h, (2, 2), &[(5, 2), (9, 7)]);
    assert_eq!(painted(&script), ["square:2,2", "square:9,7"]);
}

#[test]
fn a_click_on_a_span_canvas_paints_its_square_once() {
    let (mut h, script) = open(editor(Some("span")));
    drag(&mut h, (3, 3), &[]);
    assert_eq!(painted(&script), ["square:3,3"]);
}

#[test]
fn a_canvas_without_drag_paints_a_click_and_ignores_the_drag() {
    let (mut h, script) = open(editor(None));
    drag(&mut h, (1, 1), &[(2, 1), (3, 1)]);
    assert!(painted(&script).is_empty());
    drag(&mut h, (5, 5), &[]);
    assert_eq!(painted(&script), ["square:5,5"]);
}

/// The editor whose squares from column 5 on are neighbours: clicking one opens it as `editor:object`.
fn editor_with_neighbours(drag: &str) -> Value {
    let mut screen = editor(Some(drag));
    let canvas = find(&mut screen["root"], "editor:canvas").unwrap();
    for cell in canvas["cells"].as_array_mut().unwrap() {
        if cell["x"].as_u64().unwrap() >= 5 {
            cell["id"] = json!(format!("object:{}", cell["x"]));
            cell["affordance"] = json!("editor:object");
        }
    }
    screen
}

#[test]
fn a_click_on_a_neighbour_square_runs_its_affordance_on_the_row_of_the_objects_list() {
    let (mut h, script) = open(editor_with_neighbours("trail"));
    drag(&mut h, (6, 2), &[]);
    assert!(
        sent(&script).contains(&"editor:object".to_owned()),
        "{:?}",
        sent(&script)
    );
    assert!(painted(&script).is_empty());
}

fn first_square(h: &H) -> egui::Rect {
    h.get_all_by_label(".")
        .map(|n| n.rect())
        .min_by(|a, b| (a.min.y, a.min.x).partial_cmp(&(b.min.y, b.min.x)).unwrap())
        .unwrap()
}

#[test]
fn the_wheel_with_ctrl_zooms_the_canvas_and_a_middle_drag_pans_it() {
    let (mut h, _script) = open(editor(Some("trail")));
    let before = first_square(&h);
    h.hover_at(before.center() + egui::vec2(100.0, 100.0));
    h.run_steps(2);
    h.event(egui::Event::Zoom(2.0));
    h.run_steps(3);
    let zoomed = first_square(&h);
    assert!(
        zoomed.width() > 1.9 * before.width(),
        "{before:?} {zoomed:?}"
    );

    // Zoomed past its room the canvas scrolls: a middle drag moves the squares with the pointer.
    h.event(egui::Event::Zoom(2.0));
    h.run_steps(3);
    let big = first_square(&h);
    assert!(big.width() > 3.9 * before.width(), "{before:?} {big:?}");
    let start = big.center() + egui::vec2(300.0, 0.0);
    h.hover_at(start);
    h.run_steps(2);
    let middle = |pressed| egui::Event::PointerButton {
        pos: start,
        button: egui::PointerButton::Middle,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    h.event(middle(true));
    h.run_steps(2);
    h.hover_at(start - egui::vec2(120.0, 0.0));
    h.run_steps(3);
    h.event(middle(false));
    h.run_steps(2);
    let panned = first_square(&h);
    // The squares left of the view are not drawn: the first one shown is the next, a whole square further on.
    let moved = (panned.min.x - big.min.x).rem_euclid(big.width());
    assert!(
        (moved - (big.width() - 120.0).rem_euclid(big.width())).abs() < 1.5,
        "{big:?} {panned:?}"
    );
}

/// Replaces the engine's screen stack, as the engine's next event.
fn restack(script: &ScriptHandle, sequence: u64, stack: Vec<Value>) {
    script.push(
        json!({"jsonrpc": "2.0", "method": "bn.events", "params": {"epoch": "e7f3", "events": [{
        "sequence": sequence.to_string(), "revision": (sequence - 9).to_string(),
        "type": "interaction.changed", "changes": {"interaction": {
        "boundary_id": "b:60",
        "readiness": {"phase": "playing", "game_ready": true, "accepts_interaction_commands": true,
            "accepts_registered_actions": true},
        "actions": [], "interaction": null, "stack": stack}}}]}}),
    );
}

#[test]
fn a_drag_left_open_by_a_closed_editor_sends_nothing_into_its_next_showing() {
    let screen = editor(Some("trail"));
    let (mut h, script) = open(screen.clone());
    let at = square(&h, 1, 2);
    h.hover_at(at);
    h.run_steps(2);
    button(&mut h, at, true);
    let begun = sent(&script);
    assert_eq!(begun, ["editor:stroke_begin", "editor:paint"]);

    let map = fixture("subscribe_result")["result"]["interaction"]["stack"][0].clone();
    restack(&script, 41, vec![map]);
    h.run_steps(6);
    restack(&script, 42, vec![screen]);
    h.run_steps(6);
    button(&mut h, at, false);
    h.run_steps(4);
    assert_eq!(sent(&script), begun);
}

#[test]
fn a_click_on_the_middle_row_of_a_24_square_canvas_paints_it_in_a_laptop_window() {
    for size in [egui::vec2(1290.0, 830.0), egui::vec2(1920.0, 1080.0)] {
        let (mut h, script) = open_sized(editor_of(Some("span"), 24), size);
        let close = h.get_by_label("Close window").rect();
        let at = square(&h, 3, 12);
        assert!(
            at.y > close.bottom() && at.y < size.y,
            "row 12 is drawn in the window at {size:?}: {at:?}"
        );
        drag(&mut h, (3, 12), &[]);
        assert_eq!(painted(&script), ["square:3,12"], "at {size:?}");
    }
}

/// The rows of the symbols picked by double clicks on the legend row of `#`, at its label or at its value.
fn picked_by_double_click_on(label: bool) -> Vec<String> {
    let (mut h, script) = open(editor(Some("span")));
    let value = h
        .get_all_by_label("\"t_wall\"")
        .next()
        .unwrap()
        .rect()
        .center();
    let symbol = h
        .get_all_by_label("#")
        .map(|n| n.rect().center())
        .find(|c| (c.y - value.y).abs() < 4.0)
        .unwrap();
    h.hover_at(value);
    h.run_steps(2);
    crate::common::double_click(&mut h, if label { symbol } else { value });
    h.run_steps(6);
    script
        .sent()
        .into_iter()
        .filter(|m| m["method"] == "bn.command.submit")
        .map(|m| m["params"]["operation"].clone())
        .filter(|o| o["affordance_id"] == "editor:symbol")
        .map(|o| o["args"]["row"].as_str().unwrap().to_owned())
        .collect()
}

#[test]
fn a_double_click_on_a_legend_rows_label_runs_its_symbol() {
    assert_eq!(picked_by_double_click_on(true), ["symbol:#"]);
}

#[test]
fn a_double_click_on_a_legend_rows_value_runs_its_symbol() {
    assert_eq!(picked_by_double_click_on(false), ["symbol:#"]);
}
