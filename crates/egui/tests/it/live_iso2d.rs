//! The flat map of an isometric tileset against a real engine: pictures, and the layers the tileset has no
//! sprite or stand-in for.
//!
//! `BN_ENGINE=<engine> BN_ROOT=<game root with gfx/Ultica_iso> SHOT_DIR=<dir> cargo test --release -p
//! bn-client-egui --test it live_iso2d:: -- --ignored --nocapture`. `BN_ISO_TILESET` names another tileset,
//! `BN_ISO_SIZE=<width>x<height>` the window.
use crate::common::live::{english_transport, new_game, root, save};
use bn_client_egui::{App, EngineModel, GameModel};
use eframe::egui;
use egui_kittest::Harness;
use std::collections::BTreeMap;

#[test]
#[ignore = "requires BN_ENGINE and a game root with the Ultica_iso tileset"]
fn an_isometric_tileset_on_the_flat_map_and_the_layers_it_cannot_draw() {
    let root = root();
    let dir = std::env::temp_dir().join(format!("bn-iso2d-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let settings = dir.join("settings.json");
    let tileset = std::env::var("BN_ISO_TILESET").unwrap_or("Ultica_iso".into());
    std::fs::write(
        &settings,
        format!(r#"{{"tiles":true,"tileset":"{tileset}","three_d":false}}"#),
    )
    .unwrap();
    let app = App::new(EngineModel::new(Box::new(english_transport())))
        .with_settings(settings)
        .with_root(root.clone())
        .with_target_format(eframe::wgpu::TextureFormat::Rgba8Unorm);
    let window = std::env::var("BN_ISO_SIZE")
        .ok()
        .and_then(|v| {
            let (w, h) = v.split_once('x')?;
            Some(egui::vec2(w.parse().ok()?, h.parse().ok()?))
        })
        .unwrap_or(egui::vec2(1920.0, 1080.0));
    let mut h = Harness::builder()
        .with_size(window)
        .with_step_dt(1.0 / 60.0)
        .wgpu()
        .build_ui_state(|ui, app: &mut App<EngineModel>| app.show(ui), app);
    bn_client_egui::fonts::install(&h.ctx, &root);
    new_game(&mut h, "new_game:play_now");
    for _ in 0..120 {
        h.run_steps(2);
        std::thread::sleep(std::time::Duration::from_millis(30));
    }
    save(
        &mut h,
        &format!("{tileset}-{}x{}.png", window.x as u32, window.y as u32),
    );
    let m = &h.state().model;
    let size = m.size();
    let (tileset, _) = h.state().display().tileset().expect("a tileset");
    let season = m.season();
    // The layers with no tile: those drawn as a stand-in and those drawn as the glyph text, by id.
    let mut stood = BTreeMap::<String, (usize, char)>::new();
    let mut bare = BTreeMap::<String, (usize, char)>::new();
    let mut deep = 0;
    for y in 0..size.y {
        for x in 0..size.x {
            let cell = m.cell(bn_client_egui::Pos { x, y });
            deep += usize::from(cell.depth > 0);
            for l in &cell.layers {
                let q = bn_client_egui::display::query(l, season.as_deref());
                if l.glyph == '\0' || tileset.choose(&q).is_some() {
                    continue;
                }
                let drawn = tileset.choose_or_stand_in(&q, l.glyph, l.color_index);
                let list = if drawn.is_some() {
                    &mut stood
                } else {
                    &mut bare
                };
                list.entry(l.id.clone()).or_insert((0, l.glyph)).0 += 1;
            }
        }
    }
    println!(
        "LIVE model {}x{}, {deep} squares seen through open air",
        size.x, size.y
    );
    println!("LIVE stand-ins {stood:?}");
    println!("LIVE bare {bare:?}");
}
