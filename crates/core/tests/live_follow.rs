//! Needs a real engine build: `BN_ENGINE=... cargo test -p bn-client-core --test live_follow -- --ignored --nocapture`
//! Walks ten steps each way on the tutorial map and prints where the player is in the view.
mod common;
use bn_client_core::transport::StdioTransport;
use bn_client_core::watchdog::Watchdog;
use bn_client_core::{EngineModel, GameModel, MenuModel, Pos};
use std::time::Duration;

fn wait_for(model: &mut EngineModel, what: &str, mut done: impl FnMut(&EngineModel) -> bool) {
    let mut watchdog = Watchdog::new(Duration::from_secs(400), Duration::from_secs(1500));
    while !done(model) {
        assert!(
            !watchdog.expired(model.received()),
            "timed out waiting for {what}"
        );
        model.poll();
        if let Some(menu) = model.menu().filter(|m| m.context == "DEBUG_MSG")
            && !model.busy()
        {
            let id = menu.choices.first().map(|c| c.id.clone());
            id.iter().for_each(|id| model.choose(id));
        }
        std::thread::sleep(Duration::from_millis(20));
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

#[test]
#[ignore = "requires BN_ENGINE"]
fn live_player_stays_centred_while_walking() {
    let engine = std::env::var_os("BN_ENGINE").expect("set BN_ENGINE");
    let transport = StdioTransport::spawn_fresh(engine.as_ref()).expect("engine starts");
    let mut model = EngineModel::new(Box::new(transport));
    wait_for(&mut model, "the main menu", |m| m.live());
    common::start_tutorial(&mut model);
    model.set_viewport(Pos { x: 41, y: 26 });
    wait_for(&mut model, "the map", |m| m.size().x > 0);
    wait_for(&mut model, "idle", |m| !m.busy());
    for _ in 0..40 {
        model.poll();
        std::thread::sleep(Duration::from_millis(50));
    }
    dismiss_popups(&mut model);
    for (name, dx, dy) in [("E", 1, 0), ("W", -1, 0), ("S", 0, 1), ("N", 0, -1)] {
        for n in 0..10 {
            dismiss_popups(&mut model);
            let sent = common::step(&mut model, dx, dy);
            wait_for(&mut model, "step", |m| !m.busy());
            model.poll();
            let (size, me) = (model.size(), model.player());
            assert!(sent, "{name}{n}");
            assert_eq!(
                me,
                Pos {
                    x: size.x / 2,
                    y: size.y / 2
                },
                "{name}{n}"
            );
        }
    }
}
