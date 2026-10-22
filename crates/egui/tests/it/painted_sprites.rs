//! Sprites painted with a part's colour are made in the background without a thread each.
use bn_client_egui::display::Display;
use bn_client_egui::{Cell, Glow, Layer, Rgb};
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
    let root = crate::common::game_root("painted");
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

/// The colours of the meshes one paint of `cell` lays down, once the tileset has loaded.
fn painted_tones(display: &mut Display, cell: &Cell) -> Vec<egui::Color32> {
    let ctx = egui::Context::default();
    for _ in 0..600 {
        let mut painted = false;
        let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
            display.update(ui.ctx());
            let rect = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(4.0, 4.0));
            painted = display.paint(ui.painter(), rect, cell, None);
        });
        let tones = out
            .shapes
            .iter()
            .filter_map(|s| match &s.shape {
                egui::Shape::Mesh(mesh) => Some(mesh.vertices[0].color),
                _ => None,
            })
            .collect::<Vec<_>>();
        out.textures_delta.clear();
        if painted && !tones.is_empty() {
            return tones;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    panic!("the tileset never finished loading");
}

#[test]
fn coloured_light_adds_its_colour_over_the_sprite_of_a_square_in_sight() {
    let root = crate::common::game_root("glow");
    let settings = root.join("settings.json");
    std::fs::write(&settings, r#"{"tiles": true, "tileset": "test"}"#).unwrap();
    let mut display = Display::new(Some(&root), Some(settings));
    let cell = |glow| Cell {
        layers: vec![Layer {
            id: "t_floor".into(),
            ..Layer::default()
        }],
        light: Some(3),
        glow,
        ..Cell::default()
    };
    assert_eq!(
        painted_tones(&mut display, &cell(None)),
        [egui::Color32::WHITE]
    );
    let fire = Glow {
        color: Rgb(255, 128, 0),
        strength: 255,
    };
    // Full strength adds the colour at the game's highest alpha, 176, and blends nothing away.
    assert_eq!(
        painted_tones(&mut display, &cell(Some(fire))),
        [
            egui::Color32::WHITE,
            egui::Color32::from_rgba_premultiplied(176, 88, 0, 0)
        ]
    );
}
