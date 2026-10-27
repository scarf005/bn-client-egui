//! The editor canvas is mouse-first: the wheel zooms about the pointer, a middle drag or Space with the primary
//! drag pans, the square under the pointer is outlined and named, and every click still lands on that square.
use crate::ui_editor_drag::{H, editor, open, open_sized, painted, sent};
use eframe::egui;
use egui_kittest::kittest::{NodeT, Queryable};

const SIDE: u32 = 12;

/// The editor canvas with a trail drag, each square labelled with its own coordinates.
fn screen() -> serde_json::Value {
    let mut screen = editor(Some("trail"));
    let mut stack = vec![&mut screen["root"]];
    while let Some(node) = stack.pop() {
        if node["id"] == "editor:canvas" {
            for cell in node["cells"].as_array_mut().unwrap() {
                cell["label"] = format!("c{},{}.", cell["x"], cell["y"]).into();
            }
            break;
        }
        match node {
            serde_json::Value::Array(items) => stack.extend(items.iter_mut()),
            serde_json::Value::Object(members) => stack.extend(members.values_mut()),
            _ => {}
        }
    }
    screen
}

fn square(h: &H, x: u32, y: u32) -> Option<egui::Rect> {
    h.get_all_by_label(&format!("c{x},{y}."))
        .find(|n| n.accesskit_node().role() == egui::accesskit::Role::Button)
        .map(|n| n.rect())
}

/// The coordinates of the drawn square under `at`.
fn under(h: &H, at: egui::Pos2) -> (u32, u32) {
    (0..SIDE)
        .flat_map(|y| (0..SIDE).map(move |x| (x, y)))
        .find(|&(x, y)| square(h, x, y).is_some_and(|r| r.contains(at)))
        .expect("a square under the pointer")
}

fn wheel(h: &mut H, at: egui::Pos2, dy: f32) {
    h.hover_at(at);
    h.run_steps(2);
    h.event(egui::Event::MouseWheel {
        unit: egui::MouseWheelUnit::Point,
        delta: egui::vec2(0.0, dy),
        modifiers: egui::Modifiers::NONE,
        phase: egui::TouchPhase::Move,
    });
    h.run_steps(12);
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

fn drag_by(h: &mut H, from: egui::Pos2, by: egui::Vec2, button: egui::PointerButton) {
    h.hover_at(from);
    h.run_steps(2);
    press(h, from, button, true);
    h.hover_at(from + by);
    h.run_steps(3);
    press(h, from + by, button, false);
    h.run_steps(3);
}

fn space(h: &mut H, pressed: bool) {
    h.event(egui::Event::Key {
        key: egui::Key::Space,
        physical_key: Some(egui::Key::Space),
        pressed,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    });
    h.run_steps(2);
}

/// A canvas zoomed in about the middle of its fifth square, and the pointer on it.
fn zoomed<'a>() -> (H<'a>, bn_client_egui::transport::ScriptHandle, egui::Pos2) {
    let (mut h, script) = open(screen());
    let at = square(&h, 5, 5).unwrap().center();
    wheel(&mut h, at, 120.0);
    (h, script, at)
}

#[test]
fn the_wheel_zooms_the_canvas_about_the_pointer() {
    // A scrolled view cannot show past the grid's edge, so the square is near it; the rest of the grid then fills the room.
    let (mut h, _script) = open_sized(screen(), egui::vec2(1280.0, 800.0));
    let at = square(&h, 1, 3).unwrap().center();
    let before = square(&h, 1, 3).unwrap();
    wheel(&mut h, at, 120.0);
    let after = square(&h, 1, 3).unwrap();
    assert!(after.width() > 1.5 * before.width(), "{before:?} {after:?}");
    assert!(after.contains(at), "{at:?} left {after:?}");
    // The same offset inside the square: the point of the grid under the pointer did not move.
    let spot = (at - after.min) / after.size();
    let was = (at - before.min) / before.size();
    assert!((spot - was).length() < 0.02, "{was:?} {spot:?}");
}

#[test]
fn the_wheel_zooms_out_no_further_than_the_whole_grid_and_in_no_further_than_four_times_the_fit() {
    let (mut h, _script) = open(screen());
    let at = square(&h, 5, 5).unwrap().center();
    let fit = square(&h, 5, 5).unwrap().width();
    wheel(&mut h, at, -2000.0);
    assert!((square(&h, 5, 5).unwrap().width() - fit).abs() < 0.5);
    for _ in 0..6 {
        wheel(&mut h, at, 2000.0);
    }
    let most = (0..SIDE)
        .filter_map(|x| square(&h, x, 5))
        .map(|r| r.width())
        .fold(0.0, f32::max);
    assert!(most > 3.0 * fit && most < 4.01 * fit, "{fit} {most}");
}

#[test]
fn a_middle_drag_moves_the_zoomed_grid_by_the_drag() {
    let (mut h, _script, at) = zoomed();
    let (x, y) = under(&h, at);
    let before = square(&h, x, y).unwrap();
    drag_by(
        &mut h,
        at,
        egui::vec2(-37.0, 23.0),
        egui::PointerButton::Middle,
    );
    let after = square(&h, x, y).unwrap();
    let moved = after.min - before.min;
    assert!(
        (moved - egui::vec2(-37.0, 23.0)).length() < 1.5,
        "{moved:?}"
    );
}

#[test]
fn a_pan_cannot_take_the_grid_out_of_view() {
    let (mut h, _script, at) = zoomed();
    for _ in 0..8 {
        drag_by(
            &mut h,
            at,
            egui::vec2(-300.0, -300.0),
            egui::PointerButton::Middle,
        );
    }
    assert!(
        (0..SIDE).any(|x| (0..SIDE).any(|y| square(&h, x, y).is_some())),
        "no square left in view"
    );
}

#[test]
fn a_click_after_zooming_and_panning_paints_the_square_under_the_pointer() {
    let (mut h, script, at) = zoomed();
    drag_by(
        &mut h,
        at,
        egui::vec2(-61.0, -17.0),
        egui::PointerButton::Middle,
    );
    let (x, y) = under(&h, at);
    press(&mut h, at, egui::PointerButton::Primary, true);
    press(&mut h, at, egui::PointerButton::Primary, false);
    h.run_steps(4);
    assert_eq!(painted(&script), [format!("square:{x},{y}")]);
}

#[test]
fn space_with_a_primary_drag_pans_and_sends_nothing() {
    let (mut h, script, at) = zoomed();
    let (x, y) = under(&h, at);
    let before = square(&h, x, y).unwrap();
    space(&mut h, true);
    drag_by(
        &mut h,
        at,
        egui::vec2(-37.0, 23.0),
        egui::PointerButton::Primary,
    );
    space(&mut h, false);
    let after = square(&h, x, y).unwrap();
    assert!(
        (after.min - before.min).length() > 20.0,
        "{before:?} {after:?}"
    );
    assert!(
        sent(&script).iter().all(|a| a != "editor:paint"),
        "{:?}",
        sent(&script)
    );
}

#[test]
fn hovering_a_square_shows_its_label_near_the_pointer() {
    let (mut h, _script) = open(screen());
    let at = square(&h, 3, 4).unwrap().center();
    let drawn = h.get_all_by_label("c3,4.").count();
    h.hover_at(at);
    h.run_steps(12);
    let shown = h.get_all_by_label("c3,4.").collect::<Vec<_>>();
    assert!(shown.len() > drawn, "the label shows in a tooltip");
    assert!(shown.iter().any(|n| n.rect().min.distance(at) < 120.0));
}

#[test]
fn a_zoomed_canvas_with_a_hovered_square_renders() {
    let (mut h, _script) = open_sized(screen(), egui::vec2(1920.0, 1080.0));
    let at = square(&h, 5, 5).unwrap().center();
    wheel(&mut h, at, 120.0);
    h.hover_at(at);
    h.run_steps(12);
    let image = h.render().expect("renders");
    if let Some(dir) = std::env::var_os("SHOT_DIR") {
        std::fs::create_dir_all(&dir).unwrap();
        image
            .save(std::path::Path::new(&dir).join("zoomed-hover.png"))
            .unwrap();
    }
}
