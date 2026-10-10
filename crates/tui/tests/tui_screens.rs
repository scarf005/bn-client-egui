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
        shown.contains("[x] second thing"),
        "a row the screen marks as on\n{shown}"
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
screen!(mutation_list, "MUTATIONS");
screen!(armor_layers, "SORT_ARMOR", tabs);
screen!(zone_manager, "ZONES_MANAGER");
screen!(faction_manager, "FACTION MANAGER", tabs);
screen!(scores, "SCORES", tabs);
screen!(diary, "DIARY");
screen!(auto_notes, "AUTO_NOTES");
screen!(panel_manager, "PANEL_MGMT");
screen!(active_world_mods, "DEFAULT");
screen!(mod_manager, "MODMANAGER_DIALOG", tabs);

#[test]
fn a_click_outside_the_rows_of_a_menu_sends_nothing() {
    let (mut app, h) = engine();
    render(&mut app);
    open(&mut app, &h, "UILIST", false);
    render(&mut app);
    app.handle_mouse(click(0, 0));
    assert!(operations(&h).is_empty());
}
