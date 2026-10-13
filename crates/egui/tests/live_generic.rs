//! Needs a real engine build: `BN_ENGINE=/path/to/cataclysm-bn-tiles cargo test -p bn-client-egui --test live_generic -- --ignored --nocapture`
use bn_client_egui::transport::StdioTransport;
use bn_client_egui::watchdog::Watchdog;
use bn_client_egui::{App, EngineModel, GameModel};
use eframe::egui;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use std::time::Duration;

type H<'a> = Harness<'a, App<EngineModel>>;

fn run_until(h: &mut H, what: &str, secs: u64, mut done: impl FnMut(&mut H) -> bool) {
    let mut watchdog = Watchdog::new(Duration::from_secs(secs), Duration::from_secs(600));
    while !done(h) {
        assert!(
            !watchdog.expired(h.state().model.received()),
            "timed out waiting for {what}"
        );
        h.run_steps(1);
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// The tutorial map, with its pop-ups closed.
fn start<'a>() -> H<'a> {
    let engine = std::env::var_os("BN_ENGINE").expect("set BN_ENGINE");
    let transport = StdioTransport::spawn_fresh(engine.as_ref()).expect("engine starts");
    let model = EngineModel::new(Box::new(transport));
    let mut h = Harness::builder()
        .with_size(egui::vec2(1300.0, 800.0))
        .build_ui_state(
            |ui, app: &mut App<EngineModel>| app.show(ui),
            App::new(model),
        );
    run_until(&mut h, "the main menu", 90, |h| {
        h.query_by_label("New Game").is_some()
    });
    h.run_steps(10);
    h.get_by_label("New Game").click();
    run_until(&mut h, "the tutorial map", 300, |h| {
        let chosen = h
            .state()
            .model
            .command()
            .is_some_and(|c| c.target.as_deref() == Some("new_game:tutorial"));
        if !chosen && !h.state().model.busy() && h.query_by_label("Tutorial").is_some() {
            h.get_by_label("Tutorial").click();
        }
        h.state().model.size().x > 0
    });
    h
}

fn settle(h: &mut H, what: &str) {
    run_until(h, what, 30, |h| !h.state().model.busy());
    h.run_steps(5);
}

fn close_popups(h: &mut H) {
    let mut quiet = 0;
    while quiet < 10 {
        if h.query_by_label("Yes").is_none() && h.state().model.menu().is_some() {
            h.key_press(egui::Key::Escape);
            quiet = 0;
        } else {
            quiet += 1;
        }
        h.run_steps(1);
        std::thread::sleep(Duration::from_millis(100));
    }
}

#[test]
#[ignore = "requires BN_ENGINE pointing at an engine binary"]
fn live_debug_menu_filters_to_tele_and_a_query_popup_declines() {
    let mut h = start();
    close_popups(&mut h);

    assert!(h.state_mut().model.invoke_action("debug"));
    run_until(&mut h, "the debug menu", 60, |h| {
        h.query_by_label_contains("Filter").is_some()
    });
    h.get_by_label_contains("Spawning");
    h.get_by_label_contains("Filter").click();
    run_until(&mut h, "the filter prompt", 30, |h| {
        h.query_by_label_contains("Filter").is_none()
    });
    h.run_steps(5);
    for c in "tele".chars() {
        h.event(egui::Event::Text(c.to_string()));
        h.run_steps(2);
        settle(&mut h, "the typed letter");
    }
    run_until(&mut h, "the filtered rows", 30, |h| {
        h.query_by_label_contains("Teleport").is_some()
            && h.query_by_label_contains("Spawning").is_none()
    });
    h.key_press(egui::Key::Escape);
    settle(&mut h, "the prompt to close");
    h.key_press(egui::Key::Escape);
    run_until(&mut h, "the debug menu to close", 30, |h| {
        h.query_by_label("Confirm Choice").is_none()
    });

    assert!(h.state_mut().model.invoke_action("save"));
    run_until(&mut h, "the query popup", 60, |h| {
        if h.query_by_label("Yes").is_none() && h.state().model.menu().is_some() {
            h.key_press(egui::Key::Escape);
        }
        h.query_by_label("Yes").is_some()
    });
    h.get_by_label("No").click();
    run_until(&mut h, "the popup to close", 30, |h| {
        h.query_by_label("Yes").is_none()
    });
}
