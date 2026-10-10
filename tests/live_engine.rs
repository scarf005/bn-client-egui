//! Needs a real engine build: `BN_ENGINE=/path/to/cataclysm-bn-tiles cargo test --test live_engine -- --ignored`
use bn_client_egui::transport::StdioTransport;
use bn_client_egui::{EngineModel, GameModel, Pos};
use std::time::{Duration, Instant};

fn wait_for(model: &mut EngineModel, what: &str, done: impl Fn(&EngineModel) -> bool) {
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
    let menu = model.menu().expect("menu");
    let id = menu
        .choices
        .iter()
        .find(|c| c.label.contains(label))
        .expect("choice")
        .id
        .clone();
    // Input is ignored while a command is in flight, so wait for the previous one to finish.
    wait_for(model, "the previous command", |m| !m.busy());
    model.choose(&id);
    assert!(model.busy(), "{label} was submitted");
    wait_for(model, &format!("{label} to finish"), |m| !m.busy());
}

#[test]
#[ignore = "requires BN_ENGINE pointing at an engine binary"]
fn live_engine_starts_the_tutorial_and_walks_one_step() {
    let engine = std::env::var_os("BN_ENGINE").expect("set BN_ENGINE");
    let transport = StdioTransport::spawn(engine.as_ref()).expect("engine starts");
    let mut model = EngineModel::new(Box::new(transport));

    wait_for(&mut model, "subscription", |m| m.live());
    choose_label(&mut model, "New Game");
    choose_label(&mut model, "Tutorial");
    wait_for(&mut model, "the tutorial map", |m| m.size().x > 0);

    let start = model.player();
    let target = Pos {
        x: start.x + 1,
        y: start.y,
    };
    assert!(model.step_toward(target), "movement was submitted");
    wait_for(&mut model, "one step east", |m| m.player() != start);
    assert_eq!(model.player(), target);
}
