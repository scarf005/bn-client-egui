//! Needs a real engine build: `BN_ENGINE=/path/to/cataclysm-bn-tiles cargo test -p bn-client-egui --test live_input -- --ignored --nocapture`
mod common;
use bn_client_egui::watchdog::Watchdog;
use bn_client_egui::{App, EngineModel, GameModel, MenuModel};
use eframe::egui;
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
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

/// The enabled button labelled `label`, not a list row of that name (a row is toggled on or off) nor the
/// disabled one of a screen under the top one.
fn button<'n>(h: &'n H, label: &'n str) -> Option<egui_kittest::Node<'n>> {
    h.query_all_by_label(label).find(|n| {
        let node = n.accesskit_node();
        !node.is_disabled() && node.toggled().is_none()
    })
}

/// Where the widget labelled `label` is once it stopped moving.
fn settled(h: &mut H, label: &str) -> egui::Rect {
    let mut seen = None;
    while seen != Some(h.get_by_label(label).rect()) {
        seen = Some(h.get_by_label(label).rect());
        h.run_steps(2);
    }
    h.get_by_label(label).rect()
}

/// Clicks the button labelled `label` once it stopped moving.
fn click_button(h: &mut H, label: &str) {
    let rect = |h: &H| button(h, label).expect(label).rect();
    let mut seen = None;
    while seen != Some(rect(h)) {
        seen = Some(rect(h));
        h.run_steps(2);
    }
    button(h, label).expect(label).click();
}

fn start<'a>() -> H<'a> {
    let transport = common::live::english_transport();
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
    // A list row runs on a double click.
    let at = settled(h, "Custom Character").center();
    common::double_click(h, at);
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
    // The keybindings screen, whose list filter is an affordance that takes the typed text.
    h.event(egui::Event::Text("?".into()));
    run_until(&mut h, "the keybindings", 30, |h| {
        button(h, "Filter").is_some()
    });
    click_button(&mut h, "Filter");
    run_until(&mut h, "the filter prompt", 30, |h| {
        h.query_by_role(egui::accesskit::Role::TextInput).is_some()
    });
    h.get_by_role(egui::accesskit::Role::TextInput).click();
    h.run_steps(3);
    for c in ["m", "o", "d", "e"] {
        h.event(egui::Event::Text(c.into()));
        h.run_steps(1);
    }
    h.key_press(egui::Key::Enter);
    run_until(&mut h, "the filtered list", 30, |h| {
        h.query_by_label("/mode").is_some()
    });
}

/// Whether the top screen asks for a key.
fn asks_for_a_key(h: &H) -> bool {
    let screens = h.state().model.screens();
    let root = screens.last().and_then(|s| s.root.as_ref());
    serde_json::to_value(root)
        .unwrap()
        .to_string()
        .contains(r#""type":"key""#)
}

#[test]
#[ignore = "requires BN_ENGINE pointing at an engine binary"]
fn live_a_key_prompt_takes_an_arrow_as_the_new_key() {
    let mut h = start();
    world_screen(&mut h);
    h.event(egui::Event::Text("?".into()));
    run_until(&mut h, "the keybindings", 30, |h| {
        button(h, "Filter").is_some()
    });
    // Add a local key to the first action.
    h.event(egui::Event::Text("+".into()));
    settle(&mut h);
    h.key_press(egui::Key::Enter);
    run_until(&mut h, "the key prompt", 30, |h| asks_for_a_key(h));
    h.key_press(egui::Key::ArrowDown);
    run_until(&mut h, "the prompt to take the key", 30, |h| {
        !asks_for_a_key(h)
    });
    let title = h
        .state()
        .model
        .screens()
        .last()
        .and_then(|s| s.title.clone());
    assert!(
        title.is_none_or(|t| t.shown() != "Keybindings"),
        "the arrow was the new key, not a move in the list"
    );
}
