//! A tile the tileset marks `animated` cycles through its sprites as the game's tiles view does: each variant
//! shows for its weight in animation frames of 17 ms, and a map showing one keeps repainting.
mod common;
use bn_client_egui::display::Display;
use bn_client_egui::{Cell, Layer};
use eframe::egui;
use std::time::Duration;

/// The texture spot of the sprite one paint of `cell` at `time` seconds lays down, and how soon that frame asks
/// to be painted again; from the second paint after the tileset has loaded, when its sheets no longer wake the
/// UI.
fn painted_at(
    ctx: &egui::Context,
    display: &mut Display,
    cell: &Cell,
    time: f64,
) -> (egui::Pos2, Duration) {
    let mut loaded = false;
    for _ in 0..600 {
        let mut painted = false;
        let input = egui::RawInput {
            time: Some(time),
            ..Default::default()
        };
        let mut out = ctx.run_ui(input, |ui| {
            display.update(ui.ctx());
            let rect = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(4.0, 4.0));
            painted = display.paint(ui.painter(), rect, cell, None);
        });
        let uv = out.shapes.iter().find_map(|s| match &s.shape {
            egui::Shape::Mesh(mesh) => Some(mesh.vertices[0].uv),
            _ => None,
        });
        out.textures_delta.clear();
        if let (true, Some(uv), true) = (painted, uv, display.ready()) {
            if loaded {
                let delay = out.viewport_output[&egui::ViewportId::ROOT].repaint_delay;
                return (uv, delay);
            }
            loaded = true;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    panic!("the tileset never finished loading");
}

fn display(tag: &str, reduced_motion: bool) -> Display {
    let root = common::game_root(tag);
    let settings = root.join("settings.json");
    let json =
        format!(r#"{{"tiles": true, "tileset": "test", "reduced_motion": {reduced_motion}}}"#);
    std::fs::write(&settings, json).unwrap();
    Display::new(Some(&root), Some(settings))
}

fn cell(id: &str) -> Cell {
    Cell {
        layers: vec![Layer {
            id: id.into(),
            ..Layer::default()
        }],
        light: Some(3),
        ..Cell::default()
    }
}

/// The animation frame `n`, as a time in seconds a little past its start.
fn frame(n: u32) -> f64 {
    f64::from(n) * 0.017 + 0.001
}

#[test]
fn an_animated_tile_shows_each_sprite_for_its_weight_in_frames_and_asks_for_the_next_frame() {
    let ctx = egui::Context::default();
    let mut display = display("animated", false);
    let fire = cell("fd_fire");
    let shown = (0..6)
        .map(|n| painted_at(&ctx, &mut display, &fire, frame(n)))
        .collect::<Vec<_>>();
    let uvs = shown.iter().map(|(uv, _)| *uv).collect::<Vec<_>>();
    // Weights 2 and 3: two frames of the first sprite, three of the second, then the first again.
    assert_ne!(uvs[0], uvs[2], "{uvs:?}");
    assert_eq!(
        uvs,
        [uvs[0], uvs[0], uvs[2], uvs[2], uvs[2], uvs[0]],
        "{uvs:?}"
    );
    for (_, delay) in &shown[1..] {
        assert!(
            *delay <= Duration::from_millis(17),
            "repaint after {delay:?}"
        );
    }
}

#[test]
fn with_reduced_motion_an_animated_tile_stays_on_its_first_sprite_and_asks_for_no_repaint() {
    let ctx = egui::Context::default();
    let mut display = display("animated-still", true);
    let fire = cell("fd_fire");
    let first = painted_at(&ctx, &mut display, &fire, frame(0)).0;
    for n in 1..6 {
        let (uv, delay) = painted_at(&ctx, &mut display, &fire, frame(n));
        assert_eq!(uv, first, "frame {n}");
        assert!(delay > Duration::from_secs(1), "repaint after {delay:?}");
    }
}

#[test]
fn a_still_tile_with_variants_keeps_its_sprite_and_asks_for_no_repaint() {
    let ctx = egui::Context::default();
    let mut display = display("still", false);
    let grass = cell("grass");
    let first = painted_at(&ctx, &mut display, &grass, frame(0)).0;
    for n in 1..6 {
        let (uv, delay) = painted_at(&ctx, &mut display, &grass, frame(n));
        assert_eq!(uv, first, "frame {n}");
        assert!(delay > Duration::from_secs(1), "repaint after {delay:?}");
    }
}
