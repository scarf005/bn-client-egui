//! Against a real engine: the map editor screen, offered with the Debug Editor trait, drawn from its generic
//! components: a house mapgen opened, a rectangle drawn on its canvas and previewed, a placement picked into its
//! forms, a catalog entry picked into the legend form, and the change discarded on leaving.
//!
//! `BN_ENGINE=<engine> SHOT_DIR=<dir> cargo test --release -p bn-client-egui --test live_editor -- --ignored
//! --nocapture`; `BN_ROOT` names the game root when it is not beside the engine.
mod common;
use bn_client_egui::interaction::InvokeError;
use bn_client_egui::protocol::{Component, InvokeArgs, ScreenRole};
use bn_client_egui::transport::game_root;
use bn_client_egui::watchdog::Watchdog;
use bn_client_egui::{App, EngineModel, GameModel, MenuModel};
use common::live::{dismiss_popups, settle};
use eframe::egui;
use egui_kittest::Harness;
use std::time::Duration;

type H<'a> = Harness<'a, App<EngineModel>>;

fn run_until(h: &mut H, what: &str, secs: u64, mut done: impl FnMut(&H) -> bool) {
    let mut watchdog = Watchdog::new(Duration::from_secs(secs), Duration::from_secs(900));
    while !done(h) {
        assert!(
            !watchdog.expired(h.state().model.received()),
            "timed out: {what}; {:?}",
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

/// Invokes an affordance of the screen on top once it offers it, and waits for the engine.
fn invoke(h: &mut H, affordance: &str, args: InvokeArgs) {
    let offered = |h: &H| {
        h.state()
            .model
            .screens()
            .last()
            .is_some_and(|s| s.affordances.iter().any(|a| a.id == affordance))
    };
    loop {
        run_until(h, affordance, 180, |h| {
            offered(h) && !h.state().model.busy()
        });
        match h
            .state_mut()
            .model
            .invoke_affordance(affordance, args.clone())
        {
            Err(InvokeError::Busy) => h.run_steps(1),
            sent => {
                sent.unwrap_or_else(|e| panic!("{affordance}: {e:?}"));
                break;
            }
        }
    }
    run_until(h, affordance, 120, |h| !h.state().model.busy());
    settle(h);
}

fn row(id: &str) -> InvokeArgs {
    InvokeArgs {
        row: Some(id.into()),
        ..Default::default()
    }
}

fn text(value: &str) -> InvokeArgs {
    InvokeArgs {
        text: Some(value.into()),
        ..Default::default()
    }
}

/// Pictures of the window at both sizes.
fn save(h: &mut H, name: &str) {
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

/// The glyphs of a grid of the editor, row by row.
fn glyphs(h: &H, id: &str) -> Vec<String> {
    let top = h.state().model.screens().last().unwrap();
    let Some(Component::Grid(grid)) = top.root.as_ref().unwrap().find(id) else {
        panic!("no {id}");
    };
    let mut rows = vec![vec![' '; grid.cols as usize]; grid.rows as usize];
    for cell in &grid.cells {
        rows[cell.y as usize][cell.x as usize] = cell
            .glyph
            .as_deref()
            .and_then(|g| g.chars().next())
            .unwrap_or(' ');
    }
    rows.into_iter().map(|r| r.into_iter().collect()).collect()
}

/// The row ids of a list of the editor.
fn rows(h: &H, id: &str) -> Vec<String> {
    let top = h.state().model.screens().last().unwrap();
    let Some(Component::List(list)) = top.root.as_ref().unwrap().find(id) else {
        panic!("no {id}");
    };
    list.rows.iter().map(|r| r.id.clone()).collect()
}

/// The value of a field of a form of the editor.
fn field(h: &H, form: &str, id: &str) -> String {
    let top = h.state().model.screens().last().unwrap();
    let Some(Component::Form(form)) = top.root.as_ref().unwrap().find(form) else {
        panic!("no {form}");
    };
    form.fields
        .iter()
        .find(|f| f.id == id)
        .map(|f| f.value.clone())
        .unwrap_or_else(|| panic!("no field {id}"))
}

#[test]
#[ignore = "requires BN_ENGINE pointing at an engine binary"]
fn the_map_editor_opens_draws_and_discards() {
    let engine = std::env::var_os("BN_ENGINE").expect("set BN_ENGINE");
    let root =
        game_root(engine.as_ref(), std::env::var_os("BN_ROOT").map(Into::into)).expect("game root");
    let dir = std::env::temp_dir().join(format!("bn-editor-{}", std::process::id()));
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
    let asks_language = |h: &H| {
        h.state().model.screens().last().is_some_and(|s| {
            serde_json::to_string(s)
                .unwrap()
                .contains(r#""label":"English""#)
        })
    };
    if asks_language(&h) {
        invoke(&mut h, "bn:choose:list:choices", row("entry:0"));
        run_until(&mut h, "the language", 60, |h| !asks_language(h));
    }
    invoke(&mut h, "menu:tab", row("new_game"));
    invoke(&mut h, "menu:entry", row("new_game:play_now_default"));
    run_until(&mut h, "the map", 900, |h| h.state().model.size().x > 0);
    run_until(&mut h, "the settled game", 300, |h| {
        h.state().model.menu().is_none() && !h.state().model.busy()
    });
    dismiss_popups(&mut h);
    settle(&mut h);

    // The trait, through the Lua console's code editor.
    assert!(h.state_mut().model.invoke_action("lua_console"));
    wait_kind(&mut h, "lua_console");
    invoke(&mut h, "EDIT", InvokeArgs::default());
    wait_kind(&mut h, "text_editor");
    invoke(
        &mut h,
        "editor:set",
        text(r#"gapi.get_avatar():set_mutation(MutationBranchId.new("DEBUG_EDITOR"))"#),
    );
    invoke(&mut h, "TEXT.CONFIRM", InvokeArgs::default());
    wait_kind(&mut h, "lua_console");
    invoke(&mut h, "QUIT", InvokeArgs::default());
    run_until(&mut h, "the map", 60, |h| {
        h.state()
            .model
            .screens()
            .last()
            .is_some_and(|s| s.role == ScreenRole::Map)
    });

    assert!(h.state_mut().model.invoke_action("editor"));
    wait_kind(&mut h, "editor");
    save(&mut h, "editor-empty");
    let house = root.join("data/json/mapgen/house/2Story01.json");
    invoke(&mut h, "editor:open", text(&house.to_string_lossy()));
    let opened = glyphs(&h, "editor:canvas");
    assert_eq!((opened.len(), opened[0].chars().count()), (24, 24));
    save(&mut h, "editor-open");

    invoke(&mut h, "editor:symbol", row("symbol:#"));
    invoke(&mut h, "editor:tool", row("tool:rect"));
    invoke(&mut h, "editor:paint", row("square:2,2"));
    invoke(&mut h, "editor:paint", row("square:9,7"));
    let drawn = glyphs(&h, "editor:canvas");
    assert_eq!(
        drawn[2].chars().skip(2).take(8).collect::<String>(),
        "########"
    );
    assert_eq!(drawn[4].chars().nth(9), Some('#'));
    assert_ne!(drawn, opened);
    save(&mut h, "editor-drawn");

    // The game's own generation of the edited entry, with the rectangle in it.
    invoke(
        &mut h,
        "editor:preview",
        InvokeArgs {
            count: Some(7),
            ..Default::default()
        },
    );
    assert_eq!(glyphs(&h, "editor:preview").len(), 24);
    save(&mut h, "editor-preview");

    // A picked placement fills the move and field forms.
    let placed = rows(&h, "editor:placements");
    invoke(&mut h, "editor:placement", row(&placed[0]));
    assert!(field(&h, "editor:field", "pointer").starts_with("/object/place_"));
    save(&mut h, "editor-placement");

    // A catalog entry fills the legend form with the mapping that places it.
    invoke(&mut h, "editor:catalog_kind", row("catalog:furniture"));
    invoke(&mut h, "editor:catalog_filter", text("chair"));
    let found = rows(&h, "editor:catalog");
    invoke(&mut h, "editor:catalog_pick", row(&found[0]));
    assert_eq!(field(&h, "editor:mapping", "layer"), "furniture");
    assert_eq!(
        Some(field(&h, "editor:mapping", "value")),
        found[0].strip_prefix("entry:").map(Into::into)
    );
    save(&mut h, "editor-catalog");

    // Leaving with the change unsaved asks to save, discard or stay.
    invoke(&mut h, "QUIT", InvokeArgs::default());
    run_until(&mut h, "the leave menu", 60, |h| {
        h.state().model.menu().is_some_and(|m| m.choices.len() == 3)
    });
    save(&mut h, "editor-leave");
    let discard = h
        .state()
        .model
        .menu()
        .and_then(|m| {
            m.choices
                .iter()
                .find(|c| c.label == "Leave without saving")
                .map(|c| c.id.clone())
        })
        .expect("a discard choice");
    h.state_mut().model.choose(&discard);
    run_until(&mut h, "the map", 60, |h| {
        !h.state().model.busy() && top_kind(h).as_deref() != Some("editor")
    });
    let _ = std::fs::remove_dir_all(&dir);
}
