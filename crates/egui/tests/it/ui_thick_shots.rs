//! A picture of items and the avatar of a tileset drawn from above in the 3D map, drawn on the GPU, for a person to
//! look at and checked at the pixels where their top must be. Needs a Vulkan adapter, so it only runs when asked:
//! `SHOT_DIR=<dir> cargo test --test it ui_thick_shots:: -- --ignored`.
use crate::common::{fixture, flat_root};
use bn_client_egui::app::CELL;
use bn_client_egui::iso::camera::Viewport;
use bn_client_egui::iso::scene::world_scale;
use bn_client_egui::transport::ScriptedTransport;
use bn_client_egui::{App, EngineModel, GameModel};
use eframe::egui;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use serde_json::{Value, json};
use std::sync::atomic::Ordering;

type H<'a> = Harness<'a, App<EngineModel>>;

/// A grass room with a flower-sized item lying on two squares and the avatar at (3, 4).
fn cells() -> Vec<Value> {
    let look =
        |kind: &str, id: &str| json!({"kind": kind, "id": id, "glyph": "#", "color": "white"});
    (0..=8)
        .flat_map(|x| (1..=7).map(move |y| (x, y)))
        .map(|(x, y)| {
            let mut cell = json!({"at": {"dim": "", "x": x, "y": y, "z": 0}, "known": "visible",
                "terrain": look("terrain", "t_grass")});
            if [(5, 3), (6, 5)].contains(&(x, y)) {
                cell["items"] = json!([look("item", "f_flower")]);
            }
            cell
        })
        .collect()
}

fn harness<'a>(size: egui::Vec2) -> H<'a> {
    let root = flat_root("thick");
    let settings = root.join("settings.json");
    std::fs::write(&settings, r#"{"three_d": true}"#).unwrap();
    let mut subscribe = fixture("subscribe_result");
    subscribe["result"]["avatar"]["look"] = json!({
        "kind": "avatar", "id": "avatar", "glyph": "@", "color": "white", "tile": "mon_a"
    });
    let mut first = fixture("snapshot_part_0");
    first["params"]["cells"] = cells().into();
    let mut last = fixture("snapshot_part_1");
    last["params"]["cells"] = json!([]);
    let (t, _) = ScriptedTransport::new();
    let reply = vec![subscribe, first, last];
    let t = t
        .on("bn.hello", vec![fixture("hello_result")])
        .on("bn.subscribe", reply.clone())
        .on("bn.subscribe", reply);
    let app = App::new(EngineModel::new(Box::new(t)))
        .with_root(root)
        .with_settings(settings)
        .with_target_format(eframe::wgpu::TextureFormat::Rgba8Unorm);
    Harness::builder()
        .with_size(size)
        .wgpu()
        .build_ui_state(|ui, app: &mut App<EngineModel>| app.show(ui), app)
}

/// Runs frames until the scene is drawn.
fn settle(h: &mut H) {
    for _ in 0..300 {
        h.run_steps(2);
        h.render().expect("renders");
        if h.state().iso_stats().draws.load(Ordering::Relaxed) > 0 {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    panic!("the scene never drew");
}

/// The pixel where the world point `(x, y, z)`, from the avatar's square corner, is drawn.
fn pixel(h: &mut H, image: &egui::ColorImage, world: (f32, f32, f32)) -> egui::Color32 {
    let rect = h.get_by_label("Map").rect();
    let app = h.state();
    let me = app.model.player();
    let (tileset, _) = app.display().tileset().unwrap();
    let view = Viewport {
        camera: app.camera(),
        target: glam::Vec3::new(me.x as f32 + 0.5, me.y as f32 + 0.5, 0.0),
        scale: world_scale(tileset, app.display().cell(CELL) * app.display().zoom()),
        size: glam::Vec2::new(rect.width(), rect.height()),
    };
    let on = view
        .project(glam::Vec3::new(
            me.x as f32 + world.0,
            me.y as f32 + world.1,
            world.2,
        ))
        .unwrap();
    let at = rect.min + egui::vec2(on.x, on.y);
    image[(at.x.round() as usize, at.y.round() as usize)]
}

#[test]
#[ignore = "needs a Vulkan adapter"]
fn an_item_and_the_avatar_of_a_tileset_drawn_from_above_stand_thick_over_the_grass() {
    for (name, size) in [
        ("1280x800", (1280.0, 800.0)),
        ("1920x1080", (1920.0, 1080.0)),
    ] {
        let mut h = harness(egui::vec2(size.0, size.1));
        settle(&mut h);
        // Close in, to where a side is some pixels.
        let rect = h.get_by_label("Map").rect();
        h.hover_at(rect.center());
        h.run_steps(2);
        for _ in 0..8 {
            h.event(egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Line,
                delta: egui::vec2(0.0, 1.0),
                phase: egui::TouchPhase::Move,
                modifiers: egui::Modifiers::NONE,
            });
            h.run_steps(2);
        }
        settle(&mut h);
        let image = h.render().expect("renders");
        if let Some(dir) = std::env::var_os("SHOT_DIR") {
            std::fs::create_dir_all(&dir).unwrap();
            image
                .save(std::path::Path::new(&dir).join(format!("thick-{name}.png")))
                .unwrap();
        }
        let size = [image.width() as usize, image.height() as usize];
        let image = egui::ColorImage::from_rgba_unmultiplied(size, image.as_raw());
        // The item at (5, 3), two squares east and one north of the avatar: magenta on its top, 0.1 over the grass.
        let item = pixel(&mut h, &image, (2.5, -0.5, 0.1));
        assert!(
            item.r() > 150 && item.g() < 100 && item.b() > 150,
            "{name}: {item:?}"
        );
        // The avatar's own top, 0.4 over the grass.
        let me = pixel(&mut h, &image, (0.5, 0.5, 0.4));
        assert!(
            me.r() > 150 && me.g() < 100 && me.b() > 150,
            "{name}: {me:?}"
        );
        // A quarter turn about, where the other sides show.
        for _ in 0..6 {
            h.event(egui::Event::Key {
                key: egui::Key::ArrowLeft,
                physical_key: Some(egui::Key::ArrowLeft),
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::ALT,
            });
            h.run_steps(2);
        }
        let turned = h.render().expect("renders");
        if let Some(dir) = std::env::var_os("SHOT_DIR") {
            turned
                .save(std::path::Path::new(&dir).join(format!("thick-{name}-turned.png")))
                .unwrap();
        }
    }
}
