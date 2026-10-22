//! Against a real engine: the debug map editor, its feature pickers, its shape selection and its mapgen stamp.
//!
//! `BN_ENGINE=<engine> SHOT_DIR=<dir> cargo test --release -p bn-client-egui --test live_editmap -- --ignored
//! --nocapture`; `BN_ROOT` names the game root when it is not beside the engine, `SCREEN_DIR` keeps the
//! stack of each state as JSON.
mod common;
use bn_client_egui::protocol::{Component, InvokeArgs};
use bn_client_egui::transport::game_root;
use bn_client_egui::watchdog::Watchdog;
use bn_client_egui::{App, EngineModel, GameModel, MenuModel};
use common::live::{choose, dismiss_popups, pick, settle};
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
            h.state()
                .model
                .screens()
                .last()
                .map(|s| (&s.kind, &s.title))
        );
        h.run_steps(1);
        std::thread::sleep(Duration::from_millis(30));
    }
}

fn top_kind(h: &H) -> Option<String> {
    h.state()
        .model
        .screens()
        .last()
        .and_then(|s| s.kind.clone())
}

fn wait_kind(h: &mut H, kind: &str) {
    run_until(h, kind, 120, |h| {
        !h.state().model.busy() && top_kind(h).as_deref() == Some(kind)
    });
    settle(h);
}

/// Pictures of the window at both sizes, and the stack as JSON.
fn save(h: &mut H, name: &str) {
    if let Some(dir) = std::env::var_os("SCREEN_DIR") {
        std::fs::create_dir_all(&dir).unwrap();
        let stack = serde_json::to_string_pretty(h.state().model.screens()).unwrap();
        std::fs::write(
            std::path::Path::new(&dir).join(format!("{name}.json")),
            stack,
        )
        .unwrap();
    }
    let Some(dir) = std::env::var_os("SHOT_DIR") else {
        return;
    };
    std::fs::create_dir_all(&dir).unwrap();
    for (w, ht) in [(1280.0, 800.0), (1920.0, 1080.0)] {
        h.set_size(egui::vec2(w, ht));
        settle(h);
        let image = h.render().expect("renders");
        image
            .save(std::path::Path::new(&dir).join(format!("{name}-{w}x{ht}.png")))
            .unwrap();
    }
    h.set_size(egui::vec2(1280.0, 800.0));
    settle(h);
}

/// The label of the row after the focused one in the list on top.
fn after_focus(h: &H) -> String {
    let top = h.state().model.screens().last().unwrap();
    let Some(Component::List(list)) = top.root.as_ref().unwrap().find("list:choices") else {
        panic!("no list");
    };
    let rows = h.state().model.rows(list);
    let at = rows
        .iter()
        .position(|r| top.focus.as_ref().and_then(|f| f.row_id.as_ref()) == Some(&r.id))
        .unwrap();
    rows[at + 1].label.shown()
}

/// Invokes the top screen's affordance whose id ends with `action`.
fn act(h: &mut H, action: &str) {
    let id = h
        .state()
        .model
        .screens()
        .last()
        .and_then(|s| {
            s.affordances.iter().find(|a| {
                a.id.to_ascii_lowercase()
                    .ends_with(&action.to_ascii_lowercase())
            })
        })
        .map(|a| a.id.clone())
        .unwrap_or_else(|| {
            panic!(
                "no {action} in {:?}",
                h.state().model.screens().last().map(|s| s
                    .affordances
                    .iter()
                    .map(|a| a.id.clone())
                    .collect::<Vec<_>>())
            )
        });
    h.state_mut()
        .model
        .invoke_affordance(&id, InvokeArgs::default())
        .unwrap_or_else(|e| panic!("{id}: {e:?}"));
    run_until(h, action, 60, |h| !h.state().model.busy());
    settle(h);
}

#[test]
#[ignore = "requires BN_ENGINE pointing at an engine binary"]
fn the_map_editor_its_pickers_shape_and_mapgen() {
    let engine = std::env::var_os("BN_ENGINE").expect("set BN_ENGINE");
    let root =
        game_root(engine.as_ref(), std::env::var_os("BN_ROOT").map(Into::into)).expect("game root");
    let dir = std::env::temp_dir().join(format!("bn-editmap-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let settings = dir.join("settings.json");
    std::fs::write(&settings, r#"{"tiles":true,"tileset":"UltimateCataclysm"}"#).unwrap();
    let transport = common::live::english_transport();
    let app = App::new(EngineModel::new(Box::new(transport)))
        .with_settings(settings)
        .with_root(root.clone())
        .with_target_format(eframe::wgpu::TextureFormat::Rgba8Unorm);
    let mut h = Harness::builder()
        .with_size(egui::vec2(1280.0, 800.0))
        .with_step_dt(1.0 / 60.0)
        .wgpu()
        .build_ui_state(|ui, app: &mut App<EngineModel>| app.show(ui), app);
    bn_client_egui::fonts::install(&h.ctx, &root);
    run_until(&mut h, "the main menu", 120, |h| {
        h.state().model.menu().is_some()
    });
    // A profile without a language asks for one first, unless the system names one the game has.
    let asks_language = |h: &H| {
        h.state().model.screens().last().is_some_and(|s| {
            serde_json::to_string(s)
                .unwrap()
                .contains(r#""label":"English""#)
        })
    };
    if asks_language(&h) {
        choose(&mut h, "bn:choose:list:choices", "entry:0");
        run_until(&mut h, "the language", 60, |h| !asks_language(h));
    }
    choose(&mut h, "menu:tab", "new_game");
    choose(&mut h, "menu:entry", "new_game:play_now_default");
    run_until(&mut h, "the map", 900, |h| h.state().model.size().x > 0);
    run_until(&mut h, "the settled game", 300, |h| {
        h.state().model.menu().is_none() && !h.state().model.busy()
    });
    dismiss_popups(&mut h);
    settle(&mut h);

    assert!(h.state_mut().model.invoke_action("debug"));
    pick(&mut h, "Map");
    pick(&mut h, "Map editor");
    wait_kind(&mut h, "editmap");
    save(&mut h, "editmap");
    for _ in 0..3 {
        h.key_press(egui::Key::ArrowRight);
        settle(&mut h);
    }
    run_until(&mut h, "the cursor", 60, |h| !h.state().model.busy());
    save(&mut h, "editmap-moved");

    for (action, name) in [
        ("EDIT_TERRAIN", "terrain"),
        ("EDIT_FURNITURE", "furniture"),
        ("EDIT_TRAPS", "trap"),
        ("EDIT_FIELDS", "field"),
    ] {
        act(&mut h, action);
        save(&mut h, name);
        if name == "terrain" {
            act(&mut h, "EDITMAP_TAB");
            wait_kind(&mut h, "editmap_shape");
            save(&mut h, "shape-resize");
            for _ in 0..2 {
                h.key_press(egui::Key::ArrowDown);
                run_until(&mut h, "the selection", 60, |h| !h.state().model.busy());
                settle(&mut h);
            }
            save(&mut h, "shape-resized");
            act(&mut h, "QUIT");
            save(&mut h, "terrain-after-shape");
            let next = after_focus(&h);
            // A list row runs on a double click.
            let at = h.get_by_label(&next).rect().center();
            common::double_click(&mut h, at);
            run_until(&mut h, "the terrain", 60, |h| !h.state().model.busy());
            settle(&mut h);
            save(&mut h, "terrain-applied");
            h.key_press(egui::Key::Escape);
            wait_kind(&mut h, "editmap");
            let stack = serde_json::to_value(h.state().model.screens()).unwrap();
            let terrain = stack[1]["root"]["child"]["target"]["parts"][0]["id"].clone();
            assert!(
                next.contains(&format!("({})", terrain.as_str().unwrap())),
                "{next}: {terrain}"
            );
            save(&mut h, "editmap-applied");
            continue;
        }
        h.key_press(egui::Key::Escape);
        wait_kind(&mut h, "editmap");
    }

    act(&mut h, "EDIT_OVERMAP");
    save(&mut h, "mapgen-list");
    act(&mut h, "EDITMAP_MOVE");
    wait_kind(&mut h, "editmap_mapgen");
    save(&mut h, "mapgen-retarget");
    h.key_press(egui::Key::Escape);
    settle(&mut h);
    save(&mut h, "mapgen-back");
    // The row the list opens on, whatever the start location.
    let row = h
        .state()
        .model
        .screens()
        .last()
        .and_then(|s| s.focus.clone())
        .and_then(|f| f.row_id)
        .expect("a focused row");
    choose(&mut h, "bn:choose:list:choices", &row);
    run_until(&mut h, "the preview", 120, |h| {
        !h.state().model.busy() && top_kind(h).as_deref() == Some("uilist")
    });
    settle(&mut h);
    save(&mut h, "mapgen-preview");
}
