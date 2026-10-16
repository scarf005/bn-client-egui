//! Needs a real engine build and a log of what the speakers played:
//! `BN_ENGINE=<engine> BN_SOUND_LOG=<file> cargo test -p bn-client-egui --test live_sound -- --ignored --nocapture`.
//! `BN_SOUND_PROFILE=<dir>` runs the engine on `<dir>/user` and `<dir>/config` (a sound pack in `<dir>/user/sound`)
//! instead of a throwaway profile.
use bn_client_core::audio::SoundPack;
use bn_client_egui::transport::{Profile, StdioTransport};
use bn_client_egui::watchdog::Watchdog;
use bn_client_egui::{App, EngineModel, GameModel};
use eframe::egui;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use std::time::Duration;

type H<'a> = Harness<'a, App<EngineModel>>;

fn run_until(h: &mut H, what: &str, secs: u64, mut done: impl FnMut(&mut H) -> bool) {
    let mut watchdog = Watchdog::new(Duration::from_secs(secs), Duration::from_secs(600));
    while !done(h) {
        assert!(
            !watchdog.expired(h.state().model.received()),
            "timed out waiting for {what}"
        );
        h.run_steps(1);
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn settle(h: &mut H, steps: usize) {
    for _ in 0..steps {
        h.run_steps(1);
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn click_settled(h: &mut H, label: &str) {
    let mut seen = None;
    while seen != Some(h.get_by_label(label).rect()) {
        seen = Some(h.get_by_label(label).rect());
        h.run_steps(2);
    }
    h.get_by_label(label).click();
}

/// Whether the log names one of the files the pack has for an effect.
fn played(log: &str, pack: &SoundPack, id: &str, variant: &str) -> bool {
    pack.effect(id, variant).is_some_and(|files| {
        files
            .iter()
            .any(|f| log.contains(&format!("play Effect {}", f.file.display())))
    })
}

#[test]
#[ignore = "requires BN_ENGINE pointing at an engine binary"]
fn the_menu_and_walking_play_the_engines_sound_pack() {
    let engine = std::env::var_os("BN_ENGINE").expect("set BN_ENGINE");
    let log_path = std::env::var_os("BN_SOUND_LOG").expect("set BN_SOUND_LOG");
    let transport = match std::env::var_os("BN_SOUND_PROFILE") {
        Some(dir) => {
            let dir = std::path::PathBuf::from(dir);
            StdioTransport::spawn_with(
                engine.as_ref(),
                &Profile {
                    fresh: false,
                    userdir: Some(dir.join("user")),
                    configdir: Some(dir.join("config")),
                },
            )
        }
        None => StdioTransport::spawn_fresh(engine.as_ref()),
    }
    .expect("engine starts");
    let mut h = Harness::builder()
        .with_size(egui::vec2(1300.0, 800.0))
        .build_ui_state(
            |ui, app: &mut App<EngineModel>| app.show(ui),
            App::new(EngineModel::new(Box::new(transport))),
        );
    run_until(&mut h, "the main menu", 90, |h| {
        h.query_by_label("Tutorial").is_some()
    });
    let soundpack = h
        .state()
        .model
        .soundpack()
        .expect("the hello names a sound pack");
    println!("LIVE sound pack: {}", soundpack.display());
    let pack = SoundPack::load(&soundpack).expect("the sound pack loads");
    settle(&mut h, 20);
    // Moving along the main menu and back plays its move sound.
    for key in [egui::Key::ArrowRight, egui::Key::ArrowLeft] {
        h.key_press(key);
        settle(&mut h, 10);
    }
    click_settled(&mut h, "Tutorial");
    run_until(&mut h, "the tutorial", 300, |h| {
        h.state().model.command().is_some_and(|c| {
            c.target.as_deref() == Some("menu:entry") && c.finished && c.stage == "done"
        })
    });
    // Lessons pop up on the way; the walk goes on past them.
    for step in 0..30 {
        if h.state().model.menu().is_some() {
            h.key_press(egui::Key::Enter);
        } else {
            h.key_press(match step % 2 {
                0 => egui::Key::ArrowRight,
                _ => egui::Key::ArrowLeft,
            });
        }
        settle(&mut h, 10);
    }
    let log = std::fs::read_to_string(&log_path).expect("the speakers wrote their log");
    println!("LIVE sound log:\n{log}");
    assert!(
        log.lines().next().is_some_and(|l| l.ends_with(" open")),
        "the audio device opened"
    );
    assert!(
        played(&log, &pack, "menu_move", "default"),
        "the menu move sound played"
    );
    // Footsteps and ambience play where the pack has them.
    if pack.effect("plmove", "walk_tarmac").is_some() {
        assert!(
            log.contains("play Effect") && log.contains("/player/walk/"),
            "footsteps played"
        );
    }
    if pack.effect("environment", "indoors").is_some() {
        assert!(log.contains("play Channel"), "an ambience channel played");
    }
}
