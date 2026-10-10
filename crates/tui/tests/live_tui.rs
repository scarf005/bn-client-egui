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
            "timed out waiting for {what}: {:?}",
            app.model.messages()
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

#[test]
#[ignore = "requires BN_ENGINE pointing at an engine binary"]
fn live_tui_starts_the_tutorial_opens_the_inventory_and_closes_it() {
    let engine = std::env::var_os("BN_ENGINE").expect("set BN_ENGINE");
    let transport = StdioTransport::spawn(engine.as_ref()).expect("engine starts");
    let root = game_root(engine.as_ref(), None);
    let keys = root.as_deref().map_or_else(KeyMap::default, KeyMap::load);
    let mut app = TuiApp::new(EngineModel::new(Box::new(transport)).with_keys(keys));

    pick(&mut app, "New Game");
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
