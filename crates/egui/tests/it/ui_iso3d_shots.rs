//! Pictures of the 3D map from a fixture replica, drawn on the GPU, for a person to look at and checked at the
//! pixels where a known part of the scene must be. Needs a Vulkan adapter, so it only runs when asked:
//! `SHOT_DIR=<dir> cargo test --test it ui_iso3d_shots:: -- --ignored`.
use crate::common::{fixture, iso_root};
use bn_client_egui::app::CELL;
use bn_client_egui::iso::camera::{EYE_HEIGHT, Viewport};
use bn_client_egui::iso::scene::world_scale;
use bn_client_egui::iso::sky::way;
use bn_client_egui::transport::ScriptedTransport;
use bn_client_egui::{App, EngineModel, GameModel};
use eframe::egui;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use serde_json::{Value, json};
use std::sync::atomic::Ordering;

type H<'a> = Harness<'a, App<EngineModel>>;

/// A room: dirt floor, a wall along x = 5 from y = 2 to 4 with the zombie just east of it, a table, and the
/// avatar at (3, 4).
fn cells() -> Vec<Value> {
    let look = |kind: &str, id: &str, glyph: &str| json!({"kind": kind, "id": id, "glyph": glyph, "color": "white"});
    let mut cells = vec![];
    for x in 0..=8 {
        for y in 1..=7 {
            let mut cell = json!({"at": {"dim": "", "x": x, "y": y, "z": 0}, "known": "visible",
                "terrain": look("terrain", "t_dirt", ".")});
            if x == 5 && (2..=4).contains(&y) {
                cell["terrain"] = look("terrain", "t_wall", "#");
            }
            if (x, y) == (4, 6) {
                cell["furniture"] = look("furniture", "f_table", "#");
            }
            if x == 0 || y == 7 {
                cell["known"] = "remembered".into();
            }
            cells.push(cell);
        }
    }
    cells
}

fn harness<'a>(tag: &str) -> H<'a> {
    harness_with(tag, r#"{"three_d": true}"#, None)
}

/// The room drawn with `settings`, under `sky` when the engine names one.
fn harness_with<'a>(tag: &str, settings_json: &str, sky: Option<Value>) -> H<'a> {
    let root = iso_root(tag);
    let settings = root.join("settings.json");
    std::fs::write(&settings, settings_json).unwrap();
    let mut subscribe = fixture("subscribe_result");
    if let Some(sky) = sky {
        subscribe["result"]["environment"]["sky"] = sky;
    }
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
        .with_size(egui::vec2(1100.0, 700.0))
        .wgpu()
        .build_ui_state(|ui, app: &mut App<EngineModel>| app.show(ui), app)
}

/// Runs frames until the three sheets of the test tileset are drawn.
fn settle(h: &mut H) {
    for _ in 0..300 {
        h.run_steps(2);
        // The scene is drawn when the frame is rendered.
        h.render().expect("renders");
        if h.state().iso_stats().draws.load(Ordering::Relaxed) == 3 {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    panic!("the scene never drew its three pages");
}

fn save(h: &mut H, name: &str) -> egui::ColorImage {
    let image = h.render().expect("renders");
    if let Some(dir) = std::env::var_os("SHOT_DIR") {
        std::fs::create_dir_all(&dir).unwrap();
        image.save(std::path::Path::new(&dir).join(name)).unwrap();
    }
    let size = [image.width() as usize, image.height() as usize];
    egui::ColorImage::from_rgba_unmultiplied(size, image.as_raw())
}

/// The pixel where the world point `(x, y, z)` is drawn.
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

/// The pixel where the sky is drawn the way `way` from the first-person eye, if the view shows it.
fn sky_pixel(h: &mut H, image: &egui::ColorImage, way: glam::Vec3) -> Option<egui::Color32> {
    let rect = h.get_by_label("Map").rect();
    let app = h.state();
    let me = app.model.player();
    let (tileset, _) = app.display().tileset().unwrap();
    let view = Viewport {
        camera: bn_client_egui::iso::camera::Camera {
            first_person: true,
            ..app.camera()
        },
        target: glam::Vec3::new(me.x as f32 + 0.5, me.y as f32 + 0.5, 0.0),
        scale: world_scale(tileset, app.display().cell(CELL) * app.display().zoom()),
        size: glam::Vec2::new(rect.width(), rect.height()),
    };
    let eye = view.target + glam::Vec3::Z * EYE_HEIGHT;
    let on = view.project(eye + way * 150.0)?;
    let at = rect.min + egui::vec2(on.x, on.y);
    rect.shrink(2.0)
        .contains(at)
        .then(|| image[(at.x.round() as usize, at.y.round() as usize)])
}

fn reddish(c: egui::Color32) -> bool {
    c.r() > 140 && c.g() < 90 && c.b() < 90
}

/// Blue, lit or in the light's shadow.
fn bluish(c: egui::Color32) -> bool {
    c.b() > 80 && c.b() > c.r() + 40
}

fn whitish(c: egui::Color32) -> bool {
    c.r() > 200 && c.g() > 200 && c.b() > 200
}

fn greenish(c: egui::Color32) -> bool {
    c.g() > 150 && c.r() < 160 && c.b() < 130
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
#[ignore = "needs a Vulkan adapter"]
fn the_room_seen_from_the_south_west_as_the_game_draws_it_and_from_the_north_west() {
    let mut h = harness("shots");
    settle(&mut h);
    // The wall x = 5, y = 2 to 4: its south end is (5, 4), two squares east of the avatar at (3, 4), and its
    // north end (5, 2). Faces between two wall squares are not seen.
    let (south_end, north_end) = ((2.0, 0.0), (2.0, -2.0));
    let top = 96.0 * 0.034_026;
    // The top is white, the sprite's left face red and its right face blue; the game draws the west face on the
    // left and the south face on the right.
    let at_135 = save(&mut h, "iso3d-yaw135.png");
    assert!(
        whitish(pixel(
            &mut h,
            &at_135,
            (south_end.0 + 0.5, south_end.1 + 0.5, top)
        )),
        "top"
    );
    assert!(
        bluish(pixel(
            &mut h,
            &at_135,
            (south_end.0 + 0.5, south_end.1 + 1.0, top / 2.0)
        )),
        "south face"
    );
    assert!(
        reddish(pixel(
            &mut h,
            &at_135,
            (north_end.0, north_end.1 + 0.5, top / 2.0)
        )),
        "west face"
    );
    // The floor turns with the world: the mark on a floor diamond's top corner, its north-east one, stays there.
    let floor = (0.0, 1.0);
    assert!(
        reddish(pixel(
            &mut h,
            &at_135,
            (floor.0 + 0.83, floor.1 + 0.17, 0.0)
        )),
        "mark at 135"
    );
    assert!(
        greenish(pixel(&mut h, &at_135, (floor.0 + 0.45, floor.1 + 0.5, 0.0))),
        "floor at 135"
    );

    // 90 degrees more: the camera is north-west of the room and sees the north and the west faces.
    turn(&mut h, 6);
    let at_225 = save(&mut h, "iso3d-yaw225.png");
    assert!(
        whitish(pixel(
            &mut h,
            &at_225,
            (north_end.0 + 0.5, north_end.1 + 0.5, top)
        )),
        "top"
    );
    assert!(
        bluish(pixel(
            &mut h,
            &at_225,
            (north_end.0 + 0.5, north_end.1, top / 2.0)
        )),
        "north face"
    );
    assert!(
        reddish(pixel(
            &mut h,
            &at_225,
            (north_end.0, north_end.1 + 0.5, top / 2.0)
        )),
        "west face"
    );
    assert!(
        reddish(pixel(
            &mut h,
            &at_225,
            (floor.0 + 0.83, floor.1 + 0.17, 0.0)
        )),
        "mark at 225"
    );
}

#[test]
#[ignore = "needs a Vulkan adapter"]
fn the_wall_shades_the_floor_on_its_south_east_from_the_light_in_the_north_west() {
    let mut h = harness("shadow");
    settle(&mut h);
    let image = save(&mut h, "iso3d-shadow.png");
    // The avatar is at (3, 4); the wall's south end at (5, 4) shades (6, 5), and nothing stands north-west of (2, 2).
    let shaded = pixel(&mut h, &image, (3.5, 1.5, 0.0));
    let lit = pixel(&mut h, &image, (-0.5, -1.5, 0.0));
    assert!(greenish(lit), "{lit:?}");
    assert!(
        f32::from(shaded.g()) < 0.75 * f32::from(lit.g()) && shaded.g() > 0,
        "shaded {shaded:?}, lit {lit:?}"
    );
}

#[test]
#[ignore = "needs a Vulkan adapter"]
fn one_instanced_draw_per_sheet_page_draws_every_sprite_of_the_view() {
    let mut h = harness("draws");
    settle(&mut h);
    let stats = h.state().iso_stats();
    assert_eq!(
        stats.draws.load(Ordering::Relaxed),
        3,
        "flat, tall and upright sheets"
    );
    // 63 ground quads (the wall squares have none), 3 wall boxes in a row, whose touching faces are left out, the table, the zombie and the avatar.
    assert!(stats.instances.load(Ordering::Relaxed) >= 60 + 11 + 3);
}

#[test]
#[ignore = "needs a Vulkan adapter"]
fn the_first_person_sky_is_blue_by_day_red_low_at_dusk_and_dark_with_the_moon_and_stars_at_night() {
    const FIRST: &str = r#"{"three_d": true, "first_person": true}"#;
    let brightness = |c: egui::Color32| u32::from(c.r()) + u32::from(c.g()) + u32::from(c.b());
    // Turns the camera until the view shows `way`, and gives its pixel.
    let find = |h: &mut H, name: &str, way: glam::Vec3| {
        for _ in 0..24 {
            let image = save(h, name);
            if let Some(c) = sky_pixel(h, &image, way) {
                return (image, c);
            }
            turn(h, 1);
        }
        panic!("the view never showed {way:?}");
    };
    let mut h = harness_with(
        "sky-day",
        FIRST,
        Some(
            json!({"daylight": 100, "precipitation": "none", "dimming": 0, "sun": 50, "moon_phase": "new"}),
        ),
    );
    settle(&mut h);
    // The first-person view looks a little down, so it shows the sky up to about 20 degrees.
    let high = glam::Vec3::new(0.3, 0.2, 0.12).normalize();
    let (_, overhead) = find(&mut h, "sky-day.png", high);
    assert!(bluish(overhead), "{overhead:?}");
    let mut h = harness_with(
        "sky-dusk",
        FIRST,
        Some(
            json!({"daylight": 40, "precipitation": "none", "dimming": 0, "sun": 97, "moon_phase": "new"}),
        ),
    );
    settle(&mut h);
    let low = glam::Vec3::new(-1.0, 0.0, 0.05).normalize();
    let (_, horizon) = find(&mut h, "sky-dusk.png", low);
    assert!(horizon.r() > horizon.b() + 40, "{horizon:?}");
    let mut h = harness_with(
        "sky-night",
        FIRST,
        Some(
            json!({"daylight": 0, "precipitation": "none", "dimming": 0, "moon": 90, "moon_phase": "full"}),
        ),
    );
    settle(&mut h);
    // Low in the west, away from the wall east of the avatar.
    let (image, moon) = find(&mut h, "sky-night.png", way(90));
    assert!(whitish(moon), "{moon:?}");
    let night = sky_pixel(&mut h, &image, way(90) + glam::Vec3::new(0.0, 0.0, 0.1)).unwrap();
    assert!(brightness(night) < 60, "{night:?}");
    let mut h = harness_with(
        "sky-overcast",
        FIRST,
        Some(
            json!({"daylight": 70, "precipitation": "light", "rain": true, "dimming": 40, "moon_phase": "new"}),
        ),
    );
    settle(&mut h);
    let (_, grey) = find(&mut h, "sky-overcast.png", high);
    // Past the room's west edge, a little under the horizon, nothing is drawn, and that is dark as without a sky.
    let west = glam::Vec3::new(-1.0, 0.0, -0.1).normalize();
    let (_, below) = find(&mut h, "sky-overcast-below.png", west);
    assert!(brightness(below) < 30, "{below:?}");
    assert!(
        i32::from(grey.b()) - i32::from(grey.r()) < 40 && brightness(grey) > 250,
        "{grey:?}"
    );
}
