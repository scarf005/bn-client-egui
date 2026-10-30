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

pub(crate) type H<'a> = Harness<'a, App<EngineModel>>;

/// The editor with a 12×12 canvas that drags as `drag`.
pub(crate) fn editor(drag: Option<&str>) -> Value {
    editor_of(drag, 12)
}

/// The editor with a `side`×`side` canvas that drags as `drag`.
pub(crate) fn editor_of(drag: Option<&str>, side: u32) -> Value {
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

pub(crate) fn open<'a>(screen: Value) -> (H<'a>, ScriptHandle) {
    open_sized(screen, egui::vec2(1280.0, 1600.0))
}

pub(crate) fn open_sized<'a>(screen: Value, size: egui::Vec2) -> (H<'a>, ScriptHandle) {
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
pub(crate) fn painted(script: &ScriptHandle) -> Vec<String> {
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
pub(crate) fn sent(script: &ScriptHandle) -> Vec<String> {
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
pub(crate) fn square(h: &H, x: u32, y: u32) -> egui::Pos2 {
    let first = h
        .get_all_by_label(".")
        .map(|n| n.rect())
        // A canvas square is as wide as it is tall; a list row naming `.` is wider.
        .filter(|r| (r.width() - r.height()).abs() < 1.0)
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
    drag_from(h, square(h, from.0, from.1), via);
}

/// Presses at `at`, moves through the squares `via`, and releases on the last one.
fn drag_from(h: &mut H, at: egui::Pos2, via: &[(u32, u32)]) {
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
fn a_stroke_crossing_into_a_neighbour_square_never_runs_the_neighbours_affordance() {
    let (mut h, script) = open(editor_with_neighbours("trail"));
    drag(&mut h, (1, 2), &[(3, 2), (7, 2)]);
    assert_eq!(
        painted(&script),
        ["square:1,2", "square:2,2", "square:3,2", "square:4,2"]
    );
    assert!(!sent(&script).contains(&"editor:object".to_owned()));
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

#[test]
fn a_press_on_a_neighbour_paints_nothing_on_the_entry_it_opens() {
    let (mut h, script) = open(editor_with_neighbours("trail"));
    let at = square(&h, 6, 2);
    h.hover_at(at);
    h.run_steps(2);
    button(&mut h, at, true);
    // The engine answers the click with the neighbour's entry, painted squares where the neighbour was.
    restack(&script, 41, vec![editor(Some("trail"))]);
    h.run_steps(6);
    for x in [7, 8] {
        let over = square(&h, x, 2);
        h.hover_at(over);
        h.run_steps(2);
    }
    let last = square(&h, 8, 2);
    button(&mut h, last, false);
    h.run_steps(4);
    assert!(painted(&script).is_empty(), "{:?}", painted(&script));
}

pub(crate) fn first_square(h: &H) -> egui::Rect {
    h.get_all_by_label(".")
        .map(|n| n.rect())
        // A canvas square is as wide as it is tall; a list row naming `.` is wider.
        .filter(|r| (r.width() - r.height()).abs() < 1.0)
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

    // Zoomed past its room the canvas scrolls: a middle drag moves the squares with the pointer, here to the right as the view is at its far edge.
    h.event(egui::Event::Zoom(2.0));
    h.run_steps(3);
    let big = first_square(&h);
    assert!(big.width() > 3.9 * before.width(), "{before:?} {big:?}");
    let start = before.center() + egui::vec2(100.0, 100.0);
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
    h.hover_at(start + egui::vec2(120.0, 0.0));
    h.run_steps(3);
    h.event(middle(false));
    h.run_steps(2);
    let panned = first_square(&h);
    // The squares left of the view are not drawn: the first one shown is the next, a whole square further on.
    let moved = (panned.min.x - big.min.x).rem_euclid(big.width());
    assert!((moved - 120.0).abs() < 1.5, "{big:?} {panned:?}");
}

/// Replaces the engine's screen stack, as the engine's next event.
pub(crate) fn restack(script: &ScriptHandle, sequence: u64, stack: Vec<Value>) {
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
        let bar = h
            .get_by_role_and_label(egui::accesskit::Role::Button, "File")
            .rect();
        let at = square(&h, 3, 12);
        assert!(
            at.y > bar.bottom() && at.y < size.y,
            "row 12 is drawn in the window at {size:?}: {at:?}"
        );
        drag(&mut h, (3, 12), &[]);
        assert_eq!(painted(&script), ["square:3,12"], "at {size:?}");
    }
}

/// The rows of the symbols picked by a click, or a double click, on the legend row of `#`, at its label or at its
/// value: the legend's `select`-role affordance picks a row on one click.
fn picked_by_click_on(label: bool, double: bool) -> Vec<String> {
    let (mut h, script) = open(editor(Some("span")));
    let value = h.get_all_by_label("t_wall").next().unwrap().rect().center();
    let symbol = h
        .get_all_by_label("#")
        .map(|n| n.rect().center())
        .find(|c| (c.y - value.y).abs() < 4.0)
        .unwrap();
    let at = if label { symbol } else { value };
    h.hover_at(at);
    h.run_steps(2);
    match double {
        true => crate::common::double_click(&mut h, at),
        false => {
            h.input_mut().events.push(egui::Event::PointerMoved(at));
            for pressed in [true, false] {
                h.input_mut().events.push(egui::Event::PointerButton {
                    pos: at,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                });
            }
        }
    }
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
fn a_click_on_a_legend_rows_label_or_value_runs_its_symbol_once() {
    assert_eq!(picked_by_click_on(true, false), ["symbol:#"]);
    assert_eq!(picked_by_click_on(false, false), ["symbol:#"]);
}

#[test]
fn a_double_click_on_a_legend_rows_label_runs_its_symbol_once() {
    assert_eq!(picked_by_click_on(true, true), ["symbol:#"]);
}

#[test]
fn a_double_click_on_a_legend_rows_value_runs_its_symbol_once() {
    assert_eq!(picked_by_click_on(false, true), ["symbol:#"]);
}

/// The editor on its placements tool: a 4×3 region at (2,2) with its corner squares, a lone square at (9,9), and the
/// affordance a drag of either runs.
fn editor_with_regions(drag: Option<&str>) -> Value {
    let mut screen = editor(drag);
    screen["affordances"].as_array_mut().unwrap().push(json!({
        "id": "editor:placement_span", "label": "Move placement", "keys": [], "enabled": true,
        "args": {"row": {"component_id": "editor:canvas"}, "text": {}}, "expect": "boundary"
    }));
    let canvas = find(&mut screen["root"], "editor:canvas").unwrap();
    canvas["affordances"] = json!(["editor:paint", "editor:placement_span"]);
    canvas["region_drag"] = json!("editor:placement_span");
    canvas["regions"] = json!([
        {"id": "placement:place_items:0", "from": {"x": 2, "y": 2}, "to": {"x": 5, "y": 4}, "label": "place_items"},
        {"id": "placement:place_monster:0", "from": {"x": 9, "y": 9}, "to": {"x": 9, "y": 9}, "selected": true}
    ]);
    screen
}

/// The row and the text of the region drags sent so far.
fn spans(script: &ScriptHandle) -> Vec<(String, String)> {
    script
        .sent()
        .into_iter()
        .filter(|m| m["method"] == "bn.command.submit")
        .map(|m| m["params"]["operation"].clone())
        .filter(|o| o["affordance_id"] == "editor:placement_span")
        .map(|o| {
            (
                o["args"]["row"].as_str().unwrap().to_owned(),
                o["args"]["text"].as_str().unwrap().to_owned(),
            )
        })
        .collect()
}

#[test]
fn a_drag_inside_a_region_sends_how_far_both_corners_moved() {
    let (mut h, script) = open(editor_with_regions(None));
    drag(&mut h, (3, 3), &[(4, 3), (6, 2)]);
    assert_eq!(
        spans(&script),
        [("placement:place_items:0".into(), "3,-1,3,-1".into())]
    );
    assert!(painted(&script).is_empty());
}

#[test]
fn a_drag_from_the_outer_part_of_a_corner_square_sends_only_that_corners_move() {
    let (mut h, script) = open(editor_with_regions(None));
    let quarter = first_square(&h).width() / 4.0;
    let at = square(&h, 5, 4) + egui::vec2(quarter, quarter);
    drag_from(&mut h, at, &[(7, 6)]);
    assert_eq!(
        spans(&script),
        [("placement:place_items:0".into(), "0,0,2,2".into())]
    );
}

#[test]
fn a_drag_from_the_middle_of_a_corner_square_moves_the_whole_region() {
    let (mut h, script) = open(editor_with_regions(None));
    drag(&mut h, (5, 4), &[(7, 6)]);
    assert_eq!(
        spans(&script),
        [("placement:place_items:0".into(), "2,2,2,2".into())]
    );
}

#[test]
fn a_drag_of_a_region_whose_move_is_off_sends_nothing_and_says_why() {
    let mut screen = editor_with_regions(None);
    let span = screen["affordances"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|a| a["id"] == "editor:placement_span")
        .unwrap();
    span["enabled"] = json!(false);
    span["denial"] = json!("Nothing to move here.");
    let (mut h, script) = open(screen);
    drag(&mut h, (3, 3), &[(4, 3)]);
    assert!(spans(&script).is_empty());
    assert!(h.query_by_label_contains("Nothing to move here.").is_some());
}

#[test]
fn a_press_and_release_on_a_region_picks_its_square_and_sends_no_move() {
    let (mut h, script) = open(editor_with_regions(None));
    drag(&mut h, (3, 3), &[]);
    assert!(spans(&script).is_empty());
    assert_eq!(painted(&script), ["square:3,3"]);
}

#[test]
fn a_drag_outside_every_region_sends_no_move() {
    let (mut h, script) = open(editor_with_regions(None));
    drag(&mut h, (7, 7), &[(8, 7)]);
    assert!(spans(&script).is_empty());
}

#[test]
fn a_region_drag_leaves_a_trail_canvas_without_region_drag_unchanged() {
    let mut screen = editor_with_regions(Some("trail"));
    find(&mut screen["root"], "editor:canvas").unwrap()["region_drag"] = Value::Null;
    let (mut h, script) = open(screen);
    drag(&mut h, (3, 3), &[(4, 3)]);
    assert!(spans(&script).is_empty());
    assert_eq!(painted(&script), ["square:3,3", "square:4,3"]);
}

#[test]
fn the_outline_of_a_dragged_region_follows_the_pointer() {
    let (mut h, _script) = open(editor_with_regions(None));
    let at = square(&h, 3, 3);
    h.hover_at(at);
    h.run_steps(2);
    button(&mut h, at, true);
    let over = square(&h, 6, 3);
    h.hover_at(over);
    h.run_steps(3);
    let amber = h.render().unwrap();
    let picked = egui::Color32::from_rgb(0xff, 0xb6, 0x2e);
    let cell = first_square(&h).width();
    // The moved outline's right edge is at the right of column 8 (the region spans 4 columns from 5 to 8).
    let right = square(&h, 8, 3).x + cell / 2.0;
    let found = (0..amber.height()).any(|y| {
        (-3..=0).any(|dx| {
            amber.get_pixel((right as i32 + dx) as u32, y) == &image::Rgba(picked.to_array())
        })
    });
    assert!(found);
}

#[test]
fn a_span_drag_shows_the_squares_its_release_draws_until_it_ends() {
    let mut screen = editor(Some("span"));
    find(&mut screen["root"], "editor:canvas").unwrap()["span"] = json!({
        "shape": "rect", "size": 1, "from": {"x": 0, "y": 0}, "to": {"x": 11, "y": 11}, "looks": []
    });
    let (mut h, _script) = open(screen);
    // A spot off the square's glyph, a quarter in from its corner.
    let spot = |h: &mut H, x: u32, y: u32| {
        let at = square(h, x, y) - egui::vec2(5.0, 5.0);
        let image = h.render().unwrap();
        *image.get_pixel(at.x as u32, at.y as u32)
    };
    let edge = spot(&mut h, 4, 2);
    let inside = spot(&mut h, 4, 3);
    let (press, pointer) = (square(&h, 2, 2), square(&h, 6, 5));
    h.hover_at(press);
    h.run_steps(2);
    button(&mut h, press, true);
    h.hover_at(pointer);
    h.run_steps(2);
    // The outline of the rectangle from the press to the pointer is shaded, its inside is not.
    assert_ne!(
        spot(&mut h, 4, 2),
        edge,
        "the rectangle's top edge is shaded"
    );
    assert_ne!(spot(&mut h, 6, 4), edge, "its east edge is shaded");
    assert_eq!(
        spot(&mut h, 4, 3),
        inside,
        "the inside of an outline stays as it was"
    );
    button(&mut h, pointer, false);
    h.run_steps(4);
    assert_eq!(spot(&mut h, 4, 2), edge, "the shading ends with the drag");
}
