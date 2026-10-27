//! The map editor over a game: its window fills the map region like any screen of its own map or canvas, and every
//! field shows the whole value the engine published for it.
use crate::common::legible::cut_text;
use bn_client_egui::{App, EngineModel};
use eframe::egui;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use serde_json::Value;

type H<'a> = Harness<'a, App<EngineModel>>;

const SIZES: [egui::Vec2; 2] = [egui::vec2(1280.0, 800.0), egui::vec2(1920.0, 1080.0)];

/// The editor examples with a document open: `file` and name.
const OPEN: [(&str, &str); 4] = [
    ("editor", "map editor with a nested chunk open"),
    ("editor", "map editor with a drawn room and a new mapping"),
    ("editor", "map editor with a placement picked"),
    (
        "editor_menu",
        "map editor of the main menu with a document open",
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
    let (app, _) = crate::common::staged(crate::common::over_map(screen.clone()));
    let mut h = crate::common::themed_harness(size, app);
    h.run_steps(7);
    crate::common::settle_windows(&mut h);
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

/// The overmap's window at `size`: a screen of its own map, which fills the map region.
fn overmap_window(size: egui::Vec2) -> egui::Rect {
    let h = shown(&example("overmap", "overmap screen"), size);
    h.get_by_role_and_label(egui::accesskit::Role::Window, "Overmap")
        .rect()
}

#[test]
fn the_editor_window_fills_the_map_region_as_the_overmap_does() {
    let mut wrong = vec![];
    for size in SIZES {
        let full = overmap_window(size);
        // The main menu's editor is a workspace, the whole view: see below.
        for (file, name) in OPEN.into_iter().filter(|(file, _)| *file == "editor") {
            let window = window(&shown(&example(file, name), size));
            if (window.min - full.min).length() > 1.0 || (window.max - full.max).length() > 1.0 {
                wrong.push(format!(
                    "{name} at {size:?}: {window:?}, the overmap {full:?}"
                ));
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

/// A grid that only shows squares, which none of its squares run, is no canvas: the vehicle screen stays a window
/// beside the map.
#[test]
fn a_screen_of_a_grid_the_player_does_not_draw_on_leaves_the_map_visible() {
    for size in SIZES {
        let full = overmap_window(size);
        let h = shown(&example("vehicle", "vehicle screen"), size);
        let window = h
            .get_by_role_and_label(egui::accesskit::Role::Window, "Bicycle")
            .rect();
        assert!(
            window.height() < full.height() - 1.0,
            "{size:?}: {window:?} fills the region as {full:?}"
        );
    }
}

/// The main menu's editor is the whole view: no map, sidebar or toolbar of the game under it, and its window as wide
/// as the client.
#[test]
fn a_workspace_over_the_map_hides_the_game_and_fills_the_client() {
    for size in SIZES {
        let mut screen = example("editor", "map editor with a drawn room and a new mapping");
        screen["role"] = "workspace".into();
        let h = shown(&screen, size);
        assert!(
            h.query_by_label("Resize sidebar").is_none(),
            "{size:?}: the sidebar is drawn"
        );
        let window = window(&h);
        assert!(
            window.left() < 20.0 && window.right() > size.x - 20.0,
            "{size:?}: {window:?} is narrower than the client"
        );
    }
}
