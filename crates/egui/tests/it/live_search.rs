//! Against a real engine: the search box above a filterable list narrows it as it is typed, and the windows are
//! saved as pictures at 1920x1080 and 1290x830.
//!
//! `BN_ENGINE=<engine> SHOT_DIR=<dir> cargo test -p bn-client-egui --test it live_search:: -- --ignored --nocapture`
use crate::common::live::{H, english_transport, new_game, pick, root, run_until, save, settle};
use bn_client_egui::protocol::Component;
use bn_client_egui::{App, EngineModel, GameModel};
use eframe::egui;
use egui_kittest::kittest::{NodeT, Queryable};

const SIZES: [(f32, f32); 2] = [(1920.0, 1080.0), (1290.0, 830.0)];

fn client<'a>(size: (f32, f32)) -> H<'a> {
    let dir = std::env::temp_dir().join(format!("bn-search-{}-{}", std::process::id(), size.0));
    std::fs::create_dir_all(&dir).unwrap();
    let settings = dir.join("settings.json");
    std::fs::write(&settings, r#"{"tiles":false,"tileset":null}"#).unwrap();
    let app = App::new(EngineModel::new(Box::new(english_transport())))
        .with_settings(settings)
        .with_root(root());
    let mut h = crate::common::shot_harness(egui::vec2(size.0, size.1), app);
    h.input_mut().max_texture_side = Some(8192);
    h
}

/// The filter text the list on top holds.
fn engine_filter(h: &H) -> Option<String> {
    let screen = h.state().model.screens().last()?.clone();
    screen.components().into_iter().find_map(|c| match c {
        Component::List(list) => Some(list.filter.clone().unwrap_or_default()),
        _ => None,
    })
}

fn search<'a>(h: &'a H) -> egui_kittest::Node<'a> {
    h.get_by_role_and_label(egui::accesskit::Role::TextInput, "Search")
}

/// Types `text` one key at a time, each answered by the engine before the next, as a player's pace allows.
fn type_slowly(h: &mut H, text: &str) {
    for c in text.chars() {
        h.event(egui::Event::Text(c.to_string()));
        h.run_steps(2);
        run_until(h, "the typed letter", 60, |h| !h.state().model.busy());
    }
    settle(h);
}

fn focus_search(h: &mut H) {
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::F);
    h.run_steps(3);
    assert!(search(h).accesskit_node().is_focused());
}

#[test]
#[ignore = "needs BN_ENGINE and a Vulkan adapter"]
fn the_search_box_narrows_the_debug_menu_and_the_item_spawner_as_it_is_typed() {
    for size in SIZES {
        let mut h = client(size);
        new_game(&mut h, "new_game:play_now_default");
        let width = size.0 as u32;
        assert!(h.state_mut().model.invoke_action("debug"));
        run_until(&mut h, "the debug menu", 60, |h| {
            !h.state().model.busy() && h.query_by_label_contains("Spawning").is_some()
        });
        settle(&mut h);
        save(&mut h, &format!("debug-menu-{width}.png"));
        assert!(
            search(&h)
                .accesskit_node()
                .value()
                .unwrap_or_default()
                .is_empty()
        );

        focus_search(&mut h);
        type_slowly(&mut h, "tele");
        assert_eq!(engine_filter(&h).as_deref(), Some("tele"));
        assert!(h.query_by_label_contains("Teleport").is_some());
        assert!(h.query_by_label_contains("Spawning").is_none());
        assert_eq!(search(&h).accesskit_node().value().as_deref(), Some("tele"));
        save(&mut h, &format!("debug-menu-tele-{width}.png"));

        // Escape clears the box and the filter; the list stays open and is the engine's.
        h.key_press(egui::Key::Escape);
        run_until(&mut h, "the cleared filter", 60, |h| {
            !h.state().model.busy()
        });
        settle(&mut h);
        assert_eq!(engine_filter(&h).as_deref(), Some(""));
        assert!(h.query_by_label_contains("Spawning").is_some());
        save(&mut h, &format!("debug-menu-cleared-{width}.png"));

        // The item spawner is a long list: the engine, not the client, narrows it.
        h.key_press(egui::Key::Escape);
        h.run_steps(2);
        pick(&mut h, "Spawning");
        pick(&mut h, "Spawn an item");
        settle(&mut h);
        save(&mut h, &format!("wish-{width}.png"));
        focus_search(&mut h);
        type_slowly(&mut h, "shirt");
        assert_eq!(engine_filter(&h).as_deref(), Some("shirt"));
        assert!(h.query_all_by_label_contains("shirt").count() > 1);
        save(&mut h, &format!("wish-shirt-{width}.png"));

        // Down leaves the box for the first result, which then takes the keyboard.
        h.key_press(egui::Key::ArrowDown);
        h.run_steps(3);
        assert!(!search(&h).accesskit_node().is_focused());
        settle(&mut h);
        save(&mut h, &format!("wish-shirt-row-{width}.png"));
    }
}
