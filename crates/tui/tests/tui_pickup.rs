//! The native pickup window in the terminal client.
mod common;
use bn_client_core::transport::ScriptHandle;
use bn_client_core::{EngineModel, GameModel};
use bn_client_tui::TuiApp;
use common::{fixture, session};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::style::{Color, Modifier};
use serde_json::{Value, json};

/// The core pickup fixture with the input context's actions and a nested pair of stacks added.
fn pickup() -> Value {
    let text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../core/tests/fixtures/pickup.json"
    ))
    .unwrap();
    let mut b = serde_json::from_str::<Value>(&text).unwrap()["pickup"].clone();
    let key = |id: &str, name: &str, k: &str| json!({"id": id, "name": name, "keys": [k]});
    b["actions"] = json!([
        key("CONFIRM", "Confirm Choice", "RETURN"),
        key("WIELD", "Wield", "w"),
        key("LEFT", "Unmark", "LEFT"),
        key("RIGHT", "Mark", "RIGHT"),
        key("PREV_TAB", "Previous page", "<"),
        key("NEXT_TAB", "Next page", ">"),
        key("SELECT_ALL", "Select all", "A"),
        key("HELP_KEYBINDINGS", "Help", "?"),
    ]);
    let nesting = |v: &str| json!({"label": "Nesting", "value": v});
    let rows = b["interaction"]["choices"].as_array_mut().unwrap();
    rows[0]["columns"] = json!([nesting("parent with 1 child stacks")]);
    rows[0]["selected"] = json!(true);
    rows[0]["label"] = json!("backpack");
    rows.push(json!({
        "id": "pickup:c", "label": "<color_c_red>rock</color>", "enabled": true,
        "description": "A rock.", "denial": "", "selectable": true, "selected": false,
        "columns": [nesting("child of pickup:a")], "available_count": 1, "selected_count": 0
    }));
    b["interaction"]["choice_total"] = json!(3);
    b
}

fn open(boundary: Value) -> (TuiApp<EngineModel>, ScriptHandle) {
    let (t, h) = session();
    let t = (0..12).fold(t, |t, _| {
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

fn render(app: &mut TuiApp<EngineModel>) -> Terminal<TestBackend> {
    let mut t = Terminal::new(TestBackend::new(80, 30)).unwrap();
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

/// The engine ends the sent command, so the next key may send another.
fn finish(app: &mut TuiApp<EngineModel>, h: &ScriptHandle) {
    app.model.poll();
    h.push(fixture("command_completed"));
    app.model.poll();
}

fn press(app: &mut TuiApp<EngineModel>, code: KeyCode) {
    app.handle_key(KeyEvent::new(code, KeyModifiers::NONE));
}

#[test]
fn the_window_is_the_native_pickup_list_with_header_marks_hints_and_info() {
    let (mut app, _) = open(pickup());
    let t = render(&mut app);
    let dump = lines(&t);
    println!("{}", dump[..16].join("\n"));
    // The window is an overlay: only its own 44 columns are compared, the map panels stay behind it.
    let at = |y: usize| {
        dump[y]
            .chars()
            .take(44)
            .collect::<String>()
            .trim_end()
            .to_string()
    };
    assert_eq!(at(0), "PICK Wgt 0.2/45.0  Vol 0.30/0.00");
    assert_eq!(at(1), "a +   backpack");
    // The highlighted stack with a partial count is `#`, the child's backslash is lit by its marked parent.
    assert_eq!(at(2), "b #   tin can");
    assert_eq!(at(3), "c\\-   rock");
    assert_eq!(at(10), "[left] Unmark     [?] Help      [right] Mark");
    assert_eq!(at(11), "[<] Prev           [A] All          [>] Next");
    assert!(at(12).starts_with("┌< tin can >"));
    let buf = t.backend().buffer();
    assert_eq!(buf[(2, 1)].fg, Color::LightBlue, "+ is light blue");
    assert_eq!(
        buf[(1, 3)].fg,
        Color::Yellow,
        "child of a half-marked parent"
    );
    assert!(
        buf[(0, 2)].modifier.contains(Modifier::REVERSED),
        "cursor row"
    );
    assert!(!buf[(0, 1)].modifier.contains(Modifier::REVERSED));
}

#[test]
fn a_row_key_marks_its_row_and_a_bound_key_stays_the_engines() {
    let (mut app, h) = open(pickup());
    render(&mut app);
    // `a` is the first free key; `w` is bound to wield, so the list skips it.
    app.handle_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::NONE));
    assert_eq!(
        operations(&h),
        [json!({"kind":"choose","choice_id":"pickup:c"})]
    );
    finish(&mut app, &h);
    app.handle_key(KeyEvent::new(KeyCode::Char('w'), KeyModifiers::NONE));
    assert_eq!(
        operations(&h).last().unwrap(),
        &json!({"kind":"action","action_id":"WIELD"})
    );
}

#[test]
fn a_typed_count_marks_the_row_and_one_reaching_the_stack_toggles_it() {
    let (mut app, h) = open(pickup());
    render(&mut app);
    press(&mut app, KeyCode::Char('1'));
    assert!(lines(&render(&mut app))[0].contains("Count: 1_"));
    press(&mut app, KeyCode::Enter);
    assert_eq!(
        operations(&h),
        [json!({"kind":"set_count","choice_id":"pickup:b","count":1})]
    );
    finish(&mut app, &h);
    press(&mut app, KeyCode::Char('3'));
    press(&mut app, KeyCode::Enter);
    assert_eq!(
        operations(&h).last().unwrap(),
        &json!({"kind":"choose","choice_id":"pickup:b"})
    );
}

#[test]
fn escape_drops_a_typed_count_instead_of_cancelling() {
    let (mut app, h) = open(pickup());
    render(&mut app);
    press(&mut app, KeyCode::Char('2'));
    press(&mut app, KeyCode::Esc);
    assert!(operations(&h).is_empty());
    assert!(!lines(&render(&mut app))[0].contains("Count"));
}

#[test]
fn enter_confirms_through_the_engines_own_key() {
    let (mut app, h) = open(pickup());
    render(&mut app);
    press(&mut app, KeyCode::Enter);
    assert_eq!(
        operations(&h),
        [json!({"kind":"action","action_id":"CONFIRM"})]
    );
}
