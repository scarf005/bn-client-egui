//! Needs a real engine build: `BN_ENGINE=/path/to/cataclysm-bn-tiles cargo test -p bn-client-egui --test live_ui -- --ignored --nocapture`
//! Set `BN_CLIENT_TRACE=<file>` to log the JSON-RPC traffic.
use bn_client_egui::transport::StdioTransport;
use bn_client_egui::{App, EngineModel};
use eframe::egui;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use std::time::{Duration, Instant};

type H<'a> = Harness<'a, App<EngineModel>>;

fn run_until(h: &mut H, what: &str, secs: u64, mut done: impl FnMut(&mut H) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(secs);
    while !done(h) {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        h.run_steps(1);
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[test]
#[ignore = "requires BN_ENGINE pointing at an engine binary"]
fn clicking_new_game_shows_the_command_status() {
    let engine = std::env::var_os("BN_ENGINE").expect("set BN_ENGINE");
    let transport = StdioTransport::spawn(engine.as_ref()).expect("engine starts");
    let model = EngineModel::new(Box::new(transport));
    let mut h = Harness::builder()
        .with_size(egui::vec2(1300.0, 800.0))
        .build_ui_state(
            |ui, app: &mut App<EngineModel>| app.show(ui),
            App::new(model),
        );
    run_until(&mut h, "the main menu", 60, |h| {
        h.query_by_label("New Game").is_some()
    });
    h.get_by_label("New Game").click();
    let clicked = Instant::now();
    run_until(&mut h, "the status line to follow the click", 60, |h| {
        h.query_by_label("New Game: done").is_some()
    });
    // The engine answers with the same flattened main menu, so the status line is the only reaction.
    println!("New Game done after {:?}", clicked.elapsed());
}
