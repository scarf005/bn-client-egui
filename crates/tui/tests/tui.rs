mod common;
use bn_client_core::transport::ScriptHandle;
use bn_client_core::{EngineModel, FakeModel, GameModel, Pos, Rgb};
use bn_client_tui::TuiApp;
use common::{connected, fixture, session};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::style::Color;
use serde_json::{Value, json};

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

fn ch(c: char) -> KeyEvent {
    key(KeyCode::Char(c))
}

fn render<M: GameModel>(app: &mut TuiApp<M>) -> Terminal<TestBackend> {
    let mut t = Terminal::new(TestBackend::new(100, 30)).unwrap();
    t.draw(|f| app.draw(f)).unwrap();
    t
}

fn text(t: &Terminal<TestBackend>) -> String {
    let b = t.backend().buffer();
    b.content()
        .chunks(b.area.width as usize)
        .map(|row| row.iter().map(|c| c.symbol()).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n")
}

fn submits(h: &ScriptHandle) -> Vec<Value> {
    h.sent()
        .into_iter()
        .filter(|m| m["method"] == "bn.command.submit")
        .collect()
}

fn engine() -> (TuiApp<EngineModel>, ScriptHandle) {
    let (t, h) = session();
    let t = t
        .on("bn.command.submit", vec![fixture("submit_result")])
        .on("bn.command.submit", vec![fixture("submit_result")]);
    (TuiApp::new(connected(t)), h)
}

fn open_menu(app: &mut TuiApp<EngineModel>, h: &ScriptHandle) {
    for name in ["events_41_walk", "events_42_message", "events_43_menu"] {
        h.push(fixture(name));
    }
    app.model.poll();
}

#[test]
fn fake_model_renders_map_player_items_and_log() {
    let mut app = TuiApp::new(FakeModel::default());
    let t = render(&mut app);
    let s = text(&t);
    assert!(s.contains('@') && s.contains("Rusty knife") && s.contains("Flashlight"));
    assert!(s.contains("Welcome to the prototype."));
}

#[test]
fn engine_map_uses_glyphs_and_game_colors_and_lists_actions_with_keys() {
    let (mut app, _h) = engine();
    let t = render(&mut app);
    let s = text(&t);
    assert!(s.contains('Z') && s.contains('#'));
    assert!(s.contains("Move right [RIGHT, l]"), "{s}");
    let zombie = t
        .backend()
        .buffer()
        .content()
        .iter()
        .find(|c| c.symbol() == "Z")
        .unwrap();
    assert_eq!(
        zombie.fg,
        Color::Rgb(110, 240, 110),
        "light_green from the default palette"
    );
}

#[test]
fn menu_overlay_shows_title_message_choices_and_denial() {
    let (mut app, h) = engine();
    open_menu(&mut app, &h);
    let s = text(&render(&mut app));
    for want in [
        "Pick up",
        "Choose an item",
        "combat knife",
        "tin can",
        "Too heavy",
    ] {
        assert!(s.contains(want), "{want} in:\n{s}");
    }
}

#[test]
fn enter_chooses_the_highlighted_choice_and_disabled_rows_are_inert() {
    let (mut app, h) = engine();
    open_menu(&mut app, &h);
    app.handle_key(key(KeyCode::Down));
    assert_eq!(app.selected_label().as_deref(), Some("tin can"));
    app.handle_key(key(KeyCode::Enter));
    assert!(submits(&h).is_empty(), "disabled choice");
    app.handle_key(key(KeyCode::Up));
    app.handle_key(key(KeyCode::Enter));
    assert_eq!(
        submits(&h)[0]["params"]["operation"],
        json!({"kind":"choose","choice_id":"c:1"})
    );
}

#[test]
fn selection_stops_at_both_ends() {
    let (mut app, h) = engine();
    open_menu(&mut app, &h);
    app.handle_key(key(KeyCode::Up));
    assert_eq!(app.selected_label().as_deref(), Some("combat knife"));
    for _ in 0..5 {
        app.handle_key(key(KeyCode::Down));
    }
    assert_eq!(app.selected_label().as_deref(), Some("tin can"));
}

#[test]
fn escape_cancels_the_menu() {
    let (mut app, h) = engine();
    open_menu(&mut app, &h);
    app.handle_key(key(KeyCode::Esc));
    assert_eq!(
        submits(&h)[0]["params"]["operation"],
        json!({"kind":"cancel"})
    );
}

#[test]
fn game_keys_run_actions_but_not_while_a_menu_is_open() {
    let (mut app, h) = engine();
    app.handle_key(ch('k'));
    assert_eq!(
        submits(&h)[0]["params"]["operation"],
        json!({"kind":"action","action_id":"UP"})
    );
    let (mut app, h) = engine();
    open_menu(&mut app, &h);
    app.handle_key(ch('k'));
    assert!(submits(&h).is_empty(), "menu swallows game keys");
}

#[test]
fn arrow_keys_are_the_game_direction_keys() {
    let (mut app, h) = engine();
    app.handle_key(key(KeyCode::Right));
    assert_eq!(
        submits(&h)[0]["params"]["operation"],
        json!({"kind":"action","action_id":"RIGHT"})
    );
}

#[test]
fn arrows_walk_the_fake_model() {
    let mut app = TuiApp::new(FakeModel::default());
    let before = app.model.player();
    app.handle_key(key(KeyCode::Right));
    assert_eq!(
        app.model.player(),
        Pos {
            x: before.x + 1,
            ..before
        }
    );
}

#[test]
fn ctrl_c_quits_and_plain_keys_do_not() {
    let mut app = TuiApp::new(FakeModel::default());
    app.handle_key(ch('c'));
    assert!(!app.quit());
    app.handle_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
    assert!(app.quit());
}

#[test]
fn cells_without_a_background_use_the_default_one() {
    let _ = Rgb(0, 0, 0);
    let (mut app, _h) = engine();
    let t = render(&mut app);
    let wall = t
        .backend()
        .buffer()
        .content()
        .iter()
        .find(|c| c.symbol() == "#")
        .unwrap();
    assert_eq!(wall.bg, Color::Reset);
}

fn loading(index: u32) -> Value {
    json!({"jsonrpc":"2.0","method":"bn.loading","params":{"epoch":"e7f3","title":"Loading world",
      "entries":["Loading items","Loading monsters","Finalizing"],"index":index,"image":{"path":"gfx/a.png"}}})
}

#[test]
fn loading_screen_shows_title_progress_bar_and_steps_instead_of_the_map() {
    let (mut app, h) = engine();
    h.push(loading(1));
    app.model.poll();
    let s = text(&render(&mut app));
    for want in [
        "Loading world",
        "2/3",
        "Loading items",
        "> Loading monsters",
        "Finalizing",
    ] {
        assert!(s.contains(want), "{want} in:\n{s}");
    }
    assert!(!s.contains("Message log"), "game UI hidden:\n{s}");
}

#[test]
fn loading_screen_goes_away_when_the_engine_is_done() {
    let (mut app, h) = engine();
    h.push(loading(0));
    h.push(json!({"jsonrpc":"2.0","method":"bn.loading","params":{"epoch":"e7f3","done":true}}));
    app.model.poll();
    let s = text(&render(&mut app));
    assert!(s.contains("Message log") && !s.contains("Loading world"));
}

#[test]
fn keys_do_not_reach_the_game_while_loading() {
    let (mut app, h) = engine();
    h.push(loading(0));
    app.model.poll();
    app.handle_key(ch('k'));
    assert!(submits(&h).is_empty());
}
