mod common;
use bn_client_core::transport::ScriptHandle;
use bn_client_core::{EngineModel, GameModel};
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

fn render(app: &mut TuiApp<EngineModel>) -> Terminal<TestBackend> {
    let mut t = Terminal::new(TestBackend::new(120, 36)).unwrap();
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

fn crafting_screen(name: &str) -> Value {
    let text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../core/tests/fixtures/crafting.json"
    ))
    .unwrap();
    serde_json::from_str::<Value>(&text).unwrap()[name].clone()
}

fn open(screen: &str) -> (TuiApp<EngineModel>, ScriptHandle) {
    let (t, h) = session();
    let t = t.on("bn.command.submit", vec![fixture("submit_result")]);
    let mut app = TuiApp::new(connected(t));
    h.push(
        json!({"jsonrpc":"2.0","method":"bn.events","params":{"epoch":"e7f3","events":[{
        "sequence": "41", "revision": "32", "type": "interaction.changed",
        "changes": {"interaction": crafting_screen(screen)}}]}}),
    );
    app.model.poll();
    (app, h)
}

fn operations(h: &ScriptHandle) -> Vec<Value> {
    h.sent()
        .into_iter()
        .filter(|m| m["method"] == "bn.command.submit")
        .map(|m| m["params"]["operation"].clone())
        .collect()
}

#[test]
fn crafting_window_shows_tabs_subtabs_search_recipes_and_the_selected_detail() {
    let (mut app, _h) = open("weapon");
    let s = text(&render(&mut app));
    for want in [
        "Weapons",
        "Food",
        "Melee",
        "Ranged",
        "Search:",
        "wooden club",
        "knife spear",
        "Time to complete: 10 m",
        "fabrication 1",
    ] {
        assert!(s.contains(want), "{want} missing in\n{s}");
    }
}

#[test]
fn down_moves_the_selection_and_shows_the_denial_of_an_unavailable_recipe() {
    let (mut app, _h) = open("weapon");
    app.handle_key(key(KeyCode::Down));
    let s = text(&render(&mut app));
    assert!(s.contains("Time to complete: 30 m"), "{s}");
    assert!(s.contains("You lack the components."), "{s}");
}

#[test]
fn enter_crafts_the_selected_recipe_and_does_nothing_on_an_unavailable_one() {
    let (mut app, h) = open("weapon");
    app.handle_key(key(KeyCode::Down));
    app.handle_key(key(KeyCode::Enter));
    assert!(operations(&h).is_empty());
    app.handle_key(key(KeyCode::Up));
    app.handle_key(key(KeyCode::Enter));
    assert_eq!(
        operations(&h),
        [json!({"kind":"choose","choice_id":"recipe:aa"})]
    );
}

#[test]
fn left_right_switch_tabs_and_brackets_switch_subtabs() {
    for (code, id) in [
        (KeyCode::Right, "tab:CC_FOOD"),
        (KeyCode::Left, "tab:CC_FAVORITES"),
        (KeyCode::Char(']'), "subtab:CSC_WEAPON_RANGED"),
    ] {
        let (mut app, h) = open("weapon");
        app.handle_key(key(code));
        assert_eq!(operations(&h), [json!({"kind":"choose","choice_id":id})]);
    }
}

#[test]
fn slash_edits_the_filter_and_enter_submits_it_while_escape_abandons_the_edit() {
    let (mut app, h) = open("weapon");
    app.handle_key(ch('/'));
    for c in "club".chars() {
        app.handle_key(ch(c));
    }
    assert!(text(&render(&mut app)).contains("Search: club"));
    app.handle_key(key(KeyCode::Enter));
    assert_eq!(
        operations(&h),
        [json!({"kind":"fill","field_id":"filter","value":"club","submit":true})]
    );
    let (mut app, h) = open("weapon");
    app.handle_key(ch('/'));
    app.handle_key(ch('x'));
    app.handle_key(key(KeyCode::Esc));
    assert!(
        operations(&h).is_empty(),
        "the menu stays open and sends nothing"
    );
    assert!(app.model.menu().is_some());
}

#[test]
fn b_reads_a_count_and_enter_crafts_a_batch() {
    let (mut app, h) = open("weapon");
    app.handle_key(ch('b'));
    app.handle_key(ch('3'));
    app.handle_key(ch('x'));
    app.handle_key(key(KeyCode::Enter));
    assert_eq!(
        operations(&h),
        [json!({"kind":"set_count","choice_id":"recipe:aa","count":3})]
    );
}

#[test]
fn escape_closes_the_crafting_window() {
    let (mut app, h) = open("weapon");
    app.handle_key(key(KeyCode::Esc));
    assert_eq!(operations(&h), [json!({"kind":"cancel"})]);
}

#[test]
fn batch_mode_lists_its_rows_without_tabs_or_search() {
    let (mut app, _h) = open("batch");
    let s = text(&render(&mut app));
    assert!(s.contains("wooden club x2"), "{s}");
    assert!(!s.contains("Search:") && !s.contains("Weapons"), "{s}");
}

#[test]
fn a_colored_native_log_line_shows_its_repeat_count() {
    let (t, h) = session();
    let mut app = TuiApp::new(connected(t));
    let logged = |sequence: u32, count: u32| {
        json!({"jsonrpc": "2.0", "method": "bn.events", "params": {"epoch": "e7f3", "events": [{
            "sequence": sequence.to_string(), "revision": "32", "type": "message.logged",
            "data": {"id": "8", "text": "Bang!", "kind": "bad", "color": "c_red", "count": count}}]}})
    };
    h.push(logged(41, 1));
    h.push(logged(42, 2));
    app.model.poll();
    let t = render(&mut app);
    assert!(text(&t).contains("Bang! x2"));
    let cell = t
        .backend()
        .buffer()
        .content()
        .iter()
        .find(|c| c.symbol() == "B")
        .unwrap();
    assert_ne!(cell.fg, Color::Reset);
}

#[test]
fn inventory_and_ground_names_keep_their_slot_suffix_even_when_long() {
    let (t, h) = session();
    let mut app = TuiApp::new(connected(t));
    let item = |name: &str, count: u64, slot: Option<&str>| {
        let mut row = json!({"look": {"kind": "item", "id": "x", "glyph": "/", "color": "white"},
            "name": name, "count": count});
        if let Some(slot) = slot {
            row["slot"] = json!(slot);
        }
        row
    };
    let mut event = fixture("events_41_walk");
    let avatar = &mut event["params"]["events"][0]["changes"]["avatar"];
    avatar["inventory"] = json!([
        item("combat knife", 1, Some("wielded")),
        item(
            "reinforced hazmat suit with integrated filtration",
            1,
            Some("worn")
        ),
    ]);
    avatar["ground"] = json!([item("hammer", 2, None)]);
    h.push(event);
    app.model.poll();
    let s = text(&render(&mut app));
    for want in ["combat knife (wielded)", "(worn)", "hammer x2"] {
        assert!(s.contains(want), "{want} missing in\n{s}");
    }
}
