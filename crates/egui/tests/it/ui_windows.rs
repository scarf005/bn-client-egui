//! Every dialog is one kind of window: dragged by its title bar, resized from its edges, collapsed and
//! expanded from the title bar, closed where the engine offers cancel, and remembered per kind.
use crate::common::{fixture, session};
use bn_client_egui::transport::ScriptHandle;
use bn_client_egui::{App, EngineModel};
use eframe::egui;
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use serde_json::{Value, json};

type H<'a> = Harness<'a, App<EngineModel>>;

/// A client whose settings are kept in `settings`, showing a menu of `can_cancel` and 30 rows.
fn menu_window<'a>(settings: &std::path::Path, can_cancel: bool) -> (H<'a>, ScriptHandle) {
    menu_window_titled(settings, can_cancel, "Pick up")
}

/// `menu_window` with the menu titled `title`.
fn menu_window_titled<'a>(
    settings: &std::path::Path,
    can_cancel: bool,
    title: &str,
) -> (H<'a>, ScriptHandle) {
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
    change["interaction"]["title"] = title.into();
    change["stack"] = json!([
        {"screen_id": "screen:map", "kind": "map", "role": "map", "root": null, "affordances": []},
        {"screen_id": "screen:9", "kind": "uilist", "role": "dialog", "root": null, "affordances": []}]);
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
    crate::common::game_root(tag).join("settings.json")
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
    window_titled(h, "Pick up")
}

/// The window titled `title` as a whole.
fn window_titled(h: &H, title: &str) -> egui::Rect {
    let dialog = h
        .get_all_by_label(title)
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
    let minimize = h.get_by_label("Minimize").rect().center();
    click(&mut h, minimize);
    h.run_steps(3);
    assert!(h.query_by_label("entry 0").is_none(), "collapsed");
    drop(h);
    let (mut h, _) = menu_window(&file, true);
    assert!(h.query_by_label("entry 0").is_none(), "still collapsed");
    let expand = h.get_by_label("Expand").rect().center();
    click(&mut h, expand);
    h.run_steps(3);
    assert!(h.query_by_label("entry 0").is_some(), "expanded");
}

#[test]
fn a_window_is_resized_from_its_edge_and_keeps_its_size_after_a_restart() {
    let file = settings("windows-resize");
    let (mut h, _) = menu_window(&file, true);
    let before = window(&h);
    // On the window's bottom edge, halfway across.
    let edge = egui::pos2(before.center().x, before.bottom() - 2.0);
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
    let edge = egui::pos2(before.left() + 2.0, before.center().y);
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
    let edge = egui::pos2(before.left() + 2.0, before.center().y);
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

/// A client whose settings are kept in `settings`, in a game with the inventory's drop screen over the map.
fn inventory_window<'a>(settings: &std::path::Path) -> (H<'a>, ScriptHandle) {
    inventory_window_as(settings, |_| {})
}

/// `inventory_window` with the drop screen as `edit` leaves it.
fn inventory_window_as<'a>(
    settings: &std::path::Path,
    edit: impl FnOnce(&mut Value),
) -> (H<'a>, ScriptHandle) {
    let path = format!(
        "{}/../core/tests/fixtures/0.2.examples/inventory.json",
        env!("CARGO_MANIFEST_DIR")
    );
    let all = serde_json::from_str::<Value>(&std::fs::read_to_string(path).unwrap()).unwrap();
    let mut screen = all["valid"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["name"] == "drop selector screen")
        .unwrap()["value"]
        .clone();
    edit(&mut screen);
    let (t, script) = session();
    let app = App::new(EngineModel::new(Box::new(t))).with_settings(settings.into());
    let mut h = Harness::builder()
        .with_size(egui::vec2(1100.0, 800.0))
        .build_ui_state(|ui, app: &mut App<EngineModel>| app.show(ui), app);
    h.run_steps(3);
    script.push(fixture("events_41_walk"));
    h.run_steps(4);
    let map = json!({"screen_id": "screen:map", "role": "map", "root": null, "affordances": []});
    script.push(json!({"jsonrpc": "2.0", "method": "bn.events", "params": {"epoch": "e7f3", "events": [{
        "sequence": "42", "revision": "33", "type": "interaction.changed", "changes": {"interaction": {
        "boundary_id": "b:60",
        "readiness": {"phase": "playing", "game_ready": true, "accepts_interaction_commands": true,
            "accepts_registered_actions": true},
        "actions": [], "interaction": null, "stack": [map, screen]}}}]}}));
    h.run_steps(6);
    crate::common::settle_windows(&mut h);
    (h, script)
}

/// Drags the inventory window's bottom and right edges 100 points in, so that it can move inside its room.
fn shrink_inventory(h: &mut H) {
    let (window, _) = inventory_frame(h);
    let bottom = egui::pos2(window.center().x, window.bottom() - 2.0);
    drag(h, bottom, bottom - egui::vec2(0.0, 100.0));
    let (window, _) = inventory_frame(h);
    let right = egui::pos2(window.right() - 2.0, window.center().y);
    drag(h, right, right - egui::vec2(100.0, 0.0));
}

/// The inventory window, and a point on its title bar left of the close button.
fn inventory_frame(h: &H) -> (egui::Rect, egui::Pos2) {
    titled_frame(h, "Multidrop")
}

/// The window titled `title`, and a point on its title bar left of the close button.
fn titled_frame(h: &H, title: &str) -> (egui::Rect, egui::Pos2) {
    let window = h.get_by_label(title).rect();
    let close = h.get_by_label("Close window").rect();
    (window, egui::pos2(window.center().x, close.center().y))
}

#[test]
fn a_screen_window_closed_by_its_close_button_runs_the_screens_cancel() {
    let (mut h, script) = inventory_window(&settings("windows-screen-close"));
    let close = h.get_by_label("Close window").rect().center();
    click(&mut h, close);
    let invoked = script
        .sent()
        .into_iter()
        .filter(|m| m["method"] == "bn.command.submit")
        .map(|m| m["params"]["operation"]["affordance_id"].clone())
        .collect::<Vec<_>>();
    assert_eq!(invoked, [json!("QUIT")]);
}

#[test]
fn a_screen_window_is_moved_by_its_title_bar_and_found_there_after_a_restart() {
    let file = settings("windows-screen-move");
    let (mut h, _) = inventory_window(&file);
    shrink_inventory(&mut h);
    let (before, title) = inventory_frame(&h);
    let by = egui::vec2(60.0, 10.0);
    drag(&mut h, title, title + by);
    let (moved, _) = inventory_frame(&h);
    assert!(
        (moved.min - before.min - by).length() < 2.0,
        "{before:?} -> {moved:?}"
    );
    drop(h);
    let (h, _) = inventory_window(&file);
    let (again, _) = inventory_frame(&h);
    assert!(
        (again.min - moved.min).length() < 2.0,
        "{again:?} vs {moved:?}"
    );
}

#[test]
fn a_screen_window_kept_from_a_last_run_keeps_its_size_while_the_pointer_moves_over_it() {
    let file = settings("windows-screen-hover");
    drop(inventory_window(&file));
    let (mut h, _) = inventory_window(&file);
    let (before, _) = inventory_frame(&h);
    for i in 0..40 {
        let x = before.left() + before.width() * (i % 10) as f32 / 10.0;
        h.hover_at(egui::pos2(x, before.center().y));
        h.run_steps(1);
    }
    let (after, _) = inventory_frame(&h);
    assert!(
        (after.size() - before.size()).length() < 1.0,
        "{before:?} -> {after:?}"
    );
}

#[test]
fn a_screen_window_kept_wider_than_the_screen_opens_no_wider_than_the_screen() {
    let file = settings("windows-screen-too-wide");
    let (h, _) = inventory_window(&file);
    let (fresh, _) = inventory_frame(&h);
    drop(h);
    let kept = json!({"windows": {"screen:inventory": {
        "pos": [16.0, 39.0], "size": [3000.0, 745.0], "collapsed": false}}});
    std::fs::write(&file, kept.to_string()).unwrap();
    let (h, _) = inventory_window(&file);
    let (opened, _) = inventory_frame(&h);
    assert!(
        opened.width() <= 1100.0 && opened.width() > fresh.width(),
        "{fresh:?} vs {opened:?}"
    );
}

#[test]
fn a_screen_window_is_found_where_it_was_left_when_it_opens_under_another_title() {
    let file = settings("windows-screen-retitled");
    let (mut h, _) = inventory_window(&file);
    shrink_inventory(&mut h);
    let (before, title) = inventory_frame(&h);
    let by = egui::vec2(60.0, 10.0);
    drag(&mut h, title, title + by);
    let (moved, _) = inventory_frame(&h);
    assert!(
        (moved.min - before.min - by).length() < 2.0,
        "{before:?} -> {moved:?}"
    );
    drop(h);
    // The same screen in another language, or with a count in its title.
    let (h, _) = inventory_window_as(&file, |s| s["title"] = "Déposer (3)".into());
    let (again, _) = titled_frame(&h, "Déposer (3)");
    assert!(
        (again.min - moved.min).length() < 2.0,
        "{again:?} vs {moved:?}"
    );
    let kept = serde_json::from_str::<Value>(&std::fs::read_to_string(&file).unwrap()).unwrap();
    let kinds = kept["windows"]
        .as_object()
        .unwrap()
        .keys()
        .cloned()
        .collect::<Vec<_>>();
    assert_eq!(kinds, ["screen:inventory"]);
}

#[test]
fn a_screen_the_engine_does_not_name_leaves_nothing_to_find_after_a_restart() {
    let file = settings("windows-screen-unnamed");
    let unnamed = |s: &mut Value| drop(s.as_object_mut().unwrap().remove("kind"));
    let (mut h, _) = inventory_window_as(&file, unnamed);
    let (before, title) = inventory_frame(&h);
    drag(&mut h, title, title + egui::vec2(60.0, -10.0));
    drop(h);
    let (h, _) = inventory_window_as(&file, unnamed);
    let (again, _) = inventory_frame(&h);
    assert!(
        (again.min - before.min).length() < 2.0,
        "{again:?} vs {before:?}"
    );
    let kept =
        std::fs::read_to_string(&file).map_or(Value::Null, |t| serde_json::from_str(&t).unwrap());
    assert!(
        kept["windows"].as_object().is_none_or(|w| w.is_empty()),
        "{kept}"
    );
}

#[test]
fn a_menu_window_is_found_where_it_was_left_when_it_opens_under_another_title() {
    let file = settings("windows-menu-retitled");
    let (mut h, _) = menu_window(&file, true);
    let title = title_bar(&h);
    drag(
        &mut h,
        title.center(),
        title.center() + egui::vec2(-80.0, 90.0),
    );
    let moved = window(&h);
    drop(h);
    let (h, _) = menu_window_titled(&file, true, "Ramasser");
    let again = window_titled(&h, "Ramasser");
    assert!(
        (again.min - moved.min).length() < 2.0,
        "{again:?} vs {moved:?}"
    );
}

/// The one window shown, as a whole.
fn only_window(h: &H) -> egui::Rect {
    use egui::accesskit::Role;
    let mut windows = [Role::Window, Role::Dialog]
        .into_iter()
        .flat_map(|role| h.query_all_by_role(role));
    let found = windows.next().expect("a window");
    assert!(windows.next().is_none(), "more than one window");
    found.rect()
}

/// A client with the window of each kind the rules are held to, by name: an engine menu, an engine screen, the
/// action palette and the client's settings.
fn every_window_kind<'a>() -> Vec<(&'static str, H<'a>)> {
    let (menu, _) = menu_window(&settings("rules-menu"), true);
    let (screen, _) = inventory_window(&settings("rules-screen"));
    let (t, _) = session();
    let mut palette = Harness::builder()
        .with_size(egui::vec2(1100.0, 800.0))
        .build_ui_state(
            |ui, app: &mut App<EngineModel>| app.show(ui),
            App::new(EngineModel::new(Box::new(t))),
        );
    palette.run_steps(3);
    palette.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::P);
    palette.run_steps(5);
    let (t, _) = session();
    let mut options = Harness::builder()
        .with_size(egui::vec2(1100.0, 800.0))
        .build_ui_state(
            |ui, app: &mut App<EngineModel>| app.show(ui),
            App::new(EngineModel::new(Box::new(t))),
        );
    options.run_steps(3);
    crate::common::open_client_settings(&mut options);
    vec![
        ("menu", menu),
        ("screen", screen),
        ("palette", palette),
        ("client settings", options),
    ]
}

#[test]
fn a_window_is_the_size_it_was_whatever_its_content_does_for_every_kind_of_window() {
    for (kind, mut h) in every_window_kind() {
        let before = only_window(&h);
        for i in 0..40 {
            // The pointer crosses the window, hovering rows and buttons, and keys change what the content is.
            let at = before.min
                + egui::vec2(
                    before.width() * (i % 10) as f32 / 10.0,
                    20.0 + 15.0 * (i / 10) as f32 * 4.0,
                );
            h.hover_at(at);
            h.run_steps(1);
            if i % 7 == 0 {
                h.event(egui::Event::Text("e".into()));
            }
            assert_eq!(only_window(&h), before, "{kind} changed at frame {i}");
        }
    }
}

#[test]
fn a_menu_row_wider_than_the_window_does_not_widen_it() {
    let (mut h, script) = menu_window(&settings("rules-wide-row"), true);
    let before = window(&h);
    let mut menu = fixture("events_43_menu");
    let change = &mut menu["params"]["events"][0]["changes"]["interaction"];
    change["interaction"]["title"] = "Pick up".into();
    change["interaction"]["choices"] = json!([{"id": "c:0", "label": "wide ".repeat(300), "description": "",
        "denial": "", "enabled": true, "selectable": true, "selected": true, "columns": []}]);
    script.push(menu);
    h.run_steps(6);
    assert_eq!(window(&h), before);
}

/// The points on the border of `rect` that press its eight resize handles, with the edges each moves
/// (left, right, top, bottom).
fn borders(rect: egui::Rect) -> Vec<(egui::Pos2, [bool; 4])> {
    let (l, r, t, b) = (
        rect.left() + 3.0,
        rect.right() - 3.0,
        rect.top() + 3.0,
        rect.bottom() - 3.0,
    );
    let (cx, cy) = (rect.center().x, rect.center().y);
    vec![
        (egui::pos2(l, cy), [true, false, false, false]),
        (egui::pos2(r, cy), [false, true, false, false]),
        (egui::pos2(cx, t), [false, false, true, false]),
        (egui::pos2(cx, b), [false, false, false, true]),
        (egui::pos2(l, t), [true, false, true, false]),
        (egui::pos2(r, t), [false, true, true, false]),
        (egui::pos2(l, b), [true, false, false, true]),
        (egui::pos2(r, b), [false, true, false, true]),
    ]
}

#[test]
fn dragging_a_border_moves_only_that_edge_for_every_kind_of_window() {
    for (kind, mut h) in every_window_kind() {
        for index in 0..8 {
            let before = only_window(&h);
            let (from, [left, right, top, bottom]) = borders(before)[index];
            // Each edge is dragged inward, where the screen leaves room whatever the window's place.
            let by = egui::vec2(
                if left { 25.0 } else { -25.0 },
                if top { 20.0 } else { -20.0 },
            );
            drag(&mut h, from, from + by);
            let after = only_window(&h);
            let edges = [
                (left, before.left(), after.left()),
                (right, before.right(), after.right()),
                (top, before.top(), after.top()),
                (bottom, before.bottom(), after.bottom()),
            ];
            for (moves, was, now) in edges {
                match moves {
                    true => assert!(
                        (now - was).abs() > 5.0,
                        "{kind} {index}: {before:?} -> {after:?}"
                    ),
                    false => assert!(
                        (now - was).abs() < 0.6,
                        "{kind} {index}: {before:?} -> {after:?}"
                    ),
                }
            }
        }
    }
}

#[test]
fn pressing_a_border_without_dragging_changes_nothing_for_every_kind_of_window() {
    for (kind, mut h) in every_window_kind() {
        for index in 0..8 {
            let before = only_window(&h);
            let (at, _) = borders(before)[index];
            click(&mut h, at);
            assert_eq!(only_window(&h), before, "{kind} {index}");
        }
    }
}

#[test]
fn every_window_has_minimize_maximize_and_close_buttons_that_act() {
    for (kind, mut h) in every_window_kind() {
        let before = only_window(&h);
        let maximize = h.get_by_label("Maximize").rect().center();
        click(&mut h, maximize);
        assert_eq!(
            only_window(&h),
            egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1100.0, 800.0)),
            "{kind} maximized"
        );
        let restore = h.get_by_label("Restore").rect().center();
        click(&mut h, restore);
        assert_eq!(only_window(&h), before, "{kind} restored");
        h.get_by_label("Minimize");
        h.get_by_label("Close window");
    }
}
