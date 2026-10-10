use bn_client_egui::fonts;
use bn_client_egui::launch::{LaunchArgs, Opened, Search, open_engine, resolve};
use bn_client_egui::setup::{Launcher, Starter};
use bn_client_egui::transport::Profile;
use bn_client_egui::{App, FakeModel, GameModel};
use clap::Parser;
use eframe::egui;
use std::path::PathBuf;

fn run(model: impl GameModel + 'static, root: Option<PathBuf>) -> eframe::Result {
    eframe::run_native(
        "Cataclysm: Bright Nights client",
        eframe::NativeOptions::default(),
        Box::new(move |cc| {
            if let Some(root) = &root {
                fonts::install(&cc.egui_ctx, root);
            }
            let app = App::new(model);
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

/// Starts the engine and builds the running app.
fn start_engine(
    ctx: &egui::Context,
    engine: PathBuf,
    profile: &Profile,
) -> Result<Box<dyn eframe::App>, String> {
    let Opened { model, root } = open_engine(&engine, profile)?;
    if let Some(root) = &root {
        fonts::install(ctx, root);
    }
    let app = App::new(model);
    Ok(Box::new(match root {
        Some(root) => app.with_root(root),
        None => app,
    }))
}

fn main() -> eframe::Result {
    let cli = Cli::parse();
    if cli.launch.fake {
        return run(FakeModel::default(), None);
    }
    let profile = cli.launch.profile.clone();
    let found = resolve(cli.launch.engine.as_deref(), &Search::from_env());
    eframe::run_native(
        "Cataclysm: Bright Nights client",
        eframe::NativeOptions::default(),
        Box::new(move |cc| {
            let starter = profile.clone();
            let start: Starter = Box::new(move |ctx, engine| start_engine(ctx, engine, &starter));
            Ok(Box::new(match found {
                Ok(engine) => match start_engine(&cc.egui_ctx, engine, &profile) {
                    Ok(app) => Launcher::running(app, start),
                    Err(e) => {
                        eprintln!("{e}");
                        Launcher::new(String::new(), start).with_error(e)
                    }
                },
                Err(not_found) => {
                    eprintln!("{not_found}");
                    Launcher::new(not_found.to_string(), start)
                }
            }))
        }),
    )
}
