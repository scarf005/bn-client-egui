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
    t = t.on(
        "bn.command.submit",
        vec![fixture("submit_result"), fixture("command_completed")],
    );
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
