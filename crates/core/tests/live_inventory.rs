//! Needs a real engine build: `BN_ENGINE=/path/to/cataclysm-bn-tiles cargo test -p bn-client-core --test live_inventory -- --ignored --test-threads=1`
use bn_client_core::transport::StdioTransport;
use bn_client_core::watchdog::Watchdog;
use bn_client_core::{EngineModel, GameModel};
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

fn choose_label(model: &mut EngineModel, label: &str) {
    wait_for(model, label, |m| {
        m.menu()
            .is_some_and(|menu| menu.choices.iter().any(|c| c.label.contains(label)))
    });
    let id = model
        .menu()
        .and_then(|m| m.choices.into_iter().find(|c| c.label.contains(label)))
        .expect("choice")
        .id;
    wait_for(model, "the previous command", |m| !m.busy());
    model.choose(&id);
    wait_for(model, &format!("{label} to finish"), |m| !m.busy());
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
    choose_label(&mut model, "New Game");
    choose_label(&mut model, "Tutorial");
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
    use bn_client_core::Pos;
    let mut model = start_tutorial();
    let me = model.player();
    model.step_toward(Pos {
        x: me.x + 1,
        y: me.y,
    });
    wait_for(&mut model, "idle", |m| !m.busy());
    dismiss_popups(&mut model);
    assert_eq!(model.inventory().len(), 0, "{:?}", model.inventory());
    let item = model.ground()[0].clone();
    assert!(
        item.starts_with("lighter ("),
        "a display name, not the id: {item}"
    );

    // The tutorial avatar has no storage: the native pickup answers with its own wield offer.
    model.pick_up(0);
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

    model.drop_item(0);
    settle(&mut model);
    println!("MENU {:?}", model.menu());
    dismiss_popups(&mut model);
    println!("INV {:?} GROUND {:?}", model.inventory(), model.ground());
    assert!(model.inventory().is_empty());
    assert_eq!(model.ground(), [item]);
}

#[test]
#[ignore = "requires BN_ENGINE"]
fn live_pickup_list_has_the_carry_header_a_cursor_row_marks_and_the_confirm_action() {
    use bn_client_core::Pos;
    let mut model = start_tutorial();
    let me = model.player();
    model.step_toward(Pos {
        x: me.x + 1,
        y: me.y,
    });
    wait_for(&mut model, "idle", |m| !m.busy());
    dismiss_popups(&mut model);
    assert!(model.invoke_action("pickup"));
    settle(&mut model);
    let menu = model.menu().expect("the native pickup list");
    assert_eq!(menu.context, "PICKUP");
    assert!(menu.message.starts_with("PICK Wgt "), "{}", menu.message);
    assert!(menu.message.contains("Vol "), "{}", menu.message);
    let row = &menu.choices[0];
    assert!(!row.selected, "{row:?}");
    assert!(row.description.contains("Weight:"), "{}", row.description);

    model.choose(&row.id.clone());
    settle(&mut model);
    let menu = model.menu().expect("still the pickup list");
    assert!(menu.choices[0].selected, "the row is marked");
    assert!(model.actions().iter().any(|a| a.id == "CONFIRM"));
    assert!(model.invoke_action("CONFIRM"));
}
