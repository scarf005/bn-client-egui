//! Against a real engine: a click on a prompt's choice and a double click on a door act on the first press, and
//! the windows of a play session are saved as pictures at 1280x800 and 1920x1080.
//!
//! `BN_ENGINE=<engine> SHOT_DIR=<dir> cargo test -p bn-client-egui --test it live_beta_ui:: -- --ignored --nocapture`
use crate::common::live::{H, english_transport, new_game, root, run_until, save, settle};
use bn_client_egui::App;
use bn_client_egui::EngineModel;
use bn_client_egui::protocol::InvokeArgs;
use bn_client_egui::{GameModel, MenuModel};
use eframe::egui;
use egui_kittest::kittest::Queryable;
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
