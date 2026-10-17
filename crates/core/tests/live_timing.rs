//! Needs a real engine build: `BN_ENGINE=/path/to/cataclysm-bn-tiles cargo test -p bn-client-core --test live_timing -- --ignored --nocapture`
//! Times starting the tutorial and loading its save, and what the loading screen showed each time.
mod common;
use bn_client_core::transport::StdioTransport;
use bn_client_core::watchdog::Watchdog;
use bn_client_core::{EngineModel, GameModel, MenuModel};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

#[derive(Default)]
struct Phase {
    loading_for: Duration,
    steps: usize,
}

/// Every `bn.loading` step seen, whichever wait was running when it arrived.
static STEPS: AtomicUsize = AtomicUsize::new(0);
static LOADING_MS: AtomicU64 = AtomicU64::new(0);

fn take_loading() -> (usize, Duration) {
    let steps = STEPS.swap(0, Ordering::Relaxed);
    let ms = LOADING_MS.swap(0, Ordering::Relaxed);
    (steps, Duration::from_millis(ms))
}

fn wait_for(
    model: &mut EngineModel,
    what: &str,
    mut done: impl FnMut(&EngineModel) -> bool,
) -> Phase {
    let mut watchdog = Watchdog::new(Duration::from_secs(400), Duration::from_secs(1500));
    let mut phase = Phase::default();
    let mut loading_since = None::<Instant>;
    while !done(model) {
        assert!(
            !watchdog.expired(model.received()),
            "timed out waiting for {what}: {:?}",
            model.menu().map(|m| (m.context, m.title, m.message))
        );
        model.poll();
        // The tutorial save raises a harmless debug message while loading; acknowledge it.
        if let Some(menu) = model.menu().filter(|m| m.context == "DEBUG_MSG")
            && !model.busy()
        {
            let first = menu.choices.first().map(|c| c.id.clone());
            first.iter().for_each(|id| model.choose(id));
        }
        match (model.loading(), loading_since) {
            (Some(l), None) => {
                loading_since = Some(Instant::now());
                phase.steps = l.entries.len();
                STEPS.fetch_add(1, Ordering::Relaxed);
            }
            (Some(l), Some(_)) => {
                phase.steps = phase.steps.max(l.entries.len());
                STEPS.fetch_add(1, Ordering::Relaxed);
            }
            (None, Some(since)) => {
                phase.loading_for += since.elapsed();
                LOADING_MS.fetch_add(since.elapsed().as_millis() as u64, Ordering::Relaxed);
                loading_since = None;
            }
            (None, None) => {}
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    phase
}

fn choose_label(model: &mut EngineModel, label: &str) {
    wait_for(model, label, |m| {
        m.menu()
            .is_some_and(|x| x.choices.iter().any(|c| c.label == label))
    });
    wait_for(model, "idle", |m| !m.busy());
    let id = model
        .menu()
        .and_then(|m| m.choices.into_iter().find(|c| c.label == label))
        .expect("choice")
        .id;
    model.choose(&id);
    wait_for(model, label, |m| !m.busy());
}

fn dismiss_popups(model: &mut EngineModel) {
    std::thread::sleep(Duration::from_secs(1));
    model.poll();
    while let Some(menu) = model.menu().filter(|m| m.context != "MAIN_MENU") {
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

#[test]
#[ignore = "requires BN_ENGINE"]
fn live_timing_of_starting_the_tutorial_and_loading_its_save() {
    let engine = std::env::var_os("BN_ENGINE").expect("set BN_ENGINE");
    let transport = StdioTransport::spawn_fresh(engine.as_ref()).expect("engine starts");
    let mut model = EngineModel::new(Box::new(transport));
    wait_for(&mut model, "the main menu", |m| m.live());

    common::main_menu_tab(&mut model, "new_game");
    take_loading();
    let started = Instant::now();
    common::submit_main_menu(&mut model, "menu:entry", "new_game:tutorial");
    let new_game = wait_for(&mut model, "the tutorial map", |m| m.size().x > 0);
    let new_game_total = started.elapsed();
    let (new_game_steps, new_game_loading) = take_loading();
    assert!(
        new_game_steps > 0,
        "the engine's loading screen reached the client on new game"
    );
    wait_for(&mut model, "idle", |m| !m.busy());
    dismiss_popups(&mut model);

    assert!(model.press("S"), "Save and Quit is bound");
    wait_for(&mut model, "idle", |m| !m.busy());
    // The tutorial warns that its save is odd, then asks.
    for _ in 0..4 {
        if model
            .menu()
            .is_some_and(|m| m.choices.iter().any(|c| c.label == "Yes"))
        {
            break;
        }
        dismiss_one(&mut model);
    }
    choose_label(&mut model, "Yes");
    wait_for(&mut model, "the main menu again", |m| {
        !common::main_menu_tabs(m).is_empty()
    });

    common::main_menu_tab(&mut model, "load");
    let world = wait_for(&mut model, "the world entry", |m| {
        common::main_menu_entries(m)
            .iter()
            .any(|id| id.starts_with("load:"))
    });
    let _ = world;
    let entry = common::main_menu_entries(&model)
        .into_iter()
        .find(|id| id.starts_with("load:"))
        .unwrap();
    take_loading();
    let started = Instant::now();
    common::main_menu_entry(&mut model, &entry);
    // The world's only character is the first row of the list that follows.
    wait_for(&mut model, "the character list", |m| {
        m.menu().is_some_and(|x| !x.choices.is_empty())
    });
    let character = model.menu().unwrap().choices[0].id.clone();
    wait_for(&mut model, "idle", |m| !m.busy());
    model.choose(&character);
    // A debug message about the tutorial save may ask to continue first.
    let load = wait_for(&mut model, "the loaded map", |m| {
        m.size().x > 0 && m.menu().is_none_or(|x| x.context != "MAIN_MENU")
    });
    let load_total = started.elapsed();
    let (load_steps, load_loading) = take_loading();
    assert!(
        load_steps > 0,
        "the engine's loading screen reached the client on load"
    );
    let _ = (new_game, load);
    println!(
        "TIMING new game: {new_game_total:?} (loading screen {new_game_loading:?}, {new_game_steps} polls); load: {load_total:?} (loading screen {load_loading:?}, {load_steps} polls)"
    );
}

fn dismiss_one(model: &mut EngineModel) {
    if let Some(menu) = model.menu() {
        let id = menu.choices.first().map(|c| c.id.clone());
        match id {
            Some(id) if !menu.can_cancel => model.choose(&id),
            _ => model.cancel_menu(),
        }
        wait_for(model, "popup closing", |m| !m.busy());
    }
}
