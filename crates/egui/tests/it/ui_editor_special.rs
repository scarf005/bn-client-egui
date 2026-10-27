//! The map editor's canvas of an overmap special: a cell per overmap square, drawn as the terrain's glyph in its
//! colour, and a click on one runs the cell's `editor:open_square` with that square's row.
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

/// The editor whose canvas is a special's level: a house (rotated) beside a field.
fn special() -> Value {
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
    screen["affordances"].as_array_mut().unwrap().push(json!({
        "id": "editor:open_square", "label": "Open square", "keys": [], "enabled": true,
        "expect": "screen", "args": { "row": { "component_id": "editor:canvas" } }
    }));
    let canvas = find(&mut screen["root"], "editor:canvas").unwrap();
    let look = |id: &str, name: &str, glyph: &str, color: &str, rotation: u8| json!({ "kind": "overmap_terrain", "id": id, "name": name, "glyph": glyph, "color": color, "rotation": rotation });
    canvas["cols"] = json!(2);
    canvas["rows"] = json!(1);
    canvas["block"] = Value::Null;
    canvas["affordances"] = json!(["editor:open_square"]);
    canvas["looks"] = json!([
        look("house_01", "house", "H", "light_red", 0),
        look("field", "field", "~", "brown", 0)
    ]);
    canvas["cells"] = json!([
        { "x": 0, "y": 0, "id": "square:0,0", "glyph": "H", "color": "light_red", "label": "house",
          "looks": [0], "affordance": "editor:open_square", "enabled": true },
        { "x": 1, "y": 0, "id": "square:1,0", "glyph": "~", "color": "brown", "label": "field",
          "looks": [1], "affordance": "editor:open_square", "enabled": true }
    ]);
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
    for _ in 0..16 {
        t = t.on(
            "bn.command.submit",
            vec![fixture("submit_result"), fixture("command_completed")],
        );
    }
    let clock = Clock::manual();
    let model = EngineModel::open(Box::new(t), ConnectOptions::default()).with_clock(clock.clone());
    let mut h = Harness::builder()
        .with_size(egui::vec2(1280.0, 900.0))
        .build_ui_state(
            |ui, app: &mut App<EngineModel>| app.show(ui),
            App::new(model).with_clock(clock),
        );
    h.run_steps(6);
    (h, script)
}

#[test]
fn an_overmap_square_is_drawn_as_its_terrain_glyph_in_its_colour() {
    let (h, _) = open(special());
    let drawn = |glyph: &str| {
        h.output()
            .shapes
            .iter()
            .find_map(|c| text_color(&c.shape, glyph))
            .unwrap_or_else(|| panic!("no {glyph} drawn"))
    };
    assert_ne!(drawn("H"), drawn("~"), "the two terrains share one colour");
    assert_ne!(drawn("H"), egui::Color32::PLACEHOLDER);
}

fn text_color(shape: &egui::Shape, glyph: &str) -> Option<egui::Color32> {
    match shape {
        egui::Shape::Vec(items) => items.iter().find_map(|s| text_color(s, glyph)),
        egui::Shape::Text(t) if t.galley.text() == glyph => {
            t.galley.job.sections.first().map(|s| s.format.color)
        }
        _ => None,
    }
}

#[test]
fn a_click_on_an_overmap_square_runs_open_square_with_that_squares_row() {
    let (mut h, script) = open(special());
    h.get_by_label("field").click();
    h.run_steps(4);
    let rows = script
        .sent()
        .into_iter()
        .filter(|m| m["method"] == "bn.command.submit")
        .map(|m| m["params"]["operation"].clone())
        .filter(|o| o["affordance_id"] == "editor:open_square")
        .map(|o| o["args"]["row"].as_str().unwrap().to_owned())
        .collect::<Vec<_>>();
    assert_eq!(rows, ["square:1,0"]);
}

/// The special's editor with a drawing tool: its tabs are open, point and erase, and the canvas holds a square of
/// room around the house and the field, empty cells with the glyph ` ` and no label, all painted with a trail drag.
fn drawing() -> Value {
    let mut screen = special();
    let tab = |id: &str, label: &str, selected: bool, key: &str| json!({ "id": id, "label": label, "selected": selected, "enabled": true, "hotkey": key });
    let tools = find(&mut screen["root"], "editor:tools").unwrap();
    tools["tabs"] = json!([
        tab("tool:open", "Open mapgen", false, "o"),
        tab("tool:point", "Pencil", true, "b"),
        tab("tool:erase", "Eraser", false, "e")
    ]);
    for (id, label) in [
        ("editor:paint", "Paint"),
        ("editor:stroke_begin", "Begin stroke"),
        ("editor:stroke_end", "End stroke"),
    ] {
        screen["affordances"].as_array_mut().unwrap().push(json!({
            "id": id, "label": label, "keys": [], "enabled": true, "expect": "screen",
            "args": { "row": { "component_id": "editor:canvas" } }
        }));
    }
    let canvas = find(&mut screen["root"], "editor:canvas").unwrap();
    canvas["cols"] = json!(4);
    canvas["rows"] = json!(3);
    canvas["drag"] = json!("trail");
    canvas["drag_begin"] = json!("editor:stroke_begin");
    canvas["drag_end"] = json!("editor:stroke_end");
    let cell = |x: i32, y: i32| match (x, y) {
        (1, 1) => {
            json!({ "x": 1, "y": 1, "id": "square:0,0", "glyph": "H", "color": "light_red", "label": "house", "looks": [0], "affordance": "editor:paint", "enabled": true })
        }
        (2, 1) => {
            json!({ "x": 2, "y": 1, "id": "square:1,0", "glyph": "~", "color": "brown", "label": "field", "looks": [1], "affordance": "editor:paint", "enabled": true })
        }
        _ => {
            json!({ "x": x, "y": y, "id": format!("square:{},{}", x - 1, y - 1), "glyph": " ", "label": "", "affordance": "editor:paint", "enabled": true })
        }
    };
    canvas["cells"] = (0..3)
        .flat_map(|y| (0..4).map(move |x| cell(x, y)))
        .collect();
    screen
}

fn sent_rows(script: &ScriptHandle, affordance: &str) -> Vec<String> {
    script
        .sent()
        .into_iter()
        .filter(|m| m["method"] == "bn.command.submit")
        .map(|m| m["params"]["operation"].clone())
        .filter(|o| o["affordance_id"] == affordance)
        .filter_map(|o| o["args"]["row"].as_str().map(str::to_owned))
        .collect()
}

/// The centre of the canvas square (`x`, `y`) of `drawing`, from the house's button, the square (1, 1).
fn square(h: &H, x: i32, y: i32) -> egui::Pos2 {
    let house = h.get_by_label("house").rect();
    house.center()
        + egui::vec2(
            (x - 1) as f32 * house.width(),
            (y - 1) as f32 * house.height(),
        )
}

fn press(h: &mut H, at: egui::Pos2, pressed: bool) {
    h.event(egui::Event::PointerButton {
        pos: at,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    });
    h.run_steps(2);
}

#[test]
fn the_special_tools_choose_with_their_hotkeys() {
    let (mut h, script) = open(drawing());
    for key in ["o", "b", "e"] {
        h.event(egui::Event::Text(key.into()));
        h.run_steps(4);
    }
    assert_eq!(
        sent_rows(&script, "editor:tool"),
        ["tool:open", "tool:point", "tool:erase"]
    );
}

#[test]
fn a_click_on_an_empty_square_of_the_room_paints_it() {
    let (mut h, script) = open(drawing());
    let at = square(&h, 0, 0);
    h.hover_at(at);
    h.run_steps(2);
    press(&mut h, at, true);
    press(&mut h, at, false);
    h.run_steps(4);
    assert_eq!(sent_rows(&script, "editor:paint"), ["square:-1,-1"]);
}

#[test]
fn a_drag_from_the_room_over_the_special_paints_each_square_in_one_stroke() {
    let (mut h, script) = open(drawing());
    let from = square(&h, 0, 1);
    h.hover_at(from);
    h.run_steps(2);
    press(&mut h, from, true);
    for x in 1..=3 {
        h.hover_at(square(&h, x, 1));
        h.run_steps(2);
    }
    let last = square(&h, 3, 1);
    press(&mut h, last, false);
    h.run_steps(4);
    assert_eq!(
        sent_rows(&script, "editor:paint"),
        ["square:-1,0", "square:0,0", "square:1,0", "square:2,0"]
    );
    let sent = script
        .sent()
        .into_iter()
        .filter_map(|m| {
            m["params"]["operation"]["affordance_id"]
                .as_str()
                .map(str::to_owned)
        })
        .filter(|a| a.starts_with("editor:stroke"))
        .collect::<Vec<_>>();
    assert_eq!(sent, ["editor:stroke_begin", "editor:stroke_end"]);
}

#[test]
fn an_empty_square_of_the_room_is_drawn_as_an_outlined_square() {
    let (h, _) = open(drawing());
    let cell = h.get_by_label("house").rect();
    let outlined = |at: egui::Pos2| {
        let want = egui::Rect::from_center_size(at, cell.size());
        h.output()
            .shapes
            .iter()
            .any(|c| has_outline(&c.shape, want))
    };
    for (x, y) in [(0, 0), (3, 0), (0, 2), (3, 2), (3, 1)] {
        assert!(outlined(square(&h, x, y)), "square {x},{y} has no outline");
    }
}

/// Whether `shape` is a stroked rectangle that fills `want` to within a pixel.
fn has_outline(shape: &egui::Shape, want: egui::Rect) -> bool {
    match shape {
        egui::Shape::Vec(items) => items.iter().any(|s| has_outline(s, want)),
        egui::Shape::Rect(r) => {
            !r.stroke.is_empty()
                && (r.rect.min - want.min).abs().max_elem() < 1.5
                && (r.rect.max - want.max).abs().max_elem() < 1.5
        }
        _ => false,
    }
}

#[test]
fn a_blank_square_that_shows_something_is_not_outlined() {
    let mut screen = drawing();
    // Open air in a mapgen's rows: a blank glyph that still names what the square shows.
    let canvas = find(&mut screen["root"], "editor:canvas").unwrap();
    canvas["cells"][0]["looks"] = json!([1]);
    let (h, _) = open(screen);
    let cell = h.get_by_label("house").rect();
    let want = egui::Rect::from_center_size(square(&h, 0, 0), cell.size());
    assert!(
        !h.output()
            .shapes
            .iter()
            .any(|c| has_outline(&c.shape, want)),
        "a square that shows something is outlined as empty"
    );
}
