//! Needs a real engine build: `BN_ENGINE=/path/to/cataclysm-bn-tiles cargo test --test live_settings -- --ignored`
//! Edits the rule screens of the Settings tab through the client model, in a fresh temporary profile.
mod common;
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

#[test]
#[ignore = "requires BN_ENGINE pointing at an engine binary"]
fn live_rule_screens_add_edit_move_and_remove_rules_and_colors_pick_a_custom_color() {
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

    // Colors: choosing a color opens the native list; picking one sets the custom color.
    open(&mut model, "settings:colors", "COLORS");
    let first = model
        .menu()
        .unwrap()
        .choices
        .into_iter()
        .find(|c| c.id.starts_with("color:"))
        .unwrap();
    assert_eq!(first.columns[0].value, "default");
    choose(&mut model, &first.id);
    assert_ne!(context(&model), "COLORS", "the color list is open");
    let pick = model.menu().unwrap().choices[1].id.clone();
    choose(&mut model, &pick);
    wait_for(&mut model, "the colors screen", |m| context(m) == "COLORS");
    assert_ne!(value(&model, &first.id, 0), "default");
    leave(&mut model);
}
