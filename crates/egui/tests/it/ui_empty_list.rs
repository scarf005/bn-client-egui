//! A list with no rows keeps its pane: an outlined box as tall as the rows would have been, in the window.
use bn_client_egui::kit::tokens::BORDER;
use bn_client_egui::{App, EngineModel};
use eframe::egui::{self, epaint};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use serde_json::Value;

const SMALL: egui::Vec2 = egui::vec2(1280.0, 800.0);
const LARGE: egui::Vec2 = egui::vec2(1920.0, 1080.0);
/// The least height a pane of an empty list has: several rows.
const PANE: f32 = 150.0;

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

/// The outlines in the window's border colour that are not the window's own: the panes.
fn panes(h: &Harness<'_, App<EngineModel>>, title: &str) -> Vec<egui::Rect> {
    let window = h
        .query_all_by_role(egui::accesskit::Role::Window)
        .find(|w| w.accesskit_node().label().as_deref().map(str::trim) == Some(title))
        .unwrap_or_else(|| {
            let all = h.query_all_by_role(egui::accesskit::Role::Window);
            panic!(
                "no window {title:?} among {:?}",
                all.map(|w| w.accesskit_node().label()).collect::<Vec<_>>()
            )
        })
        .rect();
    h.output()
        .shapes
        .iter()
        .filter_map(|s| match &s.shape {
            egui::Shape::Rect(r) if r.stroke.color == BORDER => Some(r.rect),
            _ => None,
        })
        .filter(|r| window.contains_rect(*r) && r.width() < window.width() - 8.0)
        .collect()
}

#[test]
fn an_empty_list_keeps_an_outlined_pane_at_both_sizes() {
    let screens = [(
        "safemode",
        "safe mode manager without rules",
        "SAFE MODE MANAGER",
    )];
    for (file, name, title) in screens {
        let screen = example(file, name);
        for size in [SMALL, LARGE] {
            let h = shown(&screen, size);
            let found = panes(&h, title);
            assert!(
                found.iter().any(|r| r.height() >= PANE),
                "{file}@{size:?}: no pane of an empty list among {found:?}"
            );
        }
    }
}

#[test]
fn a_list_with_rows_has_no_outline_of_its_own() {
    let screen = example("safemode", "safe mode manager with the default ruleset");
    let h = shown(&screen, SMALL);
    let found = panes(&h, "SAFE MODE MANAGER");
    assert!(
        found.iter().all(|r: &epaint::Rect| r.height() < PANE),
        "{found:?}"
    );
}
