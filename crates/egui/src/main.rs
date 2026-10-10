use bn_client_egui::colors::ColorMap;
use bn_client_egui::fonts;
use bn_client_egui::keys::KeyMap;
use bn_client_egui::theme;
use bn_client_egui::transport::{StdioTransport, game_root};
use bn_client_egui::{App, EngineModel, FakeModel, GameModel};
use std::path::PathBuf;

const USAGE: &str =
    "usage: bn-client-egui [--engine PATH]   (or set BN_ENGINE); no engine: fake model";

fn run(model: impl GameModel + 'static, root: Option<PathBuf>) -> eframe::Result {
    eframe::run_native(
        "Cataclysm: Bright Nights client",
        eframe::NativeOptions {
            viewport: eframe::egui::ViewportBuilder::default().with_inner_size([1280.0, 800.0]),
            ..Default::default()
        },
        Box::new(move |cc| {
            theme::apply(&cc.egui_ctx);
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

fn main() -> eframe::Result {
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
        return run(FakeModel::default(), None);
    };
    match StdioTransport::spawn(&engine) {
        Ok(t) => {
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
