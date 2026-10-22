//! Needs a real engine build: `BN_ENGINE=/path/to/cataclysm-bn-tiles cargo test -p bn-client-core --test it live_inventory:: -- --ignored --test-threads=1`
use bn_client_core::protocol::InvokeArgs;
use bn_client_core::transport::StdioTransport;
use bn_client_core::watchdog::Watchdog;
use bn_client_core::{EngineModel, GameModel, MenuModel};
use std::time::Duration;

fn wait_for(model: &mut EngineModel, what: &str, mut done: impl FnMut(&EngineModel) -> bool) {
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

/// The tutorial greets and teaches with pop-ups; each closes with its only choice.
fn dismiss_popups(model: &mut EngineModel) {
    while let Some(menu) = model.menu() {
        let id = menu.choices.first().map(|c| c.id.clone());
        match id {
            Some(id) if !menu.can_cancel => model.choose(&id),
            _ => model.cancel_menu(),
        }
        wait_for(model, "popup closing", |m| !m.busy());
        std::thread::sleep(Duration::from_millis(500));
        model.poll();
    }
}

fn start_tutorial() -> EngineModel {
    let engine = std::env::var_os("BN_ENGINE").expect("set BN_ENGINE");
    let transport = StdioTransport::spawn_fresh(engine.as_ref()).expect("engine starts");
    let mut model = EngineModel::new(Box::new(transport));
    wait_for(&mut model, "subscription", |m| m.live());
    crate::common::start_tutorial(&mut model);
    wait_for(&mut model, "the tutorial map", |m| m.size().x > 0);
    wait_for(&mut model, "idle", |m| !m.busy());
    dismiss_popups(&mut model);
    model
}

/// Waits out the command in flight and the follow-up the model sends itself.
fn settle(model: &mut EngineModel) {
    wait_for(model, "idle", |m| !m.busy());
    std::thread::sleep(Duration::from_millis(1500));
    model.poll();
    wait_for(model, "idle", |m| !m.busy());
}

#[test]
#[ignore = "requires BN_ENGINE"]
fn live_pick_up_a_chosen_ground_item_wield_it_and_drop_it() {
    let mut model = start_tutorial();
    crate::common::step(&mut model, 1, 0);
    wait_for(&mut model, "idle", |m| !m.busy());
    dismiss_popups(&mut model);
    assert_eq!(model.inventory().len(), 0, "{:?}", model.inventory());
    let item = model.ground()[0].clone();
    assert!(
        bn_client_core::markup::plain(&item).starts_with("lighter ("),
        "a display name, not the id: {item}"
    );

    // The tutorial avatar has no storage: the native pickup answers with its own wield offer.
    model.pick_up(&model.ground_ids()[0].clone());
    settle(&mut model);
    let menu = model.menu().expect("the native capacity offer");
    assert!(menu.message.contains("Not enough capacity"), "{menu:?}");
    let wield = menu
        .choices
        .iter()
        .find(|c| c.label.starts_with("Wield"))
        .expect("wield offer");
    model.choose(&wield.id.clone());
    settle(&mut model);
    dismiss_popups(&mut model);
    assert_eq!(model.inventory(), [format!("{item} (wielded)")]);
    assert!(model.ground().is_empty(), "{:?}", model.ground());

    model.drop_item(&model.inventory_ids()[0].clone());
    settle(&mut model);
    println!("MENU {:?}", model.menu());
    dismiss_popups(&mut model);
    println!("INV {:?} GROUND {:?}", model.inventory(), model.ground());
    assert!(model.inventory().is_empty());
    assert_eq!(model.ground(), [item]);
}

#[test]
#[ignore = "needs a real engine"]
fn live_advanced_inventory_is_a_screen_of_areas_and_two_panes_and_the_main_menu_names_options() {
    let mut model = start_tutorial();
    assert!(model.invoke_action("advinv"));
    settle(&mut model);
    let screen = model
        .screens()
        .last()
        .cloned()
        .expect("the advanced inventory");
    let root = screen.root.expect("described by components");
    for id in [
        "aim:pane:left:areas",
        "aim:pane:right:areas",
        "aim:pane:left:items",
        "aim:pane:right:items",
    ] {
        assert!(root.find(id).is_some(), "{id}");
    }
    let offered = |id: &str| screen.affordances.iter().any(|a| a.id == id);
    assert!(offered("aim:move") && offered("aim:set_area") && offered("aim:close"));
    model
        .invoke_affordance("aim:close", InvokeArgs::default())
        .expect("close is offered");
    wait_for(&mut model, "the screen to close", |m| {
        m.screens()
            .last()
            .is_some_and(|s| s.context.as_deref() != Some("ADVANCED_INVENTORY"))
    });
    assert!(model.invoke_action("main_menu"));
    settle(&mut model);
    let menu = model.menu().expect("the in-game main menu");
    let options = menu.choices.iter().find(|c| c.action == "open_options");
    assert_eq!(options.map(|c| c.label.as_str()), Some("Options"));
}
