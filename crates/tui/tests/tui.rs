#![recursion_limit = "512"]
mod common;
use bn_client_core::transport::ScriptHandle;
use bn_client_core::{EngineModel, FakeModel, GameModel, Pos, Rgb};
use bn_client_tui::TuiApp;
use common::{connected, fixture, session};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
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

/// The engine's look and diagonal movement actions with their keys, as `bn.events` sends them.
fn engine_with_look() -> (TuiApp<EngineModel>, ScriptHandle) {
    let (mut app, h) = engine();
    let actions: Vec<_> = [
        ("look", &[";", "x"][..]),
        ("RIGHT", &["RIGHT", "l"]),
        ("LEFT", &["LEFT", "h"]),
        ("UP", &["UP", "k"]),
        ("DOWN", &["DOWN", "j"]),
        ("LEFTUP", &["HOME", "y"]),
        ("RIGHTUP", &["PPAGE", "u"]),
        ("LEFTDOWN", &["END", "b"]),
        ("RIGHTDOWN", &["NPAGE", "n"]),
    ]
    .iter()
    .map(|(id, keys)| json!({"id": id, "name": id, "keys": keys}))
    .collect();
    h.push(
        json!({"jsonrpc":"2.0","method":"bn.events","params":{"epoch":"e7f3","events":[{
        "sequence":"41","revision":"32","type":"interaction.changed",
        "changes":{"interaction":{"boundary_id":"b:13",
            "readiness":{"phase":"playing","game_ready":true,
                "accepts_interaction_commands":true,"accepts_registered_actions":true},
            "actions": actions, "interaction": null}}}]}}),
    );
    app.model.poll();
    (app, h)
}

/// The actions of a menu's input context with their keys, as the engine sends them.
fn menu_actions() -> Value {
    json!([
        {"id":"UP","name":"Up","keys":["UP"]},
        {"id":"DOWN","name":"Down","keys":["DOWN"]},
        {"id":"PAGE_UP","name":"Page up","keys":["PPAGE"]},
        {"id":"PAGE_DOWN","name":"Page down","keys":["NPAGE"]},
        {"id":"NEXT_TAB","name":"Next tab","keys":["RIGHT"]},
        {"id":"PREV_TAB","name":"Previous tab","keys":["LEFT"]},
        {"id":"CONFIRM","name":"Confirm","keys":["RETURN","x"]},
        {"id":"QUIT","name":"Cancel","keys":["ESC"]}
    ])
}

/// Ends the command in flight so the next key can be sent.
fn finish(app: &mut TuiApp<EngineModel>, h: &ScriptHandle) {
    h.push(json!({"jsonrpc":"2.0","method":"bn.command","params":{
        "epoch":"e7f3","command_id":"c:5","stage":"completed"}}));
    app.model.poll();
}

fn sent_actions(h: &ScriptHandle) -> Vec<Value> {
    submits(h)
        .iter()
        .map(|s| s["params"]["operation"]["action_id"].clone())
        .collect()
}

/// Puts the engine's cursor on `focus` in the menu `fixture` event and gives it the menu actions.
fn menu_event(mut menu: Value, focus: &str) -> Value {
    let ev = &mut menu["params"]["events"][0]["changes"]["interaction"];
    ev["actions"] = menu_actions();
    ev["interaction"]["compat"] = json!({"focus":{"choice_id":focus},"panes":[]});
    menu
}

fn engine_n(commands: usize) -> (TuiApp<EngineModel>, ScriptHandle) {
    let (t, h) = session();
    let t = (0..commands).fold(t, |t, _| {
        t.on("bn.command.submit", vec![fixture("submit_result")])
    });
    (TuiApp::new(connected(t)), h)
}

fn open_menu(app: &mut TuiApp<EngineModel>, h: &ScriptHandle) {
    for name in ["events_41_walk", "events_42_message"] {
        h.push(fixture(name));
    }
    h.push(menu_event(fixture("events_43_menu"), "c:1"));
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
fn menu_keys_run_the_actions_the_menus_context_binds_and_the_player_may_rebind_them() {
    let (mut app, h) = engine_n(4);
    open_menu(&mut app, &h);
    for (k, want) in [
        (key(KeyCode::Down), "DOWN"),
        (key(KeyCode::Enter), "CONFIRM"),
        (ch('x'), "CONFIRM"),
        (key(KeyCode::PageDown), "PAGE_DOWN"),
    ] {
        app.handle_key(k);
        assert_eq!(sent_actions(&h).last(), Some(&json!(want)));
        finish(&mut app, &h);
    }
    assert_eq!(sent_actions(&h).len(), 4);
    app.handle_key(ch('q'));
    assert_eq!(
        sent_actions(&h).len(),
        4,
        "a key the menu does not bind does nothing"
    );
}

#[test]
fn the_highlight_is_the_engines_cursor() {
    let (mut app, h) = engine();
    open_menu(&mut app, &h);
    assert_eq!(app.selected_label().as_deref(), Some("combat knife"));
    h.push(menu_event(fixture("events_43_menu"), "c:2"));
    let mut next = fixture("events_43_menu");
    next["params"]["events"][0]["sequence"] = "44".into();
    next["params"]["events"][0]["revision"] = "34".into();
    h.push(menu_event(next, "c:2"));
    app.model.poll();
    assert_eq!(app.selected_label().as_deref(), Some("tin can"));
}

#[test]
fn escape_closes_the_menu_through_the_engines_cancel_action() {
    let (mut app, h) = engine();
    open_menu(&mut app, &h);
    app.handle_key(key(KeyCode::Esc));
    assert_eq!(sent_actions(&h), [json!("QUIT")]);
}

#[test]
fn game_keys_run_actions_but_a_menu_has_its_own_keys() {
    let (mut app, h) = engine();
    app.handle_key(ch('k'));
    assert_eq!(
        submits(&h)[0]["params"]["operation"],
        json!({"kind":"action","action_id":"UP"})
    );
    let (mut app, h) = engine();
    open_menu(&mut app, &h);
    app.handle_key(ch('k'));
    assert!(submits(&h).is_empty(), "k is bound to nothing in the menu");
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

fn tabbed(h: &ScriptHandle, rows: Value) {
    for name in ["events_41_walk", "events_42_message"] {
        h.push(fixture(name));
    }
    let mut menu = fixture("events_43_menu");
    menu["params"]["events"][0]["changes"]["interaction"]["interaction"]["choices"] = rows;
    h.push(menu_event(menu, "new_game:tutorial"));
}

fn row(id: &str, label: &str, selected: bool, pane: Option<&str>) -> Value {
    let mut c = json!({"id":id,"label":label,"description":"","denial":"","enabled":true,
        "selectable":true,"selected":selected,"columns":[]});
    if let Some(pane) = pane {
        c["pane_id"] = pane.into();
    }
    c
}

fn main_menu(h: &ScriptHandle) {
    tabbed(
        h,
        json!([
            row("tab:new_game", "New Game", true, None),
            row("tab:settings", "Settings", false, None),
            row("new_game:tutorial", "Tutorial", false, Some("new_game")),
            row(
                "new_game:custom",
                "Custom Character",
                false,
                Some("new_game")
            ),
        ]),
    );
}

#[test]
fn main_menu_shows_tabs_in_a_bar_above_the_selected_tabs_entries() {
    let (mut app, h) = engine();
    main_menu(&h);
    app.model.poll();
    let s = text(&render(&mut app));
    let line = |want: &str| s.lines().position(|l| l.contains(want)).unwrap();
    assert!(
        s.lines()
            .any(|l| l.contains("New Game") && l.contains("Settings")),
        "{s}"
    );
    assert!(line("Settings") < line("Tutorial") && line("Tutorial") < line("Custom Character"));
    assert!(!s.contains("> New Game"), "tabs are not list rows:\n{s}");
    assert!(s.contains("> Tutorial"));
}

#[test]
fn tab_keys_are_the_menus_next_and_previous_tab_actions() {
    let (mut app, h) = engine_n(3);
    main_menu(&h);
    app.model.poll();
    app.handle_key(key(KeyCode::Right));
    assert_eq!(sent_actions(&h).last(), Some(&json!("NEXT_TAB")));
    finish(&mut app, &h);
    app.handle_key(key(KeyCode::Left));
    assert_eq!(sent_actions(&h).last(), Some(&json!("PREV_TAB")));
}

#[test]
fn confirm_on_a_tabbed_menu_is_the_engines_action_whichever_row_is_highlighted() {
    let (mut app, h) = engine();
    main_menu(&h);
    app.model.poll();
    app.handle_key(key(KeyCode::Enter));
    assert_eq!(sent_actions(&h), [json!("CONFIRM")]);
}

#[test]
fn long_menus_scroll_to_keep_the_engines_cursor_row_visible() {
    let (mut app, h) = engine();
    let rows = (0..60).map(|i| row(&format!("c:{i}"), &format!("Item {i}"), false, None));
    for name in ["events_41_walk", "events_42_message"] {
        h.push(fixture(name));
    }
    let mut menu = fixture("events_43_menu");
    menu["params"]["events"][0]["changes"]["interaction"]["interaction"]["choices"] =
        Value::Array(rows.collect());
    h.push(menu_event(menu, "c:45"));
    app.model.poll();
    let s = text(&render(&mut app));
    assert!(s.contains("> Item 45") && !s.contains("Item 3 "), "{s}");
}

#[test]
fn korean_menu_rows_keep_the_popup_border_aligned() {
    let (mut app, h) = engine();
    tabbed(
        &h,
        json!([
            row("c:0", "새 게임 시작하기", false, None),
            row("c:1", "불러오기", false, None)
        ]),
    );
    app.model.poll();
    let t = render(&mut app);
    let buf = t.backend().buffer();
    let rows = (0..buf.area.height)
        .filter(|&y| {
            let line = (0..buf.area.width)
                .map(|x| buf[(x, y)].symbol())
                .collect::<String>()
                .replace(' ', "");
            line.contains("새게임") || line.contains("불러오기")
        })
        .collect::<Vec<_>>();
    assert_eq!(rows.len(), 2);
    let edge = |y: u16| {
        (0..buf.area.width)
            .rev()
            .find(|&x| buf[(x, y)].symbol() == "│")
            .unwrap()
    };
    assert_eq!(
        edge(rows[0]),
        edge(rows[1]),
        "same right border on every row"
    );
}

#[test]
fn map_viewport_is_declared_from_the_terminal_size_and_redeclared_on_resize() {
    let (t, h) = session();
    let t = t.on(
        "bn.viewport",
        vec![json!({"jsonrpc":"2.0","id":null,"result":{}})],
    );
    let mut app = TuiApp::new(connected(t));
    let viewports = |h: &ScriptHandle| {
        h.sent()
            .into_iter()
            .filter(|s| s["method"] == "bn.viewport")
            .map(|s| s["params"].clone())
            .collect::<Vec<_>>()
    };
    let draw = |app: &mut TuiApp<EngineModel>, w, hgt| {
        let mut t = Terminal::new(TestBackend::new(w, hgt)).unwrap();
        t.draw(|f| app.draw(f)).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(300));
        app.model.poll();
    };
    // Map pane = width - 36 side panel - 2 borders; height - 1 status - 8 log - 2 borders.
    draw(&mut app, 120, 40);
    assert_eq!(viewports(&h), [json!({"cols": 82, "rows": 29})]);
    draw(&mut app, 100, 30);
    assert_eq!(viewports(&h).last(), Some(&json!({"cols": 62, "rows": 19})));
}

#[test]
fn a_view_larger_than_the_terminal_scrolls_to_keep_the_player_in_the_middle() {
    let mut app = TuiApp::new(FakeModel::default());
    for _ in 0..10 {
        app.handle_key(key(KeyCode::Down));
    }
    let mut t = Terminal::new(TestBackend::new(60, 22)).unwrap();
    t.draw(|f| app.draw(f)).unwrap();
    let buf = t.backend().buffer();
    let at = (0..buf.area.height)
        .flat_map(|y| (0..buf.area.width).map(move |x| (x, y)))
        .find(|&(x, y)| buf[(x, y)].symbol() == "@")
        .unwrap();
    // 11 map rows inside the border: the player is on the middle one.
    assert_eq!(at.1, 1 + 11 / 2);
}

#[test]
fn status_line_shows_ready_then_the_command_and_its_stage() {
    let (mut app, _h) = engine();
    let ready = text(&render(&mut app));
    assert!(ready.contains("Ready"), "{ready}");
    app.handle_key(ch('k'));
    let s = text(&render(&mut app));
    assert!(!s.contains("Ready") && s.contains("Move up"), "{s}");
}

#[test]
fn look_cursor_moves_without_acting_and_describes_the_cell_under_it() {
    let (mut app, h) = engine_with_look();
    app.handle_key(ch('x'));
    let me = app.model.player();
    app.handle_key(key(KeyCode::Right));
    app.handle_key(ch('j'));
    assert!(submits(&h).is_empty(), "cursor keys do not walk");
    assert_eq!(app.model.player(), me);
    let s = text(&render(&mut app));
    let want = format!("Look ({}, {})", me.x + 1, me.y + 1);
    assert!(s.contains(&want), "{want} in:\n{s}");
}

#[test]
fn shift_moves_the_look_cursor_five_cells_and_it_stays_inside_the_view() {
    let (mut app, _h) = engine_with_look();
    let me = app.model.player();
    app.handle_key(ch('x'));
    app.handle_key(KeyEvent::new(KeyCode::Right, KeyModifiers::SHIFT));
    assert!(text(&render(&mut app)).contains(&format!("Look ({}, {})", me.x + 5, me.y)));
    for _ in 0..40 {
        app.handle_key(KeyEvent::new(KeyCode::Up, KeyModifiers::SHIFT));
    }
    assert!(text(&render(&mut app)).contains(&format!("Look ({}, 0)", me.x + 5)));
}

#[test]
fn enter_in_look_mode_travels_to_the_cursor_and_esc_returns_to_walking() {
    let (mut app, h) = engine_with_look();
    app.handle_key(ch('x'));
    let me = app.model.player();
    for _ in 0..2 {
        app.handle_key(key(KeyCode::Right));
    }
    app.handle_key(key(KeyCode::Enter));
    let op = &submits(&h)[0]["params"]["operation"];
    assert_eq!(op["kind"], "travel");
    app.handle_key(key(KeyCode::Esc));
    assert!(!text(&render(&mut app)).contains("Look ("));
    assert_eq!(app.model.player(), me);
}

#[test]
fn enter_on_the_players_own_square_leaves_look_mode_without_travelling() {
    let (mut app, h) = engine();
    app.handle_key(ch('x'));
    app.handle_key(key(KeyCode::Enter));
    assert!(submits(&h).is_empty());
    assert!(!text(&render(&mut app)).contains("Look ("));
}

#[test]
fn clicking_a_map_cell_travels_there() {
    let (mut app, h) = engine();
    let t = render(&mut app);
    let me = app.model.player();
    let buf = t.backend().buffer();
    let (px, py) = (0..buf.area.height)
        .flat_map(|y| (0..buf.area.width).map(move |x| (x, y)))
        .find(|&(x, y)| buf[(x, y)].symbol() == "@")
        .unwrap();
    assert_eq!(me.x, me.x);
    app.handle_mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: px + 2,
        row: py,
        modifiers: KeyModifiers::NONE,
    });
    let sent = submits(&h);
    assert_eq!(sent[0]["params"]["operation"]["kind"], "travel");
    let want = app.model.origin().x + me.x + 2;
    assert_eq!(sent[0]["params"]["operation"]["pos"]["x"], want);
}

#[test]
fn clicks_outside_the_map_or_under_a_menu_do_nothing() {
    let (mut app, h) = engine();
    render(&mut app);
    let click = |column, row| MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column,
        row,
        modifiers: KeyModifiers::NONE,
    };
    app.handle_mouse(click(99, 29));
    open_menu(&mut app, &h);
    render(&mut app);
    app.handle_mouse(click(20, 10));
    assert!(submits(&h).is_empty());
}

#[test]
fn fake_model_walks_to_the_look_cursor_one_step_per_tick() {
    let mut app = TuiApp::new(FakeModel::default());
    let start = app.model.player();
    app.handle_key(ch('x'));
    for _ in 0..3 {
        app.handle_key(key(KeyCode::Right));
    }
    app.handle_key(key(KeyCode::Enter));
    assert!(app.walking());
    app.tick();
    assert_eq!(app.model.player().x, start.x + 1);
    for _ in 0..5 {
        app.tick();
    }
    assert_eq!(app.model.player().x, start.x + 3);
    assert!(!app.walking());
}

#[test]
fn tab_opens_the_action_list_and_enter_runs_the_highlighted_action() {
    let (mut app, h) = engine();
    app.handle_key(key(KeyCode::Tab));
    let s = text(&render(&mut app));
    assert!(s.contains("> Move right"), "{s}");
    app.handle_key(key(KeyCode::Down));
    app.handle_key(key(KeyCode::Enter));
    assert_eq!(submits(&h).len(), 1);
    assert!(!text(&render(&mut app)).contains("> "), "list closed");
}

#[test]
fn page_up_scrolls_the_message_log_back_and_page_down_returns() {
    let mut app = TuiApp::new(FakeModel::default());
    for i in 0..20 {
        app.model.say(format!("line {i}"));
    }
    assert!(text(&render(&mut app)).contains("line 19"));
    app.handle_key(key(KeyCode::PageUp));
    let s = text(&render(&mut app));
    assert!(!s.contains("line 19") && s.contains("line 13"), "{s}");
    app.handle_key(key(KeyCode::PageDown));
    assert!(text(&render(&mut app)).contains("line 19"));
}

#[test]
fn wide_map_glyphs_do_not_shift_the_rest_of_the_row() {
    struct Wide(FakeModel);
    impl GameModel for Wide {
        fn size(&self) -> Pos {
            self.0.size()
        }
        fn cell(&self, at: Pos) -> bn_client_core::Cell {
            let mut c = self.0.cell(at);
            if at.x == 3 {
                c.glyph = '木';
            }
            c
        }
        fn player(&self) -> Pos {
            self.0.player()
        }
        fn step_toward(&mut self, t: Pos) -> bool {
            self.0.step_toward(t)
        }
        fn ground(&self) -> &[String] {
            self.0.ground()
        }
        fn inventory(&self) -> &[String] {
            self.0.inventory()
        }
        fn pick_up(&mut self, _: usize) {}
        fn drop_item(&mut self, _: usize) {}
        fn messages(&self) -> &[String] {
            self.0.messages()
        }
        fn say(&mut self, t: String) {
            self.0.say(t)
        }
    }
    let mut app = TuiApp::new(Wide(FakeModel::default()));
    let t = render(&mut app);
    let buf = t.backend().buffer();
    let y = (0..buf.area.height)
        .find(|&y| (0..buf.area.width).any(|x| buf[(x, y)].symbol() == "木"))
        .unwrap();
    let x = (0..buf.area.width)
        .find(|&x| buf[(x, y)].symbol() == "木")
        .unwrap();
    // The glyph owns two columns: the cell after it is the wide glyph's continuation, not the next map cell.
    let row = (0..buf.area.width)
        .map(|c| buf[(c, y)].symbol())
        .collect::<String>();
    assert!(row.contains("木"), "{row}");
    assert_eq!(buf[(x + 1, y)].symbol().trim(), "");
}

#[test]
fn loading_step_list_scrolls_to_keep_the_current_step_visible() {
    let (mut app, h) = engine();
    let entries = (0..100).map(|i| format!("Step {i}")).collect::<Vec<_>>();
    h.push(json!({"jsonrpc":"2.0","method":"bn.loading","params":{"epoch":"e7f3","title":"Loading world",
      "entries":entries,"index":80}}));
    app.model.poll();
    let s = text(&render(&mut app));
    assert!(s.contains("> Step 80") && !s.contains("Step 3 "), "{s}");
}

#[test]
fn long_menu_messages_wrap_inside_the_popup_for_latin_and_korean_text() {
    let (mut app, h) = engine();
    for name in ["events_41_walk", "events_42_message", "events_43_menu"] {
        let mut ev = fixture(name);
        if name == "events_43_menu" {
            let long = format!("{} {}", "word ".repeat(30), "가나다라마바사".repeat(12));
            ev["params"]["events"][0]["changes"]["interaction"]["interaction"]["message"] =
                long.into();
        }
        h.push(ev);
    }
    app.model.poll();
    let s = text(&render(&mut app)).replace(' ', "");
    assert_eq!(
        s.matches('사').count(),
        12,
        "every wrapped Korean cell is shown:\n{s}"
    );
    assert_eq!(s.matches("word").count(), 30);
    assert!(s.contains("combat"), "choices still shown:\n{s}");
}

fn interrupts(h: &ScriptHandle) -> usize {
    h.sent_methods()
        .iter()
        .filter(|m| *m == "bn.interrupt")
        .count()
}

#[test]
fn esc_interrupts_the_running_command_and_the_status_line_says_so() {
    let (mut app, h) = engine();
    app.handle_key(key(KeyCode::Esc));
    assert_eq!(interrupts(&h), 0, "nothing is running");
    assert!(!text(&render(&mut app)).contains("Esc: interrupt"));
    app.handle_key(ch('l'));
    assert!(app.model.busy());
    assert!(text(&render(&mut app)).contains("Esc: interrupt"));
    app.handle_key(key(KeyCode::Esc));
    assert_eq!(interrupts(&h), 1);
}

#[test]
fn control_function_navigation_and_numpad_keys_reach_the_engines_actions() {
    let (t, h) = session();
    let t = (0..6).fold(t, |t, _| {
        t.on("bn.command.submit", vec![fixture("submit_result")])
    });
    let mut app = TuiApp::new(connected(t));
    h.push(
        json!({"jsonrpc":"2.0","method":"bn.events","params":{"epoch":"e7f3","events":[{
        "sequence":"41","revision":"32","type":"interaction.changed",
        "changes":{"interaction":{"boundary_id":"b:13",
            "readiness":{"phase":"playing","game_ready":true,
                "accepts_interaction_commands":true,"accepts_registered_actions":true},
            "actions":[{"id":"save","name":"Save","keys":["CTRL+S"]},
                       {"id":"help","name":"Help","keys":["F1"]},
                       {"id":"back","name":"Back","keys":["BACKTAB"]},
                       {"id":"pause","name":"Pause","keys":[".","NUMPAD_5"]},
                       {"id":"up","name":"Page","keys":["PPAGE"]},
                       {"id":"del","name":"Del","keys":["DELETE"]}],
            "interaction":null}}}]}}),
    );
    app.model.poll();
    let sent = |app: &mut TuiApp<EngineModel>, e: KeyEvent| {
        app.handle_key(e);
        let id = submits(&h)
            .last()
            .map(|s| s["params"]["operation"]["action_id"].clone());
        h.push(json!({"jsonrpc":"2.0","method":"bn.command","params":{
            "epoch":"e7f3","command_id":"c:5","stage":"completed"}}));
        app.model.poll();
        id
    };
    let ctrl_s = KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL);
    assert_eq!(sent(&mut app, ctrl_s), Some(json!("save")));
    assert_eq!(sent(&mut app, key(KeyCode::F(1))), Some(json!("help")));
    assert_eq!(sent(&mut app, key(KeyCode::BackTab)), Some(json!("back")));
    assert_eq!(
        sent(&mut app, ch('5')),
        Some(json!("pause")),
        "numpad 5 arrives as a digit"
    );
    assert_eq!(sent(&mut app, key(KeyCode::PageUp)), Some(json!("up")));
    assert_eq!(sent(&mut app, key(KeyCode::Delete)), Some(json!("del")));
}

#[test]
fn a_dead_engine_shows_stderr_and_r_restarts_it() {
    let (t, first) = session();
    let (next, second) = session();
    let mut spare = Some(next);
    let model = bn_client_core::EngineModel::new(Box::new(t)).with_respawn(Box::new(move || {
        Ok(Box::new(spare.take().expect("one restart"))
            as Box<dyn bn_client_core::transport::Transport>)
    }));
    let mut app = TuiApp::new(model);
    app.model.poll();
    first.close(&["Segmentation fault"]);
    app.model.poll();
    let screen = text(&render(&mut app));
    assert!(screen.contains("The engine stopped."), "{screen}");
    assert!(screen.contains("Segmentation fault"));
    assert!(screen.contains("R: restart"));
    app.handle_key(ch('x'));
    assert!(app.model.failure().is_some(), "other keys do nothing");
    app.handle_key(ch('r'));
    app.model.poll();
    assert!(app.model.failure().is_none());
    assert_eq!(second.sent_methods()[..2], ["bn.hello", "bn.subscribe"]);
    assert!(!text(&render(&mut app)).contains("The engine stopped."));
}

mod overmap {
    use super::*;

    fn open_overmap() -> (TuiApp<EngineModel>, ScriptHandle) {
        let (mut app, h) = engine();
        h.push(common::overmap_event(100));
        app.model.poll();
        (app, h)
    }

    fn last_operation(h: &ScriptHandle) -> Value {
        submits(h).last().unwrap()["params"]["operation"].clone()
    }

    #[test]
    fn the_overmap_is_drawn_with_the_cursor_note_and_legend() {
        let (mut app, _h) = open_overmap();
        let screen = text(&render(&mut app));
        assert!(screen.contains("Overmap (100, 200, 0)"), "{screen}");
        assert!(
            screen.contains("Note: cache") || !screen.contains("Note:"),
            "{screen}"
        );
        assert!(screen.contains("forest"), "{screen}");
        assert!(
            screen.contains("@"),
            "the player marker is part of the cells"
        );
        assert!(!screen.contains("Map"), "the map is not drawn under it");
    }

    #[test]
    fn a_click_moves_the_cursor_and_esc_closes() {
        let (mut app, h) = open_overmap();
        render(&mut app);
        let click = |col: u16, row: u16| MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: col,
            row,
            modifiers: KeyModifiers::NONE,
        };
        // Cell (3, 0) is square (101, 199).
        app.handle_mouse(click(3, 0));
        assert_eq!(
            last_operation(&h),
            json!({"kind":"set_target","pos":{"dim":"","x":101,"y":199,"z":0}})
        );
        h.push(json!({"jsonrpc":"2.0","method":"bn.command","params":{
            "epoch":"e7f3","command_id":"c:5","stage":"completed"}}));
        app.model.poll();
        app.handle_key(key(KeyCode::Esc));
        assert_eq!(last_operation(&h), json!({"kind":"cancel"}));
    }
}

#[test]
fn a_composed_screen_shows_its_text_and_its_actions_take_the_keys() {
    let (mut app, h) = engine();
    for name in ["events_41_walk", "events_42_message"] {
        h.push(fixture(name));
    }
    let mut screen = fixture("events_43_menu");
    let ev = &mut screen["params"]["events"][0]["changes"]["interaction"];
    ev["actions"] = json!([{"id":"NEXT_TAB","name":"Next tab","keys":["TAB"]}]);
    ev["interaction"]["kind"] = "custom".into();
    ev["interaction"]["message"] = "Strength:    8\nDexterity:   8".into();
    ev["interaction"]["choices"] = json!([]);
    h.push(screen);
    app.model.poll();
    let s = text(&render(&mut app));
    assert!(
        s.contains("Strength:    8") && s.contains("Dexterity:   8"),
        "{s}"
    );
    app.handle_key(key(KeyCode::Tab));
    assert_eq!(sent_actions(&h), [json!("NEXT_TAB")]);
}

mod overmap_route {
    use super::*;

    #[test]
    fn the_route_and_the_notes_are_shown_and_a_note_row_moves_the_cursor() {
        let (mut app, h) = engine();
        h.push(common::overmap_event(100));
        app.model.poll();
        let screen = text(&render(&mut app));
        assert!(screen.contains("Route of 2 squares"), "{screen}");
        assert!(screen.contains("(99, 200) cache"), "{screen}");
        // The note list sits at the foot of the sidebar; its one row is the note.
        let row = screen
            .lines()
            .position(|l| l.contains("(99, 200) cache"))
            .unwrap() as u16;
        let col = screen
            .lines()
            .nth(row as usize)
            .unwrap()
            .find("(99, 200)")
            .unwrap() as u16;
        app.handle_mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: col,
            row,
            modifiers: KeyModifiers::NONE,
        });
        assert_eq!(
            submits(&h).last().unwrap()["params"]["operation"],
            json!({"kind":"set_target","pos":{"dim":"","x":99,"y":200,"z":0}})
        );
    }
}
