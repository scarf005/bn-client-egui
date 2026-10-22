//! A screen's own actions as one row of buttons under its content: back left, confirm right, the rest between
//! in the engine's order until the row is full and in the `⋯` menu after; each button reads the engine's label
//! with its first key as a dim caption, and lists every key on hover. Tab strips stay one row each.
mod common;
use bn_client_core::protocol::{AffordanceRole, Component, Screen};
use bn_client_egui::{App, EngineModel};
use eframe::egui::{self, epaint};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use serde_json::{Value, json};
use std::collections::HashSet;

const SMALL: egui::Vec2 = egui::vec2(1280.0, 800.0);
const LARGE: egui::Vec2 = egui::vec2(1920.0, 1080.0);
/// The button of the menu that holds the footer's further actions.
const MORE: &str = "⋯";

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

/// The crafting screen the engine published for a new character's weapon recipes: two levels of tabs.
fn crafting_weapons() -> Value {
    let path = format!(
        "{}/../core/tests/fixtures/crafting-weapons.json",
        env!("CARGO_MANIFEST_DIR")
    );
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

/// `screen` open over the map at `size`, once its window stopped moving.
fn shown<'a>(screen: &Value, size: egui::Vec2) -> Harness<'a, App<EngineModel>> {
    let (app, _) = common::staged(common::over_map(screen.clone()));
    let mut h = common::themed_harness(size, app);
    h.run_steps(5);
    common::settle_windows(&mut h);
    h
}

fn window(h: &Harness<'_, App<EngineModel>>, title: &str) -> egui::Rect {
    h.query_all_by_role(egui::accesskit::Role::Window)
        .find(|w| w.accesskit_node().label().as_deref() == Some(title))
        .unwrap_or_else(|| panic!("no window {title:?}"))
        .rect()
}

/// The labels of the affordances the screen's footer offers: those no component runs, but picking, moving,
/// digits, and the cancel and help its title bar holds.
fn footer_labels(screen: &Value) -> Vec<String> {
    let screen = serde_json::from_value::<Screen>(screen.clone()).unwrap();
    let root = screen.root.as_ref().unwrap();
    let claimed = root
        .nested()
        .into_iter()
        .flat_map(|c| {
            let extra = match c {
                Component::Tabs(t) => t.select.clone(),
                Component::Form(f) => f.submit.clone(),
                Component::List(l) => l.activate.clone(),
                _ => None,
            };
            c.head().affordances.iter().cloned().chain(extra)
        })
        .collect::<HashSet<_>>();
    screen
        .affordances
        .iter()
        .filter(|a| !claimed.contains(&a.id))
        .filter(|a| {
            !matches!(
                a.role,
                Some(
                    AffordanceRole::Select
                        | AffordanceRole::Navigate
                        | AffordanceRole::Digit
                        | AffordanceRole::Cancel
                        | AffordanceRole::Help
                )
            )
        })
        .map(|a| bn_client_core::markup::plain(&a.label.shown()))
        .collect()
}

/// The buttons labelled `label`, the engine's label alone.
fn buttons(h: &Harness<'_, App<EngineModel>>, label: &str) -> Vec<egui::Rect> {
    h.query_all_by_role(egui::accesskit::Role::Button)
        .filter(|b| b.accesskit_node().label().as_deref() == Some(label))
        .map(|b| b.rect())
        .collect()
}

/// The buttons of the action labelled `label`, whatever the key hint after the label.
fn action_buttons(h: &Harness<'_, App<EngineModel>>, label: &str) -> Vec<egui::Rect> {
    h.query_all_by_role(egui::accesskit::Role::Button)
        .filter(|b| {
            b.accesskit_node()
                .label()
                .and_then(|l| {
                    l.strip_prefix(label)
                        .map(|rest| rest.is_empty() || rest.starts_with(' '))
                })
                .unwrap_or(false)
        })
        .map(|b| b.rect())
        .collect()
}

#[test]
fn look_advanced_inventory_drop_and_trade_footers_keep_one_row_inside_their_window() {
    let screens = [
        ("look", "look-around screen", "Look Around"),
        ("aim", "advanced inventory screen", "Advanced inventory"),
        ("inventory", "drop selector screen", "Multidrop"),
        (
            "trade",
            "trade screen",
            "Trading with Daniel Arevalo, Merchant",
        ),
    ];
    for (file, name, title) in screens {
        let screen = example(file, name);
        for size in [SMALL, LARGE] {
            let h = shown(&screen, size);
            let window = window(&h, title);
            let labels = footer_labels(&screen);
            let drawn = labels
                .iter()
                .filter_map(|l| action_buttons(&h, l).first().copied())
                .chain(buttons(&h, MORE))
                .collect::<Vec<_>>();
            let missing = labels
                .iter()
                .filter(|l| action_buttons(&h, l).is_empty())
                .count();
            assert!(
                missing == 0 || !buttons(&h, MORE).is_empty(),
                "{file}@{size:?}: {missing} footer actions neither drawn nor in {MORE}"
            );
            let tops = drawn.iter().map(|r| r.top() as i32).collect::<HashSet<_>>();
            assert_eq!(tops.len(), 1, "{file}@{size:?}: footer rows {drawn:?}");
            for r in &drawn {
                assert!(
                    window.contains_rect(*r),
                    "{file}@{size:?}: {r:?} outside {window:?}"
                );
            }
        }
    }
}

#[test]
fn the_confirm_sits_rightmost_and_the_actions_the_row_has_no_room_for_are_in_the_more_menu() {
    let screen = example("look", "look-around screen");
    let mut h = shown(&screen, SMALL);
    let confirm = buttons(&h, "Confirm Choice")[0];
    let labels = footer_labels(&screen);
    for label in labels.iter().filter(|l| *l != "Confirm Choice") {
        for r in buttons(&h, label) {
            assert!(
                r.right() < confirm.left(),
                "{label} at {r:?} right of {confirm:?}"
            );
        }
    }
    let hidden = labels
        .iter()
        .filter(|l| buttons(&h, l).is_empty())
        .cloned()
        .collect::<Vec<_>>();
    assert!(!hidden.is_empty(), "the look footer fits 1280 whole");
    h.get_by_label(MORE).click();
    h.run_steps(3);
    for label in &hidden {
        assert_eq!(buttons(&h, label).len(), 1, "{label} not in {MORE}");
    }
}

#[test]
fn a_back_action_in_the_footer_sits_leftmost() {
    let mut screen = example("generic", "query popup screen");
    for a in screen["affordances"].as_array_mut().unwrap() {
        if a["id"] == "NO" {
            a["role"] = json!("cancel");
        }
    }
    let h = shown(&screen, LARGE);
    // The footer comes after the list, whose rows read the same.
    let footer = |label| *buttons(&h, label).last().unwrap();
    let (no, yes, confirm) = (footer("No"), footer("Yes"), footer("Confirm Choice"));
    assert!(no.right() < yes.left(), "{no:?} {yes:?}");
    assert!(yes.right() < confirm.left(), "{yes:?} {confirm:?}");
}

/// Every painted text with its galley's first format.
fn texts(shapes: &[epaint::ClippedShape]) -> Vec<&epaint::TextShape> {
    fn walk<'a>(shape: &'a egui::Shape, out: &mut Vec<&'a epaint::TextShape>) {
        match shape {
            egui::Shape::Text(text) => out.push(text),
            egui::Shape::Vec(shapes) => shapes.iter().for_each(|s| walk(s, out)),
            _ => {}
        }
    }
    let mut out = Vec::new();
    shapes.iter().for_each(|s| walk(&s.shape, &mut out));
    out
}

fn luminance(c: egui::Color32) -> u32 {
    u32::from(c.r()) + u32::from(c.g()) + u32::from(c.b())
}

#[test]
fn a_footer_button_reads_the_engines_label_with_its_first_key_as_a_dim_caption_inside() {
    let h = shown(&example("look", "look-around screen"), LARGE);
    let node = h
        .query_all_by_role(egui::accesskit::Role::Button)
        .find(|b| b.accesskit_node().label().as_deref() == Some("Confirm Choice"))
        .expect("a button labelled with the engine's label alone");
    assert_eq!(
        node.accesskit_node().data().keyboard_shortcut(),
        Some("RETURN")
    );
    let rect = node.rect();
    let shapes = &h.output().shapes;
    let painted = texts(shapes);
    let in_button = |text: &str| {
        painted
            .iter()
            .find(|t| t.galley.text() == text && rect.contains(t.pos))
            .unwrap_or_else(|| panic!("{text:?} not painted in {rect:?}"))
    };
    let (label, key) = (in_button("Confirm Choice"), in_button("RETURN"));
    let size = |t: &epaint::TextShape| t.galley.job.sections[0].format.font_id.size;
    // A section without a colour of its own is painted in the shape's fallback colour.
    let color = |t: &epaint::TextShape| match t.override_text_color {
        Some(c) => c,
        None => match t.galley.job.sections[0].format.color {
            egui::Color32::PLACEHOLDER => t.fallback_color,
            c => c,
        },
    };
    assert!(size(key) < size(label), "{} vs {}", size(key), size(label));
    assert!(
        luminance(color(key)) < luminance(color(label)),
        "{:?} vs {:?}",
        color(key),
        color(label)
    );
    assert!(key.pos.x > label.pos.x, "the key follows the label");
}

#[test]
fn a_footer_button_lists_every_key_bound_to_it_on_hover() {
    let mut h = shown(&example("look", "look-around screen"), LARGE);
    h.get_by_label("Go Up").hover();
    h.run_steps(6);
    h.get_by(|n| n.value().as_deref() == Some("PPAGE, <"));
}

/// The number of tabs of every tab strip of `screen`, outermost first.
fn strip_counts(screen: &Value) -> Vec<usize> {
    let mut counts = vec![];
    let mut at = &screen["root"];
    while at["kind"] == "tabs" {
        counts.push(at["tabs"].as_array().unwrap().len());
        at = &at["child"];
    }
    counts
}

#[test]
fn every_tab_strip_of_crafting_keeps_one_row_inside_its_window() {
    let screens = [
        (example("crafting", "Crafting screen"), "Crafting"),
        (crafting_weapons(), "Crafting"),
    ];
    for (screen, title) in screens {
        let counts = strip_counts(&screen);
        assert!(!counts.is_empty());
        for size in [SMALL, LARGE] {
            let h = shown(&screen, size);
            let window = window(&h, title);
            let tabs = h
                .query_all_by_role(egui::accesskit::Role::Tab)
                .map(|t| (t.rect(), t.accesskit_node().is_selected() == Some(true)))
                .collect::<Vec<_>>();
            let rows = tabs
                .iter()
                .map(|(r, _)| r.top() as i32)
                .collect::<HashSet<_>>();
            assert_eq!(rows.len(), counts.len(), "{size:?}: {tabs:?}");
            let chosen = tabs.iter().filter(|(_, on)| *on).count();
            assert_eq!(
                chosen,
                counts.len(),
                "{size:?}: a strip hides its chosen tab"
            );
            if tabs.len() < counts.iter().sum::<usize>() {
                assert!(
                    !buttons(&h, MORE).is_empty(),
                    "{size:?}: tabs neither drawn nor in {MORE}"
                );
            }
            for (r, _) in &tabs {
                assert!(
                    window.contains_rect(*r),
                    "{size:?}: {r:?} outside {window:?}"
                );
            }
        }
    }
}
