use bn_client_core::colors::ColorMap;
use bn_client_core::keys::KeyMap;
use bn_client_core::transport::{StdioTransport, game_root};
use bn_client_core::{EngineModel, FakeModel, GameModel};
use bn_client_tui::TuiApp;
use crossterm::event::{self, Event, KeyEventKind};
use std::path::PathBuf;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

const USAGE: &str = "usage: bn-client-tui [--engine PATH]   (or set BN_ENGINE); no engine: fake model. Ctrl-C quits";

enum Msg {
    Key(event::KeyEvent),
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
                _ => Msg::Wake,
            };
            if tx.send(msg).is_err() {
                return;
            }
        }
    });
    let mut terminal = ratatui::init();
    let result = (|| {
        while !app.quit() {
            app.model.poll();
            terminal.draw(|f| app.draw(f))?;
            // Engine messages and key presses both arrive here, so neither waits for a timer.
            let first = rx.recv_timeout(Duration::from_secs(1));
            for msg in first.into_iter().chain(rx.try_iter()) {
                if let Msg::Key(k) = msg {
                    app.handle_key(k);
                }
            }
        }
        Ok(())
    })();
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
