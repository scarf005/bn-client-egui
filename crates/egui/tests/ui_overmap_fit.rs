//! The overmap screen over a game: its map takes most of the window's body, and what the screen draws with it
//! (the map's own button, the legend) stays in the body, off the squares and above the footer.
mod common;
use bn_client_egui::{App, EngineModel};
use eframe::egui;
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use serde_json::Value;

type H<'a> = Harness<'a, App<EngineModel>>;

const SIZES: [egui::Vec2; 2] = [egui::vec2(1280.0, 800.0), egui::vec2(1920.0, 1080.0)];

const EXAMPLES: [&str; 2] = [
    "overmap screen",
    "overmap screen for a client that draws tiles",
];

fn example(name: &str) -> Value {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../core/tests/fixtures/0.2.examples/overmap.json"
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

/// The overmap example `name` pushed onto the map, in a window of `size`, settled.
fn shown<'a>(name: &str, size: egui::Vec2) -> H<'a> {
    let (app, _) = common::staged(common::over_map(example(name)));
    let mut h = common::themed_harness(size, app);
    h.run_steps(7);
    common::settle_windows(&mut h);
    h
}

/// The button whose label starts with `label`.
fn button(h: &H, label: &str) -> egui::Rect {
    h.get_all_by_role(egui::accesskit::Role::Button)
        .find(|b| {
            b.accesskit_node()
                .label()
                .is_some_and(|l| l.starts_with(label))
        })
        .unwrap_or_else(|| panic!("no button {label}"))
        .rect()
}

/// The window's body: under its title bar and above its footer.
fn body(h: &H) -> egui::Rect {
    let window = h
        .get_by_role_and_label(egui::accesskit::Role::Window, "Overmap")
        .rect();
    let title = h.get_by_label("Close window").rect().bottom();
    let footer = button(h, "Confirm Choice").top();
    egui::Rect::from_x_y_ranges(window.x_range(), title..=footer)
}

fn map(h: &H) -> egui::Rect {
    h.get_by_role_and_label(egui::accesskit::Role::Canvas, "Overmap")
        .rect()
}

#[test]
fn the_map_takes_most_of_the_body() {
    let mut small = vec![];
    for name in EXAMPLES {
        for size in SIZES {
            let h = shown(name, size);
            let (map, body) = (map(&h), body(&h));
            if map.height() < 0.7 * body.height() || map.width() < 0.5 * body.width() {
                small.push(format!("{name} at {size:?}: map {map:?} in body {body:?}"));
            }
        }
    }
    assert!(small.is_empty(), "{small:#?}");
}

#[test]
fn the_map_button_and_legend_sit_in_the_body_off_the_map() {
    let mut hidden = vec![];
    for name in EXAMPLES {
        for size in SIZES {
            let h = shown(name, size);
            let (map, body) = (map(&h), body(&h));
            let legend = h.get_by_label("LEVEL 0, 50, 50").rect();
            let choose = button(&h, "Choose destination");
            for (what, rect) in [("legend", legend), ("Choose destination", choose)] {
                if !body.contains_rect(rect) || rect.intersects(map) {
                    hidden.push(format!(
                        "{name} at {size:?}: {what} {rect:?}, map {map:?}, body {body:?}"
                    ));
                }
            }
        }
    }
    assert!(hidden.is_empty(), "{hidden:#?}");
}
