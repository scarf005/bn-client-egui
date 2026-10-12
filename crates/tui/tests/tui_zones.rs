//! The zone manager's key rows and the target screens that tell a zone's area step or the map editor's square.
mod common;
use bn_client_core::transport::ScriptHandle;
use bn_client_core::{EngineModel, GameModel};
use bn_client_tui::TuiApp;
use common::{connected, fixture, session};
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
