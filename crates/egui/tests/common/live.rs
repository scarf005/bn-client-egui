//! What a test against a real engine (`BN_ENGINE`, and `BN_ROOT` when the game root is not beside it) does
//! between its own steps: start the client over an English engine, wait for the engine, pick from its menus.
use bn_client_egui::interaction::InvokeError;
use bn_client_egui::protocol::InvokeArgs;
use bn_client_egui::transport::{Profile, StdioTransport, game_root};
use bn_client_egui::watchdog::Watchdog;
use bn_client_egui::{App, EngineModel, GameModel, MenuModel};
use eframe::egui;
use egui_kittest::Harness;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

pub type H<'a> = Harness<'a, App<EngineModel>>;

/// The engine binary the tests run.
pub fn engine() -> PathBuf {
    std::env::var_os("BN_ENGINE").expect("set BN_ENGINE").into()
}

/// The game root of `engine()`.
pub fn root() -> PathBuf {
    game_root(&engine(), std::env::var_os("BN_ROOT").map(Into::into)).expect("game root")
}

/// A new engine in a profile of its own whose game language is English: tests name what they pick by its
/// English label, whatever the language of the machine they run on. It runs on the seed `BN_SEED`, else
/// `live`, so a new game starts in the same place every run.
pub fn english_transport() -> StdioTransport {
    static SPAWNED: AtomicUsize = AtomicUsize::new(0);
    let n = SPAWNED.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("bn-live-{}-{n}", std::process::id()));
    let config = dir.join("config");
    std::fs::create_dir_all(&config).unwrap();
    std::fs::write(
        config.join("options.json"),
        r#"[{"name":"USE_LANG","value":"en_US"}]"#,
    )
    .unwrap();
    let profile = Profile {
        fresh: false,
        userdir: Some(dir),
        configdir: Some(config),
        seed: Some(std::env::var("BN_SEED").unwrap_or_else(|_| "live".into())),
    };
    StdioTransport::spawn_with(&engine(), &profile).expect("engine starts")
}

/// The client drawing `UltimateCataclysm` tiles over `english_transport()`, in a window of `size`, its fonts the
/// game's.
pub fn tiled<'a>(name: &str, size: egui::Vec2) -> H<'a> {
    let root = root();
    let dir = std::env::temp_dir().join(format!("bn-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let settings = dir.join("settings.json");
    std::fs::write(&settings, r#"{"tiles":true,"tileset":"UltimateCataclysm"}"#).unwrap();
    let app = App::new(EngineModel::new(Box::new(english_transport())))
        .with_settings(settings)
        .with_root(root.clone())
        .with_target_format(eframe::wgpu::TextureFormat::Rgba8Unorm);
    let h = Harness::builder()
        .with_size(size)
        .with_step_dt(1.0 / 60.0)
        .wgpu()
        .build_ui_state(|ui, app: &mut App<EngineModel>| app.show(ui), app);
    bn_client_egui::fonts::install(&h.ctx, &root);
    h
}

/// Steps the client until `done`; fails naming `what` once the engine has been silent for `secs`.
pub fn run_until(h: &mut H, what: &str, secs: u64, mut done: impl FnMut(&H) -> bool) {
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

/// Invokes `affordance` on `row` of the screen on top once the screen offers it and the engine is free.
pub fn choose(h: &mut H, affordance: &str, row: &str) {
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

/// Chooses the entry of the menu on top labelled `label`, else the first whose label contains it, once the menu
/// lists it: `East` is East, not North East.
pub fn pick(h: &mut H, label: &str) {
    let listed = |h: &H| {
        h.state().model.menu().and_then(|m| {
            let exact = m.choices.iter().position(|c| c.label == label);
            let at = exact.or_else(|| m.choices.iter().position(|c| c.label.contains(label)))?;
            m.choices.into_iter().nth(at)
        })
    };
    run_until(h, label, 120, |h| {
        !h.state().model.busy() && listed(h).is_some()
    });
    let id = listed(h).expect("listed").id;
    h.state_mut().model.choose(&id);
    run_until(h, label, 120, |h| !h.state().model.busy());
    h.run_steps(3);
}

/// Closes the pop-ups on top: by their first choice where they cannot be cancelled.
pub fn dismiss_popups(h: &mut H) {
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

/// The main menu's new game `entry`, played until the map shows and its pop-ups are closed.
pub fn new_game(h: &mut H, entry: &str) {
    run_until(h, "the main menu", 120, |h| {
        h.state().model.menu().is_some()
    });
    choose(h, "menu:tab", "new_game");
    choose(h, "menu:entry", entry);
    run_until(h, "the map", 900, |h| h.state().model.size().x > 0);
    run_until(h, "the settled game", 300, |h| {
        h.state().model.menu().is_none() && !h.state().model.busy()
    });
    dismiss_popups(h);
    settle(h);
}

/// Half a second of frames, for the engine's answers to land.
pub fn settle(h: &mut H) {
    for _ in 0..30 {
        h.run_steps(1);
        std::thread::sleep(Duration::from_millis(30));
    }
}

/// Writes the client as drawn to `$SHOT_DIR/name`, when `SHOT_DIR` is set.
pub fn save(h: &mut H, name: &str) {
    let image = h.render().expect("renders");
    if let Some(dir) = std::env::var_os("SHOT_DIR") {
        std::fs::create_dir_all(&dir).unwrap();
        image.save(std::path::Path::new(&dir).join(name)).unwrap();
    }
}
