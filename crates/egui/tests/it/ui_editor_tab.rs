//! Tab walks an editor in reading order: its menus, its tools, the canvas, then each column top to bottom, a list
//! one stop whose row the arrows move, every stop named for a screen reader.
use bn_client_egui::{App, EngineModel};
use eframe::egui;
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use serde_json::Value;

type H<'a> = Harness<'a, App<EngineModel>>;
type Stop = (egui::accesskit::Role, Option<String>, egui::Rect);

const SIZES: [egui::Vec2; 2] = [egui::vec2(1290.0, 830.0), egui::vec2(1920.0, 1080.0)];

fn shown<'a>(screen: &Value, size: egui::Vec2) -> H<'a> {
    let (app, _) = crate::common::staged(crate::common::over_map(screen.clone()));
    let mut h = crate::common::themed_harness(size, app);
    h.run_steps(7);
    crate::common::settle_windows(&mut h);
    h
}

/// The focused node's role, name and place.
fn focused(h: &H) -> Option<Stop> {
    h.root()
        .children_recursive()
        .find(|n| {
            n.accesskit_node().is_focused()
                && n.accesskit_node().role() != egui::accesskit::Role::Window
        })
        .map(|n| {
            (
                n.accesskit_node().role(),
                n.accesskit_node().label(),
                n.rect(),
            )
        })
}

/// The stops Tab walks from the first until it comes back to it, at most `most`.
fn stops(h: &mut H, most: usize) -> Vec<Stop> {
    let mut out: Vec<Stop> = vec![];
    for _ in 0..most {
        h.key_press(egui::Key::Tab);
        h.run_steps(2);
        let Some(stop) = focused(h) else {
            continue;
        };
        if out
            .first()
            .is_some_and(|first| first.1 == stop.1 && first.0 == stop.0)
        {
            break;
        }
        out.push(stop);
    }
    out
}

fn names(stops: &[Stop]) -> Vec<String> {
    stops
        .iter()
        .map(|s| s.1.clone().unwrap_or_else(|| format!("<{:?}>", s.0)))
        .collect()
}

/// Where the first stop named `label` is in `stops`.
fn at(stops: &[Stop], label: &str) -> usize {
    stops
        .iter()
        .position(|s| s.1.as_deref() == Some(label))
        .unwrap_or_else(|| panic!("no stop {label}: {:?}", names(stops)))
}

/// Walks `screen` at each size: every stop is named, the stops named `order` come in that order, and no more than
/// `most` stops make the round.
fn walks(screen: &Value, order: &[&str], most: usize) {
    for size in SIZES {
        let mut h = shown(screen, size);
        let stops = stops(&mut h, 200);
        let unnamed = stops
            .iter()
            .filter(|s| s.1.as_deref().is_none_or(str::is_empty))
            .collect::<Vec<_>>();
        assert!(unnamed.is_empty(), "{size:?}: unnamed stops {unnamed:?}");
        let found = order.iter().map(|l| at(&stops, l)).collect::<Vec<_>>();
        assert!(
            found.windows(2).all(|w| w[0] < w[1]),
            "{size:?}: {order:?} at {found:?} in {:?}",
            names(&stops)
        );
        assert!(
            stops.len() <= most,
            "{size:?}: {} stops: {:?}",
            stops.len(),
            names(&stops)
        );
    }
}

#[test]
fn tab_walks_the_map_editor_in_reading_order_and_names_every_stop() {
    let screen =
        crate::ui_window_class::example("editor", "map editor with a drawn room and a new mapping");
    walks(
        &screen,
        &[
            "File",
            "History",
            "Rectangle",
            "Canvas",
            "Legend",
            "Symbol",
            "Terrain",
        ],
        70,
    );
}

#[test]
fn tab_walks_the_vehicle_editor_in_reading_order_and_names_every_stop() {
    let screen = crate::ui_window_class::example(
        "vehicle_editor",
        "vehicle editor with a prototype open on a mount",
    );
    walks(
        &screen,
        &["File", "Tools", "Canvas", "Parts", "Prototypes"],
        60,
    );
}

/// The search boxes of `screen` are named by the headings of the lists they search, never "Search".
fn searches_named(screen: &Value, headings: &[&str]) {
    let h = shown(screen, SIZES[0]);
    let boxes = h
        .root()
        .children_recursive()
        .filter(|n| n.accesskit_node().role() == egui::accesskit::Role::TextInput)
        .filter_map(|n| n.accesskit_node().label())
        .collect::<Vec<_>>();
    for heading in headings {
        assert!(
            boxes.iter().any(|b| b == heading),
            "no search box named {heading:?}: {boxes:?}"
        );
    }
    assert!(!boxes.iter().any(|b| b == "Search"), "{boxes:?}");
}

#[test]
fn the_map_editors_search_boxes_are_named_by_their_lists_headings() {
    let screen =
        crate::ui_window_class::example("editor", "map editor with a drawn room and a new mapping");
    searches_named(&screen, &["Catalog", "Files"]);
}

#[test]
fn the_vehicle_editors_search_boxes_are_named_by_their_lists_headings() {
    let screen = crate::ui_window_class::example(
        "vehicle_editor",
        "vehicle editor with a prototype open on a mount",
    );
    searches_named(&screen, &["Vehicles", "Parts"]);
}

/// The map editor with Tab walked to its legend's row, the list's stop.
fn on_a_legend_row<'a>() -> H<'a> {
    let screen =
        crate::ui_window_class::example("editor", "map editor with a drawn room and a new mapping");
    let mut h = shown(&screen, SIZES[0]);
    let mut seen = vec![];
    for _ in 0..80 {
        h.key_press(egui::Key::Tab);
        h.run_steps(2);
        seen.extend(focused(&h));
        if focused(&h).is_some_and(|f| f.1.as_deref() == Some("#")) {
            h.run_steps(4);
            assert_eq!(focused(&h).and_then(|f| f.1).as_deref(), Some("#"));
            return h;
        }
    }
    panic!("Tab never reaches the legend's row: {seen:?}");
}

#[test]
fn tab_leaves_a_list_whose_row_has_the_focus_for_the_next_stop_and_stays_there() {
    let mut h = on_a_legend_row();
    h.key_press(egui::Key::Tab);
    h.run_steps(2);
    let next = focused(&h).and_then(|f| f.1);
    assert_ne!(next.as_deref(), Some("#"));
    h.run_steps(6);
    assert_eq!(focused(&h).and_then(|f| f.1), next);
}

#[test]
fn a_click_into_a_text_field_below_a_focused_list_row_takes_the_typing() {
    let mut h = on_a_legend_row();
    let legend = focused(&h).unwrap().2;
    let field = h
        .root()
        .children_recursive()
        .filter(|n| n.accesskit_node().role() == egui::accesskit::Role::TextInput)
        .map(|n| n.rect())
        .find(|r| r.top() > legend.bottom())
        .expect("a text field below the legend");
    h.query_all_by_role(egui::accesskit::Role::TextInput)
        .find(|n| n.rect() == field)
        .unwrap()
        .click();
    h.run_steps(4);
    h.event(egui::Event::Text("q".into()));
    h.run_steps(4);
    let typed = h
        .root()
        .children_recursive()
        .find(|n| {
            n.accesskit_node().role() == egui::accesskit::Role::TextInput && n.rect() == field
        })
        .map(|n| (n.accesskit_node().is_focused(), n.accesskit_node().value()));
    assert!(
        typed
            .as_ref()
            .is_some_and(|(f, v)| *f && v.as_deref().is_some_and(|v| v.contains('q'))),
        "{typed:?}"
    );
}
