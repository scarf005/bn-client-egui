//! The terminal client's mouse against a real engine, in a private profile with the CPU compute path:
//! `TMPDIR=$HOME/scratch BN_ENGINE=... cargo test -p bn-client-tui --test live_mouse -- --ignored`
mod common;
use bn_client_core::transport::StdioTransport;
use bn_client_core::watchdog::Watchdog;
use bn_client_core::{EngineModel, GameModel};
use bn_client_tui::TuiApp;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
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

fn mouse(kind: MouseEventKind, column: u16, row: u16) -> MouseEvent {
    MouseEvent {
        kind,
        column,
        row,
        modifiers: KeyModifiers::NONE,
    }
}

fn screen(app: &mut App) -> Terminal<TestBackend> {
    let mut t = Terminal::new(TestBackend::new(120, 40)).unwrap();
    t.draw(|f| app.draw(f)).unwrap();
    t
}

fn avatar(t: &Terminal<TestBackend>) -> Option<(u16, u16)> {
    let buf = t.backend().buffer();
    (0..buf.area.height)
        .flat_map(|y| (0..buf.area.width).map(move |x| (x, y)))
        .find(|&(x, y)| buf[(x, y)].symbol() == "@")
}

fn text(t: &Terminal<TestBackend>) -> String {
    let b = t.backend().buffer();
    b.content()
        .chunks(b.area.width as usize)
        .map(|row| row.iter().map(|c| c.symbol()).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
#[ignore = "requires BN_ENGINE pointing at an engine binary with the mouse view"]
fn live_tui_mouse_ignores_the_wheel_in_the_main_menu_then_describes_and_right_clicks_a_square() {
    let engine = std::env::var_os("BN_ENGINE").expect("set BN_ENGINE");
    let transport =
        StdioTransport::spawn_with(engine.as_ref(), &common::cpu_profile()).expect("engine starts");
    let mut app = TuiApp::new(EngineModel::new(Box::new(transport)));

    // The native main menu registers no SCROLL action, so the client has nothing to send.
    wait(&mut app, "the main menu", |a| {
        a.model.menu().is_some_and(|m| m.context == "MAIN_MENU")
    });
    assert!(
        !app.model
            .actions()
            .iter()
            .any(|a| a.id.starts_with("SCROLL")),
        "the main menu offers no wheel actions"
    );
    app.handle_mouse(mouse(MouseEventKind::ScrollDown, 50, 12));
    app.handle_mouse(mouse(MouseEventKind::ScrollUp, 50, 12));
    assert!(app.model.command().is_none(), "nothing was sent");

    // Tutorial, past its pop-ups.
    let tutorial = |a: &mut App| {
        a.model
            .menu()
            .is_some_and(|m| m.title == "Cataclysm: Bright Nights")
    };
    wait(&mut app, "the menu", tutorial);
    wait(&mut app, "the entries", |a| {
        a.model
            .menu()
            .is_some_and(|m| m.choices.iter().any(|c| c.id == "new_game:tutorial"))
    });
    wait(&mut app, "idle", |a| !a.model.busy());
    app.model.choose("new_game:tutorial");
    // Pop-ups (the tutorial's lessons, engine notices) are menus too; each closes with Enter.
    let mut watchdog = Watchdog::new(Duration::from_secs(120), Duration::from_secs(240));
    loop {
        app.model.poll();
        if app.model.size().x > 0 && app.model.menu().is_none() && !app.model.busy() {
            break;
        }
        assert!(
            !watchdog.expired(app.model.received()),
            "timed out waiting for the map"
        );
        if app.model.menu().is_some() && !app.model.busy() {
            app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        }
        std::thread::sleep(Duration::from_millis(200));
    }

    // The world swap after choosing the Tutorial leaves no rejected command in the status line.
    assert!(
        app.model
            .command()
            .is_none_or(|c| !c.stage.starts_with("rejected")),
        "{:?}",
        app.model.command()
    );

    // Hovering the square next to the avatar shows what the engine's mouse view prints for it.
    // The status line is one row, cut at the terminal's width after the last command's result, so the
    // first line of the description is what it is sure to show.
    let drawn = screen(&mut app);
    let (x, y) = avatar(&drawn).expect("the avatar is drawn");
    app.handle_mouse(mouse(MouseEventKind::Moved, x + 1, y));
    wait(&mut app, "the description", |a| {
        text(&screen(a)).contains("tutorial room")
    });
    let status = text(&screen(&mut app));
    assert!(
        status.contains("tutorial room"),
        "the native text names the room:\n{status}"
    );

    // A right click next to the avatar is the native examine.
    let (x, y) = avatar(&screen(&mut app)).expect("the avatar is drawn");
    app.handle_mouse(mouse(MouseEventKind::Down(MouseButton::Right), x + 1, y));
    wait(&mut app, "the examine message", |a| {
        a.model.poll();
        a.model
            .native_log()
            .iter()
            .any(|l| l.text.contains("wooden floor"))
    });
}
