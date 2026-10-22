//! Needs a real engine build: `BN_ENGINE=<engine> cargo test --release -p bn-client-egui --test it live_seed:: -- --ignored`
use crate::common::live::{H, english_transport, new_game};
use bn_client_egui::{App, EngineModel, GameModel, Pos};
use eframe::egui;
use egui_kittest::Harness;

/// Where a new game of a randomly placed start puts the avatar on the world's squares.
fn start_square() -> Pos {
    let mut h: H = Harness::builder()
        .with_size(egui::vec2(1280.0, 720.0))
        .build_ui_state(
            |ui, app: &mut App<EngineModel>| app.show(ui),
            App::new(EngineModel::new(Box::new(english_transport()))),
        );
    new_game(&mut h, "new_game:play_now");
    let (origin, at) = (h.state().model.origin(), h.state().model.player());
    Pos {
        x: origin.x + at.x,
        y: origin.y + at.y,
    }
}

#[test]
#[ignore = "requires BN_ENGINE pointing at an engine binary"]
fn the_same_seed_starts_a_random_start_in_the_same_square() {
    assert_eq!(start_square(), start_square());
}
