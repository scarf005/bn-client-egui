//! The screens every uilist, query popup and text field shares, in the terminal client, from the engine's generated
//! `generic.json` examples: the list filter, the prompt that sends each key and shows the rows left, switches.
mod common;
use bn_client_core::transport::{ScriptHandle, ScriptedTransport};
use bn_client_core::{ConnectOptions, EngineModel, GameModel};
use bn_client_tui::TuiApp;
use common::fixture;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use serde_json::{Value, json};

fn example(name: &str) -> Value {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../core/tests/fixtures/0.2.examples/generic.json"
    );
    let all: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    all["valid"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["name"] == name)
        .unwrap_or_else(|| panic!("no example {name}"))["value"]
        .clone()
}

fn open(screen: Value) -> (TuiApp<EngineModel>, ScriptHandle) {
    let mut subscribe = fixture("subscribe_result");
    subscribe["result"]["interaction"]["stack"] = json!([screen]);
    let (t, h) = ScriptedTransport::new();
    let t = t
        .on("bn.hello", vec![fixture("hello_result")])
        .on(
            "bn.subscribe",
            vec![
                subscribe,
                fixture("snapshot_part_0"),
                fixture("snapshot_part_1"),
            ],
        )
        .on("bn.command.submit", vec![fixture("submit_result")]);
    let mut model = EngineModel::open(Box::new(t), ConnectOptions::default());
    model.poll();
    (TuiApp::new(model), h)
}

fn text(app: &mut TuiApp<EngineModel>) -> String {
    let mut t = Terminal::new(TestBackend::new(100, 30)).unwrap();
    t.draw(|f| app.draw(f)).unwrap();
    let b = t.backend().buffer();
    b.content()
        .chunks(b.area.width as usize)
        .map(|row| row.iter().map(|c| c.symbol()).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n")
}

fn press(app: &mut TuiApp<EngineModel>, code: KeyCode) {
    app.handle_key(KeyEvent::new(code, KeyModifiers::NONE));
}

fn operations(h: &ScriptHandle) -> Vec<Value> {
    h.sent()
        .into_iter()
        .filter(|m| m["method"] == "bn.command.submit")
        .map(|m| m["params"]["operation"].clone())
        .collect()
}

/// The engine takes the `n`th command sent (from 1) and finishes it.
fn finish(h: &ScriptHandle, n: usize, command: &str) {
    let id = h
        .sent()
        .into_iter()
        .filter(|m| m["method"] == "bn.command.submit")
        .nth(n - 1)
        .map(|m| m["id"].as_u64().unwrap())
        .expect("the command was sent");
    h.push(json!({"jsonrpc":"2.0","id":id,"result":
        {"epoch":"e7f3","command_id":command,"stage":"received"}}));
    h.push(json!({"jsonrpc":"2.0","method":"bn.command","params":
        {"epoch":"e7f3","command_id":command,"stage":"completed"}}));
}

#[test]
fn a_filterable_list_shows_its_filter_and_the_key_runs_it() {
    let (mut app, h) = open(example("filterable list screen"));
    assert!(text(&mut app).contains("Filter [/]"));
    press(&mut app, KeyCode::Char('/'));
    assert_eq!(
        operations(&h),
        [json!({"kind": "invoke", "affordance_id": "FILTER", "args": {}})]
    );
}

#[test]
fn a_list_that_is_not_filterable_offers_no_filter() {
    let (mut app, _) = open(example("query popup screen"));
    assert!(!text(&mut app).contains("Filter"));
}

#[test]
fn up_then_enter_on_a_query_popup_chooses_the_option_the_focus_moved_to() {
    let (mut app, h) = open(example("query popup screen"));
    press(&mut app, KeyCode::Up);
    assert!(operations(&h).is_empty(), "moving the focus sends nothing");
    press(&mut app, KeyCode::Enter);
    assert_eq!(
        operations(&h),
        [
            json!({"kind": "invoke", "affordance_id": "bn:choose:list:choices",
            "args": {"row": "option:YES"}})
        ]
    );
}

#[test]
fn the_prompt_sends_each_key_shows_what_is_typed_and_enter_submits() {
    let (mut app, h) = open(example("filter prompt after typing"));
    let shown = text(&mut app);
    assert!(
        shown.contains("Teleport…"),
        "the rows the text leaves: {shown}"
    );
    assert!(shown.contains("tele_"), "the text with its cursor: {shown}");
    press(&mut app, KeyCode::Char('s'));
    app.tick();
    let _ = text(&mut app);
    assert!(text(&mut app).contains("teles_"));
    assert_eq!(
        operations(&h),
        [
            json!({"kind": "invoke", "affordance_id": "bn:fill:form:field", "args": {"text": "teles"}})
        ]
    );
    // The engine is busy with it: the next key waits, and the newest text goes out once it is free.
    press(&mut app, KeyCode::Backspace);
    press(&mut app, KeyCode::Backspace);
    let _ = text(&mut app);
    assert_eq!(operations(&h).len(), 1);
    finish(&h, 1, "c:5");
    app.model.poll();
    let _ = text(&mut app);
    assert_eq!(operations(&h).last().unwrap()["args"]["text"], "tel");
    finish(&h, 2, "c:6");
    app.model.poll();
    press(&mut app, KeyCode::Enter);
    let _ = text(&mut app);
    assert_eq!(
        operations(&h).last().unwrap(),
        &json!({"kind": "invoke", "affordance_id": "bn:submit:form:field", "args": {"text": "tel"}})
    );
}

#[test]
fn escape_in_the_prompt_is_the_screens_own_key() {
    let (mut app, h) = open(example("filter prompt before typing"));
    press(&mut app, KeyCode::Esc);
    assert_eq!(
        operations(&h),
        [json!({"kind": "invoke", "affordance_id": "bn:cancel", "args": {}})]
    );
}

#[test]
fn a_switch_row_shows_whether_it_is_on() {
    let (mut app, _) = open(example("list of switches screen"));
    let shown = text(&mut app);
    assert!(
        shown.contains("[ ] s Toggle display local scents"),
        "{shown}"
    );
    assert!(
        shown.contains("    g Check game state"),
        "a row that is no switch keeps the column: {shown}"
    );
}

#[test]
fn a_key_prompt_submits_the_next_key_by_its_engine_name_and_leaves_escape_to_the_screen() {
    let mut screen = example("filter prompt before typing");
    let form = screen["root"]["children"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|c| c["kind"] == "form")
        .unwrap();
    form["fields"][0]["type"] = json!("key");
    let (mut app, h) = open(screen.clone());
    press(&mut app, KeyCode::Char('z'));
    assert_eq!(
        operations(&h),
        [json!({"kind": "invoke", "affordance_id": "bn:submit:form:field", "args": {"text": "z"}})]
    );
    let (mut app, h) = open(screen);
    press(&mut app, KeyCode::Esc);
    assert_eq!(
        operations(&h),
        [json!({"kind": "invoke", "affordance_id": "bn:cancel", "args": {}})]
    );
}
