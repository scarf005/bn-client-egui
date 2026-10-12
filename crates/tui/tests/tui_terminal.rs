//! The computer terminal and the extended description in the terminal client.
mod common;
use bn_client_core::transport::ScriptHandle;
use bn_client_core::{EngineModel, GameModel};
use bn_client_tui::TuiApp;
use common::{fixture, session};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
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

fn click(app: &mut TuiApp<EngineModel>, column: u16, row: u16) {
    app.handle_mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column,
        row,
        modifiers: KeyModifiers::NONE,
    });
}

#[test]
fn clicking_a_terminal_answer_chooses_it_and_a_click_elsewhere_does_nothing() {
    let b = boundary("query");
    let no = b["interaction"]["choices"][1]["id"].clone();
    let (mut app, h) = open(b);
    let text = lines(&render(&mut app, 12));
    let at = text[11].find("No").unwrap() as u16;
    click(&mut app, at, 10);
    click(&mut app, 59, 11);
    assert!(
        operations(&h).is_empty(),
        "text and the gap after the answers are not answers"
    );
    click(&mut app, at, 11);
    assert_eq!(operations(&h), [json!({"kind":"choose","choice_id":no})]);
}

#[test]
fn clicking_a_target_of_the_extended_description_chooses_it() {
    let b = boundary("description");
    let furniture = b["interaction"]["choices"][1]["id"].clone();
    let (mut app, h) = open(b);
    let text = lines(&render(&mut app, 12));
    click(
        &mut app,
        text[11].find("Describe furniture").unwrap() as u16 + 2,
        11,
    );
    assert_eq!(
        operations(&h),
        [json!({"kind":"choose","choice_id":furniture})]
    );
}
