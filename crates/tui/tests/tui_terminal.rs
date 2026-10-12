//! The computer terminal and the extended description in the terminal client.
mod common;
use bn_client_core::transport::ScriptHandle;
use bn_client_core::{EngineModel, GameModel};
use bn_client_tui::TuiApp;
use common::{fixture, session};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::style::Color;
use serde_json::{Value, json};

fn boundary(name: &str) -> Value {
    let text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../core/tests/fixtures/terminal.json"
    ))
    .unwrap();
    serde_json::from_str::<Value>(&text).unwrap()[name].clone()
}

fn open(boundary: Value) -> (TuiApp<EngineModel>, ScriptHandle) {
    let (t, h) = session();
    let t = (0..4).fold(t, |t, _| {
        t.on("bn.command.submit", vec![fixture("submit_result")])
    });
    let mut model = EngineModel::new(Box::new(t));
    model.poll();
    h.push(
        json!({"jsonrpc":"2.0","method":"bn.events","params":{"epoch":"e7f3","events":[{
        "sequence": "41", "revision": "32", "type": "interaction.changed",
        "changes": {"interaction": boundary}}]}}),
    );
    model.poll();
    (TuiApp::new(model), h)
}

fn render(app: &mut TuiApp<EngineModel>, height: u16) -> Terminal<TestBackend> {
    let mut t = Terminal::new(TestBackend::new(60, height)).unwrap();
    t.draw(|f| app.draw(f)).unwrap();
    t
}

fn lines(t: &Terminal<TestBackend>) -> Vec<String> {
    let b = t.backend().buffer();
    b.content()
        .chunks(b.area.width as usize)
        .map(|row| row.iter().map(|c| c.symbol()).collect::<String>())
        .collect()
}

fn operations(h: &ScriptHandle) -> Vec<Value> {
    h.sent()
        .into_iter()
        .filter(|m| m["method"] == "bn.command.submit")
        .map(|m| m["params"]["operation"].clone())
        .collect()
}

fn press(app: &mut TuiApp<EngineModel>, code: KeyCode) {
    app.handle_key(KeyEvent::new(code, KeyModifiers::NONE));
}

#[test]
fn a_terminal_query_fills_the_screen_with_green_text_and_lists_its_answers() {
    let (mut app, _) = open(boundary("query"));
    let t = render(&mut app, 12);
    let text = lines(&t);
    assert!(text[0].contains("Locked Terminal"));
    assert!(text[1].contains("Logging into Locked Terminal"));
    assert!(text[11].contains("Yes") && text[11].contains("No") && text[11].contains("Quit"));
    assert_eq!(t.backend().buffer()[(1, 1)].fg, Color::Green);
    // The line the game colored red keeps its color.
    assert_eq!(t.backend().buffer()[(1, 2)].fg, Color::Rgb(170, 40, 40));
}

#[test]
fn a_terminal_query_keys_go_to_the_engine() {
    let (mut app, h) = open(boundary("query"));
    render(&mut app, 12);
    press(&mut app, KeyCode::Char('n'));
    assert_eq!(operations(&h), [json!({"kind":"action","action_id":"NO"})]);
}

#[test]
fn the_description_text_scrolls_with_the_arrow_keys_and_stops_at_its_end() {
    let mut b = boundary("description");
    let body = (1..=30).map(|n| format!("line {n}")).collect::<Vec<_>>();
    b["interaction"]["message"] = json!(body.join("\n"));
    let (mut app, h) = open(b);
    let text = lines(&render(&mut app, 12));
    assert!(text[1].contains("line 1"));
    press(&mut app, KeyCode::Down);
    press(&mut app, KeyCode::Down);
    let text = lines(&render(&mut app, 12));
    assert!(text[1].contains("line 3"), "{text:?}");
    for _ in 0..10 {
        press(&mut app, KeyCode::PageDown);
    }
    let text = lines(&render(&mut app, 12));
    assert!(text[9].contains("line 30"), "{text:?}");
    // Scrolling is the client's: nothing went to the engine.
    assert!(h.sent().iter().all(|m| m["method"] != "bn.command.submit"));
    assert!(text[11].contains("Describe terrain"));
}
