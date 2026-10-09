use bn_client_egui::transport::StdioTransport;
use bn_client_egui::{App, EngineModel, FakeModel, GameModel};
use std::path::PathBuf;

const USAGE: &str =
    "usage: bn-client-egui [--engine PATH]   (or set BN_ENGINE); no engine: fake model";

fn run(model: impl GameModel + 'static) -> eframe::Result {
    eframe::run_native(
        "Cataclysm: Bright Nights client",
        eframe::NativeOptions::default(),
        Box::new(|_| Ok(Box::new(App::new(model)))),
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
        return run(FakeModel::default());
    };
    match StdioTransport::spawn(&engine) {
        Ok(t) => run(EngineModel::new(Box::new(t))),
        Err(e) => {
            eprintln!("cannot start engine {}: {e}", engine.display());
            std::process::exit(1);
        }
    }
}
