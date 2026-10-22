//! The 3D map against a real engine in a real town, with the Ultica_iso tileset: pictures from several camera
//! angles, and what a frame costs.
//!
//! `BN_ENGINE=<engine> BN_ROOT=<game root with gfx/Ultica_iso> SHOT_DIR=<dir> cargo test --release -p bn-client-egui
//! --test it live_iso3d:: -- --ignored --nocapture`. `BN_ISO_GPU=1` draws on the machine's discrete GPU, else on the
//! software rasteriser the snapshot tests use.
use crate::common::live::{choose, dismiss_popups, run_until, save};
use bn_client_egui::iso::pages::Pages;
use bn_client_egui::iso::scene;
use bn_client_egui::transport::game_root;
use bn_client_egui::watchdog::Watchdog;
use bn_client_egui::{App, EngineModel, GameModel, MenuModel};
use eframe::egui;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use std::time::{Duration, Instant};

type H<'a> = Harness<'a, App<EngineModel>>;

fn percentile(v: &mut [f64], p: f64) -> f64 {
    v.sort_by(f64::total_cmp);
    v[((v.len() as f64 - 1.0) * p).round() as usize]
}

/// Turns the camera 15 degrees `times` times with Alt+Left.
fn turn(h: &mut H, times: usize) {
    for _ in 0..times {
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

/// Drags the right button over the map by `by` points, which tilts and turns the camera.
fn drag(h: &mut H, by: egui::Vec2) {
    let from = egui::pos2(500.0, 500.0);
    let event = |pos, pressed| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Secondary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    h.hover_at(from);
    h.run_steps(2);
    h.event(event(from, true));
    h.run_steps(1);
    h.hover_at(from + by);
    h.run_steps(2);
    h.event(event(from + by, false));
    h.run_steps(2);
}

#[test]
#[ignore = "requires BN_ENGINE and a game root with the Ultica_iso tileset"]
fn the_town_in_3d_from_every_side_and_what_a_frame_costs() {
    let engine = std::env::var_os("BN_ENGINE").expect("set BN_ENGINE");
    let root =
        game_root(engine.as_ref(), std::env::var_os("BN_ROOT").map(Into::into)).expect("game root");
    let dir = std::env::temp_dir().join(format!("bn-iso3d-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let settings = dir.join("settings.json");
    // No "three_d": an isometric tileset is drawn in 3D by itself.
    let tileset = std::env::var("BN_ISO_TILESET").unwrap_or("Ultica_iso".into());
    std::fs::write(
        &settings,
        // BN_ISO_3D draws a flat tileset in 3D as the "3D" view setting does.
        match std::env::var_os("BN_ISO_3D") {
            Some(_) => format!(r#"{{"tiles":true,"tileset":"{tileset}","three_d":true}}"#),
            None => format!(r#"{{"tiles":true,"tileset":"{tileset}"}}"#),
        },
    )
    .unwrap();
    let transport = crate::common::live::english_transport();
    let app = App::new(EngineModel::new(Box::new(transport)))
        .with_settings(settings)
        .with_root(root.clone())
        .with_target_format(eframe::wgpu::TextureFormat::Rgba8Unorm);
    // BN_ISO_SIZE=<width>x<height> sets the window, 1920x1080 otherwise.
    let window = std::env::var("BN_ISO_SIZE")
        .ok()
        .and_then(|v| {
            let (w, h) = v.split_once('x')?;
            Some(egui::vec2(w.parse().ok()?, h.parse().ok()?))
        })
        .unwrap_or(egui::vec2(1920.0, 1080.0));
    let mut builder = Harness::builder()
        .with_size(window)
        .with_step_dt(1.0 / 60.0);
    builder = match std::env::var_os("BN_ISO_GPU") {
        Some(_) => {
            let mut setup = egui_kittest::wgpu::default_wgpu_setup();
            if let egui_wgpu::WgpuSetup::CreateNew(new) = &mut setup {
                new.native_adapter_selector = Some(std::sync::Arc::new(|adapters, _| {
                    let gpu = adapters.iter().find(|a| {
                        a.get_info().device_type == egui_wgpu::wgpu::DeviceType::DiscreteGpu
                    });
                    gpu.or(adapters.first())
                        .cloned()
                        .ok_or("no adapter".to_string())
                }));
            }
            builder.wgpu_setup(setup)
        }
        None => builder.wgpu(),
    };
    let mut h = builder.build_ui_state(|ui, app: &mut App<EngineModel>| app.show(ui), app);
    bn_client_egui::fonts::install(&h.ctx, &root);

    run_until(&mut h, "the main menu", 120, |h| {
        h.state().model.menu().is_some()
    });
    choose(&mut h, "menu:tab", "new_game");
    // BN_ISO_ENTRY picks another way to start, such as `new_game:play_now_default` for the evacuee's shelter.
    let entry = std::env::var("BN_ISO_ENTRY").unwrap_or("new_game:play_now".into());
    choose(&mut h, "menu:entry", &entry);
    run_until(&mut h, "the map", 900, |h| h.state().model.size().x > 0);
    run_until(&mut h, "the settled game", 300, |h| {
        h.state().model.menu().is_none() && !h.state().model.busy()
    });
    dismiss_popups(&mut h);
    run_until(&mut h, "quiet", 180, |h| !h.state().model.busy());
    // The start is random: BN_ISO_WANT names a word of the start story the run is for, and others end here.
    let story = h
        .state()
        .model
        .native_log()
        .iter()
        .map(|l| l.shown().to_string())
        .collect::<Vec<_>>()
        .join(" ");
    println!("LIVE start {story:?}");
    if let Some(want) = std::env::var_os("BN_ISO_WANT")
        && !story.contains(want.to_str().unwrap())
    {
        println!("LIVE skipped");
        return;
    }
    // BN_ISO_TINTED: only a start with painted layers in view is shot, as a vehicle or an item has.
    let painted = {
        let m = &h.state().model;
        let size = m.size();
        let mut ids = std::collections::BTreeMap::<String, usize>::new();
        for y in 0..size.y {
            for x in 0..size.x {
                for l in &m.cell(bn_client_egui::Pos { x, y }).layers {
                    if l.tint_fg.is_some()
                        || l.tint_bg.is_some()
                        || !l.tint_keys.is_empty()
                        || l.tint_vars.is_some()
                    {
                        *ids.entry(l.id.clone()).or_default() += 1;
                    }
                }
            }
        }
        println!("LIVE painted layers {ids:?}");
        ids.len()
    };
    if std::env::var_os("BN_ISO_TINTED").is_some() && painted == 0 {
        println!("LIVE skipped");
        return;
    }
    // The engine turns off the game's safe mode for steps that this test does not take; sprites arrive.
    let drawn = |h: &H| {
        h.state()
            .iso_stats()
            .draws
            .load(std::sync::atomic::Ordering::Relaxed)
    };
    for _ in 0..600 {
        h.run_steps(2);
        h.render().expect("renders");
        if drawn(&h) > 5 {
            break;
        }
        std::thread::sleep(Duration::from_millis(30));
    }
    println!(
        "LIVE drawn: {} draws, {} instances",
        h.state()
            .iso_stats()
            .draws
            .load(std::sync::atomic::Ordering::Relaxed),
        h.state()
            .iso_stats()
            .instances
            .load(std::sync::atomic::Ordering::Relaxed)
    );
    save(&mut h, "town-yaw135.png");
    // BN_FLAT_ZOOMS: the flat map at every zoom step from the farthest to the closest, and nothing more.
    if std::env::var_os("BN_FLAT_ZOOMS").is_some() {
        h.hover_at(h.get_by_label("Map").rect().center());
        for step in -4..=4 {
            let zoom = h.state().display().settings().zoom;
            for _ in 0..(step - zoom).abs() {
                h.event(egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Line,
                    delta: egui::vec2(0.0, (step - zoom).signum() as f32),
                    modifiers: egui::Modifiers::NONE,
                    phase: egui::TouchPhase::Move,
                });
                h.run_steps(2);
            }
            h.hover_at(egui::pos2(1800.0, 1000.0));
            h.run_steps(3);
            save(&mut h, &format!("town-zoom{step}.png"));
            h.hover_at(h.get_by_label("Map").rect().center());
        }
        return;
    }
    {
        let m = &h.state().model;
        let size = m.size();
        let mut empty = 0;
        let mut ids = std::collections::BTreeMap::<String, usize>::new();
        for y in 0..size.y {
            for x in 0..size.x {
                let cell = m.cell(bn_client_egui::Pos { x, y });
                match cell.layers.is_empty() {
                    true => empty += 1,
                    false => *ids.entry(cell.layers[0].id.clone()).or_default() += 1,
                }
            }
        }
        // The terrain the tileset has no sprite for, with what the engine says to draw instead.
        if let Some((tileset, _)) = h.state().display().tileset() {
            let season = m.season();
            let mut bare = std::collections::BTreeMap::<String, (usize, Vec<String>, char)>::new();
            for y in 0..size.y {
                for x in 0..size.x {
                    for l in &m.cell(bn_client_egui::Pos { x, y }).layers {
                        let q = bn_client_egui::display::query(l, season.as_deref());
                        if l.terrain && tileset.choose(&q).is_none() {
                            let e = bare.entry(l.id.clone()).or_insert((
                                0,
                                l.fallbacks.clone(),
                                l.glyph,
                            ));
                            e.0 += 1;
                        }
                    }
                }
            }
            println!("LIVE bare terrain {bare:?}");
        }
        println!(
            "LIVE model {}x{}, {empty} squares without layers; first layers {ids:?}",
            size.x, size.y
        );
    }
    // The view choice is in the client settings window, closed again so it does not cover the map.
    let choose = |h: &mut egui_kittest::Harness<'_, _>, view: &str| {
        h.get_by_label("Client settings").click();
        h.run_steps(3);
        h.get_by_label(view).click();
        h.get_by_role_and_label(egui::accesskit::Role::Button, "Client settings")
            .click();
        h.run_steps(4);
    };
    choose(&mut h, "2D");
    save(&mut h, "town-flat.png");
    choose(&mut h, "3D");
    turn(&mut h, 6);
    save(&mut h, "town-yaw225.png");
    turn(&mut h, 6);
    save(&mut h, "town-yaw315.png");
    turn(&mut h, 6);
    save(&mut h, "town-yaw45.png");
    turn(&mut h, 6);
    drag(&mut h, egui::vec2(0.0, -400.0));
    save(&mut h, "town-pitch-high.png");
    drag(&mut h, egui::vec2(0.0, 900.0));
    save(&mut h, "town-pitch-low.png");
    // Through the avatar's eyes, turned as the orbit turns.
    choose(&mut h, "First person");
    save(&mut h, "town-first-person.png");
    turn(&mut h, 12);
    save(&mut h, "town-first-person-turned.png");
    choose(&mut h, "3D");

    // Frame cost: the UI pass of a frame, and the whole frame with its GPU work, turning the camera.
    drag(&mut h, egui::vec2(0.0, -150.0));
    let (mut ui, mut whole) = (vec![], vec![]);
    for _ in 0..120 {
        h.event(egui::Event::Key {
            key: egui::Key::ArrowLeft,
            physical_key: Some(egui::Key::ArrowLeft),
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::ALT,
        });
        let start = Instant::now();
        h.step();
        ui.push(start.elapsed().as_secs_f64() * 1e3);
        h.render().expect("renders");
        whole.push(start.elapsed().as_secs_f64() * 1e3);
    }
    println!(
        "LIVE frame ms ({}x{}, camera turning): ui p50={:.2} p95={:.2}; ui+render+readback p50={:.2} p95={:.2} max={:.2}",
        window.x,
        window.y,
        percentile(&mut ui.clone(), 0.5),
        percentile(&mut ui, 0.95),
        percentile(&mut whole.clone(), 0.5),
        percentile(&mut whole.clone(), 0.95),
        percentile(&mut whole, 1.0),
    );
    // What a frame of an animated sprite in view adds: the scene built again from the pages the view decoded.
    let (tileset, _) = h.state().display().tileset().expect("a tileset");
    let pages = Pages::start(tileset.sheets.clone(), egui::Context::default());
    let mut decoding = Watchdog::new(Duration::from_secs(120), Duration::from_secs(120));
    while pages.loading() {
        assert!(!decoding.expired(0), "the sheets are decoded");
        std::thread::sleep(Duration::from_millis(10));
    }
    let builds = (0..20)
        .map(|frame| {
            let start = Instant::now();
            scene::build(
                &h.state().model,
                tileset,
                Some(&pages),
                None,
                0,
                Some(frame),
            );
            start.elapsed().as_secs_f64() * 1e3
        })
        .collect::<Vec<_>>();
    println!(
        "LIVE scene build ms: p50={:.2} max={:.2}",
        percentile(&mut builds.clone(), 0.5),
        percentile(&mut builds.clone(), 1.0)
    );
    let stats = h.state().iso_stats();
    println!(
        "LIVE frame drawn: {} draws, {} instances",
        stats.draws.load(std::sync::atomic::Ordering::Relaxed),
        stats.instances.load(std::sync::atomic::Ordering::Relaxed)
    );
    let load = std::fs::read_to_string("/proc/loadavg").unwrap_or_default();
    println!("LIVE load {}", load.trim());
}
