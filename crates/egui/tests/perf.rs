//! Headless guards against needless frames: an app with nothing to do waits, and a settled tileset costs no uploads.
mod common;
use bn_client_egui::{App, EngineModel};
use common::{game_root, session};
use eframe::egui;
use egui_kittest::Harness;
use std::time::Duration;

type H<'a> = Harness<'a, App<EngineModel>>;

fn harness<'a>(app: App<EngineModel>) -> H<'a> {
    Harness::builder()
        .with_size(egui::vec2(1100.0, 800.0))
        .with_step_dt(1.0 / 60.0)
        .build_ui_state(|ui, app: &mut App<EngineModel>| app.show(ui), app)
}

/// How long egui would wait before the next frame if nothing else happens.
fn repaint_delay(h: &H) -> Duration {
    h.output().viewport_output[&egui::ViewportId::ROOT].repaint_delay
}

#[test]
fn a_connected_app_with_nothing_to_do_waits_for_the_engine_instead_of_repainting() {
    let (t, _) = session();
    let mut h = harness(App::new(EngineModel::new(Box::new(t))));
    h.run_steps(20);
    assert!(
        repaint_delay(&h) >= Duration::from_millis(500),
        "idle repaint after {:?}",
        repaint_delay(&h)
    );
}

#[test]
fn a_settled_tileset_neither_uploads_textures_nor_asks_for_repaints() {
    let root = game_root("perf-settled");
    let (t, _) = session();
    let app = App::new(EngineModel::new(Box::new(t)))
        .with_root(root.clone())
        .with_settings(root.join("settings.json"));
    let mut h = harness(app);
    for _ in 0..500 {
        h.run_steps(2);
        if h.state().display().ready() && !h.state().display().loading() {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    h.run_steps(5);
    assert!(!h.state().display().loading(), "the tileset never loaded");
    let textures = h.ctx.tex_manager().read().allocated().count();
    h.run_steps(30);
    assert_eq!(h.ctx.tex_manager().read().allocated().count(), textures);
    assert!(repaint_delay(&h) >= Duration::from_millis(500));
}
