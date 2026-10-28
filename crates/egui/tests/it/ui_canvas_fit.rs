//! A canvas grid fits its squares to its room but never grows them past a size that keeps sprites readable, and a
//! square names what it holds and, when it is off, why, on hover.
use crate::ui_editor_drag::{H, editor_of, open_sized, painted};
use eframe::egui;
use egui_kittest::kittest::{NodeT, Queryable};
use serde_json::{Value, json};

const SMALL: egui::Vec2 = egui::vec2(1280.0, 800.0);
const LARGE: egui::Vec2 = egui::vec2(1920.0, 1080.0);

/// A canvas of `side` squares, each labelled with its coordinates; the square 1,1 is off with a reason.
fn screen(side: u32) -> Value {
    let mut screen = editor_of(Some("trail"), side);
    let mut stack = vec![&mut screen["root"]];
    while let Some(node) = stack.pop() {
        if node["id"] == "editor:canvas" {
            for cell in node["cells"].as_array_mut().unwrap() {
                cell["label"] = format!("c{},{}.", cell["x"], cell["y"]).into();
                if (cell["x"].as_u64(), cell["y"].as_u64()) == (Some(1), Some(1)) {
                    cell["enabled"] = json!(false);
                    cell["denial"] = json!("No frame to hold it here");
                }
            }
            break;
        }
        match node {
            Value::Array(items) => stack.extend(items.iter_mut()),
            Value::Object(members) => stack.extend(members.values_mut()),
            _ => {}
        }
    }
    screen
}

fn square(h: &H, x: u32, y: u32) -> egui::Rect {
    h.get_all_by_label(&format!("c{x},{y}."))
        .find(|n| n.accesskit_node().role() == egui::accesskit::Role::Button)
        .map(|n| n.rect())
        .expect("the square is drawn")
}

#[test]
fn a_small_canvas_keeps_its_squares_no_larger_than_readable_sprites_need() {
    for size in [SMALL, LARGE] {
        let (h, _) = open_sized(screen(5), size);
        let side = square(&h, 0, 0).width();
        assert!((26.0..=40.5).contains(&side), "{size:?}: {side}");
        let last = square(&h, 4, 4);
        assert!(last.max.x <= size.x && last.max.y <= size.y, "{last:?}");
    }
}

#[test]
fn a_large_canvas_still_fits_its_whole_grid_in_its_room() {
    let (h, _) = open_sized(screen(24), SMALL);
    let (first, last) = (square(&h, 0, 0), square(&h, 23, 23));
    assert!(first.width() < 40.0, "{first:?}");
    assert!(last.max.y <= SMALL.y && last.max.x <= SMALL.x, "{last:?}");
}

#[test]
fn a_square_names_what_it_holds_and_why_it_is_off_on_hover() {
    let says = |h: &H, text: &str| h.query_all_by_label_contains(text).count();
    let (mut h, _) = open_sized(screen(5), LARGE);
    h.hover_at(square(&h, 3, 3).center());
    h.run_steps(40);
    assert_eq!(says(&h, "c3,3."), 2, "the square's name and its tooltip");
    assert_eq!(says(&h, "No frame"), 0);
    h.hover_at(square(&h, 1, 1).center());
    h.run_steps(40);
    assert_eq!(says(&h, "No frame to hold it here"), 1);
    assert_eq!(
        says(&h, "c1,1."),
        2,
        "its name and the tooltip with its reason"
    );
}

/// A right click at `at`.
fn right_click(h: &mut H, at: egui::Pos2) {
    h.hover_at(at);
    h.run_steps(2);
    for pressed in [true, false] {
        h.event(egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Secondary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        });
        h.run_steps(2);
    }
}

#[test]
fn a_right_click_runs_the_grids_secondary_affordance_on_that_square() {
    let mut with = screen(5);
    let mut stack = vec![&mut with["root"]];
    while let Some(node) = stack.pop() {
        if node["id"] == "editor:canvas" {
            node["secondary"] = json!("editor:paint");
            break;
        }
        match node {
            Value::Array(items) => stack.extend(items.iter_mut()),
            Value::Object(members) => stack.extend(members.values_mut()),
            _ => {}
        }
    }
    let (mut h, script) = open_sized(with, LARGE);
    let at = square(&h, 3, 3).center();
    right_click(&mut h, at);
    assert_eq!(painted(&script), ["square:3,3"]);
    let (mut h, script) = open_sized(screen(5), LARGE);
    let at = square(&h, 3, 3).center();
    right_click(&mut h, at);
    assert!(painted(&script).is_empty(), "no secondary, no action");
}

#[test]
fn a_refused_square_says_why_on_hover_and_still_takes_its_click() {
    let mut with = screen(5);
    let mut stack = vec![&mut with["root"]];
    while let Some(node) = stack.pop() {
        if node["id"] == "editor:canvas" {
            for cell in node["cells"].as_array_mut().unwrap() {
                if (cell["x"].as_u64(), cell["y"].as_u64()) == (Some(3), Some(3)) {
                    cell["refused"] = json!("Install a frame here first");
                }
            }
            break;
        }
        match node {
            Value::Array(items) => stack.extend(items.iter_mut()),
            Value::Object(members) => stack.extend(members.values_mut()),
            _ => {}
        }
    }
    let (mut h, script) = open_sized(with, LARGE);
    let at = square(&h, 3, 3).center();
    h.hover_at(at);
    h.run_steps(40);
    assert_eq!(
        h.query_all_by_label_contains("Install a frame here first")
            .count(),
        1
    );
    h.hover_at(at);
    for pressed in [true, false] {
        h.event(egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        });
        h.run_steps(3);
    }
    assert_eq!(painted(&script), ["square:3,3"]);
}

#[test]
fn the_canvas_keeps_its_place_when_the_news_in_the_status_strip_comes_and_goes() {
    for size in [SMALL, LARGE] {
        let mut tops = vec![];
        for news in [false, true] {
            let mut screen = screen(5);
            if news {
                screen["root"]["children"].as_array_mut().unwrap().push(json!({
                    "id": "ed:news", "kind": "text", "slot": "log", "markup": "Installed frame at 1,0.",
                    "tone": "good"}));
            }
            let (h, _) = open_sized(screen, size);
            tops.push(square(&h, 0, 0).top());
        }
        assert_eq!(tops[0], tops[1], "at {size:?}");
    }
}
