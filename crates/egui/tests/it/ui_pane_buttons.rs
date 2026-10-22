//! The buttons a pane names are as wide as their labels: a pane is never so narrow that one breaks over lines.
use bn_client_egui::{App, EngineModel};
use eframe::egui;
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use serde_json::Value;

const SMALL: egui::Vec2 = egui::vec2(1280.0, 800.0);
const LARGE: egui::Vec2 = egui::vec2(1920.0, 1080.0);
/// A button of one line of text: the font and the padding around it.
const ONE_LINE: f32 = 30.0;

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
        .unwrap_or_else(|| panic!("{file}/{name}"))["value"]
        .clone()
}

fn shown<'a>(screen: &Value, size: egui::Vec2) -> Harness<'a, App<EngineModel>> {
    let (app, _) = crate::common::staged(crate::common::over_map(screen.clone()));
    let mut h = crate::common::themed_harness(size, app);
    h.run_steps(5);
    crate::common::settle_windows(&mut h);
    h
}

/// The rectangles of the buttons labelled `label`, whatever the key hint after the label.
fn buttons(h: &Harness<'_, App<EngineModel>>, label: &str) -> Vec<egui::Rect> {
    h.query_all_by_role(egui::accesskit::Role::Button)
        .filter(|b| {
            b.accesskit_node().label().is_some_and(|l| {
                l.strip_prefix(label)
                    .is_some_and(|r| r.is_empty() || r.starts_with(' '))
            })
        })
        .map(|b| b.rect())
        .collect()
}

#[test]
fn the_advanced_inventory_panes_keep_each_of_their_buttons_on_one_line() {
    let screen = example("aim", "advanced inventory screen");
    for size in [SMALL, LARGE] {
        let h = shown(&screen, size);
        for label in [
            "Move an amount of item",
            "Show area",
            "Toggle vehicle",
            "Filter",
        ] {
            let found = buttons(&h, label);
            assert!(!found.is_empty(), "{size:?}: no button {label:?}");
            for r in found {
                assert!(
                    r.height() <= ONE_LINE,
                    "{size:?}: {label:?} on several lines, {r:?}"
                );
            }
        }
    }
}
