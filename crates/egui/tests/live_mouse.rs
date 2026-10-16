//! Mouse play against a real engine: the shortcut bar, the right-click menu of a door or locker, and the inventory
//! opened from the bar, each saved as a picture.
//!
//! `BN_ENGINE=<engine> SHOT_DIR=<dir> cargo test --release -p bn-client-egui --test live_mouse -- --ignored
//! --nocapture`; `BN_ROOT` names the game root when it is not beside the engine.
use bn_client_egui::app::CELL;
use bn_client_egui::interaction::InvokeError;
use bn_client_egui::protocol::{Component, InvokeArgs};
use bn_client_egui::transport::{StdioTransport, game_root};
use bn_client_egui::watchdog::Watchdog;
use bn_client_egui::{App, EngineModel, GameModel, Pos};
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

fn dismiss_popups(h: &mut H) {
    for _ in 0..20 {
        let Some(menu) = h.state().model.menu() else {
            return;
        };
        match menu.choices.first().map(|c| c.id.clone()) {
            _ if menu.context == "DIALOGUE_CHOOSE_RESPONSE" => leave_talk(h),
            Some(id) if !menu.can_cancel => h.state_mut().model.choose(&id),
            _ => h.state_mut().model.cancel_menu(),
        }
        run_until(h, "popup", 180, |h| !h.state().model.busy());
        h.run_steps(3);
    }
    panic!("a pop-up stays open: {:?}", h.state().model.menu());
}

/// Leaves a conversation, which has no cancel, by its last response, as a person who walks up to talk may start one.
fn leave_talk(h: &mut H) {
    let last = h.state().model.screens().last().and_then(|s| {
        match s.root.as_ref()?.find("talk:responses")? {
            Component::List(l) => l.rows.last().map(|r| r.id.clone()),
            _ => None,
        }
    });
    let args = InvokeArgs {
        row: last,
        ..Default::default()
    };
    let _ = h.state_mut().model.invoke_affordance("talk:respond", args);
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

fn right_click(h: &mut H, pos: egui::Pos2) {
    h.hover_at(pos);
    h.run_steps(2);
    for pressed in [true, false] {
        h.event(egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Secondary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        });
    }
    h.run_steps(2);
}

/// The view squares holding a door or a locker, nearest the avatar first.
fn doors_and_lockers(h: &H) -> Vec<Pos> {
    let m = &h.state().model;
    let (size, me) = (m.size(), m.player());
    let mut found = (0..size.y)
        .flat_map(|y| (0..size.x).map(move |x| Pos { x, y }))
        .filter(|&at| {
            m.cell(at)
                .layers
                .iter()
                .any(|l| l.id.starts_with("t_door") || l.id.starts_with("f_locker"))
        })
        .collect::<Vec<_>>();
    found.sort_by_key(|at| (at.x - me.x).abs().max((at.y - me.y).abs()));
    found
}

/// A new game of the main menu's `entry` drawn with tiles, its pop-ups closed.
fn start<'a>(entry: &str) -> H<'a> {
    let engine = std::env::var_os("BN_ENGINE").expect("set BN_ENGINE");
    let root =
        game_root(engine.as_ref(), std::env::var_os("BN_ROOT").map(Into::into)).expect("game root");
    let dir = std::env::temp_dir().join(format!("bn-mouse-{}", std::process::id()));
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
    choose(&mut h, "menu:entry", entry);
    run_until(&mut h, "the map", 900, |h| h.state().model.size().x > 0);
    run_until(&mut h, "the settled game", 300, |h| {
        h.state().model.menu().is_none() && !h.state().model.busy()
    });
    dismiss_popups(&mut h);
    run_until(&mut h, "quiet", 180, |h| !h.state().model.busy());
    settle(&mut h);
    h
}

#[test]
#[ignore = "requires BN_ENGINE pointing at an engine binary"]
fn the_toolbar_the_square_menu_and_the_inventory_by_mouse() {
    let mut h = start("new_game:play_now");

    // The bar is the engine's: one button per published shortcut, named as the engine names it.
    let shortcuts = h.state().model.shortcuts();
    assert!(!shortcuts.is_empty(), "the map publishes a shortcut bar");
    println!(
        "LIVE shortcuts {:?}",
        shortcuts
            .iter()
            .map(|s| (s.affordance_id.clone(), s.label.shown(), s.keys.clone()))
            .collect::<Vec<_>>()
    );
    save(&mut h, "01-toolbar.png");

    // A door or locker the avatar can reach: walk beside the nearest one, then right-click it.
    let target = *doors_and_lockers(&h)
        .first()
        .expect("a door or locker in view");
    let me = h.state().model.player();
    if (target.x - me.x).abs().max((target.y - me.y).abs()) > 1 {
        let beside = Pos {
            x: target.x + (me.x - target.x).signum(),
            y: target.y + (me.y - target.y).signum(),
        };
        assert!(h.state_mut().model.tile_action(beside, "travel"));
        run_until(&mut h, "the walk", 300, |h| !h.state().model.busy());
        dismiss_popups(&mut h);
        settle(&mut h);
    }
    let target = *doors_and_lockers(&h).first().expect("the door or locker");
    println!(
        "LIVE target {target:?}: {}",
        h.state().model.cell(target).desc
    );
    let pos = on_screen(&h, target);
    right_click(&mut h, pos);
    run_until(&mut h, "the square's actions", 60, |h| {
        h.query_all_by_label_contains("[").next().is_some() && h.query_by_label("…").is_none()
    });
    settle(&mut h);
    let actions = h.state_mut().model.tile_actions(target).unwrap_or_default();
    println!(
        "LIVE actions {:?}",
        actions
            .iter()
            .map(|a| (a.id.clone(), a.label.shown()))
            .collect::<Vec<_>>()
    );
    assert!(actions.len() >= 2, "at least two engine-labelled entries");
    save(&mut h, "02-rightclick.png");
    h.key_press(egui::Key::Escape);
    settle(&mut h);
    assert!(h.query_by_label("Map actions").is_none(), "the menu closed");
    assert_eq!(h.state().model.screens().len(), 1, "only the map is open");

    // The inventory, opened by clicking its button on the bar.
    let inventory = shortcuts
        .iter()
        .find(|s| s.affordance_id == "inventory")
        .expect("an inventory shortcut");
    let label = bn_client_egui::markup::plain(&inventory.label.shown());
    h.get_by_label(&label).click();
    run_until(&mut h, "the inventory", 120, |h| {
        h.state().model.screens().len() > 1 && !h.state().model.busy()
    });
    settle(&mut h);
    println!(
        "LIVE top screen {:?}",
        h.state()
            .model
            .screens()
            .last()
            .map(|s| s.screen_id.clone())
    );
    save(&mut h, "03-inventory.png");
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

/// What the engine offers on view square `at`, once it answered; the pointer rests there, as the client asks
/// about the square under it.
fn actions_on(h: &mut H, at: Pos) -> Vec<String> {
    let pos = on_screen(h, at);
    h.hover_at(pos);
    for _ in 0..2000 {
        if let Some(actions) = h.state_mut().model.tile_actions(at) {
            return actions.into_iter().map(|a| a.id).collect();
        }
        h.run_steps(1);
        std::thread::sleep(Duration::from_millis(30));
    }
    panic!("no description of {at:?}");
}

/// Right-clicks view square `at` and saves the menu it opens as `name`.
fn shoot_menu(h: &mut H, at: Pos, name: &str) {
    let pos = on_screen(h, at);
    right_click(h, pos);
    run_until(h, "the square's actions", 60, |h| {
        h.query_all_by_label_contains("[").next().is_some() && h.query_by_label("…").is_none()
    });
    settle(h);
    println!("LIVE {name} {at:?}: {:?}", actions_on(h, at));
    save(h, name);
    h.key_press(egui::Key::Escape);
    settle(h);
}

#[test]
#[ignore = "requires BN_ENGINE pointing at an engine binary"]
fn the_square_menu_on_a_person_and_on_vehicle_controls_beside_the_avatar() {
    let mut h = start("new_game:play_now_default");

    // A person from the debug menu, then the menu of the square it stands on.
    assert!(h.state_mut().model.invoke_action("debug"));
    pick(&mut h, "Spawning");
    pick(&mut h, "Spawn NPC");
    dismiss_popups(&mut h);
    settle(&mut h);
    let me = h.state().model.player();
    let person = (-6..=6)
        .flat_map(|y| {
            (-6..=6).map(move |x| Pos {
                x: me.x + x,
                y: me.y + y,
            })
        })
        .filter(|&at| at != me)
        .find(|&at| actions_on(&mut h, at).iter().any(|a| a == "talk"))
        .expect("a person to talk to");
    shoot_menu(&mut h, person, "npc-menu.png");

    // A bicycle under the avatar, then a step off it: its saddle holds the controls beside the avatar.
    assert!(h.state_mut().model.invoke_action("debug"));
    pick(&mut h, "Spawning");
    pick(&mut h, "Spawn a vehicle");
    pick(&mut h, "(bicycle)");
    pick(&mut h, "Undamaged");
    let field = h
        .state()
        .model
        .menu()
        .and_then(|m| m.field)
        .expect("the direction prompt");
    h.state_mut().model.fill(&field.id, "-90", true);
    run_until(&mut h, "the bicycle", 120, |h| {
        h.state().model.menu().is_none() && !h.state().model.busy()
    });
    dismiss_popups(&mut h);
    let part = |h: &H, at: Pos, name: &str| {
        h.state()
            .model
            .cell(at)
            .layers
            .iter()
            .any(|l| l.id.starts_with("vp_") && l.id.contains(name))
    };
    let around = |me: Pos| {
        (-1..=1).flat_map(move |y| {
            (-1..=1).map(move |x| Pos {
                x: me.x + x,
                y: me.y + y,
            })
        })
    };
    for step in ["RIGHT", "LEFT", "UP", "DOWN"] {
        if !part(&h, h.state().model.player(), "") {
            break;
        }
        assert!(h.state_mut().model.invoke_action(step));
        run_until(&mut h, step, 120, |h| !h.state().model.busy());
        dismiss_popups(&mut h);
        settle(&mut h);
    }
    let me = h.state().model.player();
    assert!(!part(&h, me, ""), "the avatar stepped off the bicycle");
    let saddle = around(me)
        .find(|&at| part(&h, at, "saddle"))
        .expect("the saddle beside the avatar");
    shoot_menu(&mut h, saddle, "controls-menu.png");
    let actions = actions_on(&mut h, saddle);
    assert!(
        actions.iter().any(|a| a == "control_vehicle"),
        "the controls beside the avatar are used from their square: {actions:?}"
    );
}
