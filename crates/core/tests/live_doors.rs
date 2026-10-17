//! Needs a real engine build: `BN_ENGINE=/path/to/cataclysm-bn-tiles cargo test -p bn-client-core --test live_doors -- --ignored --nocapture`
mod common;
use bn_client_core::transport::{Profile, StdioTransport};
use bn_client_core::watchdog::Watchdog;
use bn_client_core::{EngineModel, GameModel, MenuModel, Pos};
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

/// The tutorial's wall row has closed doors; clicking the one next to the avatar opens it.
#[test]
#[ignore = "requires BN_ENGINE pointing at an engine binary"]
fn live_engine_click_on_an_adjacent_closed_door_opens_it() {
    let engine = std::env::var_os("BN_ENGINE").expect("set BN_ENGINE");
    let t = StdioTransport::spawn_fresh(engine.as_ref()).expect("engine starts");
    let mut m = EngineModel::new(Box::new(t));
    wait_for(&mut m, "subscription", |m| m.live());
    common::start_tutorial(&mut m);
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
        if m.cell(below(&m)).id == "t_door_c" {
            break;
        }
        wait_for(&mut m, "idle", |m| !m.busy());
        m.press("j");
        wait_for(&mut m, "the step", |m| !m.busy());
        dismiss_popups(&mut m);
    }
    let door = below(&m);
    assert_eq!(m.cell(door).id, "t_door_c", "found a closed door below");
    wait_for(&mut m, "idle", |m| !m.busy());
    m.travel(door);
    wait_for(&mut m, "the click", |m| !m.busy());
    dismiss_popups(&mut m);
    wait_for(&mut m, "the door to open", |m| {
        m.cell(door).id == "t_door_o"
    });
}

/// Starts the tutorial, walks to the first door and dismisses every popup on the way, as the
/// player does; `BN_REPEAT` runs (default 20) on a fresh engine each. A run ends the engine
/// early when the model reports a failure, and says why.
#[test]
#[ignore = "requires BN_ENGINE pointing at an engine binary; one tutorial start takes about a minute"]
fn live_engine_survives_the_door_popup_chain_repeatedly() {
    let engine = std::env::var_os("BN_ENGINE").expect("set BN_ENGINE");
    let runs = std::env::var("BN_REPEAT")
        .ok()
        .and_then(|n| n.parse::<u32>().ok())
        .unwrap_or(20);
    for run in 1..=runs {
        // `BN_SHARED_PROFILE=<dir>` makes every run, and every test process given the same
        // directory, share one user and config directory, as the GUI and the live suites do.
        let t = match std::env::var_os("BN_SHARED_PROFILE").map(std::path::PathBuf::from) {
            Some(dir) => {
                let profile = Profile {
                    userdir: Some(dir.join("user")),
                    configdir: Some(dir.join("config")),
                    ..Profile::default()
                };
                std::fs::create_dir_all(dir.join("user")).unwrap();
                std::fs::create_dir_all(dir.join("config")).unwrap();
                StdioTransport::spawn_with(engine.as_ref(), &profile)
            }
            None => StdioTransport::spawn_fresh(engine.as_ref()),
        }
        .expect("engine starts");
        let mut m = EngineModel::new(Box::new(t));
        wait_for(&mut m, "subscription", |m| m.live());
        common::start_tutorial(&mut m);
        wait_for(&mut m, "the tutorial map", |m| m.size().x > 0);
        wait_for(&mut m, "idle", |m| !m.busy());
        for step in 0..14 {
            dismiss_popups(&mut m);
            assert!(
                m.failure().is_none(),
                "run {run} step {step}: {:?}",
                m.failure()
            );
            wait_for(&mut m, "idle", |m| !m.busy());
            m.press("j");
            wait_for(&mut m, "the step", |m| !m.busy() || m.failure().is_some());
            std::thread::sleep(Duration::from_millis(300));
            m.poll();
            assert!(
                m.failure().is_none(),
                "run {run} step {step}: {:?}",
                m.failure()
            );
        }
        dismiss_popups(&mut m);
        assert!(m.failure().is_none(), "run {run}: {:?}", m.failure());
    }
}
