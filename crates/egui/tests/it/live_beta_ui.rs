//! Against a real engine: a click on a prompt's choice and a double click on a door act on the first press, and
//! the windows of a play session are saved as pictures at 1280x800 and 1920x1080.
//!
//! `BN_ENGINE=<engine> SHOT_DIR=<dir> cargo test -p bn-client-egui --test it live_beta_ui:: -- --ignored --nocapture`
use crate::common::live::{
    H, english_transport, new_game, pick, root, run_until, save, save_legible, settle,
};
use bn_client_egui::App;
use bn_client_egui::EngineModel;
use bn_client_egui::protocol::{InvokeArgs, ScreenRole};
use bn_client_egui::{GameModel, MenuModel};
use eframe::egui;
use egui_kittest::kittest::{NodeT, Queryable};
use std::time::Duration;

const SIZES: [(f32, f32); 2] = [(1280.0, 800.0), (1920.0, 1080.0)];

/// The client in ASCII over a new English engine, in a window of `size`.
fn client<'a>(size: (f32, f32)) -> H<'a> {
    let dir = std::env::temp_dir().join(format!("bn-beta-ui-{}-{}", std::process::id(), size.0));
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

fn click(h: &mut H, pos: egui::Pos2) {
    h.hover_at(pos);
    h.run_steps(2);
    for pressed in [true, false] {
        h.event(egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        });
        h.run_steps(1);
    }
}

/// Wields the inventory item `row` through the engine's wield list.
fn wield(h: &mut H, row: &str) {
    run_until(h, "the map", 60, |h| {
        !h.state().model.busy() && h.state().model.menu().is_none()
    });
    assert!(h.state_mut().model.invoke_action("wield"));
    run_until(h, "the wield list", 60, |h| {
        !h.state().model.busy()
            && h.state()
                .model
                .screens()
                .last()
                .is_some_and(|s| s.title.is_some())
    });
    let args = InvokeArgs {
        row: Some(row.into()),
        ..Default::default()
    };
    h.state_mut()
        .model
        .invoke_affordance("list:inventory:choose", args)
        .unwrap();
    run_until(h, "the wield", 60, |h| !h.state().model.busy());
    settle(h);
}

/// Frames until the prompt on top is gone, at most `frames`.
fn gone_within(h: &mut H, frames: usize) -> bool {
    for _ in 0..frames {
        h.run_steps(1);
        std::thread::sleep(Duration::from_millis(30));
        if h.state().model.menu().is_none() {
            return true;
        }
    }
    false
}

/// The prompt's first choice, "Store in inventory", clicked with the mouse: each click must act on its own.
#[test]
#[ignore = "needs BN_ENGINE and a Vulkan adapter"]
fn a_click_on_a_prompt_choice_acts_on_the_first_press() {
    let mut h = client(SIZES[0]);
    new_game(&mut h, "new_game:play_now_default");
    let mut missed = 0;
    wield(&mut h, "item:12");
    for trial in 0..10 {
        // Wielding the other of the two items asks what to do with the one in hand.
        wield(&mut h, ["item:13", "item:12"][trial % 2]);
        run_until(&mut h, "the prompt", 60, |h| {
            h.state().model.menu().is_some()
        });
        settle(&mut h);
        let at = h
            .get_by_label_contains("Store in inventory")
            .rect()
            .center();
        if trial == 0 {
            save(&mut h, "prompt.png");
        }
        click(&mut h, at);
        if !gone_within(&mut h, 20) {
            missed += 1;
            println!(
                "MISSED trial {trial}: menu {:?} busy {} command {:?}",
                h.state().model.menu().map(|m| m.message),
                h.state().model.busy(),
                h.state().model.command()
            );
            let at = h
                .get_by_label_contains("Store in inventory")
                .rect()
                .center();
            click(&mut h, at);
            assert!(
                gone_within(&mut h, 40),
                "the second click did not act either"
            );
        }
        run_until(&mut h, "idle", 60, |h| !h.state().model.busy());
        settle(&mut h);
    }
    assert_eq!(missed, 0, "clicks that did nothing");
}

/// Where the message log's lines are drawn now, in tenths of a point.
fn log_positions(h: &H) -> Vec<(String, [i32; 4])> {
    let lines = h
        .state()
        .model
        .native_log()
        .iter()
        .map(|l| l.shown())
        .collect::<Vec<_>>();
    lines
        .iter()
        .flat_map(|line| h.query_all_by_label_contains(line))
        .filter_map(|n| {
            let node = n.accesskit_node();
            let r = node.bounding_box()?;
            Some((
                node.value().unwrap_or_default().to_string(),
                [r.x0, r.y0, r.x1, r.y1].map(|v| (v * 10.0) as i32),
            ))
        })
        .collect()
}

/// The message log of a sidebar dragged narrow does not move while nothing new is logged.
#[test]
#[ignore = "needs BN_ENGINE and a Vulkan adapter"]
fn a_message_log_in_a_narrow_sidebar_stays_still() {
    let mut h = client(SIZES[0]);
    new_game(&mut h, "new_game:play_now_default");
    for _ in 0..6 {
        h.state_mut().model.invoke_action("pause");
        run_until(&mut h, "the turn", 60, |h| !h.state().model.busy());
    }
    settle(&mut h);
    let handle = h.get_by_label("Resize sidebar").rect().center();
    for width in [0.0, 160.0, 300.0] {
        let to = handle + egui::vec2(width, 0.0);
        h.hover_at(handle);
        h.run_steps(2);
        for (pos, pressed) in [(handle, true), (to, true), (to, false)] {
            h.event(egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            });
            h.hover_at(pos);
            h.run_steps(2);
        }
        settle(&mut h);
        let mut seen = vec![];
        for _ in 0..40 {
            h.run_steps(1);
            std::thread::sleep(Duration::from_millis(16));
            seen.push(log_positions(&h));
        }
        let changing = seen.windows(2).filter(|w| w[0] != w[1]).count();
        println!(
            "LOGJUMP width +{width}: lines {} moved in {changing} of 39 frames",
            seen[0].len()
        );
        save(&mut h, &format!("log-narrow-{width}.png"));
        assert_eq!(changing, 0, "the log moved at +{width}");
    }
}

/// The debug menu and its item spawner, saved at each size as `<name>-<width>.png`.
#[test]
#[ignore = "needs BN_ENGINE and a Vulkan adapter"]
fn the_debug_windows_are_saved_as_pictures() {
    for size in SIZES {
        let mut h = client(size);
        new_game(&mut h, "new_game:play_now_default");
        let width = size.0 as u32;
        assert!(h.state_mut().model.invoke_action("debug"));
        pick(&mut h, "Spawning");
        pick(&mut h, "Spawn an item");
        settle(&mut h);
        save_legible(&mut h, &format!("wish-{width}.png"));
        assert!(
            h.state_mut()
                .model
                .invoke_affordance("FILTER", Default::default())
                .is_ok()
        );
        run_until(&mut h, "the filter prompt", 60, |h| !h.state().model.busy());
        settle(&mut h);
        save_legible(&mut h, &format!("wish-filter-{width}.png"));
    }
}

/// The windows a first session opens, saved at each size as `<name>-<width>.png`: the map with its sidebar, the
/// client's settings, and the engine's own screens, each closed again before the next opens.
#[test]
#[ignore = "needs BN_ENGINE and a Vulkan adapter"]
fn the_windows_of_a_session_are_saved_as_pictures() {
    for size in SIZES {
        let mut h = client(size);
        new_game(&mut h, "new_game:play_now_default");
        let width = size.0 as u32;
        settle(&mut h);
        save_legible(&mut h, &format!("map-{width}.png"));
        h.get_by_label("Client settings").click();
        settle(&mut h);
        save_legible(&mut h, &format!("settings-{width}.png"));
        h.get_by_label("Close window").click();
        h.run_steps(3);
        for action in [
            "inventory",
            "wait",
            "player_data",
            "craft",
            "open_options",
            "messages",
            "help",
        ] {
            if !h.state_mut().model.invoke_action(action) {
                println!("no action {action}");
                continue;
            }
            run_until(&mut h, action, 60, |h| !h.state().model.busy());
            settle(&mut h);
            save_legible(&mut h, &format!("{action}-{width}.png"));
            h.key_press(egui::Key::Escape);
            run_until(&mut h, "the map", 60, |h| {
                h.state()
                    .model
                    .screens()
                    .last()
                    .is_some_and(|s| s.role == ScreenRole::Map)
                    && !h.state().model.busy()
            });
            h.run_steps(3);
        }
    }
}
