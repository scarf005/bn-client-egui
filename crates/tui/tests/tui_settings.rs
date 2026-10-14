#![recursion_limit = "512"]
mod common;
use bn_client_core::transport::ScriptHandle;
use bn_client_core::{EngineModel, GameModel};
use bn_client_tui::TuiApp;
use common::{connected, fixture, session};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use serde_json::{Value, json};

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

fn text<M: GameModel>(app: &mut TuiApp<M>) -> String {
    let mut t = Terminal::new(TestBackend::new(100, 30)).unwrap();
    t.draw(|f| app.draw(f)).unwrap();
    let b = t.backend().buffer();
    b.content()
        .chunks(b.area.width as usize)
        .map(|row| row.iter().map(|c| c.symbol()).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n")
}

fn operations(h: &ScriptHandle) -> Vec<Value> {
    h.sent()
        .into_iter()
        .filter(|m| m["method"] == "bn.command.submit")
        .map(|m| m["params"]["operation"].clone())
        .collect()
}

/// A menu over `choices`; `field` is the screen's field.
fn open(field: Option<Value>, choices: Vec<Value>) -> (TuiApp<EngineModel>, ScriptHandle) {
    let confirm = json!([{"id":"CONFIRM","name":"Confirm","keys":["RETURN"]}]);
    open_with(field, choices, confirm)
}

fn open_with(
    field: Option<Value>,
    choices: Vec<Value>,
    actions: Value,
) -> (TuiApp<EngineModel>, ScriptHandle) {
    let (t, h) = session();
    let replies = (0..8)
        .map(|_| fixture("submit_result"))
        .fold(t, |t, r| t.on("bn.command.submit", vec![r]));
    let mut app = TuiApp::new(connected(replies));
    for name in ["events_41_walk", "events_42_message"] {
        h.push(fixture(name));
    }
    let mut menu = fixture("events_43_menu");
    let interaction = &mut menu["params"]["events"][0]["changes"]["interaction"]["interaction"];
    interaction["title"] = "Options".into();
    interaction["choices"] = json!(choices);
    if let Some(field) = field {
        interaction["field"] = field;
    }
    menu["params"]["events"][0]["changes"]["interaction"]["actions"] = actions;
    h.push(menu);
    app.model.poll();
    (app, h)
}

fn row(id: &str, label: &str) -> Value {
    json!({"id":id,"label":label,"description":"","denial":"","enabled":true,
        "selectable":true,"selected":false,"columns":[]})
}

fn fill(id: &str, value: &str, submit: bool) -> Value {
    json!({"kind":"fill","field_id":id,"value":value,"submit":submit})
}

fn key_field() -> Value {
    json!({"id":"field:key","label":"New key","description":"","value":"","type":"key",
        "max_length":-1,"printable":false})
}

#[test]
fn a_key_prompt_fills_with_the_engines_name_of_the_next_key() {
    for (event, name) in [
        (key(KeyCode::Char('z')), "z"),
        (key(KeyCode::Char('=')), "="),
        (key(KeyCode::F(9)), "F9"),
        (
            KeyEvent::new(KeyCode::Char('a'), KeyModifiers::CONTROL),
            "CTRL+A",
        ),
        (key(KeyCode::PageUp), "PPAGE"),
        (key(KeyCode::Char(' ')), "SPACE"),
    ] {
        let (mut app, h) = open(Some(key_field()), vec![]);
        assert!(text(&mut app).contains("Press the new key"));
        app.handle_key(event);
        assert_eq!(operations(&h), [fill("field:key", name, true)]);
    }
}

#[test]
fn esc_cancels_a_key_prompt_instead_of_binding_it() {
    let (mut app, h) = open(Some(key_field()), vec![]);
    app.handle_key(key(KeyCode::Esc));
    assert_eq!(operations(&h), [json!({"kind":"cancel"})]);
}

#[test]
fn a_text_prompt_is_typed_and_enter_submits_it() {
    let field = json!({"id":"field:value","label":"Rule","description":"","value":"*","type":"text",
        "max_length":-1,"printable":false});
    let (mut app, h) = open(Some(field), vec![]);
    app.handle_key(key(KeyCode::Char('k')));
    assert!(text(&mut app).contains("Rule: *k_"));
    app.handle_key(key(KeyCode::Enter));
    assert_eq!(operations(&h), [fill("field:value", "*k", true)]);
}

#[test]
fn a_prompt_submitted_while_the_engine_is_busy_is_sent_once_it_is_free() {
    let field = json!({"id":"field:value","label":"Name","description":"","value":"","type":"text",
        "max_length":-1,"printable":false});
    let (mut app, h) = open(Some(field), vec![]);
    assert!(
        app.model.invoke_action("CONFIRM"),
        "puts the engine to work"
    );
    app.handle_key(key(KeyCode::Char('k')));
    app.handle_key(key(KeyCode::Enter));
    assert_eq!(
        operations(&h),
        [json!({"kind":"action","action_id":"CONFIRM"})]
    );
    assert!(
        text(&mut app).contains("Name: k_"),
        "the typed name stays shown"
    );
    h.push(json!({"jsonrpc":"2.0","method":"bn.command","params":{"epoch":"e7f3","command_id":"c:5","stage":"completed"}}));
    app.model.poll();
    app.tick();
    assert_eq!(operations(&h).last(), Some(&fill("field:value", "k", true)));
    app.tick();
    assert_eq!(operations(&h).len(), 2, "the submit is sent once");
}

#[test]
fn tab_goes_to_the_engine_when_the_screen_binds_it() {
    let next_tab = json!([{"id":"NEXT_TAB","name":"Next tab","keys":["TAB"]}]);
    let rows = vec![row("mode:add_local", "Add local")];
    let (mut app, h) = open_with(None, rows, next_tab);
    app.handle_key(key(KeyCode::Tab));
    assert_eq!(
        operations(&h),
        [json!({"kind":"action","action_id":"NEXT_TAB"})]
    );
    let before = text(&mut app);
    app.handle_key(key(KeyCode::F(2)));
    assert_ne!(
        before,
        text(&mut app),
        "F2 still opens the client's action list"
    );
}

#[test]
fn a_filter_beside_a_list_updates_the_screen_without_ending_it() {
    let field = json!({"id":"field:value","label":"Filter","description":"","value":"","type":"text",
        "max_length":-1,"printable":false});
    let (mut app, h) = open(Some(field), vec![row("action:UP", "Move North")]);
    app.handle_key(key(KeyCode::Char('/')));
    app.handle_key(key(KeyCode::Char('m')));
    assert_eq!(operations(&h), [fill("field:value", "m", false)]);
}

#[test]
fn a_filter_typed_while_the_engine_is_busy_is_sent_in_full_once_it_is_free() {
    let field = json!({"id":"field:value","label":"Filter","description":"","value":"","type":"text",
        "max_length":-1,"printable":false});
    let (mut app, h) = open(Some(field), vec![row("action:UP", "Move North")]);
    for c in ['/', 'm', 'o', 'd', 'e'] {
        app.handle_key(key(KeyCode::Char(c)));
    }
    assert_eq!(operations(&h), [fill("field:value", "m", false)]);
    h.push(json!({"jsonrpc":"2.0","method":"bn.command","params":{"epoch":"e7f3","command_id":"c:5","stage":"completed"}}));
    app.model.poll();
    app.tick();
    assert_eq!(
        operations(&h),
        [
            fill("field:value", "m", false),
            fill("field:value", "mode", false)
        ]
    );
    app.tick();
    assert_eq!(operations(&h).len(), 2, "the same text is not sent twice");
}

#[test]
fn a_plain_row_is_confirmed_by_the_engines_key_and_tab_opens_the_screens_actions() {
    let (mut app, h) = open(None, vec![row("mode:add_local", "Add local")]);
    let before = text(&mut app);
    app.handle_key(key(KeyCode::Tab));
    assert_ne!(
        before,
        text(&mut app),
        "the action list opens over the menu"
    );
    app.handle_key(key(KeyCode::Esc));
    assert_eq!(
        before,
        text(&mut app),
        "Esc closes the list and keeps the menu"
    );
    app.handle_key(key(KeyCode::Enter));
    assert_eq!(
        operations(&h),
        [json!({"kind":"action","action_id":"CONFIRM"})],
        "a plain row is confirmed by the engine's own confirm key"
    );
}

#[test]
fn typing_a_filter_keeps_the_edit_when_the_engine_answers_with_fewer_rows() {
    let field = json!({"id":"field:value","label":"Filter","description":"","value":"","type":"text",
        "max_length":-1,"printable":false});
    let (mut app, h) = open(
        Some(field.clone()),
        vec![
            row("action:UP", "Move North"),
            row("action:DOWN", "Move South"),
        ],
    );
    app.handle_key(key(KeyCode::Char('/')));
    app.handle_key(key(KeyCode::Char('n')));
    // The engine filtered the list to one row; the next key still belongs to the filter.
    let mut menu = fixture("events_43_menu");
    let interaction = &mut menu["params"]["events"][0]["changes"]["interaction"]["interaction"];
    interaction["title"] = "Options".into();
    interaction["choices"] = json!([row("action:UP", "Move North")]);
    interaction["field"] = field;
    menu["params"]["events"][0]["sequence"] = "41".into();
    menu["params"]["events"][0]["revision"] = "40".into();
    h.push(json!({"jsonrpc":"2.0","method":"bn.command","params":{"epoch":"e7f3","command_id":"c:5","stage":"completed"}}));
    h.push(menu);
    app.model.poll();
    app.handle_key(key(KeyCode::Char('o')));
    let sent = operations(&h);
    assert_eq!(
        sent.last().unwrap(),
        &fill("field:value", "no", false),
        "{sent:?}"
    );
}

#[test]
fn a_typed_hotkey_chooses_its_row_but_not_a_disabled_one() {
    let mut rows = vec![row("first", "First row"), row("second", "Second row")];
    rows[0]["hotkey"] = "4".into();
    rows[1]["hotkey"] = "b".into();
    rows[1]["enabled"] = false.into();
    let (mut app, h) = open(None, rows);
    app.handle_key(key(KeyCode::Char('b')));
    assert!(operations(&h).is_empty(), "disabled row");
    app.handle_key(key(KeyCode::Char('4')));
    assert_eq!(
        operations(&h),
        [json!({"kind":"choose","choice_id":"first"})]
    );
}
