//! Sprites painted with a part's colour are made in the background without a thread each.
mod common;
use bn_client_egui::display::Display;
use bn_client_egui::{Cell, Layer, Rgb};
use eframe::egui;

/// The threads this process runs now (Linux).
fn threads() -> usize {
    let status = std::fs::read_to_string("/proc/self/status").unwrap();
    status
        .lines()
        .find_map(|l| l.strip_prefix("Threads:"))
        .unwrap()
        .trim()
        .parse()
        .unwrap()
}

#[test]
fn many_painted_sprites_in_one_frame_start_no_thread_each() {
    let root = common::game_root("painted");
    // A sheet large enough that decoding it outlasts the frame, so the threads waiting on it are still there.
    let big = image::RgbaImage::from_pixel(4096, 4096, image::Rgba([90, 90, 90, 255]));
    big.save(root.join("gfx/test/a.png")).unwrap();
    let settings = root.join("settings.json");
    std::fs::write(&settings, r#"{"tiles": true, "tileset": "test"}"#).unwrap();
    let mut display = Display::new(Some(&root), Some(settings));
    let ctx = egui::Context::default();
    let before = threads();
    ctx.run_ui(egui::RawInput::default(), |ui| {
        display.update(ui.ctx());
        let painter = ui.painter().clone();
        for n in 0..200u32 {
            let layer = Layer {
                id: "t_floor".into(),
                tint_fg: Some(Rgb((n % 256) as u8, (n / 256) as u8 + 1, 7)),
                ..Layer::default()
            };
            let cell = Cell {
                layers: vec![layer],
                ..Cell::default()
            };
            let rect = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(4.0, 4.0));
            display.paint(&painter, rect, &cell, None);
        }
    })
    .textures_delta
    .clear();
    let started = threads().saturating_sub(before);
    assert!(
        started <= 4,
        "{started} threads started for 200 painted sprites"
    );
}
