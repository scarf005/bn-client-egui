//! The software mini-games in the terminal client: the engine's window in its own colors.
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

fn game(body: &str) -> Value {
    json!({"schema_id":"s:snake","kind":"custom","context":"SNAKE","title":"S N A K E",
        "message":"","allow_cancel":false,"allow_set_count":false,"choices":[],"choice_total":0,
        "refresh_ms":100,
        "canvas":{"cols":20,"rows":[[["Score: 0",15,0]],[[body,15,0],["*",9,0]],[]]}})
}

fn push(h: &ScriptHandle, sequence: u32, interaction: Value) {
    let mut boundary = fixture("subscribe_result")["result"]["interaction"].clone();
    boundary["interaction"] = interaction;
    boundary["actions"] = json!([{"id":"UP","name":"Up","keys":["UP"]},
        {"id":"QUIT","name":"Quit","keys":["q"]}]);
    h.push(
        json!({"jsonrpc":"2.0","method":"bn.events","params":{"epoch":"e7f3","events":[{
        "sequence": sequence.to_string(), "revision": (sequence - 9).to_string(),
        "type": "interaction.changed", "changes": {"interaction": boundary}}]}}),
    );
}

fn open() -> (TuiApp<EngineModel>, ScriptHandle) {
    let (t, h) = session();
    let t = (0..2).fold(t, |t, _| {
        t.on("bn.command.submit", vec![fixture("submit_result")])
    });
    let mut model = EngineModel::new(Box::new(t));
    model.poll();
    push(&h, 41, game("##"));
    model.poll();
    (TuiApp::new(model), h)
}

fn render(app: &mut TuiApp<EngineModel>) -> Terminal<TestBackend> {
    let mut t = Terminal::new(TestBackend::new(40, 12)).unwrap();
    t.draw(|f| app.draw(f)).unwrap();
    t
}

fn text(t: &Terminal<TestBackend>) -> String {
    let b = t.backend().buffer();
    b.content()
        .chunks(b.area.width as usize)
        .map(|row| row.iter().map(|c| c.symbol()).collect::<String>() + "\n")
        .collect()
}

fn find(t: &Terminal<TestBackend>, glyph: &str) -> Option<(u16, u16)> {
    let b = t.backend().buffer();
    (0..b.area.height)
        .flat_map(|y| (0..b.area.width).map(move |x| (x, y)))
        .find(|&(x, y)| b[(x, y)].symbol() == glyph)
}

#[test]
fn a_mini_game_window_is_drawn_with_its_colors() {
    let (mut app, _) = open();
    let t = render(&mut app);
    let shown = text(&t);
    assert!(shown.contains("Score: 0"), "{shown}");
    assert!(shown.contains("##*"), "{shown}");
    let (x, y) = find(&t, "*").unwrap();
    assert_eq!(t.backend().buffer()[(x, y)].fg, Color::Rgb(240, 90, 90));
}

#[test]
fn a_mini_game_redraw_replaces_the_drawn_window() {
    let (mut app, h) = open();
    render(&mut app);
    push(&h, 42, game("###"));
    app.model.poll();
    let shown = text(&render(&mut app));
    assert!(shown.contains("###*"), "{shown}");
}

#[test]
fn a_mini_game_key_is_the_engines_action() {
    let (mut app, h) = open();
    render(&mut app);
    app.handle_key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE));
    let sent = h
        .sent()
        .into_iter()
        .filter(|m| m["method"] == "bn.command.submit")
        .map(|m| m["params"]["operation"].clone())
        .collect::<Vec<_>>();
    assert_eq!(sent, [json!({"kind":"action","action_id":"UP"})]);
}
