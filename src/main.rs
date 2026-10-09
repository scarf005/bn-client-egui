use bn_client_egui::{App, FakeModel};

fn main() -> eframe::Result {
    eframe::run_native(
        "Cataclysm: Bright Nights client",
        eframe::NativeOptions::default(),
        Box::new(|_| Ok(Box::new(App::new(FakeModel::default())))),
    )
}
