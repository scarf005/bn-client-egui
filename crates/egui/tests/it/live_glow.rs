//! Against a real engine: at midnight a lit torch carried by the avatar lights the squares around it in its
//! colour, which the map adds over their sprites; pictures of the flat and the 3D map.
//!
//! `BN_ENGINE=<engine> SHOT_DIR=<dir> cargo test --release -p bn-client-egui --test it live_glow:: -- --ignored
//! --nocapture`; `BN_ROOT` names the game root when it is not beside the engine.
use crate::common::live::{choose, dismiss_popups, pick, run_until, save, settle};
use bn_client_egui::transport::game_root;
use bn_client_egui::{App, EngineModel, GameModel, Glow, MenuModel, Pos, Rgb};
use eframe::egui;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;

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

/// The coloured light on the squares within two of the avatar.
fn glows(h: &H) -> Vec<Glow> {
    let me = h.state().model.player();
    (-2..=2)
        .flat_map(|y| (-2..=2).map(move |x| (x, y)))
        .filter_map(|(x, y)| {
            h.state()
                .model
                .cell(Pos {
                    x: me.x + x,
                    y: me.y + y,
                })
                .glow
        })
        .collect()
}

#[test]
#[ignore = "requires BN_ENGINE pointing at an engine binary"]
fn a_lit_torch_at_midnight_tints_the_squares_around_the_avatar() {
    let engine = std::env::var_os("BN_ENGINE").expect("set BN_ENGINE");
    let root =
        game_root(engine.as_ref(), std::env::var_os("BN_ROOT").map(Into::into)).expect("game root");
    let dir = std::env::temp_dir().join(format!("bn-glow-{}", std::process::id()));
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
    assert!(glows(&h).is_empty(), "coloured light before the torch");

    // Midnight of the same day.
    assert!(h.state_mut().model.invoke_action("debug"));
    pick(&mut h, "Map");
    pick(&mut h, "Change time");
    pick(&mut h, "hour");
    fill(&mut h, "0");
    dismiss_popups(&mut h);
    settle(&mut h);
    save(&mut h, "midnight-2d.png");
    assert!(h.state_mut().model.invoke_action("debug"));
    pick(&mut h, "Spawning");
    pick(&mut h, "Spawn an item");
    // The list's filter, for an item past its first page.
    assert!(h.state_mut().model.press("/"));
    fill(&mut h, "torch (lit)");
    pick(&mut h, "torch (lit)");
    if h.state().model.menu().and_then(|m| m.field).is_some() {
        fill(&mut h, "1");
    }
    dismiss_popups(&mut h);
    // A turn passes, so the engine lights the map anew.
    h.event(egui::Event::Text(".".into()));
    run_until(&mut h, "the wait", 120, |h| {
        !h.state().model.busy() && !glows(h).is_empty()
    });
    settle(&mut h);
    let torch = Rgb(0xff, 0xb0, 0x5f);
    let lit = glows(&h);
    println!("LIVE glows around the avatar: {lit:?}");
    assert!(lit.iter().all(|g| g.color == torch), "{lit:?}");
    save(&mut h, "torch-2d.png");
    if h.query_by_label("Reduced motion").is_none() {
        h.get_by_label("Client settings").click();
        h.run_steps(3);
    }
    h.get_by_label("3D").click();
    h.run_steps(3);
    settle(&mut h);
    save(&mut h, "torch-3d.png");
}
