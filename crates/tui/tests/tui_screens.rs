//! The engine's structured screens (lists with values and a description per row) in the terminal client.
mod common;
use bn_client_core::transport::ScriptHandle;
use bn_client_core::{EngineModel, GameModel};
use bn_client_tui::TuiApp;
use common::{connected, fixture, session};
use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use serde_json::{Value, json};

fn engine() -> (TuiApp<EngineModel>, ScriptHandle) {
    let (t, h) = session();
    let t = t.on("bn.command.submit", vec![fixture("submit_result")]);
    (TuiApp::new(connected(t)), h)
}

fn render(app: &mut TuiApp<EngineModel>) -> Terminal<TestBackend> {
    let mut t = Terminal::new(TestBackend::new(120, 40)).unwrap();
    t.draw(|f| app.draw(f)).unwrap();
    t
}

fn rows(t: &Terminal<TestBackend>) -> Vec<String> {
    let b = t.backend().buffer();
    b.content()
        .chunks(b.area.width as usize)
        .map(|row| row.iter().map(|c| c.symbol()).collect::<String>())
        .collect()
}

fn text(t: &Terminal<TestBackend>) -> String {
    rows(t).join("\n")
}

fn click(column: u16, row: u16) -> MouseEvent {
    MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column,
        row,
        modifiers: KeyModifiers::NONE,
    }
}

fn operations(h: &ScriptHandle) -> Vec<Value> {
    h.sent()
        .into_iter()
        .filter(|m| m["method"] == "bn.command.submit")
        .map(|m| m["params"]["operation"].clone())
        .collect()
}

/// Opens `context` with a first row (highlighted by the engine, with a description and values) and a second.
fn open(app: &mut TuiApp<EngineModel>, h: &ScriptHandle, context: &str, tabs: bool) {
    for name in ["events_41_walk", "events_42_message"] {
        h.push(fixture(name));
    }
    let mut menu = fixture("events_43_menu");
    let change = &mut menu["params"]["events"][0]["changes"]["interaction"];
    let row = |id: &str, label: &str, description: &str, columns: Value| {
        json!({"id":id,"label":label,"description":description,"denial":"","enabled":true,
               "selectable":true,"selected":false,"columns":columns})
    };
    let mut choices = vec![
        row(
            "item:0",
            "<color_red>first thing</color>",
            "<color_green>Detail</color> of the first thing.",
            json!([{"label":"Distance","value":"3"},{"label":"Direction","value":"NE"}]),
        ),
        row("item:1", "second thing", "", json!([])),
    ];
    choices[1]["selected"] = true.into();
    if tabs {
        choices.insert(
            0,
            json!({"id":"tab:0","label":"FIRST TAB","description":"","denial":"","enabled":true,
                   "selectable":true,"selected":true,"columns":[]}),
        );
        choices.insert(
            1,
            json!({"id":"tab:1","label":"SECOND TAB","description":"","denial":"","enabled":true,
                   "selectable":true,"selected":false,"columns":[]}),
        );
        change["interaction"]["compat"] = json!({"focus":{"choice_id":"item:0"},
            "panes":[{"id":"tab0","label":"FIRST TAB","role":"focused"},
                     {"id":"tab1","label":"SECOND TAB","role":"category"}]});
        for c in choices
            .iter_mut()
            .filter(|c| c["id"].as_str().unwrap().starts_with("item"))
        {
            c["pane_id"] = "tab0".into();
        }
    } else {
        change["interaction"]["compat"] = json!({"focus":{"choice_id":"item:0"}});
    }
    change["interaction"]["choices"] = json!(choices);
    change["interaction"]["context"] = context.into();
    change["interaction"]["title"] = "A screen".into();
    change["interaction"]["message"] = "<color_yellow>Heading</color>".into();
    h.push(menu);
    app.model.poll();
}

fn the_screen_shows_values_description_and_plain_labels_and_a_click_chooses_a_row(
    context: &str,
    tabs: bool,
) {
    let (mut app, h) = engine();
    render(&mut app);
    open(&mut app, &h, context, tabs);
    let t = render(&mut app);
    let shown = text(&t);
    assert!(
        shown.contains("first thing  Distance: 3  Direction: NE"),
        "{shown}"
    );
    assert!(
        shown.contains("Detail of the first thing."),
        "the highlighted row's description\n{shown}"
    );
    assert!(
        shown.contains("second thing") && !shown.contains("[x]") && !shown.contains("[ ]"),
        "native lists have no selection markers\n{shown}"
    );
    assert!(shown.contains("Heading"), "{shown}");
    assert!(
        !shown.contains("<color_"),
        "colour tags are not shown\n{shown}"
    );
    let all = rows(&t);
    let at = |needle: &str| {
        all.iter()
            .enumerate()
            .find_map(|(y, line)| line.find(needle).map(|x| (x as u16, y as u16)))
            .unwrap()
    };
    if tabs {
        let (x, y) = at("SECOND TAB");
        app.handle_mouse(click(x + 2, y));
        assert_eq!(
            operations(&h).last(),
            Some(&json!({"kind":"choose","choice_id":"tab:1"}))
        );
        // The engine takes the next input once that command is done.
        h.push(fixture("command_completed"));
        app.model.poll();
        render(&mut app);
    }
    let (x, y) = at("second thing");
    assert_eq!(
        t.backend().buffer()[(x, y)].fg,
        ratatui::style::Color::White,
        "a row the screen marks as on is lit white"
    );
    app.handle_mouse(click(x + 1, y));
    assert_eq!(
        operations(&h).last(),
        Some(&json!({"kind":"choose","choice_id":"item:1"})),
        "a click on a row chooses it"
    );
}

macro_rules! screen {
    ($name:ident, $context:expr) => {
        #[test]
        fn $name() {
            the_screen_shows_values_description_and_plain_labels_and_a_click_chooses_a_row(
                $context, false,
            );
        }
    };
    ($name:ident, $context:expr, tabs) => {
        #[test]
        fn $name() {
            the_screen_shows_values_description_and_plain_labels_and_a_click_chooses_a_row(
                $context, true,
            );
        }
    };
}

screen!(message_log, "MESSAGE_LOG");
screen!(item_list, "LIST_ITEMS");
screen!(monster_list, "LIST_MONSTERS");
screen!(vehicle_list, "LIST_VEHICLES");
screen!(mission_list, "MISSIONS", tabs);
screen!(zone_manager, "ZONES_MANAGER");
screen!(faction_manager, "FACTION MANAGER", tabs);
screen!(scores, "SCORES", tabs);
screen!(diary, "DIARY");
screen!(panel_manager, "PANEL_MGMT");

#[test]
fn a_click_outside_the_rows_of_a_menu_sends_nothing() {
    let (mut app, h) = engine();
    render(&mut app);
    open(&mut app, &h, "UILIST", false);
    render(&mut app);
    app.handle_mouse(click(0, 0));
    assert!(operations(&h).is_empty());
}

/// Opens `context` with a row the engine's cursor is on and an `action:ACT` row for the action bound to `z`.
fn open_with_action(app: &mut TuiApp<EngineModel>, h: &ScriptHandle, context: &str, tabs: bool) {
    for name in ["events_41_walk", "events_42_message"] {
        h.push(fixture(name));
    }
    let mut menu = fixture("events_43_menu");
    let change = &mut menu["params"]["events"][0]["changes"]["interaction"];
    change["actions"] = json!([{"id":"ACT","name":"Do it","keys":["z"]}]);
    let row = |id: &str, label: &str| {
        json!({"id":id,"label":label,"description":"","denial":"","enabled":true,
               "selectable":true,"selected":false,"columns":[]})
    };
    let mut choices = vec![
        row("item:0", "first thing"),
        row("action:ACT", "Do the thing"),
    ];
    change["interaction"]["compat"] = json!({"focus":{"choice_id":"item:0"}});
    if tabs {
        choices.insert(0, row("tab:0", "A TAB"));
        choices[0]["selected"] = true.into();
        change["interaction"]["compat"]["panes"] =
            json!([{"id":"tab0","label":"A TAB","role":"focused"}]);
        for c in choices.iter_mut().skip(1) {
            c["pane_id"] = "tab0".into();
        }
    }
    change["interaction"]["choices"] = json!(choices);
    change["interaction"]["context"] = context.into();
    h.push(menu);
    app.model.poll();
}

fn alt(code: crossterm::event::KeyCode) -> crossterm::event::KeyEvent {
    crossterm::event::KeyEvent::new(code, KeyModifiers::ALT)
}

fn an_action_row_shows_its_key_and_alt_down_then_enter_sends_the_action(context: &str, tabs: bool) {
    use crossterm::event::{KeyCode, KeyEvent};
    let (mut app, h) = engine();
    render(&mut app);
    open_with_action(&mut app, &h, context, tabs);
    let shown = text(&render(&mut app));
    assert!(
        shown.contains("Do the thing  [z]"),
        "the action's own key beside the row\n{shown}"
    );
    assert!(
        shown.contains("> first thing"),
        "the engine's cursor is on its row\n{shown}"
    );
    app.handle_key(alt(KeyCode::Down));
    let shown = text(&render(&mut app));
    assert!(
        shown.contains("> Do the thing"),
        "the focus is on the action row\n{shown}"
    );
    assert!(operations(&h).is_empty(), "moving the focus sends nothing");
    app.handle_key(KeyEvent::from(KeyCode::Enter));
    assert_eq!(
        operations(&h),
        [json!({"kind":"choose","choice_id":"action:ACT"})],
        "Enter sends the action, not the engine's confirm"
    );
}

macro_rules! action_rows {
    ($name:ident, $context:expr, $tabs:expr) => {
        #[test]
        fn $name() {
            an_action_row_shows_its_key_and_alt_down_then_enter_sends_the_action($context, $tabs);
        }
    };
}

action_rows!(action_rows_of_a_tabbed_menu, "TABBED_ACTIONS", true);
action_rows!(action_rows_of_a_manager_list, "ZONES_MANAGER", false);
action_rows!(action_rows_of_a_tabbed_text_screen, "MISSIONS", true);

#[test]
fn escape_leaves_the_action_rows_before_it_closes_the_menu() {
    use crossterm::event::{KeyCode, KeyEvent};
    let (mut app, h) = engine();
    render(&mut app);
    open_with_action(&mut app, &h, "ZONES_MANAGER", false);
    render(&mut app);
    app.handle_key(alt(KeyCode::Down));
    app.handle_key(KeyEvent::from(KeyCode::Esc));
    assert!(
        operations(&h).is_empty(),
        "the first Esc only leaves the action rows"
    );
    assert!(text(&render(&mut app)).contains("> first thing"));
}

#[test]
fn a_column_repeating_the_rows_own_text_is_not_shown_twice() {
    let (mut app, h) = engine();
    render(&mut app);
    open(&mut app, &h, "A_SCREEN", false);
    let mut menu = fixture("events_43_menu");
    menu["params"]["events"][0]["sequence"] = "44".into();
    menu["params"]["events"][0]["revision"] = "34".into();
    let change = &mut menu["params"]["events"][0]["changes"]["interaction"]["interaction"];
    change["choices"][0]["label"] = "pen (100/100)".into();
    change["choices"][0]["columns"] = json!([
        {"label":"","value":"<color_c_white>pen</color> (100/100)"},
        {"label":"ACTION","value":"Write on an item"}
    ]);
    h.push(menu);
    app.model.poll();
    let shown = text(&render(&mut app));
    assert_eq!(shown.matches("pen (100/100)").count(), 1, "{shown}");
    assert!(
        shown.contains("pen (100/100)  ACTION: Write on an item"),
        "{shown}"
    );
}

#[test]
fn a_description_line_of_two_dashes_is_a_rule_across_the_window() {
    let (mut app, h) = engine();
    render(&mut app);
    open(&mut app, &h, "A_SCREEN", false);
    let mut menu = fixture("events_43_menu");
    menu["params"]["events"][0]["sequence"] = "44".into();
    menu["params"]["events"][0]["revision"] = "34".into();
    let change = &mut menu["params"]["events"][0]["changes"]["interaction"]["interaction"];
    change["choices"][0]["description"] = "Origin: here\n--\nMaterial: Iron".into();
    h.push(menu);
    app.model.poll();
    let all = rows(&render(&mut app));
    assert!(!all.iter().any(|l| l.contains("--")), "{}", all.join("\n"));
    assert!(
        all.iter().any(|l| l.contains("Origin: here")),
        "{}",
        all.join("\n")
    );
    assert!(
        all.iter().any(|l| l.contains("──────")),
        "{}",
        all.join("\n")
    );
}

/// Opens a `UILIST` whose highlighted row describes itself in `lines` numbered lines.
fn open_described(app: &mut TuiApp<EngineModel>, h: &ScriptHandle, lines: usize) {
    for name in ["events_41_walk", "events_42_message"] {
        h.push(fixture(name));
    }
    let mut menu = fixture("events_43_menu");
    let change = &mut menu["params"]["events"][0]["changes"]["interaction"];
    let description = (1..=lines)
        .map(|n| format!("detail line {n}"))
        .collect::<Vec<_>>()
        .join("\n");
    change["interaction"]["compat"] = json!({"focus":{"choice_id":"item:0"}});
    change["interaction"]["choices"] = json!([
        {"id":"item:0","label":"first thing","description":description,"denial":"","enabled":true,
         "selectable":true,"selected":true,"columns":[]},
        {"id":"item:1","label":"second thing","description":"","denial":"","enabled":true,
         "selectable":true,"selected":false,"columns":[]}
    ]);
    change["interaction"]["context"] = "UILIST".into();
    h.push(menu);
    app.model.poll();
}

#[test]
fn a_long_description_that_fits_the_screen_is_shown_whole_beside_the_rows() {
    let (mut app, h) = engine();
    render(&mut app);
    open_described(&mut app, &h, 30);
    let shown = text(&render(&mut app));
    assert!(shown.contains("detail line 30"), "{shown}");
    assert!(
        shown.contains("first thing") && shown.contains("second thing"),
        "{shown}"
    );
}

#[test]
fn alt_page_keys_scroll_a_description_taller_than_the_screen_and_stop_at_its_ends() {
    use crossterm::event::{KeyCode, KeyEvent};
    let (mut app, h) = engine();
    render(&mut app);
    open_described(&mut app, &h, 80);
    let shown = text(&render(&mut app));
    assert!(
        shown.contains("detail line 1\n") || shown.contains("detail line 1 "),
        "{shown}"
    );
    assert!(!shown.contains("detail line 80"), "{shown}");
    assert!(
        shown.contains("first thing"),
        "the rows stay listed\n{shown}"
    );
    let page = |code| KeyEvent::new(code, KeyModifiers::ALT);
    for _ in 0..10 {
        app.handle_key(page(KeyCode::PageDown));
    }
    let shown = text(&render(&mut app));
    assert!(shown.contains("detail line 80"), "{shown}");
    assert!(!shown.contains("detail line 1 "), "{shown}");
    assert!(
        operations(&h).is_empty(),
        "the engine got none of the scroll keys"
    );
    for _ in 0..10 {
        app.handle_key(page(KeyCode::PageUp));
    }
    let shown = text(&render(&mut app));
    assert!(shown.contains("detail line 1 "), "{shown}");
}

#[test]
fn the_wheel_over_a_long_description_scrolls_it_instead_of_the_engine_menu() {
    let (mut app, h) = engine();
    render(&mut app);
    open_described(&mut app, &h, 80);
    let t = render(&mut app);
    let (x, y) = rows(&t)
        .iter()
        .enumerate()
        .find_map(|(y, l)| l.find("detail line 1 ").map(|x| (x as u16, y as u16)))
        .unwrap();
    let wheel = MouseEvent {
        kind: MouseEventKind::ScrollDown,
        column: x,
        row: y,
        modifiers: KeyModifiers::NONE,
    };
    for _ in 0..3 {
        app.handle_mouse(wheel);
    }
    let shown = text(&render(&mut app));
    assert!(!shown.contains("detail line 3 "), "{shown}");
    assert!(shown.contains("detail line 4 "), "{shown}");
    assert!(operations(&h).is_empty());
}
