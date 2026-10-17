use bn_client_egui::display::{self, settings_path};
use bn_client_egui::fonts;
use bn_client_egui::launch::{LaunchArgs, Opened, Search, open_engine, resolve};
use bn_client_egui::setup::{Launcher, Starter};
use bn_client_egui::theme;
use bn_client_egui::transport::game_root;
use bn_client_egui::{App, ConnectOptions, GameModel};
use clap::Parser;
use eframe::egui;
use std::path::PathBuf;

fn native_options() -> eframe::NativeOptions {
    eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1280.0, 800.0]),
        ..Default::default()
    }
}

/// The app, keeping the display choice in the settings file; the game root comes after, since the
/// choice is read once the root is known.
fn app<M: GameModel>(model: M) -> App<M> {
    match settings_path() {
        Some(path) => App::new(model).with_settings(path),
        None => App::new(model),
    }
}

#[cfg(feature = "fake")]
fn run(model: impl GameModel + 'static, root: Option<PathBuf>) -> eframe::Result {
    eframe::run_native(
        "Cataclysm: Bright Nights client",
        native_options(),
        Box::new(move |cc| {
            theme::apply(&cc.egui_ctx);
            if let Some(root) = &root {
                fonts::install(&cc.egui_ctx, root);
            }
            let app = app(model);
            Ok(Box::new(match root {
                Some(root) => app.with_root(root),
                None => app,
            }))
        }),
    )
}

#[derive(Parser)]
#[command(
    name = "bn-client-egui",
    about = "Desktop client for Cataclysm: Bright Nights"
)]
struct Cli {
    #[command(flatten)]
    launch: LaunchArgs,
}

/// The frames per second the window shows: eframe does not report the display's refresh rate at start-up.
const MAX_HZ: u16 = 60;

/// Starts the engine and builds the running app.
fn start_engine(
    ctx: &egui::Context,
    engine: PathBuf,
    launch: &LaunchArgs,
) -> Result<Box<dyn eframe::App>, String> {
    let tiles = display::tiles_wanted(
        game_root(&engine, std::env::var_os("BN_ROOT").map(PathBuf::from)).as_deref(),
        settings_path().as_deref(),
    );
    let options = ConnectOptions {
        tiles,
        max_hz: MAX_HZ,
        locale: launch.locale.clone(),
    };
    let Opened { model, root } = open_engine(&engine, &launch.profile, options)?;
    if let Some(root) = &root {
        fonts::install(ctx, root);
    }
    let app = app(model);
    Ok(Box::new(match root {
        Some(root) => app.with_root(root),
        None => app,
    }))
}

fn main() -> eframe::Result {
    let cli = Cli::parse();
    #[cfg(feature = "fake")]
    if cli.launch.fake {
        return run(bn_client_egui::FakeModel::default(), None);
    }
    let launch = cli.launch.clone();
    let found = resolve(cli.launch.engine.as_deref(), &Search::from_env());
    eframe::run_native(
        "Cataclysm: Bright Nights client",
        native_options(),
        Box::new(move |cc| {
            theme::apply(&cc.egui_ctx);
            let starter = launch.clone();
            let start: Starter = Box::new(move |ctx, engine| start_engine(ctx, engine, &starter));
            let remember = |l: Launcher| match bn_client_egui::launch::config_file() {
                Some(file) => l.with_config(file),
                None => l,
            };
            Ok(Box::new(match found {
                Ok(engine) => match start_engine(&cc.egui_ctx, engine, &launch) {
                    Ok(app) => Launcher::running(app, start),
                    Err(e) => {
                        eprintln!("{e}");
                        remember(Launcher::new(String::new(), start)).with_error(e)
                    }
                },
                Err(not_found) => {
                    eprintln!("{not_found}");
                    remember(Launcher::new(not_found.to_string(), start))
                }
            }))
        }),
    )
}
