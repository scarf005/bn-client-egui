//! The NPC trade window and the per-choice native hotkeys in the terminal client.
mod common;
use bn_client_core::transport::ScriptHandle;
use bn_client_core::{EngineModel, GameModel};
use bn_client_tui::TuiApp;
use common::{fixture, session};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::style::{Color, Modifier};
use serde_json::{Value, json};

fn trade() -> Value {
    let text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../core/tests/fixtures/trade.json"
    ))
    .unwrap();
    serde_json::from_str::<Value>(&text).unwrap()["trade"].clone()
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
    let mut t = Terminal::new(TestBackend::new(120, 40)).unwrap();
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

fn find(t: &Terminal<TestBackend>, text: &str) -> (u16, u16) {
    lines(t)
        .iter()
        .enumerate()
        .find_map(|(y, l)| {
            l.find(text)
                .map(|at| (l[..at].chars().count() as u16, y as u16))
        })
        .unwrap_or_else(|| panic!("{text} not drawn:\n{}", lines(t).join("\n")))
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
fn both_parties_show_rows_prices_capacity_and_the_balance_line() {
    let (mut app, _) = open(trade());
    let t = render(&mut app);
    let all = lines(&t).join("\n");
    for text in [
        "Trading with Lena",
        "Credit $1.50 | Balance acceptable: yes",
        "Lena: NPC items",
        "You: Player items",
        "Free capacity: 12.00 kg, 8.00 L",
        "# tin can (3)",
        "- lighter",
        "TOOLS",
        "$0.50",
        "$1.00",
        "0.30",
        "2/3",
        "A battered tin can.",
        "Confirm trade",
        "Autobalance",
    ] {
        assert!(all.contains(text), "{text}:\n{all}");
    }
    let (npc, _) = find(&t, "tin can");
    let (you, _) = find(&t, "lighter");
    assert!(npc < 60 && you >= 60);
    // The cursor row (the engine's focus) is reversed.
    let (x, y) = find(&t, "# tin can");
    let cell = &t.backend().buffer()[(x, y)];
    assert!(cell.modifier.contains(Modifier::REVERSED));
}

#[test]
fn a_click_on_a_row_marks_it() {
    let (mut app, h) = open(trade());
    let t = render(&mut app);
    let (x, y) = find(&t, "- lighter");
    app.handle_mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: x,
        row: y,
        modifiers: KeyModifiers::NONE,
    });
    assert_eq!(
        operations(&h),
        [json!({"kind":"choose","choice_id":"trade:y1"})]
    );
}

#[test]
fn space_marks_the_row_under_the_cursor() {
    let (mut app, h) = open(trade());
    render(&mut app);
    press(&mut app, KeyCode::Char(' '));
    assert_eq!(
        operations(&h),
        [json!({"kind":"choose","choice_id":"trade:n1"})]
    );
}

#[test]
fn enter_confirms_through_the_engines_own_key() {
    let (mut app, h) = open(trade());
    render(&mut app);
    press(&mut app, KeyCode::Enter);
    assert_eq!(
        operations(&h),
        [json!({"kind":"action","action_id":"CONFIRM"})]
    );
}

#[test]
fn a_typed_count_is_sent_on_enter_and_one_beyond_the_stack_is_not() {
    let (mut app, h) = open(trade());
    render(&mut app);
    press(&mut app, KeyCode::Char('3'));
    assert!(lines(&render(&mut app)).join("\n").contains("Count: 3_"));
    press(&mut app, KeyCode::Enter);
    assert_eq!(
        operations(&h),
        [json!({"kind":"set_count","choice_id":"trade:n1","count":3})]
    );
    press(&mut app, KeyCode::Char('9'));
    press(&mut app, KeyCode::Enter);
    assert_eq!(operations(&h).len(), 1);
}

#[test]
fn escape_drops_a_typed_count_instead_of_cancelling_the_trade() {
    let (mut app, h) = open(trade());
    render(&mut app);
    press(&mut app, KeyCode::Char('2'));
    press(&mut app, KeyCode::Esc);
    assert!(operations(&h).is_empty());
    assert!(!lines(&render(&mut app)).join("\n").contains("Count:"));
}

#[test]
fn a_single_item_takes_no_count() {
    let mut boundary = trade();
    boundary["interaction"]["compat"]["focus"]["choice_id"] = "trade:y1".into();
    let (mut app, h) = open(boundary);
    render(&mut app);
    press(&mut app, KeyCode::Char('1'));
    assert!(!lines(&render(&mut app)).join("\n").contains("Count:"));
    assert!(operations(&h).is_empty());
}

#[test]
fn row_hotkeys_are_light_green_before_the_labels_with_blank_padding() {
    let mut boundary = trade();
    let i = &mut boundary["interaction"];
    i["context"] = "ITEM_LIST".into();
    i["compat"]["panes"] = json!([]);
    for c in i["choices"].as_array_mut().unwrap() {
        c["pane_id"] = Value::Null;
    }
    i["choices"][1]["hotkey"] = "b".into();
    let (mut app, _) = open(boundary);
    let t = render(&mut app);
    let (x, y) = find(&t, "b lighter");
    let buf = t.backend().buffer();
    assert_eq!(buf[(x, y)].fg, Color::LightGreen);
    // The row without a key is padded to the same column.
    let (tx, _) = find(&t, "[x] tin can");
    assert_eq!(tx, x + 2);
}

#[test]
fn trade_rows_show_the_engines_key_in_light_green_and_pad_rows_without_one() {
    let mut boundary = trade();
    boundary["interaction"]["choices"][1]["hotkey"] = "b".into();
    let (mut app, _) = open(boundary);
    let t = render(&mut app);
    let (x, y) = find(&t, "b - lighter");
    assert_eq!(t.backend().buffer()[(x, y)].fg, Color::LightGreen);
    // Both panes pad a row without a key to the same column: key, space, mark.
    let (tx, ty) = find(&t, "# tin can");
    assert_eq!(t.backend().buffer()[(tx - 2, ty)].symbol(), " ");
}

#[test]
fn each_pane_shows_its_page_and_the_category_state_is_lit() {
    let mut boundary = trade();
    boundary["interaction"]["category_mode"] = true.into();
    let (mut app, _) = open(boundary);
    let t = render(&mut app);
    let text = lines(&t).join("\n");
    assert!(
        text.contains("< Page 1/2 >") && text.contains("< Page 1/1 >"),
        "{text}"
    );
    let (x, y) = find(&t, "ON|OFF");
    let buf = t.backend().buffer();
    assert_eq!(buf[(x, y)].fg, Color::LightGreen);
    assert_eq!(buf[(x + 3, y)].fg, Color::DarkGray);
}

#[test]
fn an_engine_without_category_mode_draws_no_state() {
    let mut boundary = trade();
    boundary["interaction"]
        .as_object_mut()
        .unwrap()
        .remove("category_mode");
    let (mut app, _) = open(boundary);
    assert!(!lines(&render(&mut app)).join("\n").contains("ON|OFF"));
}

#[test]
fn rows_mark_all_some_or_none_like_native_under_a_category_header() {
    let mut boundary = trade();
    let all = &mut boundary["interaction"]["choices"][1];
    all["selected"] = true.into();
    all["selected_count"] = 1.into();
    let (mut app, _) = open(boundary);
    let t = render(&mut app);
    // Partial of three, all of one, and (in the pane test above) none.
    find(&t, "# tin can");
    find(&t, "+ lighter");
    let (x, y) = find(&t, "TOOLS");
    assert_eq!(t.backend().buffer()[(x, y)].fg, Color::Magenta);
    // The header sits on its own line above the first row of the pane.
    assert_eq!(find(&t, "# tin can").1, y + 1);
    // The unit price is lit by the engine's tone: good green, bad red.
    let (px, py) = find(&t, "$0.50");
    assert_eq!(t.backend().buffer()[(px, py)].fg, Color::LightGreen);
    let (px, py) = find(&t, "$1.00");
    assert_eq!(t.backend().buffer()[(px, py)].fg, Color::LightRed);
    // A marked stack still shows how much of it is offered.
    assert!(lines(&t).join("\n").contains("2/3"));
}
