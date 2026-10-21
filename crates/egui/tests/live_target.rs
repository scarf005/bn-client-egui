//! Against a real engine: the engine's target cursor of throwing (`t`), firing (`f`) and looking (`x`) follows the
//! pointer square by square with no frame back on the square it left, ends on the square the pointer rests on
//! however fast it moved, and a click there acts on it; each mode saved as pictures.
//!
//! `BN_ENGINE=<engine> SHOT_DIR=<dir> cargo test --release -p bn-client-egui --test live_target -- --ignored
//! --nocapture`; `BN_ROOT` names the game root when it is not beside the engine.
use bn_client_egui::app::CELL;
use bn_client_egui::interaction::InvokeError;
use bn_client_egui::protocol::{Component, InvokeArgs};
use bn_client_egui::transport::{StdioTransport, game_root};
use bn_client_egui::watchdog::Watchdog;
use bn_client_egui::{App, EngineModel, GameModel, MenuModel, Pos};
use eframe::egui;
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
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

/// The middle of view square `at` on the screen.
fn on_screen(h: &H, at: Pos) -> egui::Pos2 {
    let display = h.state().display();
    let cell = display.cell(CELL) * display.zoom();
    h.get_by_label("Map").rect().min + egui::vec2(at.x as f32 + 0.5, at.y as f32 + 0.5) * cell
}

/// A new game of the default character drawn with tiles, its pop-ups closed.
fn start<'a>() -> H<'a> {
    let engine = std::env::var_os("BN_ENGINE").expect("set BN_ENGINE");
    let root =
        game_root(engine.as_ref(), std::env::var_os("BN_ROOT").map(Into::into)).expect("game root");
    let dir = std::env::temp_dir().join(format!("bn-target-{}", std::process::id()));
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
    h
}

/// The rows of the carried items' lists on the top screen whose label contains `label`, each with the affordance
/// that chooses it.
fn rows_labelled(h: &H, label: &str) -> Vec<(String, String)> {
    fn find(c: &Component, label: &str, found: &mut Vec<(String, String)>) {
        if let Component::List(l) = c
            && let Some(activate) = l.activate.as_ref().filter(|a| a.contains("inventory"))
        {
            let rows = l.rows.iter().filter(|r| {
                r.selectable && r.header != Some(true) && r.label.shown().contains(label)
            });
            found.extend(rows.map(|r| (activate.clone(), r.id.clone())));
        }
        c.children().into_iter().for_each(|c| find(c, label, found));
    }
    let mut found = Vec::new();
    let root = h
        .state()
        .model
        .screens()
        .last()
        .and_then(|s| s.root.as_ref());
    root.into_iter().for_each(|r| find(r, label, &mut found));
    found
}

/// The range the engine's target cursor may go to.
fn target_range(h: &H) -> u64 {
    let screen = h.state().model.screens().last().expect("a screen");
    let map = screen
        .root
        .as_ref()
        .and_then(|r| r.cursor_map())
        .expect("a target map");
    map.target
        .as_ref()
        .and_then(|t| t["range"].as_u64())
        .expect("a range")
}

/// Types the game key `key` as the keyboard does.
fn type_key(h: &mut H, key: &str) {
    h.event(egui::Event::Text(key.into()));
    h.run_steps(1);
}

/// The square the map says its cursor is on, as it says it.
fn said_cursor(h: &H) -> String {
    let description = h
        .get_by_label("Map")
        .accesskit_node()
        .description()
        .unwrap_or_default();
    let at = description
        .find("Cursor (")
        .expect("the map names its cursor");
    description[at..].split(')').next().unwrap().to_string() + ")"
}

/// How the map names view square `at` as its cursor.
fn cursor_at(h: &H, at: Pos) -> String {
    let origin = h.state().model.origin();
    format!("Cursor ({}, {})", at.x + origin.x, at.y + origin.y)
}

/// With the engine's target cursor open: the cursor follows the pointer onto a square with no frame back on the
/// one it left, ends where a fast pointer comes to rest, and a click there acts on it.
fn the_target_follows_the_pointer(h: &mut H, mode: &str) {
    run_until(h, "the target cursor", 60, |h| {
        let m = &h.state().model;
        assert!(
            m.focus().is_some() || m.busy() || m.command().is_none_or(|c| !c.finished),
            "{mode}: no target cursor; {:?}",
            m.messages().iter().rev().take(3).collect::<Vec<_>>()
        );
        m.focus().is_some() && !m.busy()
    });
    settle(h);
    save(h, &format!("{mode}-0-open.png"));
    let me = h.state().model.player();
    let from = h.state().model.focus().unwrap();

    // One square: every frame until the engine has moved its cursor there shows it there.
    let one = Pos {
        x: me.x + 2,
        y: me.y + 1,
    };
    assert_ne!(one, from);
    h.hover_at(on_screen(h, one));
    let mut said = Vec::new();
    for _ in 0..200 {
        h.step();
        said.push(said_cursor(h));
        if h.state().model.focus() == Some(one) && !h.state().model.busy() {
            break;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(
        h.state().model.focus(),
        Some(one),
        "{mode}: the engine took the square"
    );
    let off = said.iter().filter(|s| **s != cursor_at(h, one)).count();
    assert_eq!(off, 0, "{mode}: frames off the pointer's square: {said:?}");
    save(h, &format!("{mode}-1-one-square.png"));

    // A fast sweep, one frame per few points, then rest: the engine's cursor ends under the pointer.
    let rest = Pos {
        x: me.x + 4,
        y: me.y - 1,
    };
    let (a, b) = (
        on_screen(
            h,
            Pos {
                x: me.x - 3,
                y: me.y + 2,
            },
        ),
        on_screen(h, rest),
    );
    let frames = 60;
    for f in 0..=frames {
        h.hover_at(a + (b - a) * (f as f32 / frames as f32));
        h.step();
    }
    for _ in 0..100 {
        h.step();
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(
        h.state().model.focus(),
        Some(rest),
        "{mode}: the cursor ends where the pointer rests"
    );
    assert_eq!(said_cursor(h), cursor_at(h, rest));
    save(h, &format!("{mode}-2-swept.png"));

    // A click on the cursor's square acts on it: the target screen closes.
    let p = on_screen(h, rest);
    for pressed in [true, false] {
        h.event(egui::Event::PointerButton {
            pos: p,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        });
    }
    h.run_steps(2);
    run_until(h, "the click's action", 60, |h| {
        h.state().model.focus().is_none() && !h.state().model.busy()
    });
    settle(h);
    save(h, &format!("{mode}-3-clicked.png"));
}

/// One `name` from the debug menu's item spawner, into the inventory.
fn spawn(h: &mut H, name: &str) {
    assert!(h.state_mut().model.invoke_action("debug"));
    pick(h, "Spawning");
    pick(h, "Spawn an item");
    pick(h, name);
    if let Some(field) = h.state().model.menu().and_then(|m| m.field) {
        h.state_mut().model.fill(&field.id, "1", true);
        run_until(h, name, 60, |h| !h.state().model.busy());
    }
    dismiss_popups(h);
}

#[test]
#[ignore = "requires BN_ENGINE pointing at an engine binary"]
fn live_throwing_follows_the_pointer_and_a_click_throws() {
    let mut h = start();
    // A random start may carry nothing to throw; a pistol (on the spawn list's first page) flies far enough.
    spawn(&mut h, "12 gauge pistol");
    let carried = h.state().model.inventory().len();
    // The first carried item that flies far enough for the squares the pointer goes to; a heavy one is put back.
    for at in 0.. {
        type_key(&mut h, "t");
        run_until(&mut h, "the item list", 60, |h| {
            !rows_labelled(h, "").is_empty()
        });
        let (activate, row) = rows_labelled(&h, "")
            .into_iter()
            .nth(at)
            .expect("an item to throw far");
        choose(&mut h, &activate, &row);
        run_until(&mut h, "the target cursor", 60, |h| {
            h.state().model.focus().is_some() && !h.state().model.busy()
        });
        if target_range(&h) >= 6 {
            break;
        }
        h.key_press(egui::Key::Escape);
        run_until(&mut h, "the throw put back", 60, |h| {
            h.state().model.focus().is_none() && !h.state().model.busy()
        });
    }
    the_target_follows_the_pointer(&mut h, "throw");
    assert!(
        h.state().model.inventory().len() < carried,
        "the item left the inventory"
    );
}

#[test]
#[ignore = "requires BN_ENGINE pointing at an engine binary"]
fn live_firing_follows_the_pointer_and_a_click_fires() {
    let mut h = start();
    wield_pistol(&mut h);
    type_key(&mut h, "f");
    the_target_follows_the_pointer(&mut h, "fire");
}

/// A loaded 12 gauge pistol, whose shell is a multishot of pellets, wielded.
fn wield_pistol(h: &mut H) {
    spawn(h, "12 gauge pistol");
    type_key(h, "w");
    run_until(h, "the wield list", 60, |h| {
        !rows_labelled(h, "pistol").is_empty()
    });
    let (activate, row) = rows_labelled(h, "pistol").remove(0);
    choose(h, &activate, &row);
    run_until(h, "wielded", 60, |h| !h.state().model.busy());
    dismiss_popups(h);
    settle(h);
}

#[test]
#[ignore = "requires BN_ENGINE pointing at an engine binary"]
fn live_a_shotgun_blast_is_saved_frame_by_frame() {
    let mut h = start();
    wield_pistol(&mut h);
    type_key(&mut h, "f");
    run_until(&mut h, "the target cursor", 60, |h| {
        h.state().model.focus().is_some() && !h.state().model.busy()
    });
    let me = h.state().model.player();
    let at = on_screen(
        &h,
        Pos {
            x: me.x + 7,
            y: me.y - 2,
        },
    );
    // The cursor goes to the square, and a click there fires.
    h.hover_at(at);
    run_until(&mut h, "the aim", 60, |h| !h.state().model.busy());
    settle(&mut h);
    for pressed in [true, false] {
        h.event(egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        });
    }
    run_until(&mut h, "the shot", 60, |h| h.state().playing_effects() > 0);
    for frame in 0..12 {
        save(&mut h, &format!("blast-{frame:02}.png"));
        h.run_steps(2);
    }
}

#[test]
#[ignore = "requires BN_ENGINE pointing at an engine binary"]
fn live_looking_follows_the_pointer_and_a_click_looks() {
    let mut h = start();
    type_key(&mut h, "x");
    the_target_follows_the_pointer(&mut h, "look");
}
