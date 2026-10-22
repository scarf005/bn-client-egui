//! Needs a real engine build: `BN_ENGINE=/path/to/cataclysm-bn-tiles cargo test -p bn-client-core --test it live_map_keys:: -- --ignored --nocapture`
use bn_client_core::transport::StdioTransport;
use bn_client_core::watchdog::Watchdog;
use bn_client_core::{EngineModel, GameModel, MenuModel};
use std::time::Duration;

fn wait_for(model: &mut EngineModel, what: &str, mut done: impl FnMut(&EngineModel) -> bool) {
    let mut watchdog = Watchdog::live();
    while !done(model) {
        assert!(
            model.failure().is_none(),
            "failed while waiting for {what}: {:?}",
            model.failure()
        );
        assert!(
            !watchdog.expired(model.received()),
            "timed out waiting for {what}: {:?} {:?}",
            model.messages(),
            model.failure()
        );
        model.poll();
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn dismiss_popups(model: &mut EngineModel) {
    while let Some(menu) = model.menu() {
        let id = menu.choices.first().map(|c| c.id.clone());
        match id {
            Some(id) if !menu.can_cancel => model.choose(&id),
            _ => model.cancel_menu(),
        }
        wait_for(model, "popup", |m| !m.busy());
        std::thread::sleep(Duration::from_millis(500));
        model.poll();
    }
}

/// At the tutorial map: `m` after the lessons are out of the way.
fn at_the_map() -> EngineModel {
    let engine = std::env::var_os("BN_ENGINE").expect("set BN_ENGINE");
    let t = StdioTransport::spawn_fresh(engine.as_ref()).expect("engine starts");
    let mut m = EngineModel::new(Box::new(t));
    wait_for(&mut m, "subscription", |m| m.live());
    crate::common::start_tutorial(&mut m);
    wait_for(&mut m, "the tutorial map", |m| m.size().x > 0);
    wait_for(&mut m, "idle", |m| !m.busy());
    dismiss_popups(&mut m);
    m
}

/// Six repeats of the south key sent without waiting for any step all run: the engine queues them.
#[test]
#[ignore = "requires BN_ENGINE pointing at an engine binary"]
fn live_engine_a_held_key_queues_its_moves_and_they_all_complete() {
    let mut m = at_the_map();
    // The view follows the avatar, so its square is the view's origin plus its cell.
    let square = |m: &EngineModel| (m.origin().x + m.player().x, m.origin().y + m.player().y);
    // The first step of the tutorial opens its welcome lesson; a queued move would meet it (`stale_screen`).
    m.press("k");
    wait_for(&mut m, "the first step", |m| !m.busy());
    dismiss_popups(&mut m);
    let start = square(&m);
    let sent = (0..6).filter(|_| m.press("j")).count();
    wait_for(&mut m, "the moves", |m| !m.busy());
    let end = square(&m);
    assert_eq!(sent, 6, "every repeat was accepted");
    assert!(
        end.1 - start.1 >= 5,
        "{start:?} to {end:?}; menu {:?}; messages {:?}",
        m.menu().map(|x| (x.context, x.message)),
        m.messages()
    );
}

/// Options opens from the map through its affordance, with no detour through the main menu.
#[test]
#[ignore = "requires BN_ENGINE pointing at an engine binary"]
fn live_engine_options_open_from_the_map_through_the_affordance() {
    let mut m = at_the_map();
    assert!(m.actions().iter().any(|a| a.id == "open_options"));
    assert!(m.invoke_action("open_options"));
    wait_for(&mut m, "the options", |m| {
        m.menu().is_some_and(|menu| menu.context == "OPTIONS")
    });
}
