//! Needs a real engine build: `BN_ENGINE=/path/to/cataclysm-bn-tiles cargo test --test live_engine -- --ignored`
use bn_client_egui::transport::StdioTransport;
use bn_client_egui::{EngineModel, GameModel};
use std::time::{Duration, Instant};

#[test]
#[ignore = "requires BN_ENGINE pointing at an engine binary"]
fn live_engine_hello_subscribe_and_snapshot() {
    let engine = std::env::var_os("BN_ENGINE").expect("set BN_ENGINE");
    let transport = StdioTransport::spawn(engine.as_ref()).expect("engine starts");
    let mut model = EngineModel::new(Box::new(transport));
    let deadline = Instant::now() + Duration::from_secs(60);
    while model.size().x == 0 && Instant::now() < deadline {
        model.poll();
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(
        model.size().x > 0,
        "no snapshot in 60 s: {:?}",
        model.messages()
    );
    assert!(model.live());
}
