#![recursion_limit = "512"]
mod common;
use bn_client_egui::transport::ScriptedTransport;
use bn_client_egui::{App, EngineModel, GameModel};
use common::{fixture, game_root};
use eframe::egui;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

type H<'a> = Harness<'a, App<EngineModel>>;

/// The fixture session whose avatar draws as the test tileset's `mon_a`.
fn session() -> ScriptedTransport {
    session_with(|_| {})
}

/// The fixture session, its subscribe result changed by `edit`.
fn session_with(edit: impl FnOnce(&mut Value)) -> ScriptedTransport {
    let mut subscribe = fixture("subscribe_result");
    edit(&mut subscribe);
    subscribe["result"]["avatar"]["look"] = json!({
        "kind": "avatar", "id": "avatar", "glyph": "@", "color": "white", "tile": "mon_a"
    });
    subscribe["result"]["environment"]["season"] = json!("winter");
    let (t, _) = ScriptedTransport::new();
    t.on("bn.hello", vec![fixture("hello_result")]).on(
        "bn.subscribe",
        vec![
            subscribe,
            fixture("snapshot_part_0"),
            fixture("snapshot_part_1"),
        ],
    )
}

fn harness<'a>(root: &Path, settings: PathBuf) -> H<'a> {
    harness_of(root, settings, session())
}

fn harness_of<'a>(root: &Path, settings: PathBuf, session: ScriptedTransport) -> H<'a> {
    let app = App::new(EngineModel::new(Box::new(session)))
        .with_root(root.to_path_buf())
        .with_settings(settings);
    Harness::builder()
        .with_size(egui::vec2(1100.0, 800.0))
        .build_ui_state(|ui, app: &mut App<EngineModel>| app.show(ui), app)
}

/// Runs frames until the tileset's sheets are decoded and uploaded.
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
    serde_json::from_slice(&std::fs::read(settings).unwrap()).unwrap()
}

#[test]
fn the_map_draws_sprites_of_the_chosen_tileset_for_what_the_engine_published() {
    let root = game_root("ui-sprites");
    let mut h = harness(&root, root.join("settings.json"));
    settle(&mut h);
    let display = h.state().display();
    assert!(display.tiles());
    assert_eq!(
        display.settings().tileset,
        None,
        "no choice yet, the one tileset is used"
    );
    // Everything the engine published in view is drawn with a sprite: terrain, the zombie and the avatar.
    // The tileset has none for the knife on the ground, which draws as its glyph.
    let model = &h.state().model;
    let size = model.size();
    let layers = (0..size.y)
        .flat_map(|y| (0..size.x).map(move |x| bn_client_egui::Pos { x, y }))
        .map(|at| model.cell(at).layers.len())
        .sum::<usize>();
    assert!(layers > 30, "{layers} layers");
    assert_eq!(display.drawn(), layers - 1);
    // Every square nothing is known about lies on the hidden-lighting sprite, like the native view.
    let known = (0..size.y)
        .flat_map(|y| (0..size.x).map(move |x| bn_client_egui::Pos { x, y }))
        .filter(|&at| !model.cell(at).layers.is_empty())
        .count();
    assert_eq!(display.hidden(), (size.x * size.y) as usize - known);
}

#[test]
fn switching_to_ascii_draws_glyphs_again_and_the_choice_survives_a_restart() {
    let root = game_root("ui-switch");
    let settings = root.join("config/settings.json");
    let mut h = harness(&root, settings.clone());
    settle(&mut h);
    assert!(h.state().display().drawn() > 0);

    common::open_client_settings(&mut h);
    h.get_by_label("ASCII").click();
    h.run_steps(3);
    assert!(!h.state().display().tiles());
    assert_eq!(h.state().display().drawn(), 0);
    assert_eq!(
        saved(&settings),
        json!({"tiles": false, "tileset": null, "reduced_motion": false})
    );

    // A new run reads the file and starts in ASCII without loading the tileset.
    let again = harness(&root, settings.clone());
    assert!(!again.state().display().tiles());

    common::open_client_settings(&mut h);
    h.get_by_label("Tiles").click();
    h.run_steps(2);
    settle(&mut h);
    assert!(h.state().display().tiles());
    assert!(h.state().display().drawn() > 0);
    assert_eq!(saved(&settings)["tiles"], json!(true));
}

#[test]
fn a_tileset_missing_from_the_game_root_is_said_and_ascii_is_drawn() {
    let root = std::env::temp_dir().join(format!("bn-client-no-gfx-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let mut h = harness(&root, root.join("settings.json"));
    h.run_steps(4);
    assert!(!h.state().display().tiles());
    common::open_client_settings(&mut h);
    assert!(h.query_by_label_contains("Tileset unavailable").is_some());
}

#[test]
fn the_chosen_tileset_name_is_kept() {
    let root = game_root("ui-name");
    let settings = root.join("settings.json");
    std::fs::write(&settings, r#"{"tiles": true, "tileset": "test"}"#).unwrap();
    let mut h = harness(&root, settings);
    settle(&mut h);
    assert_eq!(
        h.state().display().settings().tileset.as_deref(),
        Some("test")
    );
    assert!(h.state().display().tiles());
}

#[test]
fn the_overmap_draws_the_engines_tile_ids_with_fallbacks_and_keeps_markers_as_glyphs() {
    let root = game_root("ui-overmap");
    let (t, script) = ScriptedTransport::new();
    let transport = t.on("bn.hello", vec![fixture("hello_result")]).on(
        "bn.subscribe",
        vec![
            fixture("subscribe_result"),
            fixture("snapshot_part_0"),
            fixture("snapshot_part_1"),
        ],
    );
    let app = App::new(EngineModel::connect(Box::new(transport), true))
        .with_root(root.clone())
        .with_settings(root.join("settings.json"));
    let mut h: H = Harness::builder()
        .with_size(egui::vec2(1100.0, 800.0))
        .build_ui_state(|ui, app: &mut App<EngineModel>| app.show(ui), app);
    settle(&mut h);
    script.push(common::with_tiles(common::overmap_event(100)));
    h.run_steps(4);
    settle(&mut h);
    // Twelve squares, each with a sprite under it: the floor, and the forest through its `t_dirt`
    // fallback. The player's marker is a glyph over its sprite.
    h.run_steps(2);
    let display = h.state().display();
    assert!(display.tiles());
    assert_eq!(
        display.drawn(),
        12,
        "a sprite under every square, markers on top"
    );
    h.get_by_label("Overmap forest");
    h.get_by_label("Route of 2 squares.");
    // The note is listed and a click on it moves the overmap cursor there.
    h.get_by_label("(99, 200) cache").click();
    h.run_steps(2);
    let sent = script.sent();
    let last = sent
        .iter()
        .rfind(|m| m["method"] == "bn.command.submit")
        .unwrap();
    assert_eq!(
        last["params"]["operation"],
        json!({"kind":"invoke","affordance_id":"overmap:cursor",
            "args":{"pos":{"dim":"","x":99,"y":200,"z":0}}})
    );
    assert_eq!(
        sent[0]["params"]["tiles"], true,
        "hello said the client draws tiles"
    );
}

#[test]
fn switching_between_tiles_and_ascii_tells_the_engine_without_a_restart() {
    let root = game_root("ui-switch-hello");
    let (t, script) = ScriptedTransport::new();
    let transport = t.on("bn.hello", vec![fixture("hello_result")]).on(
        "bn.subscribe",
        vec![
            fixture("subscribe_result"),
            fixture("snapshot_part_0"),
            fixture("snapshot_part_1"),
        ],
    );
    let app = App::new(EngineModel::connect(Box::new(transport), true))
        .with_root(root.clone())
        .with_settings(root.join("settings.json"));
    let mut h: H = Harness::builder()
        .with_size(egui::vec2(1100.0, 800.0))
        .build_ui_state(|ui, app: &mut App<EngineModel>| app.show(ui), app);
    settle(&mut h);
    let tiles_said = |script: &bn_client_egui::transport::ScriptHandle| -> Vec<bool> {
        script
            .sent()
            .iter()
            .filter(|m| m["method"] == "bn.hello")
            .map(|m| m["params"]["tiles"] == true)
            .collect()
    };
    assert_eq!(tiles_said(&script), [true], "started with tiles");

    common::open_client_settings(&mut h);
    h.get_by_label("ASCII").click();
    h.run_steps(4);
    assert_eq!(tiles_said(&script), [true, false]);

    common::open_client_settings(&mut h);
    h.get_by_label("Tiles").click();
    h.run_steps(2);
    settle(&mut h);
    assert_eq!(tiles_said(&script), [true, false, true]);
    let methods = script.sent_methods();
    // Each change of the cell size is declared once it has settled; a slow frame can add another.
    assert!(methods.iter().filter(|m| *m == "bn.viewport").count() >= 2);
}

fn session_with_display(tiles: bool) -> ScriptedTransport {
    let mut hello = fixture("hello_result");
    hello["result"]["engine"]["display"] = json!({"tiles": tiles, "tileset": "test"});
    let (t, _) = ScriptedTransport::new();
    t.on("bn.hello", vec![hello]).on(
        "bn.subscribe",
        vec![
            fixture("subscribe_result"),
            fixture("snapshot_part_0"),
            fixture("snapshot_part_1"),
        ],
    )
}

fn harness_with(root: &Path, settings: PathBuf, t: ScriptedTransport) -> H<'static> {
    let app = App::new(EngineModel::new(Box::new(t)))
        .with_root(root.to_path_buf())
        .with_settings(settings);
    Harness::builder()
        .with_size(egui::vec2(1100.0, 800.0))
        .build_ui_state(|ui, app: &mut App<EngineModel>| app.show(ui), app)
}

#[test]
fn with_no_saved_choice_the_map_starts_in_the_mode_the_engines_options_say() {
    let root = game_root("ui-engine-default");
    let mut off = harness_with(&root, root.join("off.json"), session_with_display(false));
    off.run_steps(4);
    assert!(!off.state().display().tiles(), "USE_TILES is off: ASCII");
    let mut on = harness_with(&root, root.join("on.json"), session_with_display(true));
    settle(&mut on);
    assert!(on.state().display().tiles());
    assert!(
        !root.join("off.json").exists(),
        "following the engine is not a choice and saves nothing"
    );
}

#[test]
fn a_saved_choice_wins_over_the_engines_options() {
    let root = game_root("ui-saved-wins");
    let settings = root.join("config/settings.json");
    std::fs::create_dir_all(settings.parent().unwrap()).unwrap();
    std::fs::write(
        &settings,
        json!({"tiles": true, "tileset": null}).to_string(),
    )
    .unwrap();
    let mut h = harness_with(&root, settings, session_with_display(false));
    settle(&mut h);
    assert!(h.state().display().tiles());
}

fn lit_cell(sequence: u32, x: i32, light: u8) -> Value {
    json!({"jsonrpc":"2.0","method":"bn.events","params":{"epoch":"e7f3","events":[{
        "sequence": sequence.to_string(), "revision": (sequence - 9).to_string(),
        "type": "cells.seen", "changes": {"cells": [
            {"at": {"dim":"","x":x,"y":4,"z":0}, "known":"visible", "light": light,
             "terrain": {"kind":"terrain","id":"t_floor","glyph":".","color":"c_cyan"}}]}}]}})
}

/// The native view draws a square it cannot see at all (dark) or can only tell is bright (bright only)
/// as its lighting overlay instead of the terrain; low light, lit and bright draw the terrain.
#[test]
fn dark_and_bright_only_squares_draw_the_lowlight_overlay_instead_of_their_terrain() {
    let root = game_root("ui-light");
    let run = |light: u8| {
        let (t, handle) = ScriptedTransport::new();
        let snapshot = || {
            vec![
                fixture("subscribe_result"),
                fixture("snapshot_part_0"),
                fixture("snapshot_part_1"),
            ]
        };
        // The second answers the subscription that follows telling the engine the client draws tiles.
        let t = t
            .on("bn.hello", vec![fixture("hello_result")])
            .on("bn.subscribe", snapshot())
            .on("bn.subscribe", snapshot());
        let mut h = harness_with(&root, root.join(format!("l{light}.json")), t);
        settle(&mut h);
        handle.push(lit_cell(41, 5, light));
        h.run_steps(4);
        let d = h.state().display();
        (d.drawn(), d.dimmed())
    };
    let (clear_drawn, clear_dimmed) = run(4);
    assert_eq!(clear_dimmed, 0);
    assert_eq!(run(3), (clear_drawn, 0));
    assert_eq!(
        run(1),
        (clear_drawn, 0),
        "low light is the grayscale terrain"
    );
    for light in [0, 2] {
        let (drawn, dimmed) = run(light);
        assert_eq!(dimmed, 1, "light {light} draws the overlay");
        assert_eq!(
            drawn,
            clear_drawn - 1,
            "light {light} hides the terrain sprite"
        );
    }
}

#[test]
fn starting_up_and_following_the_engine_write_no_settings_but_a_click_does() {
    let root = game_root("ui-no-write");
    let settings = root.join("quiet/settings.json");
    let mut h = harness_with(&root, settings.clone(), session_with_display(false));
    h.run_steps(6);
    assert!(
        !settings.exists(),
        "the engine's option is followed, not saved"
    );
    common::open_client_settings(&mut h);
    h.get_by_label("Tiles").click();
    h.run_steps(3);
    assert!(settings.exists(), "the player's click is saved");
}

/// The thin bars painted in this frame: rects 2 pt high.
fn bars(h: &H) -> Vec<egui::epaint::RectShape> {
    h.output()
        .shapes
        .iter()
        .filter_map(|c| match &c.shape {
            egui::Shape::Rect(r) if r.rect.height() == 2.0 => Some(r.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn a_hurt_creature_has_its_health_bar_along_the_bottom_of_its_square_in_tiles_and_ascii() {
    let root = game_root("ui-health");
    let hurt = session_with(|s| {
        s["result"]["entities"][0]["health"] = json!({"fifths": 2, "color": "c_light_red"});
    });
    let mut h = harness_of(&root, root.join("settings.json"), hurt);
    settle(&mut h);
    let color = h.state().model.colors().resolve("c_light_red").0;
    let filled = egui::Color32::from_rgb(color.0, color.1, color.2);
    for mode in ["tiles", "ASCII"] {
        if mode == "ASCII" {
            common::open_client_settings(&mut h);
            h.get_by_label("ASCII").click();
            h.run_steps(3);
        }
        let bars = bars(&h);
        let part = bars.iter().find(|r| r.fill == filled).expect(mode);
        let track = bars.iter().find(|r| r.fill != filled).expect(mode);
        assert_eq!(part.rect.left_bottom(), track.rect.left_bottom(), "{mode}");
        assert!(
            (part.rect.width() / track.rect.width() - 0.4).abs() < 0.01,
            "{mode}: {part:?} {track:?}"
        );
    }
}

#[test]
fn a_creature_at_full_health_has_no_health_bar() {
    let root = game_root("ui-health-full");
    let mut h = harness(&root, root.join("settings.json"));
    settle(&mut h);
    assert!(bars(&h).is_empty(), "{:?}", bars(&h));
}
