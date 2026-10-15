#![recursion_limit = "512"]
//! The flat map's sprites meet without seams at every zoom: a map of one sprite shows no pixel of the sprites
//! next to it in the sheet, which a sample on the edge of a sprite's cell would pick up.
mod common;
use bn_client_egui::app::CELL;
use bn_client_egui::transport::ScriptedTransport;
use bn_client_egui::{App, EngineModel, GameModel};
use common::{fixture, game_root};
use eframe::egui;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use serde_json::{Value, json};

type H<'a> = Harness<'a, App<EngineModel>>;

/// Dirt, which is sprite 0 of the test tileset, all over the view.
fn cells() -> Vec<Value> {
    let look = json!({"kind": "terrain", "id": "t_dirt", "glyph": ".", "color": "white"});
    (-40..45)
        .flat_map(|x| (-40..45).map(move |y| (x, y)))
        .map(|(x, y)| json!({"at": {"dim": "", "x": x, "y": y, "z": 0}, "known": "visible", "terrain": look}))
        .collect()
}

fn harness<'a>(tag: &str) -> H<'a> {
    let root = game_root(tag);
    let settings = root.join("settings.json");
    let mut subscribe = fixture("subscribe_result");
    subscribe["result"]["avatar"]["look"] = json!({
        "kind": "avatar", "id": "avatar", "glyph": "@", "color": "white"
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
        .with_settings(settings);
    Harness::builder()
        .with_size(egui::vec2(1100.0, 800.0))
        .wgpu()
        .build_ui_state(|ui, app: &mut App<EngineModel>| app.show(ui), app)
}

/// The colour of sprite `n` of the test tileset's first sheet.
fn sprite(n: u8) -> [u8; 4] {
    [40 + 25 * n, 255 - 25 * n, 128, 255]
}

fn wheel(h: &mut H, steps: i32) {
    h.hover_at(h.get_by_label("Map").rect().center());
    h.run_steps(2);
    for _ in 0..steps.abs() {
        h.event(egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Line,
            delta: egui::vec2(0.0, steps.signum() as f32),
            modifiers: egui::Modifiers::NONE,
            phase: egui::TouchPhase::Move,
        });
        h.run_steps(2);
    }
}

#[test]
fn sprites_meet_without_seams_at_every_zoom() {
    let mut h = harness("seams");
    for _ in 0..200 {
        h.run_steps(2);
        if h.state().display().ready() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    wheel(&mut h, -4);
    for zoom in -4..=4 {
        // The pointer leaves the map, whose tooltip and cursor are not sprites.
        h.hover_at(egui::pos2(900.0, 700.0));
        h.run_steps(3);
        let image = h.render().expect("renders");
        let map = h.get_by_label("Map").rect();
        let cell = h.state().display().cell(CELL) * h.state().display().zoom();
        // The dirt is the box of pixels of its own colour; the zombie's sprite and the avatar's glyph at the middle, with their cursor and a cell around, are left out.
        let bounds = |color: [u8; 4]| {
            (map.min.y as u32..map.max.y as u32)
                .flat_map(|y| (map.min.x as u32..map.max.x as u32).map(move |x| (x, y)))
                .filter(|&(x, y)| image.get_pixel(x, y).0 == color)
                .map(|(x, y)| egui::pos2(x as f32, y as f32))
                .fold(egui::Rect::NOTHING, |r, p| {
                    r.union(egui::Rect::from_min_max(p, p))
                })
        };
        let dirt = bounds(sprite(0));
        let others = [
            bounds(sprite(7)),
            egui::Rect::from_center_size(map.center(), egui::Vec2::splat(2.0 * cell)),
        ]
        .map(|r| r.expand(cell + 40.0));
        let seams = (dirt.min.y as u32..=dirt.max.y as u32)
            .flat_map(|y| (dirt.min.x as u32..=dirt.max.x as u32).map(move |x| (x, y)))
            .filter(|&(x, y)| {
                !others
                    .iter()
                    .any(|r| r.contains(egui::pos2(x as f32, y as f32)))
            })
            .filter(|&(x, y)| image.get_pixel(x, y).0 != sprite(0))
            .collect::<Vec<_>>();
        assert!(
            seams.is_empty(),
            "zoom {zoom}: {} seam pixels of {dirt:?}, first {:?}",
            seams.len(),
            &seams[..seams.len().min(5)]
        );
        wheel(&mut h, 1);
    }
}
