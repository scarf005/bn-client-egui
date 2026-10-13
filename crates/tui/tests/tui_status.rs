//! The engine's sidebar in the terminal client: it follows the events pushed while a command runs.
mod common;
use bn_client_core::{EngineModel, GameModel};
use bn_client_tui::TuiApp;
use common::session;
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use serde_json::{Value, json};

fn avatar() -> Value {
    let text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../core/tests/fixtures/0.2.examples/world.json"
    ))
    .unwrap();
    let all: Value = serde_json::from_str(&text).unwrap();
    let example = all["valid"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["name"] == "avatar with sidebar")
        .unwrap();
    example["value"].clone()
}

fn push(model: &mut EngineModel, h: &bn_client_core::transport::ScriptHandle, n: u32, a: Value) {
    h.push(
        json!({"jsonrpc":"2.0","method":"bn.events","params":{"epoch":"e7f3","events":[{
        "sequence": (40 + n).to_string(), "revision": (31 + n).to_string(), "type": "cells.seen",
        "changes": {"avatar": a}}]}}),
    );
    model.poll();
}

fn screen(app: &mut TuiApp<EngineModel>, height: u16) -> String {
    let mut t = Terminal::new(TestBackend::new(120, height)).unwrap();
    t.draw(|f| app.draw(f)).unwrap();
    let b = t.backend().buffer();
    b.content()
        .chunks(b.area.width as usize)
        .map(|row| row.iter().map(|c| c.symbol()).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn the_status_panel_shows_the_clock_of_each_event_pushed_mid_command() {
    let (t, h) = session();
    let mut model = EngineModel::new(Box::new(t));
    model.poll();
    let mut app = TuiApp::new(model);
    assert!(
        !screen(&mut app, 40).contains("Status"),
        "no avatar, no status"
    );
    for (n, clock) in ["Around noon", "Mid-afternoon", "Dusk"]
        .into_iter()
        .enumerate()
    {
        let mut a = avatar();
        a["sidebar"]["time"]["clock"] = json!(clock);
        push(&mut app.model, &h, n as u32 + 1, a);
        let shown = screen(&mut app, 40);
        assert!(shown.contains("Status"), "{shown}");
        assert!(shown.contains(&format!("Spring, Day 1 {clock}")), "{shown}");
        assert!(shown.contains("HEAD |||||"), "{shown}");
    }
}

#[test]
fn a_short_terminal_keeps_the_actions_and_items_when_the_status_is_drawn() {
    let (t, h) = session();
    let mut model = EngineModel::new(Box::new(t));
    model.poll();
    push(&mut model, &h, 1, avatar());
    let mut app = TuiApp::new(model);
    let shown = screen(&mut app, 24);
    assert!(
        shown.contains("Actions") && shown.contains("Ground"),
        "{shown}"
    );
}
