//! Needs a real engine build: `BN_ENGINE=/path/to/cataclysm-bn-tiles BN_ROOT=/path/to/game cargo test -p bn-client-core --test live_delivery -- --ignored --nocapture`
//! A one hour wait shows its turns as they pass instead of one jump when it ends.
use bn_client_core::transport::StdioTransport;
use bn_client_core::{EngineModel, GameModel};
use std::time::{Duration, Instant};

fn wait_for(model: &mut EngineModel, what: &str, mut done: impl FnMut(&mut EngineModel) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(300);
    while !done(model) {
        assert!(
            Instant::now() < deadline && model.failure().is_none(),
            "timed out waiting for {what}: {:?}",
            model.failure().map(|f| f.message)
        );
        model.poll();
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn choose_label(model: &mut EngineModel, label: &str) {
    wait_for(model, label, |m| {
        !m.busy()
            && m.menu()
                .is_some_and(|x| x.choices.iter().any(|c| c.label.starts_with(label)))
    });
    let id = model
        .menu()
        .and_then(|m| m.choices.into_iter().find(|c| c.label.starts_with(label)))
        .expect("choice")
        .id;
    model.choose(&id);
}

fn dismiss_popups(model: &mut EngineModel) {
    std::thread::sleep(Duration::from_secs(1));
    model.poll();
    while let Some(menu) = model.menu().filter(|m| m.context != "MAIN_MENU") {
        match menu.choices.first().map(|c| c.id.clone()) {
            Some(id) if !menu.can_cancel => model.choose(&id),
            _ => model.cancel_menu(),
        }
        wait_for(model, "popup closing", |m| !m.busy());
        std::thread::sleep(Duration::from_millis(500));
        model.poll();
    }
}

fn start_wait(model: &mut EngineModel, label: &str) {
    wait_for(model, "idle", |m| !m.busy());
    assert!(model.invoke_action("wait"), "wait is offered");
    choose_label(model, label);
}

#[test]
#[ignore = "requires BN_ENGINE pointing at an engine with delivery credits"]
fn live_engine_one_hour_wait_streams_its_turns_before_it_completes() {
    let engine = std::env::var_os("BN_ENGINE").expect("set BN_ENGINE");
    let transport = StdioTransport::spawn_fresh(engine.as_ref()).expect("engine starts");
    let mut model = EngineModel::new(Box::new(transport));
    wait_for(&mut model, "the main menu", |m| m.live());
    choose_label(&mut model, "New Game");
    choose_label(&mut model, "Tutorial");
    wait_for(&mut model, "the tutorial map", |m| {
        m.size().x > 0 && !m.busy()
    });
    dismiss_popups(&mut model);

    // The first turns raise tutorial pop-ups that would end a wait; let a short one pass first.
    start_wait(&mut model, "5 minutes");
    wait_for(&mut model, "the short wait", |m| !m.busy());
    dismiss_popups(&mut model);

    start_wait(&mut model, "1 hour");
    model.take_trace();
    let started = Instant::now();
    let mut frames = 0;
    wait_for(&mut model, "the wait to end", |m| {
        frames += m
            .take_trace()
            .iter()
            .filter(|(_, what)| *what == "events")
            .count();
        !m.busy()
    });
    println!(
        "WAIT wall {:?}, event frames while busy {frames}",
        started.elapsed()
    );
    assert!(
        frames >= 600,
        "turns reached the client while the wait ran: {frames}"
    );
}
