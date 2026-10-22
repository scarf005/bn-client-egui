#![recursion_limit = "512"]
//! The title screen as a centred column and the vehicle screen's facts, grid and parts, at 1280x800 and 1920x1080
//! in the client's own look, from the engine's examples.
mod common;
use bn_client_egui::{App, EngineModel};
use eframe::egui::{self, epaint};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use serde_json::{Value, json};

type H<'a> = Harness<'a, App<EngineModel>>;

const SIZES: [egui::Vec2; 2] = [egui::vec2(1280.0, 800.0), egui::vec2(1920.0, 1080.0)];

/// The widest the title's column is.
const COLUMN: f32 = 720.0;

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
        .unwrap_or_else(|| panic!("no example {name}"))["value"]
        .clone()
}

/// `stack` shown as a player sees it in a window of `size`, with the classic sidebar once a game runs.
fn shown<'a>(stack: Value, size: egui::Vec2) -> H<'a> {
    let (app, _) = common::staged(stack);
    let mut h = common::themed_harness(size, app);
    h.run_steps(6);
    common::settle_windows(&mut h);
    h
}

fn walk(shape: &egui::Shape, out: &mut Vec<egui::Rect>, fill: egui::Color32) {
    match shape {
        egui::Shape::Rect(r) if r.fill == fill => out.push(r.rect),
        egui::Shape::Vec(shapes) => shapes.iter().for_each(|s| walk(s, out, fill)),
        _ => {}
    }
}

/// The rectangles filled with `fill` in the last frame.
fn filled(h: &H, fill: egui::Color32) -> Vec<egui::Rect> {
    let mut out = vec![];
    h.output()
        .shapes
        .iter()
        .for_each(|s: &epaint::ClippedShape| walk(&s.shape, &mut out, fill));
    out
}

/// The selection bars of rows, not the 2 px bar under a selected tab: the cells of a row's bar joined.
fn bars(h: &H) -> Vec<egui::Rect> {
    let mut bars = Vec::<egui::Rect>::new();
    for cell in filled(h, bn_client_egui::kit::tokens::SELECTION) {
        if cell.height() <= 8.0 {
            continue;
        }
        match bars.iter_mut().find(|b| b.y_range() == cell.y_range()) {
            Some(bar) => *bar = bar.union(cell),
            None => bars.push(cell),
        }
    }
    bars
}

/// The title screen's selection bars, its banner and its tabs, as rectangles.
#[test]
fn the_title_is_a_centred_column_no_wider_than_the_selection_bar_may_be() {
    let title = example("title", "title screen with its banner and tabs");
    for size in SIZES {
        let h = shown(json!([title.clone()]), size);
        let bars = bars(&h);
        assert!(!bars.is_empty(), "{size:?}: no selection bar");
        for bar in bars {
            assert!(
                bar.width() <= COLUMN,
                "{size:?}: {bar:?} wider than {COLUMN}"
            );
            let middle = h.ctx.content_rect().center().x;
            assert!(
                (bar.center().x - middle).abs() < 8.0,
                "{size:?}: {bar:?} not centred on {middle}"
            );
        }
        let tab = h.get_by_label("New Game").rect();
        assert!(
            tab.right() <= h.ctx.content_rect().center().x + COLUMN / 2.0,
            "{size:?}: {tab:?}"
        );
    }
}

#[test]
fn the_title_on_the_editor_tab_is_a_centred_column_too() {
    let title = example("editor_menu", "title screen on the editor tab");
    for size in SIZES {
        let h = shown(json!([title.clone()]), size);
        for bar in bars(&h) {
            assert!(
                bar.width() <= COLUMN,
                "{size:?}: {bar:?} wider than {COLUMN}"
            );
        }
    }
}

/// The width of `text` on one line in the body font.
fn natural(h: &H, text: &str) -> f32 {
    h.ctx.fonts_mut(|f| {
        f.layout_no_wrap(
            text.into(),
            egui::FontId::proportional(bn_client_egui::kit::tokens::BODY),
            egui::Color32::WHITE,
        )
        .size()
        .x
    })
}

fn vehicle_stack(name: &str) -> Value {
    common::over_map(example("vehicle", name))
}

/// The Health column starts right after the longest part name, not at the far edge of the window; the vehicle's
/// grid and its parts sit side by side, and the overview's facts have their values in one column.
fn parts_beside_grid(name: &str) {
    for size in SIZES {
        let h = shown(vehicle_stack(name), size);
        let label = h
            .get_all_by_label_contains("extra light frame")
            .map(|n| n.rect())
            .max_by(|a, b| a.width().total_cmp(&b.width()))
            .expect("no part row");
        let wide = natural(&h, "[|| extra light frame]");
        let health = h
            .get_all_by_label("100%")
            .map(|n| n.rect())
            .min_by(|a, b| a.left().total_cmp(&b.left()))
            .expect("no health value");
        assert!(
            health.left() < label.left() + wide + 48.0,
            "{name} {size:?}: Health at {} past {} + {wide} + 48",
            health.left(),
            label.left()
        );
        let glyph = h.get_by_label("#").rect();
        assert!(
            glyph.right() < label.left() && glyph.center().y > label.top() - 80.0,
            "{name} {size:?}: grid {glyph:?} not beside the parts {label:?}"
        );
        let owner = h.get_by_label("Owner").rect();
        let mass = h.get_by_label("Mass").rect();
        let value = h.get_by_label("97 kg").rect();
        let follower = h.get_by_label("Your Followers").rect();
        assert!(
            (value.left() - follower.left()).abs() < 1.0 && owner.left() == mass.left(),
            "{name} {size:?}: facts not in a grid: {owner:?} {mass:?} {value:?} {follower:?}"
        );
    }
}

#[test]
fn the_vehicle_screen_puts_health_after_the_part_names_and_the_grid_beside_the_parts() {
    parts_beside_grid("vehicle screen");
}

#[test]
fn the_install_list_keeps_the_vehicles_parts_beside_its_grid_with_health_after_the_names() {
    parts_beside_grid("install list of the vehicle screen");
}
