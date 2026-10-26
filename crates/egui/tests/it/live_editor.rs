//! Against a real engine: the map editor screen, offered with the Debug Editor trait, drawn from its generic
//! components: a house mapgen opened, a rectangle drawn on its canvas and previewed, a placement picked into its
//! forms, a catalog entry picked into the legend form, and the change discarded on leaving.
//!
//! `BN_ENGINE=<engine> SHOT_DIR=<dir> cargo test --release -p bn-client-egui --test it live_editor:: -- --ignored
//! --nocapture`; `BN_ROOT` names the game root when it is not beside the engine.
use crate::common::live::{dismiss_popups, settle};
use bn_client_egui::interaction::InvokeError;
use bn_client_egui::protocol::{Component, InvokeArgs, ScreenRole};
use bn_client_egui::transport::game_root;
use bn_client_egui::watchdog::Watchdog;
use bn_client_egui::{App, EngineModel, GameModel, MenuModel};
use eframe::egui;
use egui_kittest::Harness;
use std::time::Duration;

pub(crate) type H<'a> = Harness<'a, App<EngineModel>>;

pub(crate) fn run_until(h: &mut H, what: &str, secs: u64, mut done: impl FnMut(&H) -> bool) {
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

pub(crate) fn wait_kind(h: &mut H, kind: &str) {
    run_until(h, kind, 120, |h| {
        !h.state().model.busy() && top_kind(h).as_deref() == Some(kind)
    });
    settle(h);
}

/// Invokes an affordance of the screen on top once it offers it, and waits for the engine.
pub(crate) fn invoke(h: &mut H, affordance: &str, args: InvokeArgs) {
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

pub(crate) fn row(id: &str) -> InvokeArgs {
    InvokeArgs {
        row: Some(id.into()),
        ..Default::default()
    }
}

pub(crate) fn text(value: &str) -> InvokeArgs {
    InvokeArgs {
        text: Some(value.into()),
        ..Default::default()
    }
}

/// Pictures of the window at both sizes.
pub(crate) fn save(h: &mut H, name: &str) {
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
pub(crate) fn glyphs(h: &H, id: &str) -> Vec<String> {
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
pub(crate) fn rows(h: &H, id: &str) -> Vec<String> {
    let top = h.state().model.screens().last().unwrap();
    let Some(Component::List(list)) = top.root.as_ref().unwrap().find(id) else {
        panic!("no {id}");
    };
    list.rows.iter().map(|r| r.id.clone()).collect()
}

/// The value of a field of a form of the editor.
pub(crate) fn field(h: &H, form: &str, id: &str) -> String {
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

/// The client at the engine's main menu in English, with the game root and the scratch directory it uses.
fn at_main_menu() -> (H<'static>, std::path::PathBuf, std::path::PathBuf) {
    let dir = std::env::temp_dir().join(format!("bn-editor-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let (h, root) = start(&dir);
    (h, root, dir)
}

/// A harness on a real engine's main menu in English, with the tiles drawn, and the game root.
pub(crate) fn start(dir: &std::path::Path) -> (H<'static>, std::path::PathBuf) {
    let engine = std::env::var_os("BN_ENGINE").expect("set BN_ENGINE");
    let root =
        game_root(engine.as_ref(), std::env::var_os("BN_ROOT").map(Into::into)).expect("game root");
    let settings = dir.join("settings.json");
    std::fs::write(&settings, r#"{"tiles":true,"tileset":"UltimateCataclysm"}"#).unwrap();
    let transport = crate::common::live::english_transport();
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
    // The theme the game's window applies, solid scroll bars among it.
    bn_client_egui::kit::tokens::apply(&h.ctx);
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
    (h, root)
}

#[test]
#[ignore = "requires BN_ENGINE pointing at an engine binary"]
fn the_map_editor_opens_draws_and_discards() {
    let (mut h, root, dir) = at_main_menu();
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

    // Each shape tool is one step, and the history goes back to any of them.
    invoke(&mut h, "editor:tool", row("tool:line"));
    invoke(&mut h, "editor:paint", row("square:1,12"));
    invoke(&mut h, "editor:paint", row("square:8,12"));
    let lined = glyphs(&h, "editor:canvas");
    assert_eq!(
        lined[12].chars().skip(1).take(8).collect::<String>(),
        "########"
    );
    let inside = rows(&h, "editor:legend")[0].replace("symbol:", "");
    invoke(&mut h, "editor:inside", row(&format!("symbol:{inside}")));
    invoke(&mut h, "editor:tool", row("tool:room"));
    invoke(&mut h, "editor:paint", row("square:12,14"));
    invoke(&mut h, "editor:paint", row("square:20,20"));
    let roomed = glyphs(&h, "editor:canvas");
    assert_eq!(roomed[14].chars().nth(12), Some('#'));
    assert_eq!(roomed[16].chars().nth(14).map(String::from), Some(inside));
    save(&mut h, "editor-room");
    let steps = rows(&h, "editor:history");
    assert_eq!(steps, ["step:0", "step:1", "step:2", "step:3"]);
    invoke(&mut h, "editor:jump", row("step:1"));
    assert_eq!(glyphs(&h, "editor:canvas"), drawn);
    save(&mut h, "editor-history");
    invoke(&mut h, "editor:jump", row("step:3"));
    assert_eq!(glyphs(&h, "editor:canvas"), roomed);
    let top = h.state().model.screens().last().unwrap();
    let keys = |id: &str| {
        top.affordances
            .iter()
            .find(|a| a.id == id)
            .map(|a| a.keys.clone())
    };
    assert_eq!(keys("editor:undo"), Some(vec!["CTRL+Z".to_string()]));

    // A region selected, copied, turned a quarter and pasted is one more step of the history.
    invoke(&mut h, "editor:tool", row("tool:select"));
    invoke(&mut h, "editor:paint", row("square:2,2"));
    invoke(&mut h, "editor:paint", row("square:9,4"));
    invoke(&mut h, "editor:copy", InvokeArgs::default());
    invoke(&mut h, "editor:rotate", InvokeArgs::default());
    invoke(&mut h, "editor:tool", row("tool:paste"));
    invoke(&mut h, "editor:paint", row("square:14,2"));
    assert_ne!(glyphs(&h, "editor:canvas"), roomed);
    assert_eq!(rows(&h, "editor:history").len(), 5);
    save(&mut h, "editor-pasted");
    invoke(&mut h, "editor:jump", row("step:3"));
    assert_eq!(glyphs(&h, "editor:canvas"), roomed);

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

    // The entry applied to the game, stamped over the avatar's square, and tested there.
    invoke(&mut h, "editor:apply", InvokeArgs::default());
    invoke(&mut h, "editor:stamp", InvokeArgs::default());
    save(&mut h, "editor-stamped");
    invoke(&mut h, "editor:test", InvokeArgs::default());
    run_until(&mut h, "the map", 60, |h| {
        !h.state().model.busy() && top_kind(h).as_deref() != Some("editor")
    });
    save(&mut h, "map-tested");
    // The editor comes back to the document it was left with.
    assert!(h.state_mut().model.invoke_action("editor"));
    wait_kind(&mut h, "editor");
    assert_eq!(glyphs(&h, "editor:canvas"), roomed);

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

/// Presses a key with the modifiers and waits for the engine to take what it runs.
fn press(h: &mut H, mods: egui::Modifiers, key: egui::Key) {
    h.key_press_modifiers(mods, key);
    h.run_steps(3);
    run_until(h, "the key", 120, |h| !h.state().model.busy());
    settle(h);
}

#[test]
#[ignore = "requires BN_ENGINE pointing at an engine binary"]
fn the_main_menu_map_editor_draws_with_keys_browses_and_stamps() {
    let (mut h, root, dir) = at_main_menu();
    invoke(&mut h, "menu:tab", row("editor"));
    invoke(&mut h, "menu:entry", row("editor:map"));
    wait_kind(&mut h, "editor");
    save(&mut h, "menu-editor-empty");

    // The browser lists the data files; its filter finds one and opens it.
    assert_eq!(rows(&h, "editor:browser").len(), 50);
    invoke(&mut h, "editor:browser_filter", text("house/2story01.json"));
    let found = rows(&h, "editor:browser");
    assert_eq!(found.len(), 1);
    save(&mut h, "menu-editor-browser");
    invoke(&mut h, "editor:browser_open", row(&found[0]));
    let opened = glyphs(&h, "editor:canvas");
    assert_eq!((opened.len(), opened[0].chars().count()), (24, 24));

    // Ctrl+Z and Ctrl+Y undo and redo one stroke.
    invoke(&mut h, "editor:symbol", row("symbol:#"));
    invoke(&mut h, "editor:tool", row("tool:rect_filled"));
    invoke(&mut h, "editor:paint", row("square:3,3"));
    invoke(&mut h, "editor:paint", row("square:9,9"));
    let drawn = glyphs(&h, "editor:canvas");
    assert_ne!(drawn, opened);
    press(&mut h, egui::Modifiers::CTRL, egui::Key::Z);
    assert_eq!(glyphs(&h, "editor:canvas"), opened);
    press(&mut h, egui::Modifiers::CTRL, egui::Key::Y);
    assert_eq!(glyphs(&h, "editor:canvas"), drawn);

    // A brush of two with the mirror on draws the square and its twin across the middle column.
    invoke(&mut h, "editor:tool", row("tool:point"));
    invoke(
        &mut h,
        "editor:brush",
        InvokeArgs {
            count: Some(2),
            ..Default::default()
        },
    );
    invoke(&mut h, "editor:mirror_x", InvokeArgs::default());
    invoke(&mut h, "editor:paint", row("square:2,15"));
    let mirrored = glyphs(&h, "editor:canvas");
    assert_eq!(mirrored[15].chars().nth(2), Some('#'));
    assert_eq!(mirrored[16].chars().nth(21), Some('#'));
    save(&mut h, "menu-editor-brush");

    // Applied to the game, stamped over the avatar's square, tested there, and the editor comes back.
    invoke(&mut h, "editor:apply", InvokeArgs::default());
    invoke(&mut h, "editor:stamp", InvokeArgs::default());
    save(&mut h, "menu-editor-stamped");
    invoke(&mut h, "editor:test", InvokeArgs::default());
    run_until(&mut h, "the map", 60, |h| {
        !h.state().model.busy() && top_kind(h).as_deref() != Some("editor")
    });
    save(&mut h, "menu-map-tested");
    assert!(h.state_mut().model.invoke_action("editor"));
    wait_kind(&mut h, "editor");
    assert_eq!(glyphs(&h, "editor:canvas"), mirrored);
    let _ = std::fs::remove_dir_all(&dir);
    let _ = root;
}
