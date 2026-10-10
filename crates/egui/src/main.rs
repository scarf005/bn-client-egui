use bn_client_egui::colors::ColorMap;
use bn_client_egui::fonts;
use bn_client_egui::keys::KeyMap;
use bn_client_egui::launch::{LaunchArgs, Search, resolve, save_engine};
use bn_client_egui::transport::{StdioTransport, game_root};
use bn_client_egui::{App, EngineModel, FakeModel, GameModel};
use clap::Parser;
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

fn main() -> eframe::Result {
    let cli = Cli::parse();
    if cli.launch.fake {
        return run(FakeModel::default(), None);
    }
    let search = Search::from_env();
    let engine = resolve(cli.launch.engine.as_deref(), &search).unwrap_or_else(|e| {
        eprintln!("{e}");
        std::process::exit(1);
    });
    match StdioTransport::spawn_with(&engine, &cli.launch.profile) {
        Ok(t) => {
            if let Some(file) = &search.config_file {
                let _ = save_engine(file, &engine);
            }
            let root = game_root(&engine, std::env::var_os("BN_ROOT").map(PathBuf::from));
            let colors = root
                .as_deref()
                .map_or_else(ColorMap::default, ColorMap::load);
            let keys = root.as_deref().map_or_else(KeyMap::default, KeyMap::load);
            run(
                EngineModel::new(Box::new(t))
                    .with_colors(colors)
                    .with_keys(keys),
                root,
            )
        }
        Err(e) => {
            eprintln!("cannot start engine {}: {e}", engine.display());
            std::process::exit(1);
        }
    }
}
