//! Needs a real engine build: `BN_ENGINE=/path/to/cataclysm-bn-tiles cargo test --test live_settings -- --ignored`
//! Edits the rule screens of the Settings tab through the client model, in a fresh temporary profile: the
//! auto pickup and safe mode rules through the 0.1 menu, the distraction and colour managers through the
//! affordances of their component screens.
mod common;
use bn_client_core::protocol::{Component, InvokeArgs, Row};
use bn_client_core::transport::StdioTransport;
use bn_client_core::watchdog::Watchdog;
use bn_client_core::{EngineModel, GameModel, MenuChoice};
use std::time::Duration;

fn wait_for(model: &mut EngineModel, what: &str, mut done: impl FnMut(&EngineModel) -> bool) {
    let mut watchdog = Watchdog::live();
    while !done(model) {
        assert!(
            !watchdog.expired(model.received()),
            "timed out waiting for {what}: {:?}",
            model.menu().map(|m| (m.context, m.title))
        );
        model.poll();
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn settled(model: &mut EngineModel, what: &str) {
    wait_for(model, what, |m| !m.busy());
    model.poll();
}

fn context(model: &EngineModel) -> String {
    model.menu().map(|m| m.context).unwrap_or_default()
}

fn row(model: &EngineModel, id: &str) -> Option<MenuChoice> {
    model.menu()?.choices.into_iter().find(|c| c.id == id)
}

fn choose(model: &mut EngineModel, id: &str) {
    wait_for(model, id, |m| row(m, id).is_some());
    settled(model, "the previous command");
    model.choose(id);
    settled(model, id);
}

fn act(model: &mut EngineModel, id: &str) {
    settled(model, "the previous command");
    assert!(model.invoke_action(id), "{id} is offered");
    settled(model, id);
}

fn fill(model: &mut EngineModel, field: &str, value: &str) {
    settled(model, "the previous command");
    model.fill(field, value, true);
    settled(model, field);
}

fn open(model: &mut EngineModel, entry: &str, ctx: &str) {
    common::main_menu_tab(model, "settings");
    common::main_menu_entry(model, entry);
    wait_for(model, ctx, |m| context(m) == ctx);
}

fn leave(model: &mut EngineModel) {
    settled(model, "the previous command");
    model.cancel_menu();
    settled(model, "leaving");
    // A changed screen asks to save; answering yes returns to the main menu.
    if common::main_menu_tabs(model).is_empty() {
        let yes = model
            .menu()
            .and_then(|m| m.choices.into_iter().find(|c| c.label == "Yes"))
            .expect("save prompt");
        choose(model, &yes.id);
    }
    wait_for(model, "the main menu", |m| {
        !common::main_menu_tabs(m).is_empty()
    });
}

fn value(model: &EngineModel, id: &str, column: usize) -> String {
    row(model, id)
        .map(|r| r.columns[column].value.clone())
        .unwrap_or_default()
}

/// A fresh engine on the main menu.
fn start() -> EngineModel {
    let engine = std::env::var_os("BN_ENGINE").expect("set BN_ENGINE");
    let transport = StdioTransport::spawn_fresh(engine.as_ref()).expect("engine starts");
    let mut model = EngineModel::new(Box::new(transport));
    wait_for(&mut model, "subscription", |m| m.live());
    // Startup prompts close with their only choice.
    while common::main_menu_tabs(&model).is_empty() {
        wait_for(&mut model, "a menu", |m| {
            m.menu().is_some() || !common::main_menu_tabs(m).is_empty()
        });
        if !common::main_menu_tabs(&model).is_empty() {
            break;
        }
        let id = model.menu().unwrap().choices[0].id.clone();
        choose(&mut model, &id);
    }

    model
}

/// Opens a Settings entry whose screen is made of components, once it is on top.
fn open_screen(model: &mut EngineModel, entry: &str, ctx: &str) {
    common::main_menu_tab(model, "settings");
    common::main_menu_entry(model, entry);
    wait_for(model, ctx, |m| top_context(m) == ctx);
}

/// The top screen's context, for diagnostics.
fn top_context(model: &EngineModel) -> String {
    model
        .screens()
        .last()
        .and_then(|s| s.context.clone())
        .unwrap_or_default()
}

/// Runs the affordance of the top screen on the row `row` (or with the text `text`) and waits for its command.
fn invoke(model: &mut EngineModel, affordance: &str, row: Option<&str>, text: Option<&str>) {
    settled(model, "the previous command");
    let args = InvokeArgs {
        row: row.map(Into::into),
        text: text.map(Into::into),
        ..Default::default()
    };
    model
        .invoke_affordance(affordance, args)
        .unwrap_or_else(|e| panic!("{affordance} on {row:?}: {e:?}"));
    settled(model, affordance);
}

/// The rows of the list `id` on the top screen.
fn rows_of(model: &EngineModel, id: &str) -> Vec<Row> {
    let list = model.screens().last().and_then(|s| s.root.as_ref()?.find(id));
    match list {
        Some(Component::List(list)) => model.rows(list).into_iter().cloned().collect(),
        other => panic!("no list {id}: {other:?}"),
    }
}

/// The value of the column `key` of the row `id` of the list `list`.
fn cell(model: &EngineModel, list: &str, id: &str, key: &str) -> String {
    let row = rows_of(model, list).into_iter().find(|r| r.id == id);
    let column = row.and_then(|r| r.columns.into_iter().find(|c| c.key.as_deref() == Some(key)));
    column.map(|c| c.value.shown()).unwrap_or_default()
}

/// Leaves the top screen with its cancel action, answering the save question with its Yes.
fn quit(model: &mut EngineModel) {
    invoke(model, "QUIT", None, None);
    if common::main_menu_tabs(model).is_empty() && model.menu().is_some() {
        let yes = model
            .menu()
            .and_then(|m| m.choices.into_iter().find(|c| c.label == "Yes"))
            .expect("save prompt");
        choose(model, &yes.id);
    }
    wait_for(model, "the main menu", |m| {
        !common::main_menu_tabs(m).is_empty()
    });
}

#[test]
#[ignore = "requires BN_ENGINE pointing at an engine binary"]
fn live_rule_screens_add_edit_move_and_remove_rules() {
    let mut model = start();

    // Auto pickup: add a rule, type its pattern, add a second, move it up, remove it.
    open(&mut model, "settings:autopickup", "AUTO_PICKUP");
    act(&mut model, "ADD_RULE");
    fill(&mut model, "field:value", "*knife*");
    assert_eq!(context(&model), "AUTO_PICKUP");
    assert_eq!(row(&model, "rule:0").unwrap().label, "*knife*");
    act(&mut model, "ADD_RULE");
    fill(&mut model, "field:value", "*axe*");
    assert_eq!(row(&model, "rule:1").unwrap().label, "*axe*");
    act(&mut model, "MOVE_RULE_DOWN");
    assert_eq!(row(&model, "rule:0").unwrap().label, "*axe*");
    choose(&mut model, "column:2");
    choose(&mut model, "rule:0");
    assert_eq!(value(&model, "rule:0", 0), "Exclude");
    act(&mut model, "REMOVE_RULE");
    assert!(row(&model, "rule:1").is_none());
    leave(&mut model);

    // Safe mode: an empty rule is added, its pattern typed, its list kind switched.
    open(&mut model, "settings:safemode", "SAFEMODE");
    act(&mut model, "ADD_RULE");
    choose(&mut model, "column:0");
    choose(&mut model, "rule:0");
    fill(&mut model, "field:value", "zombie");
    assert_eq!(row(&model, "rule:0").unwrap().label, "zombie");
    choose(&mut model, "column:3");
    choose(&mut model, "rule:0");
    assert_eq!(value(&model, "rule:0", 2), "Whitelist");
    act(&mut model, "REMOVE_RULE");
    assert!(row(&model, "rule:0").is_none());
    leave(&mut model);

}

#[test]
#[ignore = "requires BN_ENGINE pointing at an engine binary"]
fn live_distraction_and_color_managers_are_driven_by_their_affordances_by_id() {
    let mut model = start();

    // Distractions: a distraction is switched off by its row, kept when the screen is left, and shown after a
    // restart of the screen.
    open_screen(&mut model, "settings:distractions", "DISTRACTION_MANAGER");
    assert_eq!(top_context(&model), "DISTRACTION_MANAGER");
    let distractions = rows_of(&model, "distractions:items");
    assert!(distractions.len() >= 8 && distractions.iter().all(|r| r.toggled == Some(true)));
    let target = distractions[1].id.clone();
    invoke(&mut model, "distractions:toggle", Some(&target), None);
    assert_eq!(
        cell(&model, "distractions:items", &target, "status"),
        "Disabled"
    );
    quit(&mut model);
    open_screen(&mut model, "settings:distractions", "DISTRACTION_MANAGER");
    let again = rows_of(&model, "distractions:items");
    assert_eq!(again[1].toggled, Some(false));
    assert_eq!(again[0].toggled, Some(true));
    quit(&mut model);

    // Colors: the custom color of the invert column is set through the editor beside the list, by the names of
    // the colors, and kept by the save the screen asks for when it is left.
    open_screen(&mut model, "settings:colors", "COLORS");
    let colors = rows_of(&model, "colors:items");
    let (first, second) = (colors[0].clone(), colors[1].clone());
    assert_eq!(cell(&model, "colors:items", &second.id, "invert_custom"), "");
    invoke(&mut model, "colors:column", Some("invert"), None);
    invoke(&mut model, "colors:select", Some(&second.id), None);
    let name = first.label.shown();
    invoke(&mut model, "colors:set", Some(&second.id), Some(&name));
    assert_eq!(cell(&model, "colors:items", &second.id, "invert_custom"), name);
    // The activation opens the native list of colors over the screen; leaving it returns to the screen.
    invoke(&mut model, "colors:activate", Some(&second.id), None);
    assert_ne!(top_context(&model), "COLORS", "the color list is open");
    settled(&mut model, "the color list");
    model.cancel_menu();
    wait_for(&mut model, "the colors screen", |m| top_context(m) == "COLORS");
    quit(&mut model);
    open_screen(&mut model, "settings:colors", "COLORS");
    assert_eq!(cell(&model, "colors:items", &second.id, "invert_custom"), name);
    quit(&mut model);
}
