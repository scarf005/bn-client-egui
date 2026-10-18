//! Against a real engine: while the avatar waits out six hours, the top bar names the activity with how much of
//! it is done, and a bar over the avatar fills as the hours pass; pictures of both.
//!
//! `BN_ENGINE=<engine> SHOT_DIR=<dir> cargo test --release -p bn-client-egui --test live_activity -- --ignored
//! --nocapture`; `BN_ROOT` names the game root when it is not beside the engine.
use bn_client_egui::interaction::InvokeError;
use bn_client_egui::protocol::InvokeArgs;
use bn_client_egui::transport::{StdioTransport, game_root};
use bn_client_egui::watchdog::Watchdog;
use bn_client_egui::{App, EngineModel, GameModel, MenuModel};
use eframe::egui;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use std::time::Duration;

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

/// Chooses the entry of the menu on top whose label contains `label`, once the menu lists it.
fn pick(h: &mut H, label: &str) {
    let listed = |h: &H| {
        h.state()
            .model
            .menu()
            .and_then(|m| m.choices.into_iter().find(|c| c.label.contains(label)))
    };
    run_until(h, label, 120, |h| {
        !h.state().model.busy() && listed(h).is_some()
    });
    let id = listed(h).expect("listed").id;
    h.state_mut().model.choose(&id);
    run_until(h, label, 120, |h| !h.state().model.busy());
    h.run_steps(3);
}

fn dismiss_popups(h: &mut H) {
    for _ in 0..20 {
        let Some(menu) = h.state().model.menu() else {
            return;
        };
        match menu.choices.first().map(|c| c.id.clone()) {
            Some(id) if !menu.can_cancel => h.state_mut().model.choose(&id),
            _ => h.state_mut().model.cancel_menu(),
        }
        run_until(h, "popup", 180, |h| !h.state().model.busy());
        h.run_steps(3);
    }
    panic!("a pop-up stays open: {:?}", h.state().model.menu());
}

fn settle(h: &mut H) {
    for _ in 0..30 {
        h.run_steps(1);
        std::thread::sleep(Duration::from_millis(30));
    }
}

fn save(h: &mut H, name: &str) {
    let image = h.render().expect("renders");
    if let Some(dir) = std::env::var_os("SHOT_DIR") {
        std::fs::create_dir_all(&dir).unwrap();
        image.save(std::path::Path::new(&dir).join(name)).unwrap();
    }
}

/// The thousandths of the avatar's activity done, if it has one with an end.
fn done(h: &H) -> Option<u16> {
    h.state().model.activity().and_then(|a| a.done)
}

#[test]
#[ignore = "requires BN_ENGINE pointing at an engine binary"]
fn waiting_six_hours_names_the_activity_and_fills_its_bar() {
    let engine = std::env::var_os("BN_ENGINE").expect("set BN_ENGINE");
    let root =
        game_root(engine.as_ref(), std::env::var_os("BN_ROOT").map(Into::into)).expect("game root");
    let dir = std::env::temp_dir().join(format!("bn-activity-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let settings = dir.join("settings.json");
    std::fs::write(&settings, r#"{"tiles":true,"tileset":"UltimateCataclysm"}"#).unwrap();
    let transport = StdioTransport::spawn_fresh(engine.as_ref()).expect("engine starts");
    let app = App::new(EngineModel::new(Box::new(transport)))
        .with_settings(settings)
        .with_root(root.clone())
        .with_target_format(eframe::wgpu::TextureFormat::Rgba8Unorm);
    let mut h = Harness::builder()
        .with_size(egui::vec2(1280.0, 720.0))
        .with_step_dt(1.0 / 60.0)
        .wgpu()
        .build_ui_state(|ui, app: &mut App<EngineModel>| app.show(ui), app);
    bn_client_egui::fonts::install(&h.ctx, &root);
    run_until(&mut h, "the main menu", 120, |h| {
        h.state().model.menu().is_some()
    });
    choose(&mut h, "menu:tab", "new_game");
    choose(&mut h, "menu:entry", "new_game:play_now_default");
    run_until(&mut h, "the map", 900, |h| h.state().model.size().x > 0);
    run_until(&mut h, "the settled game", 300, |h| {
        h.state().model.menu().is_none() && !h.state().model.busy()
    });
    dismiss_popups(&mut h);
    settle(&mut h);
    assert!(
        h.state().model.activity().is_none(),
        "an activity before waiting"
    );
    assert!(h.state_mut().model.invoke_action("wait"));
    run_until(&mut h, "the wait menu", 60, |h| {
        !h.state().model.busy() && h.state().model.menu().is_some()
    });
    // With an alarm clock, the game first asks whether to wait or set the alarm.
    let asks = |h: &H| {
        h.state()
            .model
            .menu()
            .is_some_and(|m| m.choices.iter().any(|c| c.label == "Wait a while"))
    };
    if asks(&h) {
        pick(&mut h, "Wait a while");
    }
    // Chosen without waiting for the engine to be free again, which it is only once the wait is over.
    let six = |h: &H| {
        h.state()
            .model
            .menu()
            .and_then(|m| m.choices.into_iter().find(|c| c.label.contains("6 hours")))
    };
    run_until(&mut h, "6 hours", 120, |h| {
        !h.state().model.busy() && six(h).is_some()
    });
    let id = six(&h).expect("listed").id;
    h.state_mut().model.choose(&id);
    run_until(&mut h, "the wait under way", 120, |h| {
        done(h).is_some_and(|d| d > 0)
    });
    let first = done(&h).unwrap();
    let name = h.state().model.activity().unwrap().name;
    println!("LIVE activity {name:?} at {first}/1000");
    assert!(h.query_by_label(&name).is_some(), "{name} in the top bar");
    settle(&mut h);
    save(&mut h, "waiting-early.png");
    run_until(&mut h, "more of the wait done", 300, |h| {
        done(h).is_some_and(|d| d >= first + 100)
    });
    settle(&mut h);
    println!("LIVE activity later at {:?}/1000", done(&h));
    save(&mut h, "waiting-later.png");
}
