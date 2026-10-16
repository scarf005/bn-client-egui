//! The 3D map against a real engine in a real town, with the Ultica_iso tileset: pictures from several camera
//! angles, and what a frame costs.
//!
//! `BN_ENGINE=<engine> BN_ROOT=<game root with gfx/Ultica_iso> SHOT_DIR=<dir> cargo test --release -p bn-client-egui
//! --test live_iso3d -- --ignored --nocapture`. `BN_ISO_GPU=1` draws on the machine's discrete GPU, else on the
//! software rasteriser the snapshot tests use.
use bn_client_egui::interaction::InvokeError;
use bn_client_egui::protocol::InvokeArgs;
use bn_client_egui::transport::{StdioTransport, game_root};
use bn_client_egui::watchdog::Watchdog;
use bn_client_egui::{App, EngineModel, GameModel};
use eframe::egui;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use std::time::{Duration, Instant};

type H<'a> = Harness<'a, App<EngineModel>>;

fn run_until(h: &mut H, what: &str, secs: u64, mut done: impl FnMut(&H) -> bool) {
    let mut watchdog = Watchdog::new(Duration::from_secs(secs), Duration::from_secs(900));
    while !done(h) {
        assert!(
            !watchdog.expired(h.state().model.received()),
            "timed out: {what}; {:?}; {:?}",
            h.state().model.command(),
            h.state().model.menu()
        );
        h.run_steps(1);
        std::thread::sleep(Duration::from_millis(30));
    }
}

fn choose(h: &mut H, affordance: &str, row: &str) {
    let offered = |h: &H| {
        h.state()
            .model
            .screens()
            .last()
            .is_some_and(|s| s.affordances.iter().any(|a| a.id == affordance))
    };
    loop {
        run_until(h, row, 180, |h| offered(h) && !h.state().model.busy());
        let args = InvokeArgs {
            row: Some(row.into()),
            ..Default::default()
        };
        match h.state_mut().model.invoke_affordance(affordance, args) {
            Err(InvokeError::Busy) => h.run_steps(1),
            sent => return sent.unwrap_or_else(|e| panic!("{affordance} on {row}: {e:?}")),
        }
    }
}

fn dismiss_popups(h: &mut H) {
    while let Some(menu) = h.state().model.menu() {
        match menu.choices.first().map(|c| c.id.clone()) {
            Some(id) if !menu.can_cancel => h.state_mut().model.choose(&id),
            _ => h.state_mut().model.cancel_menu(),
        }
        run_until(h, "popup", 180, |h| !h.state().model.busy());
        h.run_steps(3);
    }
}

fn percentile(v: &mut [f64], p: f64) -> f64 {
    v.sort_by(f64::total_cmp);
    v[((v.len() as f64 - 1.0) * p).round() as usize]
}

fn save(h: &mut H, name: &str) {
    let image = h.render().expect("renders");
    if let Some(dir) = std::env::var_os("SHOT_DIR") {
        std::fs::create_dir_all(&dir).unwrap();
        image.save(std::path::Path::new(&dir).join(name)).unwrap();
    }
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
    let transport = StdioTransport::spawn_fresh(engine.as_ref()).expect("engine starts");
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
    choose(&mut h, "menu:entry", "new_game:play_now");
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
    let stats = h.state().iso_stats();
    println!(
        "LIVE frame drawn: {} draws, {} instances",
        stats.draws.load(std::sync::atomic::Ordering::Relaxed),
        stats.instances.load(std::sync::atomic::Ordering::Relaxed)
    );
    let load = std::fs::read_to_string("/proc/loadavg").unwrap_or_default();
    println!("LIVE load {}", load.trim());
}
