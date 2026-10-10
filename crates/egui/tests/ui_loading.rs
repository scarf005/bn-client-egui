mod common;
use bn_client_egui::{App, EngineModel};
use common::session;
use eframe::egui;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use serde_json::json;
use std::path::PathBuf;

const PNG: &[u8] = &[
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0, 0, 0, 0x0d, 0x49, 0x48, 0x44, 0x52, 0, 0, 0,
    1, 0, 0, 0, 1, 8, 6, 0, 0, 0, 0x1f, 0x15, 0xc4, 0x89, 0, 0, 0, 0x0d, 0x49, 0x44, 0x41, 0x54,
    0x78, 0x9c, 0x63, 0, 1, 0, 0, 5, 0, 1, 0x0d, 0x0a, 0x2d, 0xb4, 0, 0, 0, 0, 0x49, 0x45, 0x4e,
    0x44, 0xae, 0x42, 0x60, 0x82,
];

fn root() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("bn-loading-ui-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("gfx")).unwrap();
    std::fs::write(dir.join("gfx/a.png"), PNG).unwrap();
    dir
}

fn harness<'a>() -> (
    Harness<'a, App<EngineModel>>,
    bn_client_egui::transport::ScriptHandle,
) {
    let (t, h) = session();
    let app = App::new(EngineModel::new(Box::new(t))).with_root(root());
    let harness = Harness::builder()
        .with_size(egui::vec2(900.0, 700.0))
        .build_ui_state(|ui, app: &mut App<EngineModel>| app.show(ui), app);
    (harness, h)
}

fn progress(index: u32) -> serde_json::Value {
    json!({"jsonrpc":"2.0","method":"bn.loading","params":{"epoch":"e7f3","title":"Loading world",
      "entries":["Loading items","Loading monsters","Finalizing"],"index":index,"image":{"path":"gfx/a.png"}}})
}

#[test]
fn loading_screen_shows_title_steps_and_a_live_current_step() {
    let (mut h, script) = harness();
    h.run_steps(3);
    script.push(progress(1));
    h.run_steps(5);
    h.get_by_label("Loading world");
    h.get_by_label("Loading items");
    h.get_by_label("Finalizing");
    h.get_by_label("Loading world: Loading monsters (2 of 3)");
    assert!(
        h.query_by_label("Map").is_none(),
        "the game UI is hidden while loading"
    );
}

#[test]
fn live_text_follows_the_current_step() {
    let (mut h, script) = harness();
    h.run_steps(3);
    script.push(progress(0));
    h.run_steps(3);
    script.push(progress(2));
    h.run_steps(3);
    h.get_by_label("Loading world: Finalizing (3 of 3)");
    assert!(
        h.query_by_label("Loading world: Loading items (1 of 3)")
            .is_none()
    );
}

#[test]
fn loading_screen_has_an_image_node_and_disappears_when_done() {
    let (mut h, script) = harness();
    h.run_steps(3);
    script.push(progress(0));
    h.run_steps(8);
    h.get_by_label("Loading image");
    script
        .push(json!({"jsonrpc":"2.0","method":"bn.loading","params":{"epoch":"e7f3","done":true}}));
    h.run_steps(5);
    assert!(h.query_by_label("Loading world").is_none());
    assert!(h.query_by_label("Loading image").is_none());
}

#[test]
fn missing_image_file_does_not_break_the_screen() {
    let (mut h, script) = harness();
    h.run_steps(3);
    let mut p = progress(0);
    p["params"]["image"]["path"] = "gfx/missing.png".into();
    script.push(p);
    h.run_steps(8);
    h.get_by_label("Loading world");
}

#[test]
fn progress_without_steps_shows_just_the_title() {
    let (mut h, script) = harness();
    h.run_steps(3);
    script.push(json!({"jsonrpc":"2.0","method":"bn.loading","params":{"epoch":"e7f3","title":"Starting","entries":[],"index":0}}));
    h.run_steps(4);
    h.get_by_label("Starting");
}
