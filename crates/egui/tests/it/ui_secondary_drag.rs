//! A secondary drag over a canvas that asks for one runs the grid's secondary affordance on every square it
//! crosses, each once, between the grid's begin and end; a canvas that does not ask keeps the click only.
use crate::ui_editor_drag::{H, editor, open, painted, sent, square};
use eframe::egui;
use serde_json::{Value, json};

fn canvas(screen: &mut Value) -> &mut Value {
    let mut stack = vec![&mut screen["root"]];
    while let Some(node) = stack.pop() {
        if node["id"] == "editor:canvas" {
            return node;
        }
        match node {
            Value::Array(items) => stack.extend(items.iter_mut()),
            Value::Object(members) => stack.extend(members.values_mut()),
            _ => {}
        }
    }
    unreachable!()
}

/// The editor whose canvas erases on a secondary click, and on a secondary drag when `drag`.
fn erasing(primary: Option<&str>, drag: bool) -> Value {
    let mut screen = editor(primary);
    let node = canvas(&mut screen);
    node["secondary"] = json!("editor:paint");
    node["secondary_drag"] = json!(drag.then_some("trail"));
    node["drag_begin"] = json!("editor:stroke_begin");
    node["drag_end"] = json!("editor:stroke_end");
    screen
}

fn press(h: &mut H, at: egui::Pos2, button: egui::PointerButton, pressed: bool) {
    h.event(egui::Event::PointerButton {
        pos: at,
        button,
        pressed,
        modifiers: egui::Modifiers::NONE,
    });
    h.run_steps(2);
}

fn move_to(h: &mut H, at: egui::Pos2) {
    h.hover_at(at);
    h.run_steps(2);
}

/// Presses the secondary button on `from`, moves through `via`, and releases on the last square.
fn right_drag(h: &mut H, from: (u32, u32), via: &[(u32, u32)]) {
    let at = square(h, from.0, from.1);
    move_to(h, at);
    press(h, at, egui::PointerButton::Secondary, true);
    let mut last = at;
    for &(x, y) in via {
        last = square(h, x, y);
        move_to(h, last);
    }
    press(h, last, egui::PointerButton::Secondary, false);
    h.run_steps(4);
}

#[test]
fn a_secondary_drag_runs_the_secondary_once_on_every_square_it_crosses_between_begin_and_end() {
    let (mut h, script) = open(erasing(None, true));
    right_drag(&mut h, (1, 2), &[(4, 2)]);
    assert_eq!(
        sent(&script),
        [
            "editor:stroke_begin",
            "editor:paint",
            "editor:paint",
            "editor:paint",
            "editor:paint",
            "editor:stroke_end"
        ]
    );
    assert_eq!(
        painted(&script),
        ["square:1,2", "square:2,2", "square:3,2", "square:4,2"]
    );
}

#[test]
fn a_plain_secondary_click_on_a_secondary_drag_canvas_runs_the_secondary_once() {
    let (mut h, script) = open(erasing(None, true));
    right_drag(&mut h, (3, 3), &[]);
    assert_eq!(painted(&script), ["square:3,3"]);
}

#[test]
fn a_secondary_drag_on_a_canvas_without_it_runs_nothing_but_a_click() {
    let (mut h, script) = open(erasing(None, false));
    right_drag(&mut h, (1, 2), &[(4, 2)]);
    assert!(sent(&script).is_empty(), "{:?}", sent(&script));
    right_drag(&mut h, (3, 3), &[]);
    assert_eq!(painted(&script), ["square:3,3"]);
}

#[test]
fn a_secondary_drag_runs_on_squares_with_no_primary_affordance() {
    let mut screen = erasing(None, true);
    for cell in canvas(&mut screen)["cells"].as_array_mut().unwrap() {
        cell.as_object_mut().unwrap().remove("affordance");
    }
    let (mut h, script) = open(screen);
    right_drag(&mut h, (1, 2), &[(3, 2)]);
    assert_eq!(painted(&script), ["square:1,2", "square:2,2", "square:3,2"]);
}

#[test]
fn a_secondary_drag_with_space_held_pans_and_sends_nothing() {
    let (mut h, script) = open(erasing(None, true));
    let key = |pressed| egui::Event::Key {
        key: egui::Key::Space,
        physical_key: Some(egui::Key::Space),
        pressed,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    };
    let at = square(&h, 1, 2);
    move_to(&mut h, at);
    h.event(key(true));
    h.run_steps(2);
    right_drag(&mut h, (1, 2), &[(4, 2)]);
    h.event(key(false));
    h.run_steps(2);
    assert!(sent(&script).is_empty(), "{:?}", sent(&script));
}

#[test]
fn a_primary_and_a_secondary_drag_each_make_their_own_stroke() {
    let (mut h, script) = open(erasing(Some("trail"), true));
    let at = square(&h, 1, 1);
    move_to(&mut h, at);
    press(&mut h, at, egui::PointerButton::Primary, true);
    let to = square(&h, 2, 1);
    move_to(&mut h, to);
    press(&mut h, to, egui::PointerButton::Primary, false);
    h.run_steps(4);
    right_drag(&mut h, (1, 3), &[(2, 3)]);
    let sent = sent(&script);
    let strokes = |what: &str| sent.iter().filter(|s| *s == what).count();
    assert_eq!(
        (strokes("editor:stroke_begin"), strokes("editor:stroke_end")),
        (2, 2)
    );
    assert_eq!(
        painted(&script),
        ["square:1,1", "square:2,1", "square:1,3", "square:2,3"]
    );
}

#[test]
fn a_secondary_press_during_a_primary_drag_joins_no_stroke() {
    let (mut h, script) = open(erasing(Some("trail"), true));
    let at = square(&h, 1, 1);
    move_to(&mut h, at);
    press(&mut h, at, egui::PointerButton::Primary, true);
    let over = square(&h, 4, 4);
    move_to(&mut h, over);
    press(&mut h, over, egui::PointerButton::Secondary, true);
    let to = square(&h, 5, 4);
    move_to(&mut h, to);
    press(&mut h, to, egui::PointerButton::Secondary, false);
    press(&mut h, to, egui::PointerButton::Primary, false);
    h.run_steps(4);
    let sent = sent(&script);
    let strokes = |what: &str| sent.iter().filter(|s| *s == what).count();
    assert_eq!(
        (strokes("editor:stroke_begin"), strokes("editor:stroke_end")),
        (1, 1)
    );
}
