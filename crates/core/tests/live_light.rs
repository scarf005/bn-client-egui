//! Needs a real engine build: `BN_ENGINE=... cargo test -p bn-client-core --test live_light -- --ignored --nocapture`
//! Counts the squares of the tutorial's first view by the light level the engine publishes.
mod common;
use bn_client_core::transport::StdioTransport;
use bn_client_core::watchdog::Watchdog;
use bn_client_core::{EngineModel, GameModel, Pos};
use std::collections::BTreeMap;
use std::time::Duration;

fn wait_for(model: &mut EngineModel, what: &str, mut done: impl FnMut(&EngineModel) -> bool) {
    let mut watchdog = Watchdog::new(Duration::from_secs(400), Duration::from_secs(1500));
    while !done(model) {
        assert!(
            !watchdog.expired(model.received()),
            "timed out waiting for {what}"
        );
        model.poll();
        if let Some(menu) = model.menu().filter(|m| m.context == "DEBUG_MSG")
            && !model.busy()
        {
            let id = menu.choices.first().map(|c| c.id.clone());
            id.iter().for_each(|id| model.choose(id));
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
#[ignore = "requires BN_ENGINE"]
fn live_light_levels_of_the_first_tutorial_view() {
    let engine = std::env::var_os("BN_ENGINE").expect("set BN_ENGINE");
    let transport = StdioTransport::spawn_fresh(engine.as_ref()).expect("engine starts");
    let mut model = EngineModel::new(Box::new(transport));
    wait_for(&mut model, "the main menu", |m| m.live());
    common::start_tutorial(&mut model);
    wait_for(&mut model, "the map", |m| m.size().x > 0);
    wait_for(&mut model, "idle", |m| !m.busy());
    std::thread::sleep(Duration::from_secs(3));
    model.poll();
    let size = model.size();
    let mut counts = BTreeMap::<String, usize>::new();
    let mut grid = String::new();
    for y in 0..size.y {
        for x in 0..size.x {
            let cell = model.cell(Pos { x, y });
            let known = !cell.layers.is_empty() || cell.glyph != ' ';
            let key = match (known, cell.remembered, cell.light) {
                (false, _, _) => "unknown".to_string(),
                (true, true, _) => "remembered".to_string(),
                (true, false, Some(l)) => format!("light {l}"),
                (true, false, None) => "visible without light".to_string(),
            };
            *counts.entry(key.clone()).or_default() += 1;
            grid.push(match (known, cell.remembered, cell.light) {
                (false, _, _) => ' ',
                (true, true, _) => 'm',
                (true, false, Some(l)) => char::from_digit(u32::from(l), 10).unwrap_or('?'),
                (true, false, None) => 'v',
            });
        }
        grid.push('\n');
    }
    println!(
        "LIGHT view {}x{} player {:?}",
        size.x,
        size.y,
        model.player()
    );
    println!("LIGHT {counts:?}");
    println!("LIGHT grid\n{grid}");
}
