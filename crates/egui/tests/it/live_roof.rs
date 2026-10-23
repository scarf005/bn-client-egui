//! Against a real engine: in the first-person view inside the evac shelter, a ceiling covers the room and the rain
//! the debug menu forces does not fall on the avatar; pictures of the view in two window sizes.
//!
//! `BN_ENGINE=<engine> SHOT_DIR=<dir> cargo test --release -p bn-client-egui --test it live_roof:: -- --ignored
//! --nocapture`; `BN_ROOT` names the game root when it is not beside the engine.
use crate::common::live::{choose, dismiss_popups, pick, run_until, save, settle};
use bn_client_egui::protocol::Precipitation;
use bn_client_egui::transport::game_root;
use bn_client_egui::{App, EngineModel, GameModel, MenuModel};
use eframe::egui;
use egui_kittest::Harness;

const FIRST: &str =
    r#"{"tiles":true,"tileset":"HitButton_iso","three_d":true,"first_person":true}"#;
const ORBIT: &str = r#"{"tiles":true,"tileset":"HitButton_iso","three_d":true}"#;

fn indoors_in_the_rain(size: egui::Vec2, name: &str, settings_json: &str) {
    let engine = std::env::var_os("BN_ENGINE").expect("set BN_ENGINE");
    let root =
        game_root(engine.as_ref(), std::env::var_os("BN_ROOT").map(Into::into)).expect("game root");
    let dir = std::env::temp_dir().join(format!("bn-roof-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let settings = dir.join("settings.json");
    std::fs::write(&settings, settings_json).unwrap();
    let transport = crate::common::live::english_transport();
    let app = App::new(EngineModel::new(Box::new(transport)))
        .with_settings(settings)
        .with_root(root.clone())
        .with_target_format(eframe::wgpu::TextureFormat::Rgba8Unorm);
    let mut h = Harness::builder()
        .with_size(size)
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
    save(&mut h, &format!("{name}-00-dry.png"));
    assert!(h.state_mut().model.invoke_action("debug"));
    pick(&mut h, "Map");
    pick(&mut h, "Change weather");
    pick(&mut h, "Heavy Rain");
    dismiss_popups(&mut h);
    // A turn passes, so the engine draws the new weather.
    h.event(egui::Event::Text(".".into()));
    run_until(&mut h, "the rain", 120, |h| {
        !h.state().model.busy()
            && h.state()
                .model
                .sky()
                .is_some_and(|s| s.precipitation != Precipitation::None)
    });
    settle(&mut h);
    let me = h.state().model.player();
    let roofed = h.state().model.cell(me).roofed;
    println!("LIVE avatar roofed: {roofed}");
    assert!(roofed, "the evac shelter has a roof");
    save(&mut h, &format!("{name}-01-indoors-in-rain.png"));
}

#[test]
#[ignore = "requires BN_ENGINE pointing at an engine binary"]
fn live_roof_covers_the_room_and_keeps_the_rain_out() {
    indoors_in_the_rain(egui::vec2(1920.0, 1080.0), "fp-1920", FIRST);
    indoors_in_the_rain(egui::vec2(1290.0, 830.0), "fp-1290", FIRST);
    indoors_in_the_rain(egui::vec2(1920.0, 1080.0), "orbit-1920", ORBIT);
}
