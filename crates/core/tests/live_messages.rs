//! Needs a real engine build: `BN_ENGINE=/path/to/cataclysm-bn-tiles cargo test -p bn-client-core --test live_messages -- --ignored --nocapture`
use bn_client_core::transport::StdioTransport;
use bn_client_core::{EngineModel, GameModel};
use std::time::{Duration, Instant};

fn wait_for(model: &mut EngineModel, what: &str, mut done: impl FnMut(&EngineModel) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(90);
    while !done(model) {
        assert!(
            Instant::now() < deadline,
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
        .unwrap()
        .choices
        .iter()
        .find(|c| c.label.contains(label))
        .unwrap()
        .id
        .clone();
    wait_for(model, "idle", |m| !m.busy());
    model.choose(&id);
    wait_for(model, "choice", |m| !m.busy());
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

/// The game prints "You can't ..." when the avatar tries to go up where nothing leads up; asked twice
/// in a row the native log counts the repeat, and the client shows one line with "x2".
#[test]
#[ignore = "requires BN_ENGINE pointing at an engine binary"]
fn live_engine_message_log_shows_native_lines_with_repeat_counts() {
    let engine = std::env::var_os("BN_ENGINE").expect("set BN_ENGINE");
    let t = StdioTransport::spawn(engine.as_ref()).expect("engine starts");
    let mut m = EngineModel::new(Box::new(t));
    wait_for(&mut m, "subscription", |m| m.live());
    choose_label(&mut m, "New Game");
    choose_label(&mut m, "Tutorial");
    wait_for(&mut m, "the tutorial map", |m| m.size().x > 0);
    wait_for(&mut m, "idle", |m| !m.busy());
    dismiss_popups(&mut m);

    let before = m.native_log().len();
    for _ in 0..2 {
        wait_for(&mut m, "idle", |m| !m.busy());
        assert!(m.invoke_action("LEVEL_UP"));
        wait_for(&mut m, "the key", |m| !m.busy());
    }
    wait_for(&mut m, "the repeated native line", |m| {
        m.native_log().last().is_some_and(|l| l.count == 2)
    });
    let log = m.native_log();
    assert_eq!(log.len(), before + 1, "the repeat replaced its line");
    let line = log.last().unwrap();
    assert!(line.shown().starts_with("You can't"), "{}", line.shown());
    assert!(line.shown().ends_with(" x2"), "{}", line.shown());
    assert!(
        line.color.is_some(),
        "the game's colour for the message type"
    );
    assert!(
        m.messages().iter().all(|s| !s.starts_with("You can't")),
        "native lines stay out of the command lines: {:?}",
        m.messages()
    );
}
