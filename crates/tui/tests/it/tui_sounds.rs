//! Sounds the game did not write to its message log, in the terminal client.
use crate::common::{fixture, session};
use bn_client_core::{Clock, EngineModel, GameModel};
use bn_client_tui::TuiApp;
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::style::Color;
use serde_json::json;
use std::time::Duration;

fn heard(sequence: u32, text: &str, logged: bool) -> serde_json::Value {
    json!({"jsonrpc": "2.0", "method": "bn.events", "params": {"epoch": "e7f3", "events": [{
        "sequence": sequence.to_string(), "revision": "32", "type": "sound.heard",
        "data": {"text": text, "volume": 52, "logged": logged}}]}})
}

fn screen(app: &mut TuiApp<EngineModel>) -> (String, Option<Color>) {
    let mut t = Terminal::new(TestBackend::new(120, 40)).unwrap();
    t.draw(|f| app.draw(f)).unwrap();
    let b = t.backend().buffer();
    let rows = b
        .content()
        .chunks(b.area.width as usize)
        .map(|row| row.iter().map(|c| c.symbol()).collect::<String>())
        .collect::<Vec<_>>();
    let color = b
        .content()
        .chunks(b.area.width as usize)
        .zip(&rows)
        .find(|(_, r)| r.contains("gargling"))
        .map(|(row, _)| row[1].fg);
    (rows.join("\n"), color)
}

#[test]
fn unlogged_sounds_are_grouped_fade_and_leave_after_their_lifetime() {
    let (t, h) = session();
    let clock = Clock::manual();
    let mut model = EngineModel::new(Box::new(t)).with_clock(clock.clone());
    model.poll();
    h.push(fixture("events_41_walk"));
    h.push(heard(42, "gargling grunting", false));
    h.push(heard(43, "gargling grunting", false));
    h.push(heard(44, "Zoe saying hello", true));
    model.poll();
    let mut app = TuiApp::new(model);
    let (fresh, bright) = screen(&mut app);
    assert!(fresh.contains("gargling grunting ×2"), "{fresh}");
    assert!(!fresh.contains("Zoe saying hello"), "{fresh}");
    clock.advance(Duration::from_secs(6));
    let (_, dimmer) = screen(&mut app);
    let level = |c: Option<Color>| match c {
        Some(Color::Rgb(r, ..)) => r,
        other => panic!("{other:?}"),
    };
    assert!(level(dimmer) < level(bright));
    clock.advance(Duration::from_secs(7));
    assert!(!screen(&mut app).0.contains("gargling"));
}
