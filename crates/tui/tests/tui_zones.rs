//! The zone manager's key rows and the target screens (look around, a zone's area step, the map editor's square).
mod common;
use bn_client_core::transport::ScriptHandle;
use bn_client_core::{EngineModel, GameModel};
use bn_client_tui::TuiApp;
use common::{connected, fixture, session};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use serde_json::{Value, json};

fn engine() -> (TuiApp<EngineModel>, ScriptHandle) {
    let (t, h) = session();
    let t = t.on("bn.command.submit", vec![fixture("submit_result")]);
    (TuiApp::new(connected(t)), h)
}

fn text(app: &mut TuiApp<EngineModel>) -> String {
    let mut t = Terminal::new(TestBackend::new(120, 40)).unwrap();
    t.draw(|f| app.draw(f)).unwrap();
    let b = t.backend().buffer();
    b.content()
        .chunks(b.area.width as usize)
        .map(|row| row.iter().map(|c| c.symbol()).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n")
}

fn mouse(column: u16, row: u16) -> MouseEvent {
    MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column,
        row,
        modifiers: KeyModifiers::NONE,
    }
}

/// The ids of the rows chosen so far.
fn chosen(h: &ScriptHandle) -> Vec<String> {
    h.sent()
        .iter()
        .filter(|m| m["method"] == "bn.command.submit")
        .filter_map(|m| {
            m["params"]["operation"]["choice_id"]
                .as_str()
                .map(String::from)
        })
        .collect()
}

fn row(id: &str, pane: &str, label: &str, description: &str, hotkey: &str) -> Value {
    json!({"id":id,"label":label,"description":description,"denial":"","hotkey":hotkey,
        "pane_id":pane,"enabled":true,"selectable":true,"selected":false,"columns":[]})
}

/// Shows `interaction` as the engine's current boundary, with `actions` bound.
fn show(app: &mut TuiApp<EngineModel>, h: &ScriptHandle, interaction: Value, actions: Value) {
    for name in ["events_41_walk", "events_42_message"] {
        h.push(fixture(name));
    }
    let mut menu = fixture("events_43_menu");
    let change = &mut menu["params"]["events"][0]["changes"]["interaction"];
    change["actions"] = actions;
    change["interaction"] = interaction;
    h.push(menu);
    app.model.poll();
}

fn screen(kind: &str, context: &str, title: &str, message: &str, choices: Vec<Value>) -> Value {
    json!({"schema_id":"s:z","kind":kind,"context":context,"title":title,"message":message,
        "allow_cancel":true,"choices":choices,"choice_total":choices.len(),"allow_set_count":false,
        "compat":{"focus":{"choice_id":"zone:0"}},
        "target":{"source":{"dim":"","x":2,"y":4,"z":0},"current":{"dim":"","x":3,"y":4,"z":0},
            "range":0,"distance_metric":"square","status":"zone_first","candidates":[]}})
}

#[test]
fn the_status_line_tells_the_step_of_a_zones_area() {
    let (mut app, h) = engine();
    text(&mut app);
    let look = screen(
        "target",
        "LOOK",
        "Look Around",
        "Select first point.",
        vec![],
    );
    show(&mut app, &h, look, json!([]));
    let shown = text(&mut app);
    assert!(
        shown.contains("Look Around: Select first point."),
        "the area step beside the cursor\n{shown}"
    );
}

#[test]
fn the_status_line_lists_the_map_editors_square_and_its_keys() {
    let (mut app, h) = engine();
    text(&mut app);
    let editor = screen(
        "target",
        "EDITMAP",
        "Map editor",
        "",
        vec![
            row("tile:terrain", "tile", "Terrain", "grass", ""),
            row("action:EDIT_TERRAIN", "actions", "Edit terrain", "", "e"),
        ],
    );
    show(&mut app, &h, editor, json!([]));
    let shown = text(&mut app);
    assert!(
        shown.contains("Map editor: Terrain grass. e Edit terrain"),
        "{shown}"
    );
}

#[test]
fn a_zone_manager_key_row_shows_its_hotkey_once() {
    let (mut app, h) = engine();
    text(&mut app);
    let manager = screen(
        "choices",
        "ZONES_MANAGER",
        "Zones",
        "",
        vec![
            row("zone:0", "zones", "stockpile", "", ""),
            row("action:ADD_ZONE", "actions", "Add zone", "", "a"),
            row("action:CONFIRM", "actions", "Confirm", "", ""),
        ],
    );
    let actions = json!([{"id":"ADD_ZONE","name":"Add zone","keys":["a","A"]},
        {"id":"CONFIRM","name":"Confirm","keys":["RETURN"]}]);
    show(&mut app, &h, manager, actions);
    let shown = text(&mut app);
    assert!(shown.contains("a Add zone"), "the row's own key\n{shown}");
    assert!(!shown.contains("Add zone  [a]"), "not repeated\n{shown}");
    assert!(
        shown.contains("Confirm  [RETURN]"),
        "a row without a key keeps the action's\n{shown}"
    );
}

fn with_panes(mut interaction: Value, panes: Value) -> Value {
    interaction["compat"]["panes"] = panes;
    interaction
}

#[test]
fn the_zone_manager_lists_its_zones_options_and_actions_under_headings() {
    let (mut app, h) = engine();
    text(&mut app);
    let manager = screen(
        "choices",
        "ZONES_MANAGER",
        "Zones",
        "",
        vec![
            row("zone:0", "zones", "stockpile", "", ""),
            row("action:ADD_ZONE", "actions", "Add zone", "", "a"),
        ],
    );
    let panes = json!([{"id":"zones","label":"Zones","role":"current"},
        {"id":"options","label":"Options","role":"inactive"},
        {"id":"actions","label":"Actions","role":"controls"}]);
    show(&mut app, &h, with_panes(manager, panes), json!([]));
    let shown = text(&mut app);
    let lines = shown.lines().collect::<Vec<_>>();
    let at = |needle: &str| lines.iter().rposition(|l| l.contains(needle)).unwrap();
    assert!(at("stockpile") == at("│  Zones") + 1, "{shown}");
    assert!(at("│  a Add zone") == at("│  Actions") + 1, "{shown}");
    let y = at("│  a Add zone") as u16;
    app.handle_mouse(mouse(lines[y as usize].find("Add zone").unwrap() as u16, y));
    assert_eq!(
        chosen(&h),
        ["action:ADD_ZONE"],
        "a click maps to its row past a heading"
    );
}

#[test]
fn the_footer_describes_the_cursor_row_under_headings() {
    let (mut app, h) = engine();
    text(&mut app);
    let mut manager = screen(
        "choices",
        "ZONES_MANAGER",
        "Zones",
        "",
        vec![
            row("zone:0", "zones", "stockpile", "Holds the loot.", ""),
            row("zone:1", "zones", "farm", "Grows the crops.", ""),
            row("action:ADD_ZONE", "actions", "Add zone", "Adds one.", "a"),
        ],
    );
    manager["compat"]["focus"]["choice_id"] = json!("zone:1");
    let panes = json!([{"id":"zones","label":"Zones","role":"current"},
        {"id":"actions","label":"Actions","role":"controls"}]);
    show(&mut app, &h, with_panes(manager, panes), json!([]));
    let shown = text(&mut app);
    assert!(shown.contains("Grows the crops."), "{shown}");
    assert!(!shown.contains("Adds one."), "{shown}");
}

/// The ids of the actions invoked so far.
fn invoked(h: &ScriptHandle) -> Vec<String> {
    h.sent()
        .iter()
        .filter(|m| m["method"] == "bn.command.submit")
        .filter_map(|m| {
            m["params"]["operation"]["action_id"]
                .as_str()
                .map(String::from)
        })
        .collect()
}

#[test]
fn hovering_the_cursor_square_while_picking_a_zone_point_does_not_confirm_it_but_a_click_does() {
    let (mut app, h) = engine();
    text(&mut app);
    let look = screen(
        "target",
        "LOOK",
        "Look Around",
        "Select first point.",
        vec![],
    );
    show(
        &mut app,
        &h,
        look,
        json!([{"id":"CONFIRM","name":"Confirm","keys":["RETURN"]}]),
    );
    // The cursor is at world (3, 4), one square west of the avatar the map draws as `@`.
    let shown = text(&mut app);
    let (row, at) = shown
        .lines()
        .enumerate()
        .find_map(|(y, l)| {
            l.chars()
                .position(|c| c == '@')
                .map(|x| (y as u16, x as u16))
        })
        .expect("the avatar");
    let on_cursor = |kind| MouseEvent {
        kind,
        column: at - 1,
        row,
        modifiers: KeyModifiers::NONE,
    };
    app.handle_mouse(on_cursor(MouseEventKind::Moved));
    assert_eq!(
        invoked(&h),
        Vec::<String>::new(),
        "a hover only moves the cursor"
    );
    app.handle_mouse(on_cursor(MouseEventKind::Down(MouseButton::Left)));
    assert_eq!(
        invoked(&h),
        ["CONFIRM"],
        "a click on the cursor takes the point"
    );
}

#[test]
fn the_engines_cursor_is_lit_on_the_map_while_picking_a_zone_point() {
    let (mut app, h) = engine();
    text(&mut app);
    let look = screen(
        "target",
        "LOOK",
        "Look Around",
        "Select first point.",
        vec![],
    );
    show(&mut app, &h, look, json!([]));
    let mut t = Terminal::new(TestBackend::new(120, 40)).unwrap();
    t.draw(|f| app.draw(f)).unwrap();
    let buf = t.backend().buffer();
    let lit = (0..buf.area.height)
        .flat_map(|y| (0..buf.area.width).map(move |x| (x, y)))
        .filter(|&pos| {
            buf[pos]
                .modifier
                .contains(ratatui::style::Modifier::REVERSED)
        })
        .collect::<Vec<_>>();
    let avatar = (0..buf.area.height)
        .flat_map(|y| (0..buf.area.width).map(move |x| (x, y)))
        .find(|&pos| buf[pos].symbol() == "@")
        .expect("the avatar");
    // The cursor is at world (3, 4), one square west of the avatar.
    assert_eq!(lit, [(avatar.0 - 1, avatar.1)]);
}

#[test]
fn enter_confirms_a_zone_point_after_a_click_moved_the_engines_cursor() {
    let (mut app, h) = engine();
    text(&mut app);
    let look = screen(
        "target",
        "LOOK",
        "Look Around",
        "Select first point.",
        vec![],
    );
    show(
        &mut app,
        &h,
        look,
        json!([{"id":"CONFIRM","name":"Confirm","keys":["RETURN"]}]),
    );
    let shown = text(&mut app);
    let (row, at) = shown
        .lines()
        .enumerate()
        .find_map(|(y, l)| {
            l.chars()
                .position(|c| c == '@')
                .map(|x| (y as u16, x as u16))
        })
        .expect("the avatar");
    app.handle_mouse(mouse(at + 2, row));
    h.push(
        json!({"jsonrpc":"2.0","method":"bn.command","params":{"epoch":"e7f3",
        "command_id":"c:5","stage":"completed"}}),
    );
    app.model.poll();
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert_eq!(
        invoked(&h),
        ["CONFIRM"],
        "the engine owns the cursor, not the client's look mode"
    );
    let shown = text(&mut app);
    assert!(
        !shown.contains("Look ("),
        "no client look note over the engine's\n{shown}"
    );
}

#[test]
fn native_look_shows_the_engines_parts_and_its_keys_move_the_engines_cursor() {
    let (mut app, h) = engine();
    text(&mut app);
    for name in ["events_41_walk", "events_42_message"] {
        h.push(fixture(name));
    }
    let mut menu = fixture("events_43_menu");
    let change = &mut menu["params"]["events"][0]["changes"]["interaction"];
    change["actions"] = json!([{"id":"RIGHT","name":"Move right","keys":["RIGHT"]}]);
    change["interaction"] = Value::Null;
    change["stack"] = json!([
        {"screen_id":"screen:map","role":"map","root":null,"affordances":[]},
        {"screen_id":"screen:7","role":"dialog","title":"Look Around",
         "root":{"id":"target","kind":"panel","title":"Look Around","child":{"id":"target:map",
             "kind":"map","affordances":["target:move"],"mode":"world",
             "target":{"source":{"dim":"","x":2,"y":4,"z":0},"current":{"dim":"","x":3,"y":4,"z":0},
                 "range":8,"distance_metric":"square","status":"look","candidates":[],
                 "parts":[{"kind":"terrain","text":"Grass; Lighting: bright"},
                     {"kind":"furniture","text":"There is a tree here."}]}}},
         "affordances":[{"id":"target:move","label":"Select","keys":[],"enabled":true,
             "args":{"pos":{"unit":"square"}},"expect":"boundary"},
             {"id":"RIGHT","label":"Move right","keys":["RIGHT"],"enabled":true,
              "args":{},"expect":"screen"}]}]);
    h.push(menu);
    app.model.poll();
    let shown = text(&mut app);
    assert!(
        shown.contains("Grass; Lighting: bright") && shown.contains("There is a tree here."),
        "the engine's look parts beside the cursor\n{shown}"
    );
    app.handle_key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE));
    let sent = h.sent();
    let operation = sent
        .iter()
        .rev()
        .find(|m| m["method"] == "bn.command.submit")
        .map(|m| m["params"]["operation"].clone())
        .unwrap();
    assert_eq!(operation["affordance_id"], "RIGHT", "{operation}");
}

#[test]
fn native_editmap_shows_the_engines_parts_and_its_edit_key_runs_the_engines_affordance() {
    let (mut app, h) = engine();
    text(&mut app);
    for name in ["events_41_walk", "events_42_message"] {
        h.push(fixture(name));
    }
    let mut menu = fixture("events_43_menu");
    let change = &mut menu["params"]["events"][0]["changes"]["interaction"];
    change["actions"] = json!([]);
    change["interaction"] = Value::Null;
    change["stack"] = json!([
        {"screen_id":"screen:map","role":"map","root":null,"affordances":[]},
        {"screen_id":"screen:7","role":"dialog","title":"Edit map",
         "root":{"id":"editmap","kind":"panel","title":"Edit map","child":{"id":"editmap:map",
             "kind":"map","affordances":["target:move"],"mode":"world",
             "target":{"source":{"dim":"","x":2,"y":4,"z":0},"current":{"dim":"","x":3,"y":4,"z":0},
                 "range":20,"distance_metric":"square","status":"edit","candidates":[],
                 "parts":[{"kind":"terrain","text":"Grass","id":"t_grass"}]}}},
         "affordances":[{"id":"EDIT_TERRAIN","label":"Edit terrain","keys":["e"],"enabled":true,
              "args":{},"expect":"screen"}]}]);
    h.push(menu);
    app.model.poll();
    let shown = text(&mut app);
    assert!(
        shown.contains("Grass"),
        "the engine's editmap parts\n{shown}"
    );
    app.handle_key(KeyEvent::new(KeyCode::Char('e'), KeyModifiers::NONE));
    let operation = h
        .sent()
        .iter()
        .rev()
        .find(|m| m["method"] == "bn.command.submit")
        .map(|m| m["params"]["operation"].clone())
        .unwrap();
    assert_eq!(operation["affordance_id"], "EDIT_TERRAIN", "{operation}");
}
