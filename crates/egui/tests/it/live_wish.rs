//! Against a real engine: the debug item spawner, used with the mouse as a player does, spawns the item and says so.
//!
//! `BN_ENGINE=<engine> SHOT_DIR=<dir> cargo test -p bn-client-egui --test it live_wish:: -- --ignored --nocapture`
use crate::common::live::{H, english_transport, new_game, pick, root, run_until, save, settle};
use bn_client_egui::{App, EngineModel, GameModel};
use eframe::egui;
use egui_kittest::kittest::Queryable;

fn client<'a>(size: (f32, f32)) -> H<'a> {
    let dir = std::env::temp_dir().join(format!("bn-wish-{}-{}", std::process::id(), size.0));
    std::fs::create_dir_all(&dir).unwrap();
    let settings = dir.join("settings.json");
    std::fs::write(&settings, r#"{"tiles":false,"tileset":null}"#).unwrap();
    let app = App::new(EngineModel::new(Box::new(english_transport())))
        .with_settings(settings)
        .with_root(root());
    crate::common::shot_harness(egui::vec2(size.0, size.1), app)
}

/// Clicks the widget labelled `label` and lets the engine answer.
fn click(h: &mut H, label: &str) {
    h.query_all_by_label_contains(label)
        .next()
        .unwrap_or_else(|| panic!("no {label}"))
        .click();
    h.run_steps(3);
    run_until(h, label, 60, |h| !h.state().model.busy());
    settle(h);
}

fn shown(h: &H) -> String {
    let screens = h.state().model.screens();
    let titles = screens
        .iter()
        .map(|s| s.title.as_ref().map(|t| t.shown()).unwrap_or_default())
        .collect::<Vec<_>>();
    format!(
        "screens {titles:?} inventory {:?}",
        h.state().model.inventory()
    )
}

#[test]
#[ignore = "needs BN_ENGINE and a Vulkan adapter"]
fn the_item_spawner_spawns_the_chosen_item_and_says_so() {
    let mut h = client((1290.0, 830.0));
    new_game(&mut h, "new_game:play_now_default");
    assert!(h.state_mut().model.invoke_action("debug"));
    run_until(&mut h, "the debug menu", 60, |h| {
        !h.state().model.busy() && h.query_by_label_contains("Spawning").is_some()
    });
    pick(&mut h, "Spawning");
    pick(&mut h, "Spawn an item");
    settle(&mut h);
    println!("LIVE open: {}", shown(&h));
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::F);
    h.run_steps(3);
    for c in "dress shirt".chars() {
        h.event(egui::Event::Text(c.to_string()));
        h.run_steps(2);
        run_until(&mut h, "the typed letter", 60, |h| !h.state().model.busy());
    }
    settle(&mut h);
    click(&mut h, "dress shirt");
    save(&mut h, "wish-0-row.png");
    assert!(
        h.query_all_by_label_contains("How many?").next().is_some(),
        "{}",
        shown(&h)
    );
    click(&mut h, "Confirm text input");
    save(&mut h, "wish-1-granted.png");
    // The spawner lists the items again for the next wish, and says this one was granted.
    assert!(
        h.query_all_by_label_contains("Wish granted")
            .next()
            .is_some(),
        "{}",
        shown(&h)
    );
    assert!(
        h.query_by_label_contains("Drop ||\u{a0}dress shirt")
            .is_some()
    );
}
