#![allow(dead_code)]
use bn_client_core::protocol::{Component, InvokeArgs};
use bn_client_core::transport::{ScriptHandle, ScriptedTransport};
use bn_client_core::watchdog::Watchdog;
use bn_client_core::{EngineModel, GameModel};
use serde_json::Value;
use std::time::Duration;

pub fn fixture(name: &str) -> Value {
    let text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/design.json"
    ))
    .unwrap();
    serde_json::from_str::<Value>(&text).unwrap()[name].clone()
}

/// The screen `bn.panel.view` answers for `panel` (`character` or `inventory`), from the engine's examples.
pub fn panel_view(panel: &str) -> Value {
    let path = format!(
        "{}/tests/fixtures/0.2.examples/{panel}.json",
        env!("CARGO_MANIFEST_DIR")
    );
    let all = serde_json::from_str::<Value>(&std::fs::read_to_string(path).unwrap()).unwrap();
    all["valid"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["name"] == format!("{panel} panel"))
        .unwrap()["value"]
        .clone()
}

pub fn subscribe_reply() -> Vec<Value> {
    ["subscribe_result", "snapshot_part_0", "snapshot_part_1"]
        .map(fixture)
        .to_vec()
}

/// Answers hello and subscribe with the fixture session; extra replies can be chained.
pub fn session() -> (ScriptedTransport, ScriptHandle) {
    let (t, h) = ScriptedTransport::new();
    (
        t.on("bn.hello", vec![fixture("hello_result")])
            .on("bn.subscribe", subscribe_reply()),
        h,
    )
}

/// A model that finished hello, subscribe and the snapshot.
pub fn connected(script: ScriptedTransport) -> EngineModel {
    let mut m = EngineModel::new(Box::new(script));
    m.poll();
    assert!(m.live());
    m
}

/// One step of the avatar by the engine's move action for the direction `(dx, dy)`.
pub fn step(model: &mut EngineModel, dx: i32, dy: i32) -> bool {
    let id = match (dx, dy) {
        (0, -1) => "UP",
        (1, -1) => "RIGHTUP",
        (1, 0) => "RIGHT",
        (1, 1) => "RIGHTDOWN",
        (0, 1) => "DOWN",
        (-1, 1) => "LEFTDOWN",
        (-1, 0) => "LEFT",
        _ => "LEFTUP",
    };
    model.invoke_action(id)
}

/// Polls the model until `done`; the engine is silent while it loads, so the watchdog is the live one.
pub fn wait_for(model: &mut EngineModel, what: &str, mut done: impl FnMut(&EngineModel) -> bool) {
    let mut watchdog = Watchdog::live();
    while !done(model) {
        assert!(
            !watchdog.expired(model.received()),
            "timed out waiting for {what}: {:?}",
            model.messages()
        );
        model.poll();
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// Submits `affordance` of the main menu on the row `row` (a tab or an entry, by id) once the menu offers it and
/// no command is in flight.
pub fn submit_main_menu(model: &mut EngineModel, affordance: &str, row: &str) {
    wait_for(model, affordance, |m| {
        m.screens()
            .last()
            .is_some_and(|s| s.affordances.iter().any(|a| a.id == affordance))
    });
    wait_for(model, "the previous command", |m| !m.busy());
    let args = InvokeArgs {
        row: Some(row.into()),
        ..Default::default()
    };
    model
        .invoke_affordance(affordance, args)
        .unwrap_or_else(|e| panic!("{affordance} on {row}: {e:?}"));
    assert!(model.busy(), "{row} was submitted");
}

/// Runs `affordance` of the main menu on `row` and waits for the command to finish.
pub fn main_menu(model: &mut EngineModel, affordance: &str, row: &str) {
    submit_main_menu(model, affordance, row);
    wait_for(model, row, |m| !m.busy());
}

/// The ids of the main menu's tabs, empty while the menu is not on top.
pub fn main_menu_tabs(model: &EngineModel) -> Vec<String> {
    let tabs = model
        .screens()
        .last()
        .and_then(|s| s.root.as_ref()?.find("menu:tabs"));
    match tabs {
        Some(Component::Tabs(tabs)) => tabs.tabs.iter().map(|t| t.id.clone()).collect(),
        _ => vec![],
    }
}

/// The ids of the rows the main menu lists for its selected tab.
pub fn main_menu_entries(model: &EngineModel) -> Vec<String> {
    let list = model
        .screens()
        .last()
        .and_then(|s| s.root.as_ref()?.find("menu:entries"));
    match list {
        Some(Component::List(list)) => model.rows(list).iter().map(|r| r.id.clone()).collect(),
        _ => vec![],
    }
}

/// Selects a tab of the main menu.
pub fn main_menu_tab(model: &mut EngineModel, tab: &str) {
    main_menu(model, "menu:tab", tab);
}

/// Runs an entry of the selected tab of the main menu.
pub fn main_menu_entry(model: &mut EngineModel, entry: &str) {
    main_menu(model, "menu:entry", entry);
}

/// Starts the tutorial from the main menu's New Game tab.
pub fn start_tutorial(model: &mut EngineModel) {
    main_menu_tab(model, "new_game");
    main_menu_entry(model, "new_game:tutorial");
}
