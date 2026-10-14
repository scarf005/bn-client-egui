use bn_client_core::launch::{LaunchArgs, Search, open_engine, resolve};
use bn_client_core::{ConnectOptions, FakeModel, GameModel};
use bn_client_tui::TuiApp;
use clap::Parser;
use crossterm::event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyEventKind};
use crossterm::execute;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

enum Msg {
    Key(event::KeyEvent),
    Mouse(event::MouseEvent),
    Wake,
}

/// The frames per second worth redrawing a terminal at; the loop redraws on every engine frame it wakes for.
const MAX_HZ: u16 = 30;

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
            let first = rx.recv_timeout(Duration::from_secs(1));
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

#[derive(Parser)]
#[command(
    name = "bn-client-tui",
    about = "Terminal client for Cataclysm: Bright Nights. Ctrl-C quits; the engine's look key starts a look cursor, Tab lists the actions, PageUp/Down log"
)]
struct Cli {
    #[command(flatten)]
    launch: LaunchArgs,
}

fn main() -> std::io::Result<()> {
    let cli = Cli::parse();
    if cli.launch.fake {
        return run(FakeModel::default());
    }
    let search = Search::from_env();
    let engine = resolve(cli.launch.engine.as_deref(), &search).unwrap_or_else(|e| {
        eprintln!("{e}");
        std::process::exit(1);
    });
    let options = ConnectOptions {
        max_hz: MAX_HZ,
        locale: cli.launch.locale.clone(),
        ..ConnectOptions::default()
    };
    let opened = open_engine(&engine, &cli.launch.profile, options).unwrap_or_else(|e| {
        eprintln!("{e}");
        std::process::exit(1);
    });
    run(opened.model)
}
