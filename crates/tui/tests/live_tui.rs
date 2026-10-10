//! Drives the terminal client's key handling against a real engine:
//! `BN_ENGINE=... cargo test -p bn-client-tui --test live_tui -- --ignored --nocapture`
use bn_client_core::keys::KeyMap;
use bn_client_core::transport::{StdioTransport, game_root};
use bn_client_core::{EngineModel, GameModel};
use bn_client_tui::TuiApp;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::time::{Duration, Instant};

type App = TuiApp<EngineModel>;

fn wait(app: &mut App, what: &str, done: impl Fn(&mut App) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(90);
    loop {
        app.model.poll();
        if done(app) {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "timed out waiting for {what}: {:?} {:?}",
            app.model.messages(),
            app.model.command()
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn press(app: &mut App, code: KeyCode) {
    wait(app, "the previous command", |a| !a.model.busy());
    app.handle_key(KeyEvent::new(code, KeyModifiers::NONE));
}

/// Moves the menu highlight to the row whose label contains `label` and presses Enter.
fn pick(app: &mut App, label: &str) {
    wait(app, label, |a| {
        a.model
            .menu()
            .is_some_and(|m| m.choices.iter().any(|c| c.label.contains(label)))
    });
    for _ in 0..50 {
        if app.selected_label().is_some_and(|l| l.contains(label)) {
            break;
        }
        press(app, KeyCode::Down);
    }
    assert!(app.selected_label().unwrap().contains(label));
    press(app, KeyCode::Enter);
}

/// Switches the main menu to the tab named `label` with Right.
fn tab(app: &mut App, label: &str) {
    let on = |a: &mut App| {
        a.model
            .menu()
            .is_some_and(|m| m.tabs().iter().any(|t| t.selected && t.label == label))
    };
    wait(app, label, |a| {
        a.model.menu().is_some_and(|m| !m.tabs().is_empty())
    });
    for _ in 0..20 {
        if on(app) {
            return;
        }
        press(app, KeyCode::Right);
        std::thread::sleep(Duration::from_millis(300));
        app.model.poll();
    }
    assert!(on(app), "tab {label}");
}

#[test]
#[ignore = "requires BN_ENGINE pointing at an engine binary"]
fn live_tui_starts_the_tutorial_opens_the_inventory_and_closes_it() {
    let engine = std::env::var_os("BN_ENGINE").expect("set BN_ENGINE");
    let transport = StdioTransport::spawn(engine.as_ref()).expect("engine starts");
    let root = game_root(engine.as_ref(), None);
    let keys = root.as_deref().map_or_else(KeyMap::default, KeyMap::load);
    let mut app = TuiApp::new(EngineModel::new(Box::new(transport)).with_keys(keys));

    tab(&mut app, "New Game");
    pick(&mut app, "Tutorial");
    wait(&mut app, "the tutorial map", |a| a.model.size().x > 0);
    // Tutorial pop-ups are menus; dismiss them like a player would.
    wait(&mut app, "idle", |a| !a.model.busy());
    while app.model.menu().is_some() {
        press(&mut app, KeyCode::Enter);
        wait(&mut app, "idle", |a| !a.model.busy());
        std::thread::sleep(Duration::from_millis(500));
        app.model.poll();
    }

    press(&mut app, KeyCode::Char('i'));
    wait(&mut app, "the inventory menu", |a| {
        a.model.menu().is_some_and(|m| m.title == "Inventory")
    });
    press(&mut app, KeyCode::Esc);
    wait(&mut app, "the inventory to close", |a| {
        a.model.menu().is_none()
    });
}

fn absolute(app: &App) -> (i32, i32) {
    let (me, o) = (app.model.player(), app.model.origin());
    (me.x + o.x, me.y + o.y)
}

#[test]
#[ignore = "requires BN_ENGINE pointing at an engine binary"]
fn live_tui_walks_with_arrows_then_shows_and_walks_a_route_from_the_look_cursor() {
    let engine = std::env::var_os("BN_ENGINE").expect("set BN_ENGINE");
    let transport = StdioTransport::spawn(engine.as_ref()).expect("engine starts");
    let root = game_root(engine.as_ref(), None);
    let keys = root.as_deref().map_or_else(KeyMap::default, KeyMap::load);
    let mut app = TuiApp::new(EngineModel::new(Box::new(transport)).with_keys(keys));

    tab(&mut app, "New Game");
    pick(&mut app, "Tutorial");
    wait(&mut app, "the tutorial map", |a| a.model.size().x > 0);
    wait(&mut app, "idle", |a| !a.model.busy());
    while app.model.menu().is_some() {
        press(&mut app, KeyCode::Enter);
        wait(&mut app, "idle", |a| !a.model.busy());
        std::thread::sleep(Duration::from_millis(500));
        app.model.poll();
    }

    let start = absolute(&app);
    press(&mut app, KeyCode::Right);
    wait(&mut app, "the step east", |a| absolute(a) != start);
    // The tutorial's pop-ups can arrive a moment after the step; give them time before the next key.
    std::thread::sleep(Duration::from_secs(1));
    app.model.poll();
    while app.model.menu().is_some() {
        press(&mut app, KeyCode::Enter);
        wait(&mut app, "idle", |a| !a.model.busy());
        std::thread::sleep(Duration::from_millis(700));
        app.model.poll();
    }
    let east = absolute(&app);
    assert_eq!(east, (start.0 + 1, start.1), "one square east");

    // Look cursor back on the start square, Enter shows the route, Enter again walks it.
    press(&mut app, KeyCode::Char('x'));
    press(&mut app, KeyCode::Char('h'));
    press(&mut app, KeyCode::Enter);
    wait(&mut app, "a planned route", |a| !a.model.route().is_empty());
    assert_eq!(absolute(&app), east, "the first Enter only plans");
    press(&mut app, KeyCode::Enter);
    wait(&mut app, "the walk", |a| absolute(a) == start);
}
