//! The terminal sidebar follows the game's panel options like the graphical one.
mod common;
use bn_client_core::{EngineModel, GameModel};
use bn_client_tui::TuiApp;
use common::session;
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use serde_json::{Value, json};

fn example(name: &str) -> Value {
    let text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../core/tests/fixtures/0.2.examples/world.json"
    ))
    .unwrap();
    let all: Value = serde_json::from_str(&text).unwrap();
    let mut found = all["valid"].as_array().unwrap().iter();
    found.find(|e| e["name"] == name).unwrap()["value"].clone()
}

fn panel(id: &str, enabled: bool) -> Value {
    json!({"id": id, "label": format!("<{id}>"), "enabled": enabled})
}

/// The screen rows of a 120x60 terminal whose avatar has a sidebar in `side` of `width` columns.
fn screen(side: &str, width: u32, panels: &[Value]) -> Vec<String> {
    said(side, width, panels, None)
}

/// As `screen`, after the client said `message`.
fn said(side: &str, width: u32, panels: &[Value], message: Option<&str>) -> Vec<String> {
    let (t, h) = session();
    let mut model = EngineModel::new(Box::new(t));
    model.poll();
    let mut avatar = example("avatar with creatures on the compass");
    avatar["sidebar"]["layout"] =
        json!({"id": "labels", "side": side, "width": width, "panels": panels});
    h.push(
        json!({"jsonrpc":"2.0","method":"bn.events","params":{"epoch":"e7f3","events":[{
        "sequence": "41", "revision": "32", "type": "cells.seen", "changes": {"avatar": avatar}}]}}),
    );
    model.poll();
    if let Some(message) = message {
        model.say(message.into());
    }
    let mut app = TuiApp::new(model);
    let mut t = Terminal::new(TestBackend::new(120, 60)).unwrap();
    t.draw(|f| app.draw(f)).unwrap();
    let b = t.backend().buffer();
    b.content()
        .chunks(b.area.width as usize)
        .map(|row| row.iter().map(|c| c.symbol()).collect::<String>())
        .collect()
}

fn column_of(rows: &[String], text: &str) -> usize {
    rows.iter()
        .find_map(|r| r.find(text).map(|byte| r[..byte].chars().count()))
        .unwrap_or_else(|| panic!("{text} not on screen:\n{}", rows.join("\n")))
}

fn row_of(rows: &[String], text: &str) -> usize {
    rows.iter().position(|r| r.contains(text)).unwrap()
}

fn all() -> Vec<Value> {
    ["Health", "Location", "Compass", "Log"]
        .map(|id| panel(id, true))
        .to_vec()
}

#[test]
fn the_sidebar_takes_the_side_and_the_columns_the_game_options_name() {
    let right = screen("right", 40, &all());
    assert_eq!(column_of(&right, "<Health>"), 120 - 40 + 1);
    let left = screen("left", 30, &all());
    assert_eq!(column_of(&left, "<Health>"), 1);
}

#[test]
fn the_panels_come_in_the_games_order_under_its_labels_and_a_disabled_one_is_missing() {
    let mut panels = all();
    let rows = screen("right", 40, &panels);
    assert!(row_of(&rows, "<Health>") < row_of(&rows, "<Location>"));
    assert!(row_of(&rows, "<Location>") < row_of(&rows, "<Compass>"));
    panels[1] = panel("Location", false);
    let rows = screen("right", 40, &panels);
    assert!(!rows.iter().any(|r| r.contains("<Location>")));
}

#[test]
fn the_compass_lists_each_direction_and_its_creatures_nearest_first() {
    let rows = screen("right", 40, &all());
    let at = |t: &str| row_of(&rows, t);
    assert!(at("East:") > at("<Compass>"));
    assert!(at("Z zombie 4") > at("East:"));
    assert!(at("@ Rae 9") > at("Z zombie 4"));
    assert!(at("b bat 1") > at("center"));
}

#[test]
fn the_message_log_follows_its_panel() {
    assert!(
        screen("right", 40, &all())
            .iter()
            .any(|r| r.contains("Message log"))
    );
    let off = ["Health", "Location", "Compass"].map(|id| panel(id, true));
    assert!(
        !screen("right", 40, &off)
            .iter()
            .any(|r| r.contains("Message log"))
    );
}

#[test]
fn the_clients_own_results_show_in_the_status_line_when_the_log_panel_is_off() {
    let off = ["Health", "Location", "Compass"].map(|id| panel(id, true));
    let rows = said("right", 40, &off, Some("Nothing to pick up."));
    assert!(
        rows.iter()
            .any(|r| r.contains("Ready | Nothing to pick up."))
    );
}
