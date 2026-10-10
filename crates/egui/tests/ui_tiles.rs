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
    let mut subscribe = fixture("subscribe_result");
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
    let app = App::new(EngineModel::new(Box::new(session())))
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
}

#[test]
fn switching_to_ascii_draws_glyphs_again_and_the_choice_survives_a_restart() {
    let root = game_root("ui-switch");
    let settings = root.join("config/settings.json");
    let mut h = harness(&root, settings.clone());
    settle(&mut h);
    assert!(h.state().display().drawn() > 0);

    h.get_by_label("ASCII").click();
    h.run_steps(3);
    assert!(!h.state().display().tiles());
    assert_eq!(h.state().display().drawn(), 0);
    assert_eq!(saved(&settings), json!({"tiles": false, "tileset": null}));

    // A new run reads the file and starts in ASCII without loading the tileset.
    let again = harness(&root, settings.clone());
    assert!(!again.state().display().tiles());

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
