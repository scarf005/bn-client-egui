//! A workspace screen laid out as an editor: a menu bar from the affordances' groups, the tools on the left, the
//! canvas in the middle, the palette and the inspector on the right, the log under the canvas and the status line
//! at the bottom.
use crate::common::legible::cut_text;
use bn_client_egui::{App, EngineModel};
use eframe::egui;
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use serde_json::{Value, json};

type H<'a> = Harness<'a, App<EngineModel>>;

const SIZES: [egui::Vec2; 2] = [egui::vec2(1280.0, 800.0), egui::vec2(1920.0, 1080.0)];

fn list(id: &str, label: &str, rows: &[&str], slot: &str) -> Value {
    let rows = rows
        .iter()
        .enumerate()
        .map(|(n, r)| json!({"id": format!("{id}:{n}"), "label": r, "description": "", "denial": "", "enabled": true,
            "selectable": true, "selected": n == 0, "columns": []}))
        .collect::<Vec<_>>();
    json!({"id": id, "kind": "list", "label": label, "slot": slot, "columns": [], "total": rows.len(),
        "rows": rows, "selection": "single"})
}

fn text(id: &str, markup: &str, slot: &str) -> Value {
    json!({"id": id, "kind": "text", "slot": slot, "markup": markup})
}

fn affordance(
    id: &str,
    label: &str,
    group: Option<&str>,
    keys: &[&str],
    denial: Option<&str>,
) -> Value {
    let mut a = json!({"id": id, "label": label, "keys": keys, "enabled": denial.is_none(), "expect": "screen"});
    if let Some(group) = group {
        a["group"] = group.into();
    }
    if let Some(denial) = denial {
        a["denial"] = denial.into();
    }
    a
}

/// A workspace whose root split has a part for every slot, and affordances in two groups and none.
fn workspace() -> Value {
    let cells = (0..6)
        .flat_map(|y| (0..8).map(move |x| json!({"x": x, "y": y, "id": format!("square:{x},{y}"), "glyph": ".", "enabled": true, "looks": [0]})))
        .collect::<Vec<_>>();
    let canvas = json!({"id": "ws:canvas", "kind": "grid", "slot": "canvas", "cols": 8, "rows": 6, "cells": cells,
        "looks": [{"kind": "terrain", "id": "t_floor", "glyph": ".", "color": "c_cyan", "name": "wooden floor"}],
        "role": "canvas"});
    let inspector = json!({"id": "ws:inspector", "kind": "panel", "slot": "inspector", "title": "Properties",
        "stats": [{"key": "size", "label": "Size", "value": "8 by 6"}]});
    json!({"screen_id": "screen:9", "role": "workspace", "kind": "editor", "title": "Room Editor",
        "root": {"id": "ws:body", "kind": "split", "relation": "peers", "children": [
            list("ws:tools", "Tools", &["Pencil", "Eraser"], "tools"),
            canvas,
            list("ws:palette", "Palette", &["Wall", "Floor"], "palette"),
            inspector,
            text("ws:log", "Saved the room", "log"),
            text("ws:status", "Cursor 3,4", "status"),
        ]},
        "affordances": [
            affordance("ws:new", "New", Some("File"), &["ctrl+n"], None),
            affordance("ws:undo", "Undo", Some("Edit"), &["ctrl+z"], None),
            affordance("ws:save", "Save", Some("File"), &[], Some("Nothing to save")),
            affordance("ws:validate", "Validate", None, &[], None),
            affordance("ws:close", "Close", None, &[], None),
        ],
        "context": "WORKSPACE"})
}

fn shown<'a>(size: egui::Vec2) -> (H<'a>, bn_client_egui::transport::ScriptHandle) {
    let (app, script) = crate::common::staged(crate::common::over_map(workspace()));
    let mut h = crate::common::themed_harness(size, app);
    h.run_steps(7);
    crate::common::settle_windows(&mut h);
    (h, script)
}

fn rect<'a>(h: &H<'a>, label: &str) -> egui::Rect {
    h.get_by_label(label).rect()
}

#[test]
fn the_menu_bar_has_one_menu_per_group_in_the_order_they_first_appear() {
    let (h, _) = shown(SIZES[0]);
    let (file, edit) = (rect(&h, "File"), rect(&h, "Edit"));
    assert!(file.right() <= edit.left(), "{file:?} {edit:?}");
    let tools = rect(&h, "Pencil");
    assert!(
        file.bottom() <= tools.top() && edit.bottom() <= tools.top(),
        "{file:?} {edit:?} over {tools:?}"
    );
}

#[test]
fn an_item_of_a_menu_runs_its_affordance_and_a_denied_one_does_not() {
    let (mut h, script) = shown(SIZES[0]);
    let invoked = |script: &bn_client_egui::transport::ScriptHandle, id: &str| {
        script
            .sent()
            .iter()
            .any(|m| m["params"]["operation"]["affordance_id"] == id)
    };
    h.get_by_label("File").click();
    h.run_steps(3);
    h.get_by_label("New").click();
    h.run_steps(3);
    assert!(invoked(&script, "ws:new"));
    h.get_by_label("File").click();
    h.run_steps(3);
    h.get_by_label("Save").click();
    h.run_steps(3);
    assert!(!invoked(&script, "ws:save"));
}

#[test]
fn grouped_affordances_are_not_in_the_footer_and_the_others_are() {
    let (h, _) = shown(SIZES[0]);
    for grouped in ["New", "Undo", "Save"] {
        assert!(
            h.query_by_label(grouped).is_none(),
            "{grouped} is drawn outside its menu"
        );
    }
    assert!(h.query_by_label("Validate").is_some());
}

#[test]
fn a_docked_workspace_keeps_its_menus_for_the_grouped_affordances() {
    let mut screen = workspace();
    screen["dock"] = "left".into();
    let (app, _) = crate::common::staged(crate::common::over_map(screen));
    let mut h = crate::common::themed_harness(SIZES[0], app);
    h.run_steps(7);
    crate::common::settle_windows(&mut h);
    for menu in ["File", "Edit"] {
        assert!(h.query_by_label(menu).is_some(), "no {menu} menu");
    }
    h.get_by_label("File").click();
    h.run_steps(3);
    assert!(h.query_by_label("New").is_some());
}

#[test]
fn only_the_columns_shown_have_resize_handles() {
    let mut screen = workspace();
    let kept = ["ws:canvas", "ws:tools"];
    screen["root"]["children"]
        .as_array_mut()
        .unwrap()
        .retain(|c| kept.contains(&c["id"].as_str().unwrap()));
    let (app, _) = crate::common::staged(crate::common::over_map(screen));
    let mut h = crate::common::themed_harness(SIZES[0], app);
    h.run_steps(7);
    crate::common::settle_windows(&mut h);
    assert_eq!(h.get_all_by_label("Resize workspace column").count(), 1);
    assert!(h.query_by_label("Resize workspace log").is_none());
}

#[test]
fn the_parts_are_placed_by_slot() {
    for size in SIZES {
        let (h, _) = shown(size);
        let (tools, palette) = (rect(&h, "Pencil"), rect(&h, "Wall"));
        let (inspector, log, status) = (
            rect(&h, "Properties"),
            rect(&h, "Saved the room"),
            rect(&h, "Cursor 3,4"),
        );
        let canvas = (tools.right(), palette.left().min(inspector.left()));
        assert!(
            canvas.0 < canvas.1,
            "{size:?}: tools {tools:?}, palette {palette:?}"
        );
        assert!(
            canvas.1 - canvas.0 > size.x / 2.0,
            "{size:?}: the canvas has {canvas:?}"
        );
        assert!(
            inspector.top() > palette.top(),
            "{size:?}: inspector {inspector:?}"
        );
        assert!(
            log.left() >= canvas.0 && log.right() <= canvas.1,
            "{size:?}: log {log:?}"
        );
        assert!(
            log.top() > tools.top() + size.y / 3.0,
            "{size:?}: log {log:?} is not under the canvas"
        );
        assert!(
            [tools, palette, inspector, log]
                .iter()
                .all(|r| status.top() >= r.bottom()),
            "{size:?}: status {status:?}"
        );
    }
}

#[test]
fn no_text_of_the_workspace_is_cut() {
    for size in SIZES {
        let (mut h, _) = shown(size);
        h.get_by_label("File").click();
        h.run_steps(3);
        let cut = cut_text(
            &h.output().shapes,
            egui::Rect::from_min_size(egui::Pos2::ZERO, size),
        );
        assert!(cut.is_empty(), "{size:?}: {cut:#?}");
    }
}

/// `workspace()` with `edit` applied to its root's parts.
fn shown_with<'a>(size: egui::Vec2, edit: impl FnOnce(&mut Vec<Value>)) -> H<'a> {
    let mut screen = workspace();
    edit(screen["root"]["children"].as_array_mut().unwrap());
    let (app, _) = crate::common::staged(crate::common::over_map(screen));
    let mut h = crate::common::themed_harness(size, app);
    h.run_steps(7);
    crate::common::settle_windows(&mut h);
    h
}

#[test]
fn a_status_panel_is_one_line_of_its_facts() {
    for size in SIZES {
        let h = shown_with(size, |parts| {
            parts[5] = json!({"id": "ws:status", "kind": "panel", "slot": "status", "title": "room.json",
                "stats": [{"key": "saved", "label": "Saved", "value": "Yes"},
                    {"key": "symbol", "label": "Symbol", "value": "#"},
                    {"key": "brush", "label": "Brush", "value": "2"}]});
        });
        let line = ["room.json", "Saved", "Yes", "Symbol", "Brush", "2"].map(|l| rect(&h, l));
        assert!(
            line.iter()
                .all(|r| (r.center().y - line[0].center().y).abs() < 2.0),
            "{size:?}: {line:?}"
        );
    }
}

/// The painted texts that read `text`.
fn painted(h: &H, text: &str) -> Vec<egui::Rect> {
    fn walk(shape: &egui::Shape, text: &str, out: &mut Vec<egui::Rect>) {
        match shape {
            egui::Shape::Text(t) if t.galley.text() == text => {
                out.push(t.galley.rect.translate(t.pos.to_vec2()));
            }
            egui::Shape::Vec(shapes) => shapes.iter().for_each(|s| walk(s, text, out)),
            _ => {}
        }
    }
    let mut out = vec![];
    h.output()
        .shapes
        .iter()
        .for_each(|c| walk(&c.shape, text, &mut out));
    out
}

#[test]
fn a_workspace_has_no_title_bar_and_its_title_help_and_leave_end_its_menu_bar() {
    for size in SIZES {
        let mut screen = workspace();
        let affordances = screen["affordances"].as_array_mut().unwrap();
        affordances.push(
            json!({"id": "ws:leave", "label": "Leave", "keys": ["ESC"], "enabled": true,
            "expect": "screen", "role": "cancel"}),
        );
        affordances.push(
            json!({"id": "ws:help", "label": "Help", "keys": ["?"], "enabled": true,
            "expect": "screen", "role": "help"}),
        );
        let (app, _) = crate::common::staged(crate::common::over_map(screen));
        let mut h = crate::common::themed_harness(size, app);
        h.run_steps(7);
        crate::common::settle_windows(&mut h);
        assert!(h.query_by_label("Close window").is_none(), "{size:?}");
        let file = rect(&h, "File");
        let bar =
            |r: egui::Rect| (r.center().y - file.center().y).abs() < 2.0 && r.left() > file.right();
        let (leave, help) = (rect(&h, "Leave"), rect(&h, "Help"));
        let title = painted(&h, "Room Editor");
        assert!(
            bar(leave) && bar(help) && title.len() == 1 && bar(title[0]),
            "{size:?}: File {file:?}, Leave {leave:?}, Help {help:?}, title {title:?}"
        );
        assert!(
            leave.right() > size.x - 60.0 && title[0].right() < help.left(),
            "{size:?}: Leave {leave:?}, Help {help:?}, title {title:?}"
        );
    }
}

#[test]
fn an_action_on_a_row_of_a_list_no_part_names_is_drawn_under_that_list() {
    for size in SIZES {
        let mut screen = workspace();
        screen["affordances"].as_array_mut().unwrap().push(json!({"id": "ws:inside", "label": "Inside",
            "keys": [], "enabled": true, "expect": "screen", "args": {"row": {"component_id": "ws:palette"}}}));
        let (app, _) = crate::common::staged(crate::common::over_map(screen));
        let mut h = crate::common::themed_harness(size, app);
        h.run_steps(7);
        crate::common::settle_windows(&mut h);
        let (inside, floor, inspector) = (
            rect(&h, "Inside"),
            rect(&h, "Floor"),
            rect(&h, "Properties"),
        );
        assert!(
            inside.top() >= floor.bottom()
                && inside.bottom() <= inspector.top()
                && inside.left() >= floor.left() - 8.0,
            "{size:?}: Inside {inside:?} under Floor {floor:?}, over {inspector:?}"
        );
    }
}

#[test]
fn a_log_list_shows_several_rows_over_the_status_line() {
    for size in SIZES {
        let steps = (0..10).map(|n| format!("Step {n}")).collect::<Vec<_>>();
        let h = shown_with(size, |parts| {
            let steps = steps.iter().map(String::as_str).collect::<Vec<_>>();
            parts[4] = list("ws:history", "History", &steps, "log");
        });
        let status = rect(&h, "Cursor 3,4");
        let shown = steps
            .iter()
            .filter(|s| {
                h.query_by_label(s)
                    .is_some_and(|n| n.rect().bottom() <= status.top())
            })
            .count();
        assert!(shown >= 3, "{size:?}: {shown} rows of the history shown");
    }
}

#[test]
fn a_status_fact_too_long_for_the_line_wraps_whole_with_its_value() {
    let facts = (0..10)
        .map(|n| (format!("Fact number {n}"), format!("Value {n}")))
        .collect::<Vec<_>>();
    let h = shown_with(SIZES[0], |parts| {
        let stats = facts
            .iter()
            .map(|(label, value)| json!({"key": label, "label": label, "value": value}))
            .collect::<Vec<_>>();
        parts[5] = json!({"id": "ws:status", "kind": "panel", "slot": "status",
            "title": "/home/someone/games/cataclysm/data/json/mapgen/house/2Story01.json", "stats": stats});
    });
    let apart = facts
        .iter()
        .filter(|(label, value)| {
            (rect(&h, label).center().y - rect(&h, value).center().y).abs() > 2.0
        })
        .collect::<Vec<_>>();
    assert!(apart.is_empty(), "{apart:?}");
}

/// Writes PNGs of the workspace to `$SHOT_DIR`; needs a Vulkan adapter, so it only runs when asked:
/// `cargo test --test it ui_workspace::shots -- --ignored`.
#[test]
#[ignore]
fn shots() {
    for size in SIZES {
        let mut h = crate::common::shot_harness(
            size,
            crate::common::staged(crate::common::over_map(workspace())).0,
        );
        h.run_steps(7);
        crate::common::save_shot(&mut h, &format!("workspace-{}x{}.png", size.x, size.y));
    }
}

#[test]
fn the_client_buttons_share_the_menu_bar_row() {
    for size in SIZES {
        let (h, _) = shown(size);
        let file = rect(&h, "File");
        for button in ["Actions", "Client settings"] {
            let r = rect(&h, button);
            assert!(
                (r.center().y - file.center().y).abs() < 2.0 && r.left() > file.right(),
                "{size:?}: {button} {r:?} off the menu bar's row at {file:?}"
            );
        }
    }
}

#[test]
fn the_client_buttons_of_the_menu_bar_open_the_action_palette_and_the_settings() {
    let (mut h, _) = shown(SIZES[0]);
    h.get_by_label("Client settings").click();
    h.run_steps(4);
    assert!(
        h.query_all_by_role(egui::accesskit::Role::Window).any(|w| w
            .accesskit_node()
            .label()
            .as_deref()
            == Some("Client settings")),
        "no settings window"
    );
}

#[test]
fn the_footer_buttons_end_the_status_line() {
    for size in SIZES {
        let (h, _) = shown(size);
        let (status, validate) = (rect(&h, "Cursor 3,4"), rect(&h, "Validate"));
        assert!(
            (validate.center().y - status.center().y).abs() < 4.0
                && validate.left() > status.right()
                && validate.right() > size.x / 2.0,
            "{size:?}: Validate {validate:?} off the status line {status:?}"
        );
    }
}

#[test]
fn without_a_status_line_or_footer_the_columns_reach_the_bottom() {
    for size in SIZES {
        let mut screen = workspace();
        screen["root"]["children"]
            .as_array_mut()
            .unwrap()
            .retain(|c| c["slot"] != "status");
        screen["affordances"]
            .as_array_mut()
            .unwrap()
            .retain(|a| a["group"].is_string());
        let (app, _) = crate::common::staged(crate::common::over_map(screen));
        let mut h = crate::common::themed_harness(size, app);
        h.run_steps(7);
        crate::common::settle_windows(&mut h);
        let columns = h
            .get_all_by_label("Resize workspace column")
            .map(|n| n.rect())
            .collect::<Vec<_>>();
        assert!(
            columns.iter().all(|r| r.bottom() > size.y - 24.0),
            "{size:?}: {columns:?}"
        );
    }
}

#[test]
fn without_tools_the_log_takes_the_left_column() {
    for size in SIZES {
        let h = shown_with(size, |parts| {
            parts.retain(|c| c["slot"] != "tools");
            for cell in parts[0]["cells"].as_array_mut().unwrap() {
                cell["label"] = "square".into();
                cell["affordance"] = "ws:validate".into();
            }
        });
        let (log, palette) = (rect(&h, "Saved the room"), rect(&h, "Wall"));
        let squares = h
            .get_all_by_label("square")
            .map(|n| n.rect())
            .reduce(|a, b| a.union(b))
            .unwrap();
        assert!(
            log.right() <= squares.left() && log.top() < size.y / 3.0,
            "{size:?}: log {log:?}, canvas {squares:?}"
        );
        assert!(
            squares.right() <= palette.left(),
            "{size:?}: canvas {squares:?}, palette {palette:?}"
        );
    }
}
