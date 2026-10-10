//! Changes an option and a keybinding through the terminal client's keys, against a real engine:
//! `BN_ENGINE=... cargo test -p bn-client-tui --test live_settings -- --ignored --nocapture`
use bn_client_core::transport::StdioTransport;
use bn_client_core::watchdog::Watchdog;
use bn_client_core::{EngineModel, GameModel};
use bn_client_tui::TuiApp;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::time::Duration;

type App = TuiApp<EngineModel>;

fn wait(app: &mut App, what: &str, done: impl Fn(&mut App) -> bool) {
    let mut watchdog = Watchdog::new(Duration::from_secs(300), Duration::from_secs(900));
    loop {
        app.model.poll();
        if done(app) {
            return;
        }
        assert!(
            !watchdog.expired(app.model.received()),
            "timed out waiting for {what}: {:?}",
            app.model.menu().map(|m| m.title)
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn press(app: &mut App, code: KeyCode) {
    wait(app, "the previous command", |a| !a.model.busy());
    app.handle_key(KeyEvent::new(code, KeyModifiers::NONE));
}

fn type_text(app: &mut App, text: &str) {
    text.chars().for_each(|c| press(app, KeyCode::Char(c)));
}

fn context(app: &App) -> String {
    app.model.menu().map(|m| m.context).unwrap_or_default()
}

/// Moves the highlight to the row whose label contains `label` and presses Enter.
fn pick(app: &mut App, label: &str) {
    wait(app, label, |a| {
        a.model
            .menu()
            .is_some_and(|m| m.choices.iter().any(|c| c.label.contains(label)))
    });
    for _ in 0..400 {
        if app.selected_label().is_some_and(|l| l.contains(label)) {
            break;
        }
        press(app, KeyCode::Down);
    }
    let menu = app.model.menu().unwrap();
    let labels = menu
        .entries()
        .iter()
        .map(|c| c.label.clone())
        .collect::<Vec<_>>();
    assert!(
        app.selected_label().is_some_and(|l| l.contains(label)),
        "{label} in {} {labels:?} selected {:?}",
        menu.title,
        app.selected_label()
    );
    press(app, KeyCode::Enter);
}

/// Switches the menu to the tab whose label contains `label`, stepping with Left or Right.
fn tab(app: &mut App, label: &str) {
    let positions = |a: &mut App| {
        let menu = a.model.menu()?;
        let tabs = menu.tabs();
        Some((
            tabs.iter().position(|t| t.selected)?,
            tabs.iter().position(|t| t.label.contains(label))?,
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
    panic!("tab {label}");
}

/// Leaves a settings screen with Esc and answers the save prompt.
fn leave_saving(app: &mut App) {
    press(app, KeyCode::Esc);
    wait(app, "the save prompt or the main menu", |a| {
        a.model.menu().is_some()
    });
    if context(app) != "MAIN_MENU" {
        pick(app, "Yes");
    }
    wait(app, "the main menu", |a| context(a) == "MAIN_MENU");
}

fn row_label(app: &App, id: &str) -> String {
    app.model
        .menu()
        .and_then(|m| m.choices.into_iter().find(|c| c.id == id))
        .map(|c| c.editor.map_or(c.label, |e| e.value))
        .unwrap_or_default()
}

fn rows(app: &App, prefix: &str) -> Vec<String> {
    let menu = app.model.menu().map(|m| m.choices).unwrap_or_default();
    menu.into_iter()
        .filter(|c| c.id.starts_with(prefix))
        .map(|c| c.id)
        .collect()
}

#[test]
#[ignore = "requires BN_ENGINE pointing at an engine binary"]
fn live_tui_changes_an_option_and_rebinds_a_key_that_then_walks() {
    let engine = std::env::var_os("BN_ENGINE").expect("set BN_ENGINE");
    let transport = StdioTransport::spawn_fresh(engine.as_ref()).expect("engine starts");
    let mut app = TuiApp::new(EngineModel::new(Box::new(transport)));
    wait(&mut app, "the main menu", |a| context(a) == "MAIN_MENU");

    // Options: the General page holds the integer; Enter types a value, Enter commits it.
    tab(&mut app, "Settings");
    pick(&mut app, "Options");
    wait(&mut app, "the options", |a| context(a) == "OPTIONS");
    let option = "option:SAFEMODEPROXIMITY";
    wait(&mut app, "the option row", |a| {
        !row_label(a, option).is_empty()
    });
    pick(&mut app, "Safe mode proximity distance");
    type_text(&mut app, "9");
    press(&mut app, KeyCode::Enter);
    wait(&mut app, "the new value", |a| row_label(a, option) == "9");
    leave_saving(&mut app);

    // Keybindings: clear the keys of Move East, then bind z to it.
    pick(&mut app, "Keybindings");
    wait(&mut app, "the keybindings", |a| {
        context(a) == "HELP_KEYBINDINGS"
    });
    press(&mut app, KeyCode::Char('/'));
    type_text(&mut app, "Move East");
    press(&mut app, KeyCode::Enter);
    wait(&mut app, "the filtered list", |a| {
        rows(a, "action:") == ["action:RIGHT"]
    });
    let mode = |a: &App, id: &str| {
        a.model
            .menu()
            .and_then(|m| m.choices.into_iter().find(|c| c.id == id))
            .map(|c| c.label)
            .unwrap()
    };
    let remove = mode(&app, "mode:remove");
    pick(&mut app, &remove);
    pick(&mut app, "Move East");
    // The native game asks to confirm the removal; the row then shows no keys.
    let unbound = |a: &App| {
        a.model
            .menu()
            .and_then(|m| m.choices.into_iter().find(|c| c.id == "action:RIGHT"))
            .is_some_and(|c| c.columns[1].1 == "Unbound")
    };
    wait(&mut app, "the removal", |a| {
        context(a) != "HELP_KEYBINDINGS" || unbound(a)
    });
    if context(&app) != "HELP_KEYBINDINGS" {
        pick(&mut app, "Yes");
    }
    wait(&mut app, "the keys removed", |a| unbound(a));
    let add = mode(&app, "mode:add_global");
    pick(&mut app, &add);
    pick(&mut app, "Move East");
    wait(&mut app, "the key prompt", |a| {
        a.model
            .menu()
            .is_some_and(|m| m.field.is_some_and(|f| f.kind == "key"))
    });
    press(&mut app, KeyCode::Char('z'));
    wait(&mut app, "the keybindings", |a| {
        context(a) == "HELP_KEYBINDINGS"
    });
    let keys = |a: &App| {
        a.model
            .menu()
            .and_then(|m| m.choices.into_iter().find(|c| c.id == "action:RIGHT"))
            .map(|c| c.columns[0].1.clone())
    };
    wait(&mut app, "the new key", |a| keys(a).as_deref() == Some("z"));
    leave_saving(&mut app);

    // In the game the action resolves to the new key, which walks the avatar east.
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
    let square = |a: &App| (a.model.player().x + a.model.origin().x, a.model.player().y);
    let start = square(&app);
    assert!(app.model.invoke_action("RIGHT"));
    wait(&mut app, "the step east", |a| square(a) != start);
    assert_eq!(square(&app).0, start.0 + 1);
}
