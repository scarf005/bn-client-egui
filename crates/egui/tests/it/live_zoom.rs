//! Against a real engine: the flat map fills its room at the farthest and the closest zoom, with the avatar's square
//! in the middle.
//!
//! `BN_ENGINE=<engine> BN_ROOT=<game root> SHOT_DIR=<dir> cargo test --release -p bn-client-egui --test it
//! live_zoom:: -- --ignored --nocapture`.
use crate::common::live::{english_transport, new_game, root, save, settle};
use bn_client_egui::{App, EngineModel, GameModel};
use eframe::egui;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;

type H<'a> = Harness<'a, App<EngineModel>>;

/// Turns the wheel over the map by `lines` and lets the engine send the squares the new zoom asks for.
fn wheel(h: &mut H, lines: f32) {
    let at = h.get_by_label("Map").rect().center();
    h.hover_at(at);
    h.run_steps(2);
    h.event(egui::Event::MouseWheel {
        unit: egui::MouseWheelUnit::Line,
        delta: egui::vec2(0.0, lines),
        phase: egui::TouchPhase::Move,
        modifiers: egui::Modifiers::NONE,
    });
    settle(h);
}

/// The map's room, the drawn squares' rect and the avatar's square's centre.
fn layout(h: &H) -> (egui::Rect, egui::Rect, egui::Pos2) {
    let room = h.get_by_label("Map").rect();
    let m = &h.state().model;
    let (size, me) = (m.size(), m.player());
    let grid = h.state().map_squares().unwrap();
    let cell = grid.width() / size.x as f32;
    (
        room,
        grid,
        grid.min + egui::vec2(me.x as f32 + 0.5, me.y as f32 + 0.5) * cell,
    )
}

#[test]
#[ignore = "requires BN_ENGINE"]
fn the_flat_map_fills_its_room_at_the_farthest_and_the_closest_zoom() {
    let root = root();
    for window in [egui::vec2(1920.0, 1080.0), egui::vec2(1290.0, 830.0)] {
        let dir = std::env::temp_dir().join(format!("bn-zoom-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let settings = dir.join("settings.json");
        std::fs::write(&settings, r#"{"tiles":true,"three_d":false}"#).unwrap();
        let app = App::new(EngineModel::new(Box::new(english_transport())))
            .with_settings(settings)
            .with_root(root.clone())
            .with_target_format(eframe::wgpu::TextureFormat::Rgba8Unorm);
        let mut h = Harness::builder()
            .with_size(window)
            .with_step_dt(1.0 / 60.0)
            .wgpu()
            .build_ui_state(|ui, app: &mut App<EngineModel>| app.show(ui), app);
        bn_client_egui::fonts::install(&h.ctx, &root);
        new_game(&mut h, "new_game:play_now");
        settle(&mut h);
        for (name, lines) in [("farthest", -40.0), ("closest", 60.0)] {
            wheel(&mut h, lines);
            let (room, grid, me) = layout(&h);
            println!("LIVE {window:?} {name}: room {room:?} squares {grid:?} avatar {me:?}");
            save(
                &mut h,
                &format!("zoom-{name}-{}x{}.png", window.x, window.y),
            );
            assert!(
                grid.expand(0.5).contains_rect(room),
                "{name}: {grid:?} covers {room:?}"
            );
            assert!(
                me.distance(room.center()) <= 1.0,
                "{name}: {me:?} in the middle of {room:?}"
            );
        }
    }
}
