//! Drives the terminal client's key handling against a real engine:
//! `BN_ENGINE=... cargo test -p bn-client-tui --test live_tui -- --ignored --nocapture`
mod common;
use bn_client_core::protocol::{Component, InvokeArgs, RowPick};
use bn_client_core::transport::StdioTransport;
use bn_client_core::watchdog::Watchdog;
use bn_client_core::{EngineModel, GameModel};
use bn_client_tui::TuiApp;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
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

/// Starts the tutorial and closes the lesson pop-ups between it and the map with Enter, like a player.
fn start_tutorial(app: &mut App) {
    common::main_menu(&mut app.model, "menu:tab", "new_game");
    common::main_menu(&mut app.model, "menu:entry", "new_game:tutorial");
    let mut watchdog = Watchdog::new(Duration::from_secs(300), Duration::from_secs(900));
    loop {
        app.model.poll();
        if app.model.size().x > 0 && app.model.menu().is_none() && !app.model.busy() {
            return;
        }
        assert!(
            !watchdog.expired(app.model.received()),
            "timed out waiting for the tutorial map: menu {:?} command {:?}",
            app.model.menu().map(|m| (m.context, m.title)),
            app.model.command()
        );
        if app.model.menu().is_some() && !app.model.busy() {
            app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
            std::thread::sleep(Duration::from_millis(500));
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[test]
#[ignore = "requires BN_ENGINE pointing at an engine binary"]
fn live_tui_starts_the_tutorial_opens_the_inventory_and_closes_it() {
    let engine = std::env::var_os("BN_ENGINE").expect("set BN_ENGINE");
    let transport =
        StdioTransport::spawn_with(engine.as_ref(), &common::cpu_profile()).expect("engine starts");
    let mut app = TuiApp::new(EngineModel::new(Box::new(transport)));

    start_tutorial(&mut app);

    press(&mut app, KeyCode::Char('i'));
    let inventory = |a: &App| {
        a.model
            .screens()
            .last()
            .is_some_and(|s| s.context.as_deref() == Some("INVENTORY"))
    };
    wait(&mut app, "the inventory screen", |a| inventory(a));
    press(&mut app, KeyCode::Esc);
    wait(&mut app, "the inventory to close", |a| !inventory(a));
}

fn absolute(app: &App) -> (i32, i32) {
    let (me, o) = (app.model.player(), app.model.origin());
    (me.x + o.x, me.y + o.y)
}

#[test]
#[ignore = "requires BN_ENGINE pointing at an engine binary"]
fn live_tui_walks_with_arrows_then_plans_and_walks_a_route_by_clicking_a_square() {
    let engine = std::env::var_os("BN_ENGINE").expect("set BN_ENGINE");
    let transport =
        StdioTransport::spawn_with(engine.as_ref(), &common::cpu_profile()).expect("engine starts");
    let mut app = TuiApp::new(EngineModel::new(Box::new(transport)));

    start_tutorial(&mut app);

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

    // A click on the start square plans the engine's route to it; a second click walks it.
    let click = |app: &mut App| {
        let mut t = ratatui::Terminal::new(ratatui::backend::TestBackend::new(120, 40)).unwrap();
        t.draw(|f| app.draw(f)).unwrap();
        let buf = t.backend().buffer();
        let (x, y) = (0..buf.area.height)
            .flat_map(|y| (0..buf.area.width).map(move |x| (x, y)))
            .find(|&(x, y)| buf[(x, y)].symbol() == "@")
            .expect("the avatar is drawn");
        wait(app, "idle", |a| !a.model.busy());
        app.handle_mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: x - 1,
            row: y,
            modifiers: KeyModifiers::NONE,
        });
    };
    click(&mut app);
    wait(&mut app, "a planned route", |a| !a.model.route().is_empty());
    assert_eq!(absolute(&app), east, "the first click only plans");
    click(&mut app);
    wait(&mut app, "the walk", |a| absolute(a) == start);
}

/// Waits for `done`, closing any tutorial lesson pop-up that arrives over the advanced inventory.
fn settle(app: &mut App, what: &str, done: impl Fn(&App) -> bool) {
    let mut watchdog = Watchdog::new(Duration::from_secs(120), Duration::from_secs(240));
    loop {
        app.model.poll();
        if done(app) {
            return;
        }
        assert!(
            !watchdog.expired(app.model.received()),
            "timed out waiting for {what}"
        );
        let popup = app.model.menu().is_some_and(|m| m.context == "POPUP_WAIT");
        if popup && !app.model.busy() {
            app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        }
        std::thread::sleep(Duration::from_millis(200));
    }
}

/// The rows of the advanced inventory list `id` on the top screen: the engine's row id and the label.
fn pane_rows(app: &App, id: &str) -> Vec<(String, String)> {
    let Some(Component::List(list)) = app
        .model
        .screens()
        .last()
        .and_then(|s| s.root.as_ref()?.find(id))
    else {
        return vec![];
    };
    list.rows
        .iter()
        .map(|r| (r.id.clone(), r.label.shown()))
        .collect()
}

/// The tutorial gives its character no shirt, so the item moved is the lighter it starts with: from the
/// Inventory pane to the pane of the square under the avatar, by the rows the move affordance names.
#[test]
#[ignore = "requires BN_ENGINE pointing at an engine binary"]
fn live_tui_advanced_inventory_moves_an_item_between_panes_with_the_move_affordance() {
    let engine = std::env::var_os("BN_ENGINE").expect("set BN_ENGINE");
    let transport =
        StdioTransport::spawn_with(engine.as_ref(), &common::cpu_profile()).expect("engine starts");
    let mut app = TuiApp::new(EngineModel::new(Box::new(transport)));
    start_tutorial(&mut app);
    let act = |app: &mut App, id: &str, args: InvokeArgs| {
        wait(app, "idle", |a| !a.model.busy());
        app.model.invoke_affordance(id, args).expect(id);
        wait(app, "idle", |a| !a.model.busy());
    };
    wait(&mut app, "idle", |a| !a.model.busy());
    assert!(app.model.invoke_action("advinv"));
    wait(&mut app, "the advanced inventory", |a| {
        a.model
            .screens()
            .last()
            .is_some_and(|s| s.context.as_deref() == Some("ADVANCED_INVENTORY"))
    });
    // The left pane shows the inventory and the right one the square under the avatar: items cannot be
    // put on the one below.
    let area = |area: &str, pane: &str| InvokeArgs {
        row: Some(area.into()),
        pane: Some(pane.into()),
        ..InvokeArgs::default()
    };
    act(
        &mut app,
        "aim:set_area",
        area("area:INVENTORY", "aim:pane:left"),
    );
    act(
        &mut app,
        "aim:set_area",
        area("area:CENTER", "aim:pane:right"),
    );
    settle(&mut app, "the lighter listed", |a| {
        pane_rows(a, "aim:pane:left:items")
            .iter()
            .any(|(_, label)| label.contains("lighter"))
    });
    let lighter = pane_rows(&app, "aim:pane:left:items")
        .into_iter()
        .find(|(_, label)| label.contains("lighter"))
        .unwrap()
        .0;
    assert!(
        pane_rows(&app, "aim:pane:right:items")
            .iter()
            .all(|(_, label)| !label.contains("lighter"))
    );
    let rows = vec![RowPick {
        id: lighter,
        count: Some(1),
    }];
    act(
        &mut app,
        "aim:move",
        InvokeArgs {
            rows: Some(rows),
            pane: Some("aim:pane:right".into()),
            ..InvokeArgs::default()
        },
    );
    settle(&mut app, "the lighter on the floor", |a| {
        pane_rows(a, "aim:pane:right:items")
            .iter()
            .any(|(_, label)| label.contains("lighter"))
    });
    assert!(
        pane_rows(&app, "aim:pane:left:items")
            .iter()
            .all(|(_, label)| !label.contains("lighter"))
    );
}
