//! Needs a real engine build: `BN_ENGINE=/path/to/cataclysm-bn-tiles cargo test -p bn-client-egui --test live_ui -- --ignored --nocapture`
//! Set `BN_CLIENT_TRACE=<file>` to log the JSON-RPC traffic.
use bn_client_egui::transport::StdioTransport;
use bn_client_egui::watchdog::Watchdog;
use bn_client_egui::{App, EngineModel, GameModel, MenuModel};
use eframe::egui;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use std::time::{Duration, Instant};

type H<'a> = Harness<'a, App<EngineModel>>;

fn run_until(h: &mut H, what: &str, secs: u64, mut done: impl FnMut(&mut H) -> bool) {
    let mut watchdog = Watchdog::new(Duration::from_secs(secs), Duration::from_secs(600));
    while !done(h) {
        let model = &h.state().model;
        assert!(
            !watchdog.expired(model.received()),
            "timed out waiting for {what}; status: {:?}; menu: {:?}; last messages: {:?}",
            model.command(),
            model.menu().map(|m| (m.context, m.title)),
            model.messages().iter().rev().take(5).collect::<Vec<_>>()
        );
        h.run_steps(1);
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// Click a label once its window has stopped moving: the first frames of a dialog lay it out
/// at another place, and a click aimed at the earlier place lands on its neighbour.
fn click_settled(h: &mut H, label: &str) {
    let mut seen = None;
    while seen != Some(h.get_by_label(label).rect()) {
        seen = Some(h.get_by_label(label).rect());
        h.run_steps(2);
    }
    h.get_by_label(label).click();
}

#[test]
#[ignore = "requires BN_ENGINE pointing at an engine binary"]
fn clicking_a_main_menu_entry_shows_the_command_status() {
    let engine = std::env::var_os("BN_ENGINE").expect("set BN_ENGINE");
    let transport = StdioTransport::spawn_fresh(engine.as_ref()).expect("engine starts");
    let model = EngineModel::new(Box::new(transport));
    let mut h = Harness::builder()
        .with_size(egui::vec2(1300.0, 800.0))
        .build_ui_state(
            |ui, app: &mut App<EngineModel>| app.show(ui),
            App::new(model),
        );
    run_until(&mut h, "the main menu", 60, |h| {
        h.query_by_label("Tutorial").is_some()
    });
    click_settled(&mut h, "Tutorial");
    let clicked = Instant::now();
    // A command that went well is no longer shown, so the model's status is what reports it.
    run_until(&mut h, "the command to finish", 300, |h| {
        h.state().model.command().is_some_and(|c| {
            c.target.as_deref() == Some("menu:entry") && c.finished && c.stage == "done"
        })
    });
    println!("Tutorial done after {:?}", clicked.elapsed());
}

/// Set `SHOT_DIR` to keep the main menu as a PNG.
#[test]
#[ignore = "requires BN_ENGINE pointing at an engine binary"]
fn a_korean_connection_words_the_clients_own_chrome_in_korean() {
    let engine = std::env::var_os("BN_ENGINE").expect("set BN_ENGINE");
    let root = bn_client_egui::transport::game_root(
        engine.as_ref(),
        std::env::var_os("BN_ROOT").map(Into::into),
    )
    .expect("game root");
    let transport = StdioTransport::spawn_fresh(engine.as_ref()).expect("engine starts");
    let options = bn_client_egui::ConnectOptions {
        locale: Some("ko_KR".into()),
        ..Default::default()
    };
    let app = App::new(EngineModel::open(Box::new(transport), options))
        .with_root(root.clone())
        .with_target_format(eframe::wgpu::TextureFormat::Rgba8Unorm);
    let mut h = Harness::builder()
        .with_size(egui::vec2(1280.0, 720.0))
        .wgpu()
        .build_ui_state(|ui, app: &mut App<EngineModel>| app.show(ui), app);
    bn_client_egui::theme::apply(&h.ctx);
    assert!(bn_client_egui::fonts::install(&h.ctx, &root));
    run_until(&mut h, "the main menu", 120, |h| {
        !h.state().model.screens().is_empty()
    });
    for _ in 0..10 {
        h.run_steps(1);
    }
    h.get_by_label("준비");
    h.get_by_label("클라이언트 설정");
    assert!(h.query_by_label("Ready").is_none());
    let image = h.render().expect("renders");
    if let Some(dir) = std::env::var_os("SHOT_DIR") {
        std::fs::create_dir_all(&dir).unwrap();
        image
            .save(std::path::Path::new(&dir).join("main-menu-ko.png"))
            .unwrap();
    }
}
