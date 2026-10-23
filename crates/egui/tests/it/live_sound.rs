//! Needs a real engine build and a log of what the speakers played:
//! `BN_ENGINE=<engine> BN_SOUND_LOG=<file> cargo test -p bn-client-egui --test it live_sound:: -- --ignored --nocapture`.
//! `BN_SOUND_PROFILE=<dir>` runs the engine on `<dir>/user` and `<dir>/config` (a sound pack in `<dir>/user/sound`)
//! instead of a throwaway profile.
use bn_client_core::audio::SoundPack;
use bn_client_egui::transport::{Profile, StdioTransport};
use bn_client_egui::watchdog::Watchdog;
use bn_client_egui::{App, EngineModel, GameModel, MenuModel};
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
    // A list row runs on a double click.
    let at = h.get_by_label(label).rect().center();
    crate::common::double_click(h, at);
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
                    ..Profile::default()
                },
            )
            .expect("engine starts")
        }
        None => crate::common::live::english_transport(),
    };
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

const LOCKED_MODINFO: &str = r#"[{"type":"MOD_INFO","id":"locked_door","name":"Locked door","authors":["test"],
"description":"A locked door beside the avatar.","category":"content","lua_api_version":2,"dependencies":["bn"]}]"#;

/// Puts a locked door that opens from neither side on the square east of the avatar, at the first turn.
const LOCKED_PRELOAD: &str = r#"
local built = false
gapi.add_on_every_x_hook(TimeDuration.from_turns(1), function()
  if built then return end
  built = true
  local map = gapi.get_map()
  local p = gapi.get_avatar():get_pos_ms()
  local door = TripointBubMs.new(p.x + 1, p.y, p.z)
  map:set_furn_at(door, FurnId.new("f_null"):int_id())
  map:set_ter_at(door, TerId.new("t_door_locked_interior"):int_id())
  gdebug.log_info("locked_door built")
end)
"#;

#[test]
#[ignore = "requires BN_ENGINE pointing at an engine binary"]
fn a_double_click_on_a_locked_door_plays_the_error_sound() {
    let log_path = std::env::var_os("BN_SOUND_LOG").expect("set BN_SOUND_LOG");
    let dir = std::env::temp_dir().join(format!("bn-locked-{}", std::process::id()));
    let config = dir.join("config");
    let modpath = dir.join("mods").join("locked_door");
    std::fs::create_dir_all(&config).unwrap();
    std::fs::create_dir_all(&modpath).unwrap();
    std::fs::write(modpath.join("modinfo.json"), LOCKED_MODINFO).unwrap();
    std::fs::write(modpath.join("preload.lua"), LOCKED_PRELOAD).unwrap();
    std::fs::write(
        config.join("options.json"),
        r#"[{"name":"USE_LANG","value":"en_US"},{"name":"SAFEMODE","value":"false"}]"#,
    )
    .unwrap();
    std::fs::write(config.join("default_mods.json"), r#"["bn","locked_door"]"#).unwrap();
    let profile = Profile {
        fresh: false,
        userdir: Some(dir),
        configdir: Some(config),
        seed: Some("2".into()),
    };
    let transport = StdioTransport::spawn_with(&crate::common::live::engine(), &profile)
        .expect("engine starts");
    let mut h = Harness::builder()
        .with_size(egui::vec2(1290.0, 830.0))
        .build_ui_state(
            |ui, app: &mut App<EngineModel>| app.show(ui),
            App::new(EngineModel::new(Box::new(transport))),
        );
    let pack = SoundPack::load(&soundpack_of(&mut h)).expect("the sound pack loads");
    crate::common::live::new_game(&mut h, "new_game:play_now");
    crate::common::live::dismiss_popups(&mut h);
    // The mod builds the door at the first turn.
    for _ in 0..2 {
        assert!(h.state_mut().model.invoke_action("pause"));
        run_until(&mut h, "a turn", 120, |h| !h.state().model.busy());
    }
    let me = h.state().model.player();
    let door = h.state().model.cell(bn_client_egui::Pos {
        x: me.x + 1,
        y: me.y,
    });
    assert!(
        door.layers.iter().any(|l| l.id == "t_door_locked_interior"),
        "the door is built: {:?}",
        door.layers
    );
    let squares = h.state().map_squares().unwrap();
    let cell = squares.width() / h.state().model.size().x as f32;
    let at = squares.min + egui::vec2(me.x as f32 + 1.5, me.y as f32 + 0.5) * cell;
    h.hover_at(at);
    settle(&mut h, 4);
    let before = std::fs::read_to_string(&log_path).map_or(0, |log| log.len());
    for _ in 0..2 {
        for pressed in [true, false] {
            h.event(egui::Event::PointerButton {
                pos: at,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            });
            h.run_steps(1);
        }
    }
    // The engine answers the route into the door with its message and the error sound.
    run_until(&mut h, "the error sound", 60, |_| {
        std::fs::read_to_string(&log_path)
            .is_ok_and(|log| played(&log[before..], &pack, "menu_error", "default"))
    });
}

/// The sound pack the engine's hello names, once the main menu is up.
fn soundpack_of(h: &mut H) -> std::path::PathBuf {
    run_until(h, "the main menu", 90, |h| {
        h.query_by_label("Tutorial").is_some()
    });
    (h.state().model.soundpack()).expect("the hello names a sound pack")
}
