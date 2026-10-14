#![recursion_limit = "512"]
//! Every dialog is one kind of window: dragged by its title bar, resized from its edges, collapsed and
//! expanded from the title bar, closed where the engine offers cancel, and remembered per kind.
mod common;
use bn_client_egui::transport::ScriptHandle;
use bn_client_egui::{App, EngineModel};
use common::{fixture, session};
use eframe::egui;
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use serde_json::{Value, json};

type H<'a> = Harness<'a, App<EngineModel>>;

/// A client whose settings are kept in `settings`, showing a menu of `can_cancel` and 30 rows.
fn menu_window<'a>(settings: &std::path::Path, can_cancel: bool) -> (H<'a>, ScriptHandle) {
    let (t, script) = session();
    let app = App::new(EngineModel::new(Box::new(t))).with_settings(settings.into());
    let mut h = Harness::builder()
        .with_size(egui::vec2(1100.0, 800.0))
        .build_ui_state(|ui, app: &mut App<EngineModel>| app.show(ui), app);
    h.run_steps(3);
    for name in ["events_41_walk", "events_42_message"] {
        script.push(fixture(name));
    }
    let mut menu = fixture("events_43_menu");
    let change = &mut menu["params"]["events"][0]["changes"]["interaction"];
    change["interaction"]["allow_cancel"] = can_cancel.into();
    change["interaction"]["context"] = "UILIST".into();
    change["interaction"]["choices"] = (0..30)
        .map(|i| {
            json!({"id":format!("c:{i}"),"label":format!("entry {i}"),"description":"","denial":"",
            "enabled":true,"selectable":true,"selected":i == 0,"columns":[]})
        })
        .collect::<Value>();
    script.push(menu);
    h.run_steps(5);
    (h, script)
}

fn settings(tag: &str) -> std::path::PathBuf {
    common::game_root(tag).join("settings.json")
}

fn press(h: &mut H, pos: egui::Pos2, pressed: bool) {
    h.event(egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    });
    h.run_steps(1);
}

fn click(h: &mut H, pos: egui::Pos2) {
    h.hover_at(pos);
    h.run_steps(2);
    press(h, pos, true);
    press(h, pos, false);
    h.run_steps(2);
}

fn drag(h: &mut H, from: egui::Pos2, to: egui::Pos2) {
    h.hover_at(from);
    h.run_steps(2);
    press(h, from, true);
    h.hover_at((from + to.to_vec2()) / 2.0);
    h.run_steps(1);
    h.hover_at(to);
    h.run_steps(2);
    press(h, to, false);
    h.run_steps(3);
}

/// The title bar the window offers, which egui lays over the window's top edge.
fn title_bar(h: &H) -> egui::Rect {
    let bar = h
        .get_all_by_label("Pick up")
        .find(|n| n.accesskit_node().role() == egui::accesskit::Role::TitleBar);
    bar.unwrap().rect()
}

/// The window as a whole.
fn window(h: &H) -> egui::Rect {
    let dialog = h
        .get_all_by_label("Pick up")
        .find(|n| n.accesskit_node().role() == egui::accesskit::Role::Dialog);
    dialog.unwrap().rect()
}

#[test]
fn a_window_is_moved_by_its_title_bar_and_found_there_after_a_restart() {
    let file = settings("windows-move");
    let (mut h, _) = menu_window(&file, true);
    let title = title_bar(&h);
    let before = window(&h);
    drag(
        &mut h,
        title.center(),
        title.center() + egui::vec2(-80.0, 90.0),
    );
    let moved = window(&h);
    assert!(
        (moved.min - before.min - egui::vec2(-80.0, 90.0)).length() < 2.0,
        "{before:?} -> {moved:?}"
    );
    drop(h);
    let (h, _) = menu_window(&file, true);
    assert!(
        (window(&h).min - moved.min).length() < 2.0,
        "{:?} vs {moved:?}",
        window(&h)
    );
}

#[test]
fn a_window_is_collapsed_and_expanded_from_its_title_bar_and_stays_collapsed_after_a_restart() {
    let file = settings("windows-collapse");
    let (mut h, _) = menu_window(&file, true);
    let arrow = egui::pos2(window(&h).left() + 14.0, title_bar(&h).center().y);
    click(&mut h, arrow);
    h.run_steps(3);
    assert!(h.query_by_label("entry 0").is_none(), "collapsed");
    drop(h);
    let (mut h, _) = menu_window(&file, true);
    assert!(h.query_by_label("entry 0").is_none(), "still collapsed");
    let arrow = egui::pos2(window(&h).left() + 14.0, title_bar(&h).center().y);
    click(&mut h, arrow);
    h.run_steps(3);
    assert!(h.query_by_label("entry 0").is_some(), "expanded");
}

#[test]
fn a_window_is_resized_from_its_edge_and_keeps_its_size_after_a_restart() {
    let file = settings("windows-resize");
    let (mut h, _) = menu_window(&file, true);
    let before = window(&h);
    // On the frame's bottom edge, halfway across.
    let edge = egui::pos2(before.center().x, before.bottom() + 2.0);
    drag(&mut h, edge, edge + egui::vec2(0.0, -100.0));
    let after = window(&h);
    assert!(
        after.height() < before.height() - 60.0,
        "{before:?} -> {after:?}"
    );
    drop(h);
    let (h, _) = menu_window(&file, true);
    assert!(
        (window(&h).height() - after.height()).abs() < 4.0,
        "{after:?} vs {:?}",
        window(&h)
    );
}

#[test]
fn a_window_resized_from_its_left_edge_keeps_its_right_edge() {
    let (t, _) = session();
    let mut h = Harness::builder()
        .with_size(egui::vec2(1100.0, 700.0))
        .build_ui_state(
            |ui, app: &mut App<EngineModel>| app.show(ui),
            App::new(EngineModel::new(Box::new(t))),
        );
    h.run_steps(3);
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::P);
    h.run_steps(5);
    let dialog = |h: &H| {
        let found = h
            .get_all_by_label("Actions")
            .find(|n| n.accesskit_node().role() == egui::accesskit::Role::Dialog);
        found.unwrap().rect()
    };
    let before = dialog(&h);
    let edge = egui::pos2(before.left() - 2.0, before.center().y);
    drag(&mut h, edge, edge + egui::vec2(60.0, 0.0));
    let after = dialog(&h);
    assert!(
        after.width() < before.width() - 30.0,
        "{before:?} -> {after:?}"
    );
    assert!(
        (after.right() - before.right()).abs() < 2.0,
        "{before:?} -> {after:?}"
    );
}

#[test]
fn a_window_the_engine_lets_cancel_has_a_close_button_that_cancels() {
    let (mut h, script) = menu_window(&settings("windows-close"), true);
    // The close button ends the title bar, inside the frame.
    let at = egui::pos2(window(&h).right() - 14.0, title_bar(&h).center().y);
    click(&mut h, at);
    h.run_steps(3);
    let cancels = script
        .sent()
        .iter()
        .filter(|m| m["params"]["operation"]["kind"] == "cancel")
        .count();
    assert_eq!(cancels, 1);
}

#[test]
fn a_window_the_engine_does_not_let_cancel_has_no_close_button() {
    let (mut h, script) = menu_window(&settings("windows-no-close"), false);
    let at = egui::pos2(window(&h).right() - 14.0, title_bar(&h).center().y);
    click(&mut h, at);
    assert!(
        script
            .sent()
            .iter()
            .all(|m| m["params"]["operation"]["kind"] != "cancel")
    );
}

#[test]
fn a_menu_window_is_narrowed_by_dragging_its_left_edge() {
    let (mut h, _) = menu_window(&settings("windows-menu-narrow"), true);
    let before = window(&h);
    let edge = egui::pos2(before.left() - 2.0, before.center().y);
    drag(&mut h, edge, edge + egui::vec2(150.0, 0.0));
    let after = window(&h);
    assert!(
        after.width() < before.width() - 100.0,
        "{before:?} -> {after:?}"
    );
    assert!(
        (after.right() - before.right()).abs() < 2.0,
        "{before:?} -> {after:?}"
    );
}
