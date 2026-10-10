//! Needs a real engine build: `BN_ENGINE=/path/to/cataclysm-bn-tiles cargo test -p bn-client-core --test live_doors -- --ignored --nocapture`
use bn_client_core::transport::StdioTransport;
use bn_client_core::watchdog::Watchdog;
use bn_client_core::{EngineModel, GameModel, Pos};
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

/// The tutorial's wall row has closed doors; clicking the one next to the avatar opens it.
#[test]
#[ignore = "requires BN_ENGINE pointing at an engine binary"]
fn live_engine_click_on_an_adjacent_closed_door_opens_it() {
    let engine = std::env::var_os("BN_ENGINE").expect("set BN_ENGINE");
    let t = StdioTransport::spawn_fresh(engine.as_ref()).expect("engine starts");
    let mut m = EngineModel::new(Box::new(t));
    wait_for(&mut m, "subscription", |m| m.live());
    choose_label(&mut m, "New Game");
    choose_label(&mut m, "Tutorial");
    wait_for(&mut m, "the tutorial map", |m| m.size().x > 0);
    wait_for(&mut m, "idle", |m| !m.busy());
    dismiss_popups(&mut m);

    let below = |m: &EngineModel| {
        let p = m.player();
        Pos { x: p.x, y: p.y + 1 }
    };
    // Walk south with the game key until the square below is a closed door; the door row is dark
    // until the avatar is a few squares away.
    for _ in 0..40 {
        if m.cell(below(&m)).desc == "t_door_c" {
            break;
        }
        wait_for(&mut m, "idle", |m| !m.busy());
        m.press("j");
        wait_for(&mut m, "the step", |m| !m.busy());
        dismiss_popups(&mut m);
    }
    let door = below(&m);
    assert_eq!(m.cell(door).desc, "t_door_c", "found a closed door below");
    wait_for(&mut m, "idle", |m| !m.busy());
    assert!(m.travel(door));
    wait_for(&mut m, "the click", |m| !m.busy());
    dismiss_popups(&mut m);
    wait_for(&mut m, "the door to open", |m| {
        m.cell(door).desc == "t_door_o"
    });
}
