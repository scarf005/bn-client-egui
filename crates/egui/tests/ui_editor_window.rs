//! The map editor over a game: its window fills the map region like any screen of its own canvas, and every
//! field shows the whole value the engine published for it.
mod common;
use bn_client_egui::{App, EngineModel};
use common::legible::{cut_text, room};
use eframe::egui;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use serde_json::Value;

type H<'a> = Harness<'a, App<EngineModel>>;

const SIZES: [egui::Vec2; 2] = [egui::vec2(1280.0, 800.0), egui::vec2(1920.0, 1080.0)];

/// The space a window keeps from the edge of the map region.
const MARGIN: f32 = 8.0;

/// The editor examples with a document open: `file` and name.
const OPEN: [(&str, &str); 4] = [
    ("editor", "map editor with a nested chunk open"),
    ("editor", "map editor with a drawn room and a new mapping"),
    ("editor", "map editor with a placement picked"),
    (
        "editor_menu",
        "map editor reopened on the document it was left with",
    ),
];

fn example(file: &str, name: &str) -> Value {
    let path = format!(
        "{}/../core/tests/fixtures/0.2.examples/{file}.json",
        env!("CARGO_MANIFEST_DIR")
    );
    let all = serde_json::from_str::<Value>(&std::fs::read_to_string(path).unwrap()).unwrap();
    all["valid"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["name"] == name)
        .unwrap()["value"]
        .clone()
}

/// `screen` pushed onto the map, in a window of `size`, settled.
fn shown<'a>(screen: &Value, size: egui::Vec2) -> H<'a> {
    let (app, _) = common::staged(common::over_map(screen.clone()));
    let mut h = common::themed_harness(size, app);
    h.run_steps(7);
    common::settle_windows(&mut h);
    h
}

fn window(h: &H) -> egui::Rect {
    h.get_by_role_and_label(egui::accesskit::Role::Window, "Map Editor")
        .rect()
}

/// Every non-empty value the screen's text fields publish.
fn field_values(screen: &Value) -> Vec<String> {
    fn walk(v: &Value, out: &mut Vec<String>) {
        match v {
            Value::Object(o) => {
                if o.get("kind").and_then(Value::as_str) == Some("form") {
                    let fields = o["fields"].as_array().into_iter().flatten();
                    out.extend(
                        fields
                            .filter(|f| f["type"] == "text")
                            .filter_map(|f| f["value"].as_str())
                            .filter(|v| !v.is_empty())
                            .map(str::to_owned),
                    );
                }
                o.values().for_each(|v| walk(v, out));
            }
            Value::Array(a) => a.iter().for_each(|v| walk(v, out)),
            _ => {}
        }
    }
    let mut out = vec![];
    walk(screen, &mut out);
    out
}

#[test]
fn the_editor_window_fills_the_map_region() {
    let mut wrong = vec![];
    for (file, name) in OPEN {
        for size in SIZES {
            let h = shown(&example(file, name), size);
            let (window, inner) = (window(&h), room(&h).shrink(MARGIN));
            let near = |a: f32, b: f32| (a - b).abs() <= 2.0;
            let fills = near(window.left(), inner.left())
                && near(window.right(), inner.right())
                && near(window.top(), inner.top())
                && near(window.bottom(), inner.bottom());
            if !fills {
                wrong.push(format!("{name} at {size:?}: {window:?} in {inner:?}"));
            }
        }
    }
    assert!(wrong.is_empty(), "{wrong:#?}");
}

#[test]
fn every_field_shows_its_whole_value() {
    let mut cut = vec![];
    for (file, name) in OPEN {
        let screen = example(file, name);
        let values = field_values(&screen);
        for size in SIZES {
            let h = shown(&screen, size);
            cut.extend(
                cut_text(&h.output().shapes, window(&h))
                    .into_iter()
                    .filter(|line| values.iter().any(|v| line.starts_with(&format!("{v:?}"))))
                    .map(|line| format!("{name} at {size:?}: {line}")),
            );
        }
    }
    assert!(cut.is_empty(), "{cut:#?}");
}
