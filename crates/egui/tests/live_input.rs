//! Needs a real engine build: `BN_ENGINE=/path/to/cataclysm-bn-tiles cargo test -p bn-client-egui --test live_input -- --ignored --nocapture`
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
            "timed out waiting for {what}; menu: {:?}",
            h.state().model.menu().map(|m| (m.context, m.message))
        );
        h.run_steps(1);
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn click_settled(h: &mut H, label: &str) {
    let mut seen = None;
    while seen != Some(h.get_by_label(label).rect()) {
        seen = Some(h.get_by_label(label).rect());
        h.run_steps(2);
    }
    h.get_by_label(label).click();
}

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
    settle(&mut h);
    h.run_steps(20);
    h
}

/// The world's mods screen, reached from the main menu by keys.
fn world_screen(h: &mut H) {
    run_until(h, "Custom Character", 60, |h| {
        h.query_by_label("Custom Character").is_some()
    });
    click_settled(h, "Custom Character");
    run_until(h, "the world's mods", 60, |h| {
        h.state()
            .model
            .menu()
            .is_some_and(|m| m.context != "MAIN_MENU")
    });
    settle(h);
}

fn snapshot(h: &H) -> String {
    match h.state().model.menu() {
        Some(m) => format!(
            "context={} message={:?} focus={:?} field={:?} rows={:?}",
            m.context,
            m.message.chars().take(40).collect::<String>(),
            m.focus,
            m.field.as_ref().map(|f| f.value.clone()),
            m.choices
                .iter()
                .take(3)
                .map(|c| (c.label.clone(), c.selected, c.highlighted))
                .collect::<Vec<_>>()
        ),
        None => "no menu".into(),
    }
}

fn settle(h: &mut H) {
    for _ in 0..40 {
        h.run_steps(1);
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn quit_popup(h: &mut H) {
    h.key_press(egui::Key::Escape);
    run_until(h, "the quit popup", 30, |h| {
        h.state()
            .model
            .menu()
            .is_some_and(|m| m.message.contains("Really quit"))
    });
    settle(h);
}

#[test]
#[ignore = "requires BN_ENGINE pointing at an engine binary"]
fn live_up_then_enter_on_the_quit_popup_takes_the_option_the_arrow_moved_to() {
    let mut h = start();
    quit_popup(&mut h);
    println!("LIVE popup opened: {}", snapshot(&h));
    h.key_press(egui::Key::ArrowUp);
    settle(&mut h);
    println!("LIVE after Up: {}", snapshot(&h));
    h.key_press(egui::Key::Enter);
    settle(&mut h);
    println!("LIVE after Enter: {}", snapshot(&h));
    println!("LIVE failure after Enter: {:?}", h.state().model.failure());
    assert!(
        h.state().model.failure().is_some() || !h.state().model.live(),
        "Up moved to Yes and Enter quit the game"
    );
}

#[test]
#[ignore = "requires BN_ENGINE pointing at an engine binary"]
fn live_a_word_typed_fast_into_a_filter_arrives_in_order() {
    let mut h = start();
    world_screen(&mut h);
    h.event(egui::Event::Text("?".into()));
    settle(&mut h);
    h.event(egui::Event::Text("/".into()));
    settle(&mut h);
    println!("LIVE prompt open: {}", snapshot(&h));
    h.get_by_role(egui::accesskit::Role::TextInput).click();
    h.run_steps(3);
    for c in ["m", "o", "d", "e"] {
        h.event(egui::Event::Text(c.into()));
        h.run_steps(1);
    }
    settle(&mut h);
    println!("LIVE after typing: {}", snapshot(&h));
    let field = h
        .state()
        .model
        .menu()
        .and_then(|m| m.field)
        .map(|f| f.value);
    assert_eq!(field.as_deref(), Some("mode"));
}
