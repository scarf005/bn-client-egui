//! Against a real engine: a digit typed in the drop list opens its count prompt with that digit, as the native
//! list reads it, though the list binds the key to marking a row too; pictures of the list and the prompt.
//!
//! `BN_ENGINE=<engine> SHOT_DIR=<dir> cargo test --release -p bn-client-egui --test it live_count:: -- --ignored
//! --nocapture`; `BN_ROOT` names the game root when it is not beside the engine.
use crate::common::live::{choose, dismiss_popups, run_until, save, settle};
use bn_client_egui::transport::game_root;
use bn_client_egui::{App, EngineModel, GameModel, MenuModel};
use eframe::egui;
use egui_kittest::Harness;

type H<'a> = Harness<'a, App<EngineModel>>;

/// Answers the prompt on top with `text`.
fn fill(h: &mut H, text: &str) {
    run_until(h, "the prompt", 60, |h| {
        h.state().model.menu().and_then(|m| m.field).is_some()
    });
    let field = h.state().model.menu().and_then(|m| m.field).unwrap();
    h.state_mut().model.fill(&field.id, text, true);
    run_until(h, text, 60, |h| !h.state().model.busy());
    h.run_steps(3);
}

#[test]
#[ignore = "requires BN_ENGINE pointing at an engine binary"]
fn a_digit_typed_in_the_drop_list_opens_its_count_prompt_with_it() {
    let engine = std::env::var_os("BN_ENGINE").expect("set BN_ENGINE");
    let root =
        game_root(engine.as_ref(), std::env::var_os("BN_ROOT").map(Into::into)).expect("game root");
    let dir = std::env::temp_dir().join(format!("bn-count-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let settings = dir.join("settings.json");
    std::fs::write(&settings, r#"{"tiles":true,"tileset":"UltimateCataclysm"}"#).unwrap();
    let transport = crate::common::live::english_transport();
    let app = App::new(EngineModel::new(Box::new(transport)))
        .with_settings(settings)
        .with_root(root.clone())
        .with_target_format(eframe::wgpu::TextureFormat::Rgba8Unorm);
    let mut h = Harness::builder()
        .with_size(egui::vec2(1280.0, 720.0))
        .with_step_dt(1.0 / 60.0)
        .wgpu()
        .build_ui_state(|ui, app: &mut App<EngineModel>| app.show(ui), app);
    bn_client_egui::fonts::install(&h.ctx, &root);
    run_until(&mut h, "the main menu", 120, |h| {
        h.state().model.menu().is_some()
    });
    choose(&mut h, "menu:tab", "new_game");
    choose(&mut h, "menu:entry", "new_game:play_now_default");
    run_until(&mut h, "the map", 900, |h| h.state().model.size().x > 0);
    run_until(&mut h, "the settled game", 300, |h| {
        h.state().model.menu().is_none() && !h.state().model.busy()
    });
    dismiss_popups(&mut h);
    settle(&mut h);

    assert!(h.state_mut().model.invoke_action("drop"));
    run_until(&mut h, "the drop list", 120, |h| {
        !h.state().model.busy()
            && h.state()
                .model
                .screens()
                .last()
                .is_some_and(|s| s.affordances.iter().any(|a| a.id == "inventory:digit:6"))
    });
    settle(&mut h);
    save(&mut h, "drop-list.png");
    h.event(egui::Event::Text("6".into()));
    run_until(&mut h, "the count prompt", 120, |h| {
        !h.state().model.busy() && h.state().model.menu().and_then(|m| m.field).is_some()
    });
    settle(&mut h);
    save(&mut h, "count-prompt.png");
    let field = h.state().model.menu().and_then(|m| m.field).unwrap();
    println!("LIVE count prompt: {field:?}");
    assert_eq!(field.value, "6");
    // One of the highlighted stack, marked in the list.
    fill(&mut h, "1");
    settle(&mut h);
    save(&mut h, "counted.png");
}
