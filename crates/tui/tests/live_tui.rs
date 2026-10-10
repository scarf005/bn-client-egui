//! Drives the terminal client's key handling against a real engine:
//! `BN_ENGINE=... cargo test -p bn-client-tui --test live_tui -- --ignored --nocapture`
use bn_client_core::transport::StdioTransport;
use bn_client_core::watchdog::Watchdog;
use bn_client_core::{EngineModel, GameModel};
use bn_client_tui::TuiApp;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::time::Duration;

type App = TuiApp<EngineModel>;

fn wait(app: &mut App, what: &str, done: impl Fn(&mut App) -> bool) {
    // The engine is silent while it loads, which takes minutes on a busy machine.
    let mut watchdog = Watchdog::new(Duration::from_secs(300), Duration::from_secs(900));
    loop {
        app.model.poll();
        if done(app) {
            return;
        }
        assert!(
            !watchdog.expired(app.model.received()),
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

/// Switches the main menu to the tab named `label` with Left or Right, whichever is nearer in tab order.
fn tab(app: &mut App, label: &str) {
    // (index of the selected tab, index of `label`) in the tab row.
    let positions = |a: &mut App| {
        let menu = a.model.menu()?;
        let tabs = menu.tabs();
        Some((
            tabs.iter().position(|t| t.selected)?,
            tabs.iter().position(|t| t.label == label)?,
        ))
    };
    wait(app, label, |a| positions(a).is_some());
    for _ in 0..20 {
        let (at, want) = positions(app).expect("tab row");
        if at == want {
            return;
        }
        press(
            app,
            if want < at {
                KeyCode::Left
            } else {
                KeyCode::Right
            },
        );
        std::thread::sleep(Duration::from_millis(300));
        app.model.poll();
    }
    assert_eq!(
        positions(app).map(|(at, want)| at == want),
        Some(true),
        "tab {label}"
    );
}

#[test]
#[ignore = "requires BN_ENGINE pointing at an engine binary"]
fn live_tui_starts_the_tutorial_opens_the_inventory_and_closes_it() {
    let engine = std::env::var_os("BN_ENGINE").expect("set BN_ENGINE");
    let transport = StdioTransport::spawn_fresh(engine.as_ref()).expect("engine starts");
    let mut app = TuiApp::new(EngineModel::new(Box::new(transport)));

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
    let mut app = TuiApp::new(EngineModel::new(Box::new(transport)));

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

fn screen(app: &mut App) -> String {
    let mut t = ratatui::Terminal::new(ratatui::backend::TestBackend::new(120, 36)).unwrap();
    t.draw(|f| app.draw(f)).unwrap();
    let b = t.backend().buffer();
    let rows = b.content().chunks(b.area.width as usize);
    rows.map(|row| row.iter().map(|c| c.symbol()).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n")
}

fn recipes(app: &App) -> Vec<bn_client_core::MenuChoice> {
    let rows = app.model.menu().map(|m| m.choices).unwrap_or_default();
    rows.into_iter()
        .filter(|c| c.id.starts_with("recipe:"))
        .collect()
}

#[test]
#[ignore = "requires BN_ENGINE pointing at an engine binary"]
fn live_tui_crafting_switches_tabs_shows_details_searches_and_closes() {
    let engine = std::env::var_os("BN_ENGINE").expect("set BN_ENGINE");
    let transport = StdioTransport::spawn_fresh(engine.as_ref()).expect("engine starts");
    let mut app = TuiApp::new(EngineModel::new(Box::new(transport)));

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

    press(&mut app, KeyCode::Char('&'));
    wait(&mut app, "the crafting menu", |a| {
        a.model.menu().is_some_and(|m| m.context == "CRAFTING")
    });
    // Right through the tabs until one lists recipes.
    for _ in 0..12 {
        if !recipes(&app).is_empty() {
            break;
        }
        press(&mut app, KeyCode::Right);
        wait(&mut app, "idle", |a| !a.model.busy());
    }
    let list = recipes(&app);
    assert!(!list.is_empty(), "some tab lists recipes");
    wait(&mut app, "the detail text", |a| {
        recipes(a)
            .first()
            .is_some_and(|r| !r.description.is_empty())
    });
    let first = recipes(&app).remove(0);
    let shown = screen(&mut app);
    assert!(shown.contains(&first.label), "{shown}");
    assert!(shown.contains("Time to complete"), "{shown}");

    let word = first.label.split(' ').next().unwrap().to_owned();
    press(&mut app, KeyCode::Char('/'));
    for c in word.chars() {
        app.handle_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE));
    }
    press(&mut app, KeyCode::Enter);
    wait(&mut app, "the search result", |a| {
        a.model
            .menu()
            .is_some_and(|m| m.field.is_some_and(|f| f.value == word))
    });
    assert!(recipes(&app).iter().any(|r| r.label.contains(&word)));

    wait(&mut app, "idle", |a| !a.model.busy());
    press(&mut app, KeyCode::Esc);
    wait(&mut app, "the crafting menu to close", |a| {
        a.model.menu().is_none()
    });
}
