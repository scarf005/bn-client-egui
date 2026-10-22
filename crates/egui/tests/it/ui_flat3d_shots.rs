//! Pictures of a garden, a house wall and a car from a fixture replica, drawn in 3D with the UltimateCataclysm
//! tileset, which is drawn from above, for a person to look at. Needs a Vulkan adapter and a game root:
//! `BN_ROOT=<game root> SHOT_DIR=<dir> cargo test --test it ui_flat3d_shots:: -- --ignored`.
use crate::common::fixture;
use bn_client_egui::transport::ScriptedTransport;
use bn_client_egui::{App, EngineModel};
use eframe::egui;
use egui_kittest::Harness;
use serde_json::{Value, json};
use std::sync::atomic::Ordering;

type H<'a> = Harness<'a, App<EngineModel>>;

fn look(kind: &str, id: &str) -> Value {
    json!({"kind": kind, "id": id, "glyph": "#", "color": "white"})
}

/// Grass from (0, 0) to (15, 11) around the avatar at (3, 4): shrubs and flowers in rows, a tree, a house wall
/// along y = 1 and a car of three by four parts.
fn cells() -> Vec<Value> {
    let mut cells = vec![];
    for x in 0..=15 {
        for y in 0..=11 {
            let mut cell = json!({"at": {"dim": "", "x": x, "y": y, "z": 0}, "known": "visible",
                "terrain": look("terrain", "t_grass")});
            match (x, y) {
                (2..=12, 1) => {
                    cell["terrain"] = json!({"kind": "terrain", "id": "t_wall", "glyph": "#",
                    "color": "white", "wall": true})
                }
                (6..=12, 4) => cell["terrain"] = look("terrain", "t_shrub"),
                (6..=12, 6) => {
                    let flower = if x % 2 == 0 {
                        "f_bluebell"
                    } else {
                        "f_dandelion"
                    };
                    cell["furniture"] = look("furniture", flower);
                }
                (5, 8) => cell["terrain"] = look("terrain", "t_tree"),
                (8..=10, 8..=11) => {
                    let mut part = look("vehicle_part", "frame");
                    part["tile"] = "vp_frame_cover".into();
                    cell["vehicle"] = part;
                }
                _ => {}
            }
            cells.push(cell);
        }
    }
    cells
}

fn harness<'a>(size: egui::Vec2) -> H<'a> {
    let root = std::path::PathBuf::from(std::env::var_os("BN_ROOT").expect("set BN_ROOT"));
    let dir = std::env::temp_dir().join(format!("bn-flat3d-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let settings = dir.join("settings.json");
    std::fs::write(
        &settings,
        r#"{"tiles": true, "tileset": "UltimateCataclysm", "three_d": true}"#,
    )
    .unwrap();
    let mut subscribe = fixture("subscribe_result");
    subscribe["result"]["avatar"]["look"] = json!({
        "kind": "avatar", "id": "avatar", "glyph": "@", "color": "white", "tile": "player_male"
    });
    subscribe["result"]["entities"] = json!([]);
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

/// Runs frames until the scene has drawn the same draws for a second: the sheets are decoded.
fn settle(h: &mut H) {
    let (mut last, mut same) = (0, 0);
    for _ in 0..600 {
        h.run_steps(2);
        h.render().expect("renders");
        let draws = h.state().iso_stats().draws.load(Ordering::Relaxed);
        same = if draws > 0 && draws == last {
            same + 1
        } else {
            0
        };
        last = draws;
        if same == 50 {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    panic!("the scene was never drawn");
}

fn save(h: &mut H, name: &str) {
    let image = h.render().expect("renders");
    if let Some(dir) = std::env::var_os("SHOT_DIR") {
        std::fs::create_dir_all(&dir).unwrap();
        image.save(std::path::Path::new(&dir).join(name)).unwrap();
    }
}

/// Presses Alt+Left `n` times: 15 degrees each.
fn turn(h: &mut H, n: usize) {
    for _ in 0..n {
        h.event(egui::Event::Key {
            key: egui::Key::ArrowLeft,
            physical_key: Some(egui::Key::ArrowLeft),
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::ALT,
        });
        h.run_steps(2);
    }
}

#[test]
#[ignore = "needs a Vulkan adapter and BN_ROOT with the UltimateCataclysm tileset"]
fn a_garden_a_wall_and_a_car_of_a_tileset_drawn_from_above_in_3d() {
    for (w, h_) in [(1280.0, 720.0), (1366.0, 768.0)] {
        let mut h = harness(egui::vec2(w, h_));
        settle(&mut h);
        save(&mut h, &format!("garden-{w}x{h_}-yaw135.png"));
        turn(&mut h, 3);
        save(&mut h, &format!("garden-{w}x{h_}-yaw180.png"));
        turn(&mut h, 6);
        save(&mut h, &format!("garden-{w}x{h_}-yaw270.png"));
    }
}
