#![recursion_limit = "512"]
mod common;
use bn_client_core::{EngineModel, GameModel};
use bn_client_core::transport::ScriptHandle;
use bn_client_tui::TuiApp;
use common::{connected, fixture, session};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use serde_json::{Value, json};

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

fn operations(h: &ScriptHandle) -> Vec<Value> {
    h.sent()
        .into_iter()
        .filter(|m| m["method"] == "bn.command.submit")
        .map(|m| m["params"]["operation"].clone())
        .collect()
}

/// The native Yes / No query: the engine binds Left, Right and Confirm and keeps its own cursor on Yes.
fn query() -> (TuiApp<EngineModel>, ScriptHandle) {
    let (t, h) = session();
    let replies = (0..4)
        .map(|_| fixture("submit_result"))
        .fold(t, |t, r| t.on("bn.command.submit", vec![r]));
    let mut app = TuiApp::new(connected(replies));
    for name in ["events_41_walk", "events_42_message"] {
        h.push(fixture(name));
    }
    let row = |id: &str, label: &str, on: bool| {
        json!({"id":id,"label":label,"description":"","denial":"","enabled":true,
            "selectable":true,"selected":on,"highlighted":on,"columns":[]})
    };
    let mut menu = fixture("events_43_menu");
    let changes = &mut menu["params"]["events"][0]["changes"]["interaction"];
    changes["actions"] = json!([{"id":"LEFT","name":"Left","keys":["LEFT"]},
        {"id":"RIGHT","name":"Right","keys":["RIGHT"]},
        {"id":"CONFIRM","name":"Confirm","keys":["RETURN"]}]);
    let i = &mut changes["interaction"];
    i["title"] = "".into();
    i["context"] = "POPUP_WAIT".into();
    i["message"] = "Are you SURE you're finished?".into();
    i["choices"] = json!([row("option:0", "Yes", true), row("option:1", "No", false)]);
    i["choice_total"] = 2.into();
    h.push(menu);
    app.model.poll();
    (app, h)
}

#[test]
fn down_then_enter_on_a_query_popup_chooses_the_option_the_cursor_moved_to() {
    let (mut app, h) = query();
    app.handle_key(key(KeyCode::Down));
    assert!(operations(&h).is_empty(), "the engine binds no Down");
    assert_eq!(app.selected_label().as_deref(), Some("No"));
    app.handle_key(key(KeyCode::Enter));
    assert_eq!(
        operations(&h),
        [json!({"kind":"choose","choice_id":"option:1"})]
    );
}

#[test]
fn up_returns_to_the_engines_cursor_and_enter_confirms_through_the_engine() {
    let (mut app, h) = query();
    app.handle_key(key(KeyCode::Down));
    app.handle_key(key(KeyCode::Up));
    assert_eq!(app.selected_label().as_deref(), Some("Yes"));
    app.handle_key(key(KeyCode::Enter));
    assert_eq!(operations(&h).len(), 1);
    assert_ne!(operations(&h)[0]["kind"], "choose", "{:?}", operations(&h));
}

#[test]
fn right_stays_the_engines_on_a_query_popup() {
    let (mut app, h) = query();
    app.handle_key(key(KeyCode::Right));
    assert_eq!(operations(&h).len(), 1, "the engine moves its own cursor");
}
