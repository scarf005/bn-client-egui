#![recursion_limit = "512"]
mod common;
use bn_client_egui::transport::ScriptHandle;
use bn_client_egui::{App, EngineModel};
use common::{fixture, session};
use eframe::egui;
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use serde_json::json;

type H<'a> = Harness<'a, App<EngineModel>>;

fn harness<'a>(size: egui::Vec2) -> (H<'a>, ScriptHandle) {
    let (t, h) = session();
    let t = t.on("bn.command.submit", vec![fixture("submit_result")]);
    let harness = Harness::builder().with_size(size).build_ui_state(
        |ui, app: &mut App<EngineModel>| app.show(ui),
        App::new(EngineModel::new(Box::new(t))),
    );
    (harness, h)
}

fn operations(script: &ScriptHandle) -> Vec<serde_json::Value> {
    script
        .sent()
        .into_iter()
        .filter(|m| m["method"] == "bn.command.submit")
        .map(|m| m["params"]["operation"].clone())
        .collect()
}

fn key(h: &mut H, key: egui::Key, modifiers: egui::Modifiers) {
    h.event(egui::Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers,
    });
    h.run_steps(3);
}

fn open_palette(h: &mut H) {
    key(h, egui::Key::P, egui::Modifiers::COMMAND);
}

#[test]
fn the_map_gets_most_of_the_window_and_the_sidebar_sits_beside_it() {
    for size in [egui::vec2(1100.0, 800.0), egui::vec2(1600.0, 1000.0)] {
        let (mut h, _script) = harness(size);
        h.run_steps(5);
        let map = h.get_by_label("Map").rect();
        let ground = h.get_by_label("Ground").rect();
        assert!(map.max.x <= ground.min.x, "sidebar is beside the map");
        let sidebar_width = size.x - ground.min.x;
        assert!(
            (size.x * 0.25..=size.x * 0.4).contains(&sidebar_width),
            "sidebar {sidebar_width} of {size:?}"
        );
        assert!(map.min.x < 20.0, "map starts at the left edge: {map:?}");
        assert!(
            map.min.y < 100.0,
            "map starts under the bar and display controls: {map:?}"
        );
    }
}

#[test]
fn the_message_log_is_in_the_sidebar_below_the_item_lists() {
    let (mut h, _script) = harness(egui::vec2(1100.0, 800.0));
    h.run_steps(5);
    let log = h.get_by_label("Message log").rect();
    let inventory = h.get_by_label("Inventory").rect();
    let map = h.get_by_label("Map").rect();
    assert!(log.min.y > inventory.max.y, "log below the lists");
    assert!(log.min.x >= map.max.x, "log shares the sidebar column");
    assert!(log.min.y > 800.0 * 0.5, "log gets the lower part");
}

#[test]
fn no_action_buttons_are_listed_until_the_palette_opens() {
    let (mut h, _script) = harness(egui::vec2(1100.0, 800.0));
    h.run_steps(3);
    assert!(h.query_by_label("Move right (RIGHT, l)").is_none());
    open_palette(&mut h);
    h.get_by_label("Move right (RIGHT, l)");
}

#[test]
fn the_actions_button_opens_the_palette() {
    let (mut h, _script) = harness(egui::vec2(1100.0, 800.0));
    h.run_steps(3);
    h.get_by_label("Actions").click();
    h.run_steps(3);
    h.get_by_label("Search actions");
    h.get_by_label("Move left (LEFT, h)");
}

#[test]
fn palette_search_narrows_the_list_and_enter_runs_the_first_match() {
    let (mut h, script) = harness(egui::vec2(1100.0, 800.0));
    h.run_steps(3);
    open_palette(&mut h);
    h.get_by_label("Search actions").type_text("left");
    h.run_steps(3);
    assert!(h.query_by_label("Move right (RIGHT, l)").is_none());
    h.get_by_label("Move left (LEFT, h)");
    key(&mut h, egui::Key::Enter, egui::Modifiers::NONE);
    assert_eq!(
        operations(&script),
        [json!({"kind":"action","action_id":"LEFT"})]
    );
    assert!(
        h.query_by_label("Search actions").is_none(),
        "closes after running"
    );
}

#[test]
fn typing_in_the_palette_search_is_not_a_game_key() {
    let (mut h, script) = harness(egui::vec2(1100.0, 800.0));
    h.run_steps(3);
    open_palette(&mut h);
    h.get_by_label("Search actions").type_text("k");
    h.run_steps(3);
    assert!(operations(&script).is_empty(), "k stays in the search box");
}

#[test]
fn escape_closes_the_palette_without_running_anything() {
    let (mut h, script) = harness(egui::vec2(1100.0, 800.0));
    h.run_steps(3);
    open_palette(&mut h);
    key(&mut h, egui::Key::Escape, egui::Modifiers::NONE);
    assert!(h.query_by_label("Search actions").is_none());
    assert!(operations(&script).is_empty());
}

#[test]
fn a_search_matching_nothing_runs_nothing() {
    let (mut h, script) = harness(egui::vec2(1100.0, 800.0));
    h.run_steps(3);
    open_palette(&mut h);
    h.get_by_label("Search actions").type_text("zzzz");
    h.run_steps(3);
    key(&mut h, egui::Key::Enter, egui::Modifiers::NONE);
    assert!(operations(&script).is_empty());
    h.get_by_label("Search actions");
}

#[test]
fn the_palette_search_keeps_focus_for_the_accessibility_tree() {
    let (mut h, _script) = harness(egui::vec2(1100.0, 800.0));
    h.run_steps(3);
    open_palette(&mut h);
    assert!(
        h.get_by_label("Search actions")
            .accesskit_node()
            .is_focused()
    );
}

#[test]
fn a_small_window_still_gives_the_map_most_of_the_width() {
    let (mut h, _script) = harness(egui::vec2(700.0, 450.0));
    h.run_steps(5);
    let map = h.get_by_label("Map").rect();
    let ground = h.get_by_label("Ground").rect();
    assert!(map.width() >= 700.0 * 0.5, "map {map:?}");
    assert!(map.max.x <= ground.min.x, "sidebar beside the map");
}

#[test]
fn sidebar_width_does_not_change_with_its_content() {
    let (mut h, script) = harness(egui::vec2(1100.0, 800.0));
    h.run_steps(5);
    let before = h.get_by_label("Ground").rect().min.x;
    for name in ["events_41_walk", "events_42_message"] {
        script.push(fixture(name));
    }
    h.run_steps(5);
    assert_eq!(h.get_by_label("Ground").rect().min.x, before);
}

#[test]
fn the_viewport_is_declared_once_while_the_window_keeps_its_size() {
    let (mut h, script) = harness(egui::vec2(1100.0, 800.0));
    h.run_steps(5);
    std::thread::sleep(std::time::Duration::from_millis(300));
    h.run_steps(20);
    let sent = script
        .sent_methods()
        .into_iter()
        .filter(|m| m == "bn.viewport")
        .count();
    assert_eq!(sent, 1);
}

#[test]
fn every_dialog_opens_at_the_same_place() {
    let (mut h, _script) = harness(egui::vec2(1100.0, 800.0));
    h.run_steps(3);
    open_palette(&mut h);
    let palette = h.get_by_label("Search actions").rect();
    assert!(palette.min.y > 800.0 * 0.12, "12% down: {palette:?}");
    assert!(
        (palette.center().x - 550.0).abs() < 1.0,
        "centered: {palette:?}"
    );
}

#[test]
fn the_message_log_keeps_two_fifths_of_the_sidebar_height_when_the_status_is_tall() {
    let (mut h, script) = harness(egui::vec2(1100.0, 800.0));
    h.run_steps(3);
    for name in ["events_41_walk", "events_42_message"] {
        script.push(fixture(name));
    }
    h.run_steps(8);
    let log = h.get_by_label("Message log").rect();
    assert!(800.0 - log.min.y >= 800.0 * 0.35, "{log:?}");
}

#[test]
fn the_resizable_log_keeps_its_default_share_instead_of_shrinking_to_its_content() {
    let (mut h, _script) = harness(egui::vec2(1100.0, 800.0));
    h.run_steps(10);
    let log = h.get_by_label("Message log").rect();
    assert!(800.0 - log.min.y >= 800.0 * 0.35, "{log:?}");
}

#[test]
fn dragging_the_log_edge_resizes_it_and_the_size_sticks() {
    let (mut h, _script) = harness(egui::vec2(1100.0, 800.0));
    h.run_steps(5);
    let heading = h.get_by_label("Message log").rect();
    let before = heading.min.y;
    let edge = h.get_by_label("Resize message log").rect().center();
    h.hover_at(edge);
    h.run_steps(2);
    h.drag_at(edge);
    h.run_steps(2);
    for dy in [-40.0, -80.0, -120.0] {
        h.hover_at(edge + egui::vec2(0.0, dy));
        h.run_steps(2);
    }
    h.drop_at(edge + egui::vec2(0.0, -120.0));
    h.run_steps(5);
    let after = h.get_by_label("Message log").rect().min.y;
    assert!(after < before - 60.0, "{before} -> {after}");
}

#[test]
fn the_sidebar_is_wide_enough_for_the_longest_status_line_on_one_line() {
    let (mut h, _script) = harness(egui::vec2(1280.0, 800.0));
    h.run_steps(5);
    let ground = h.get_by_label("Ground").rect();
    // The text starts inside the panel margin.
    assert!(1280.0 - ground.min.x >= 410.0, "{ground:?}");
}
