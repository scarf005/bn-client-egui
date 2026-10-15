#![recursion_limit = "512"]
//! The 3D map: switching to it, turning its camera, and picking the square under the pointer, checked in the
//! egui client against the wire the flat map uses. The pictures of the scene itself are in `ui_iso3d_shots`.
mod common;
use bn_client_egui::iso::camera::{PITCH_MAX, PITCH_MIN, Viewport};
use bn_client_egui::iso::scene::world_scale;
use bn_client_egui::transport::{ScriptHandle, ScriptedTransport};
use bn_client_egui::{App, EngineModel, GameModel, Pos};
use common::{fixture, game_root, iso_root, open_client_settings};
use eframe::egui;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use serde_json::{Value, json};
use std::path::Path;

type H<'a> = Harness<'a, App<EngineModel>>;

/// The fixture session; `bound` are keys the engine binds to an action of its own, `drink`.
fn session(bound: &[&str]) -> (ScriptedTransport, ScriptHandle) {
    let mut subscribe = fixture("subscribe_result");
    if !bound.is_empty() {
        let actions = subscribe["result"]["interaction"]["actions"]
            .as_array_mut()
            .unwrap();
        actions.push(json!({"id": "drink", "name": "Drink", "keys": bound}));
    }
    subscribe["result"]["avatar"]["look"] = json!({
        "kind": "avatar", "id": "avatar", "glyph": "@", "color": "white", "tile": "mon_a"
    });
    let (t, handle) = ScriptedTransport::new();
    let reply = vec![
        subscribe,
        fixture("snapshot_part_0"),
        fixture("snapshot_part_1"),
    ];
    let t = t
        .on("bn.hello", vec![fixture("hello_result")])
        .on("bn.subscribe", reply.clone())
        .on("bn.subscribe", reply);
    let submit = || vec![fixture("submit_result")];
    (
        t.on("bn.command.submit", submit())
            .on("bn.command.submit", submit()),
        handle,
    )
}

/// A client with the isometric test tileset; `renderer` says whether it knows the format of a window to draw
/// the 3D map into, as one with a GPU does.
fn harness<'a>(root: &Path, settings: &Path, renderer: bool) -> (H<'a>, ScriptHandle) {
    harness_bound(root, settings, renderer, &[])
}

fn harness_bound<'a>(
    root: &Path,
    settings: &Path,
    renderer: bool,
    bound: &[&str],
) -> (H<'a>, ScriptHandle) {
    let (t, handle) = session(bound);
    let model = EngineModel::new(Box::new(t));
    let mut app = App::new(model)
        .with_root(root.to_path_buf())
        .with_settings(settings.to_path_buf());
    if renderer {
        app = app.with_target_format(eframe::wgpu::TextureFormat::Rgba8Unorm);
    }
    let h = Harness::builder()
        .with_size(egui::vec2(1100.0, 800.0))
        .build_ui_state(|ui, app: &mut App<EngineModel>| app.show(ui), app);
    (h, handle)
}

fn settle(h: &mut H) {
    for _ in 0..200 {
        h.run_steps(2);
        if h.state().display().ready() {
            h.run_steps(2);
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    panic!("the tileset never finished loading");
}

fn saved(settings: &Path) -> Value {
    serde_json::from_slice(&std::fs::read(settings).unwrap_or_default()).unwrap_or_default()
}

fn pick_2d(h: &mut H) {
    open_client_settings(h);
    h.get_by_label("2D").click();
    h.run_steps(3);
}

fn pick_3d(h: &mut H) {
    open_client_settings(h);
    h.get_by_label("3D").click();
    h.run_steps(3);
}

fn press(h: &mut H, text: &str) {
    h.event(egui::Event::Text(text.into()));
    h.run_steps(2);
}

fn travels(script: &ScriptHandle) -> Vec<Value> {
    script
        .sent()
        .into_iter()
        .filter(|m| m["method"] == "bn.command.submit")
        .map(|m| m["params"]["operation"].clone())
        .collect()
}

/// Where the middle of view square `at` is on the screen of the 3D map, as the app draws it.
fn screen_of(h: &mut H, at: Pos) -> egui::Pos2 {
    let rect = h.get_by_label("Map").rect();
    let app = h.state();
    let (tileset, _) = app.display().tileset().expect("a tileset");
    let player = app.model.player();
    let view = Viewport {
        camera: app.camera(),
        target: glam::Vec3::new(player.x as f32 + 0.5, player.y as f32 + 0.5, 0.0),
        scale: world_scale(tileset, app.display().zoom()),
        size: glam::Vec2::new(rect.width(), rect.height()),
    };
    let on = view.project(glam::Vec3::new(at.x as f32 + 0.5, at.y as f32 + 0.5, 0.0));
    rect.min + egui::vec2(on.x, on.y)
}

fn click(h: &mut H, pos: egui::Pos2) {
    h.hover_at(pos);
    h.run_steps(2);
    for pressed in [true, false] {
        h.event(egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        });
        h.run_steps(1);
    }
    h.run_steps(3);
}

#[test]
fn an_isometric_tileset_is_drawn_in_3d_by_default_and_the_choice_is_kept_in_the_settings() {
    let root = iso_root("switch");
    let settings = root.join("settings.json");
    let (mut h, _) = harness(&root, &settings, true);
    settle(&mut h);
    assert!(
        h.state().display().three_d(),
        "isometric: 3D until told not to"
    );
    assert_eq!(saved(&settings).get("three_d"), None);
    pick_2d(&mut h);
    assert!(!h.state().display().three_d());
    assert_eq!(saved(&settings)["three_d"], false);
    pick_3d(&mut h);
    assert!(h.state().display().three_d());
    assert_eq!(saved(&settings)["three_d"], true);
}

#[test]
fn a_tileset_that_is_not_isometric_stays_flat_unless_3d_is_asked_for() {
    let root = game_root("flat-set");
    let (mut h, _) = harness(&root, &root.join("settings.json"), true);
    settle(&mut h);
    assert!(!h.state().display().three_d());
}

#[test]
fn switching_the_view_keeps_the_tileset_that_is_loaded() {
    let root = iso_root("keeps");
    let (mut h, _) = harness(&root, &root.join("settings.json"), true);
    settle(&mut h);
    let epoch = h.state().display().tileset().unwrap().1;
    pick_3d(&mut h);
    assert_eq!(h.state().display().tileset().unwrap().1, epoch);
}

#[test]
fn without_a_renderer_for_it_the_map_stays_flat_even_when_3d_is_chosen() {
    let root = iso_root("flat");
    let (mut h, _) = harness(&root, &root.join("settings.json"), false);
    settle(&mut h);
    pick_3d(&mut h);
    h.run_steps(3);
    // The sprites are drawn the flat way.
    assert!(h.state().display().drawn() > 30);
}

/// Presses `key` with Alt held.
fn alt(h: &mut H, key: egui::Key) {
    h.event(egui::Event::Key {
        key,
        physical_key: Some(key),
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::ALT,
    });
    h.run_steps(2);
}

fn keys_sent(script: &ScriptHandle) -> usize {
    script
        .sent()
        .into_iter()
        .filter(|m| m["method"] == "bn.command.submit")
        .count()
}

#[test]
fn alt_arrows_turn_the_3d_camera_and_none_of_the_games_keys_do() {
    let root = iso_root("keys");
    let (mut h, script) = harness(&root, &root.join("settings.json"), true);
    settle(&mut h);
    let before = h.state().camera();
    pick_2d(&mut h);
    alt(&mut h, egui::Key::ArrowLeft);
    assert_eq!(h.state().camera(), before, "flat: no turn");
    pick_3d(&mut h);
    let sent = keys_sent(&script);
    alt(&mut h, egui::Key::ArrowLeft);
    let turned = h.state().camera();
    assert!((turned.yaw - before.yaw - 15f32.to_radians()).abs() < 1e-4);
    assert_eq!(turned.pitch, before.pitch);
    alt(&mut h, egui::Key::ArrowRight);
    alt(&mut h, egui::Key::ArrowRight);
    let back = h.state().camera();
    assert!((back.yaw - before.yaw + 15f32.to_radians()).abs() < 1e-4);
    assert_eq!(
        keys_sent(&script),
        sent,
        "the turn keys did not reach the game"
    );
}

fn drinks(script: &ScriptHandle) -> usize {
    script
        .sent()
        .into_iter()
        .filter(|m| m["params"]["operation"]["action_id"] == "drink")
        .count()
}

#[test]
fn with_the_3d_view_on_the_keys_the_engine_binds_still_reach_it() {
    for (n, key) in ["q", "e", "E"].into_iter().enumerate() {
        let root = iso_root(&format!("bound-{n}"));
        let (mut h, script) = harness_bound(&root, &root.join("settings.json"), true, &[key]);
        settle(&mut h);
        pick_3d(&mut h);
        let before = h.state().camera();
        press(&mut h, key);
        assert_eq!(drinks(&script), 1, "{key}: {:?}", script.sent());
        assert_eq!(h.state().camera(), before, "{key} did not turn the camera");
    }
}

#[test]
fn an_alt_arrow_the_engine_binds_is_the_engines_and_does_not_turn_the_camera() {
    let root = iso_root("alt-bound");
    let (mut h, script) = harness_bound(&root, &root.join("settings.json"), true, &["ALT+LEFT"]);
    settle(&mut h);
    pick_3d(&mut h);
    let before = h.state().camera();
    alt(&mut h, egui::Key::ArrowLeft);
    assert_eq!(h.state().camera(), before);
    alt(&mut h, egui::Key::ArrowRight);
    assert_ne!(h.state().camera(), before, "the unbound one still turns it");
    let _ = script;
}

#[test]
fn dragging_with_the_right_button_turns_and_tilts_the_camera_within_its_limits() {
    let root = iso_root("drag");
    let (mut h, _) = harness(&root, &root.join("settings.json"), true);
    settle(&mut h);
    pick_3d(&mut h);
    let start = h.get_by_label("Map").rect().center();
    let drag = |h: &mut H, by: egui::Vec2| {
        h.hover_at(start);
        h.run_steps(2);
        let event = |pos, pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Secondary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        h.event(event(start, true));
        h.run_steps(1);
        h.hover_at(start + by);
        h.run_steps(2);
        h.event(event(start + by, false));
        h.run_steps(2);
    };
    let before = h.state().camera();
    drag(&mut h, egui::vec2(100.0, 0.0));
    let after = h.state().camera();
    assert!(after.yaw != before.yaw, "{before:?} {after:?}");
    assert_eq!(after.pitch, before.pitch);
    drag(&mut h, egui::vec2(0.0, 5000.0));
    assert_eq!(h.state().camera().pitch, PITCH_MIN);
    drag(&mut h, egui::vec2(0.0, -9000.0));
    assert_eq!(h.state().camera().pitch, PITCH_MAX);
}

#[test]
fn a_click_on_a_3d_square_sends_what_a_click_on_that_square_sends_in_the_flat_map() {
    let root = iso_root("click");
    // The flat map's click on the square east of the avatar.
    let (mut flat, script) = harness(&root, &root.join("flat.json"), true);
    settle(&mut flat);
    pick_2d(&mut flat);
    let me = flat.state().model.player();
    let target = Pos {
        x: me.x + 2,
        y: me.y + 1,
    };
    let rect = flat.get_by_label("Map").rect();
    let cell =
        flat.state().display().cell(bn_client_egui::app::CELL) * flat.state().display().zoom();
    click(
        &mut flat,
        rect.min + egui::vec2(target.x as f32 + 0.5, target.y as f32 + 0.5) * cell,
    );
    let expected = travels(&script);
    assert_eq!(expected.len(), 1, "{:?}", script.sent());

    for turns in 0..4 {
        // A quarter turn is six presses of 15 degrees.
        let (mut h, script) = harness(&root, &root.join(format!("three-{turns}.json")), true);
        settle(&mut h);
        pick_3d(&mut h);
        for _ in 0..turns * 6 {
            press(&mut h, "q");
        }
        let me = h.state().model.player();
        let at = screen_of(
            &mut h,
            Pos {
                x: me.x + 2,
                y: me.y + 1,
            },
        );
        click(&mut h, at);
        assert_eq!(travels(&script), expected, "after {} quarter turns", turns);
    }
}

#[test]
fn hovering_a_3d_square_asks_the_engine_to_describe_that_square() {
    let root = iso_root("hover");
    let (mut h, script) = harness(&root, &root.join("settings.json"), true);
    settle(&mut h);
    pick_3d(&mut h);
    let me = h.state().model.player();
    let at = screen_of(
        &mut h,
        Pos {
            x: me.x + 1,
            y: me.y,
        },
    );
    h.hover_at(at);
    h.run_steps(4);
    let described = script
        .sent()
        .into_iter()
        .rfind(|m| m["method"] == "bn.world.describe")
        .expect("asked");
    assert_eq!(
        described["params"]["pos"],
        json!({"dim": "", "x": 4, "y": 4, "z": 0})
    );
}
