use bn_client_core::colors::ColorMap;
use bn_client_core::keys::KeyMap;
use bn_client_core::transport::{StdioTransport, game_root};
use bn_client_core::{EngineModel, FakeModel, GameModel};
use bn_client_tui::TuiApp;
use crossterm::event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyEventKind};
use crossterm::execute;
use std::path::PathBuf;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

const USAGE: &str = "usage: bn-client-tui [--engine PATH]   (or set BN_ENGINE); no engine: fake model. Ctrl-C quits; x/; look cursor, Tab action list, PageUp/Down log";

enum Msg {
    Key(event::KeyEvent),
    Mouse(event::MouseEvent),
    Wake,
}

fn run(model: impl GameModel) -> std::io::Result<()> {
    let (tx, rx) = mpsc::channel();
    let mut app = TuiApp::new(model);
    let wake = Mutex::new(tx.clone());
    app.model.set_waker(Arc::new(move || {
        let _ = wake.lock().unwrap().send(Msg::Wake);
    }));
    std::thread::spawn(move || {
        while let Ok(ev) = event::read() {
            let msg = match ev {
                Event::Key(k) if k.kind == KeyEventKind::Press => Msg::Key(k),
                Event::Mouse(m) => Msg::Mouse(m),
                _ => Msg::Wake,
            };
            if tx.send(msg).is_err() {
                return;
            }
        }
    });
    let mut terminal = ratatui::init();
    let _ = execute!(std::io::stdout(), EnableMouseCapture);
    let result = (|| {
        while !app.quit() {
            app.model.poll();
            terminal.draw(|f| app.draw(f))?;
            // Engine messages and key presses both arrive here, so neither waits for a timer.
            let wait = match app.walking() {
                true => Duration::from_millis(50),
                false => Duration::from_secs(1),
            };
            let first = rx.recv_timeout(wait);
            for msg in first.into_iter().chain(rx.try_iter()) {
                match msg {
                    Msg::Key(k) => app.handle_key(k),
                    Msg::Mouse(m) => app.handle_mouse(m),
                    Msg::Wake => {}
                }
            }
            app.tick();
        }
        Ok(())
    })();
    let _ = execute!(std::io::stdout(), DisableMouseCapture);
    ratatui::restore();
    result
}

fn main() -> std::io::Result<()> {
    let mut args = std::env::args().skip(1);
    let mut engine = std::env::var_os("BN_ENGINE").map(PathBuf::from);
    while let Some(arg) = args.next() {
        match (arg.as_str(), args.next()) {
            ("--engine", Some(path)) => engine = Some(path.into()),
            _ => {
                eprintln!("{USAGE}");
                std::process::exit(2);
            }
        }
    }
    let Some(engine) = engine else {
        return run(FakeModel::default());
    };
    let transport = StdioTransport::spawn(&engine).unwrap_or_else(|e| {
        eprintln!("cannot start engine {}: {e}", engine.display());
        std::process::exit(1);
    });
    let root = game_root(&engine, std::env::var_os("BN_ROOT").map(PathBuf::from));
    let colors = root
        .as_deref()
        .map_or_else(ColorMap::default, ColorMap::load);
    let keys = root.as_deref().map_or_else(KeyMap::default, KeyMap::load);
    run(EngineModel::new(Box::new(transport))
        .with_colors(colors)
        .with_keys(keys))
}
