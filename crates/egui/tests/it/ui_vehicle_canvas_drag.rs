//! A canvas turned a quarter, its columns and rows swapped, is placed by the cells' own `x` and `y`, never by their
//! ids, and a span over it runs its affordance on the press and the release square between the grid's begin and end.
use crate::ui_editor_drag::{editor, open, painted, sent, square};
use eframe::egui;
use serde_json::{Value, json};

/// The editor whose canvas is `cols`×`rows` squares, each named for the square it was before the view turned: the
/// view turned clockwise puts the square at `(x, y)` of the old `rows`×`cols` canvas at `(rows - 1 - y, x)`.
fn turned(cols: u32, rows: u32, drag: &str) -> Value {
    let mut screen = editor(Some(drag));
    let mut stack = vec![&mut screen["root"]];
    while let Some(node) = stack.pop() {
        if node["id"] == "editor:canvas" {
            node["cols"] = json!(cols);
            node["rows"] = json!(rows);
            node["drag_begin"] = json!("editor:stroke_begin");
            node["drag_end"] = json!("editor:stroke_end");
            node["cells"] = (0..rows)
                .flat_map(|y| (0..cols).map(move |x| (x, y)))
                .map(|(x, y)| (x, y, (y, cols - 1 - x)))
                .map(|(x, y, (was_x, was_y))| json!({ "x": x, "y": y, "id": format!("square:{was_x},{was_y}"), "glyph": ".", "affordance": "editor:paint", "enabled": true }))
                .collect();
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

/// The id the cell at `(x, y)` of a canvas of `cols` columns has.
fn id_at(cols: u32, (x, y): (u32, u32)) -> String {
    format!("square:{},{}", y, cols - 1 - x)
}

fn click(h: &mut crate::ui_editor_drag::H, at: egui::Pos2) {
    h.hover_at(at);
    h.run_steps(2);
    for pressed in [true, false] {
        h.event(egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        });
        h.run_steps(2);
    }
    h.run_steps(4);
}

#[test]
fn a_canvas_with_its_columns_and_rows_swapped_is_placed_by_the_cells_x_and_y() {
    for (cols, rows) in [(9, 5), (5, 9)] {
        let (mut h, script) = open(turned(cols, rows, "trail"));
        for at in [
            (0, 0),
            (cols - 1, 0),
            (0, rows - 1),
            (cols - 1, rows - 1),
            (2, 3),
        ] {
            let place = square(&h, at.0, at.1);
            click(&mut h, place);
            assert_eq!(
                painted(&script).last(),
                Some(&id_at(cols, at)),
                "the square at {at:?} of a {cols}x{rows} canvas"
            );
        }
    }
}

#[test]
fn a_span_over_a_turned_canvas_runs_the_press_and_the_release_square_between_begin_and_end() {
    let (mut h, script) = open(turned(9, 5, "span"));
    let (from, to) = (square(&h, 1, 4), square(&h, 7, 0));
    h.hover_at(from);
    h.run_steps(2);
    let event = |pos, pressed| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    h.event(event(from, true));
    h.run_steps(2);
    h.hover_at(to);
    h.run_steps(2);
    h.event(event(to, false));
    h.run_steps(4);
    assert_eq!(
        sent(&script),
        [
            "editor:stroke_begin",
            "editor:paint",
            "editor:paint",
            "editor:stroke_end"
        ]
    );
    assert_eq!(painted(&script), [id_at(9, (1, 4)), id_at(9, (7, 0))]);
}
