//! Against a real engine: aiming an M16A4 at a zombie in the flat, the 3D and the first-person map, each step saved
//! as pictures.
//!
//! `BN_ENGINE=<engine> SHOT_DIR=<dir> cargo test --release -p bn-client-egui --test it live_aim:: -- --ignored
//! --nocapture`; `BN_ROOT` names the game root when it is not beside the engine.
use crate::common::live::{choose, dismiss_popups, pick, run_until, save, settle};
use bn_client_egui::protocol::Component;
use bn_client_egui::transport::game_root;
use bn_client_egui::{App, EngineModel, GameModel, MenuModel};
use eframe::egui;
use egui_kittest::Harness;

type H<'a> = Harness<'a, App<EngineModel>>;

/// A new game of the default character drawn with `settings`, its pop-ups closed, in a window of `size`.
fn start<'a>(settings_json: &str, size: egui::Vec2) -> H<'a> {
    let engine = std::env::var_os("BN_ENGINE").expect("set BN_ENGINE");
    let root =
        game_root(engine.as_ref(), std::env::var_os("BN_ROOT").map(Into::into)).expect("game root");
    let dir = std::env::temp_dir().join(format!("bn-aim-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let settings = dir.join("settings.json");
    std::fs::write(&settings, settings_json).unwrap();
    let transport = crate::common::live::english_transport();
    let app = App::new(EngineModel::new(Box::new(transport)))
        .with_settings(settings)
        .with_root(root.clone())
        .with_target_format(eframe::wgpu::TextureFormat::Rgba8Unorm);
    let mut h = Harness::builder()
        .with_size(size)
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
    h
}

/// A game key token straight to the model, so a first-person view cannot turn the key into a step.
fn type_key(h: &mut H, key: &str) {
    assert!(h.state_mut().model.press(key), "{key} ran nothing");
    h.run_steps(1);
}

fn press(h: &mut H, key: &str) {
    type_key(h, key);
    h.run_steps(1);
    run_until(h, "the key's answer", 60, |h| !h.state().model.busy());
}

/// The rows of the item lists on the top screen whose label contains `label`, each with the affordance that chooses it.
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

#[allow(dead_code)]
fn dump(h: &H) {
    let screen = h.state().model.screens().last().cloned();
    println!(
        "SCREEN {}",
        serde_json::to_string(&screen).unwrap_or_default()
    );
    println!(
        "MENU {:?}",
        h.state()
            .model
            .menu()
            .map(|m| m.choices.into_iter().map(|c| c.label).collect::<Vec<_>>())
    );
}

/// One `name` from the debug menu's item spawner, into the inventory.
fn spawn(h: &mut H, name: &str) {
    assert!(h.state_mut().model.invoke_action("debug"));
    pick(h, "Spawning");
    pick(h, "Spawn an item");
    filter(h, name);
    pick(h, name);
    if h.state().model.menu().and_then(|m| m.field).is_some() {
        fill(h, "1");
    }
    dismiss_popups(h);
}

/// Types `text` into the field the engine asks for.
fn fill(h: &mut H, text: &str) {
    run_until(h, "the prompt", 60, |h| {
        h.state().model.menu().and_then(|m| m.field).is_some()
    });
    let field = h.state().model.menu().and_then(|m| m.field).unwrap();
    h.state_mut().model.fill(&field.id, text, true);
    run_until(h, text, 60, |h| !h.state().model.busy());
    h.run_steps(3);
}

/// The list's filter, for an entry past its first page.
fn filter(h: &mut H, text: &str) {
    assert!(h.state_mut().model.press("/"));
    fill(h, text);
}

/// A zombie three squares east of the avatar, from the debug menu.
fn spawn_zombie(h: &mut H) {
    assert!(h.state_mut().model.invoke_action("debug"));
    pick(h, "Spawning");
    pick(h, "Spawn monster");
    filter(h, "zombie");
    pick(h, "zombie");
}

fn aim_at_zombie(name: &str, settings: &str, size: egui::Vec2) {
    let mut h = start(settings, size);
    spawn(&mut h, "M16A4");
    type_key(&mut h, "w");
    run_until(&mut h, "the wield list", 60, |h| {
        !rows_labelled(h, "M16").is_empty()
    });
    let (activate, row) = rows_labelled(&h, "M16").remove(0);
    choose(&mut h, &activate, &row);
    run_until(&mut h, "wielded", 60, |h| !h.state().model.busy());
    dismiss_popups(&mut h);
    settle(&mut h);
    save(&mut h, &format!("{name}-00-wielded.png"));
    spawn_zombie(&mut h);
    for _ in 0..3 {
        press(&mut h, "RIGHT");
    }
    press(&mut h, "RETURN");
    dismiss_popups(&mut h);
    settle(&mut h);
    save(&mut h, &format!("{name}-01-spawned.png"));
    type_key(&mut h, "f");
    run_until(&mut h, "the aim", 60, |h| {
        h.state().model.focus().is_some() && !h.state().model.busy()
    });
    settle(&mut h);
    save(&mut h, &format!("{name}-02-aim-open.png"));
    // The next target, and back.
    for (n, label) in ["03-next-target", "04-target-again"]
        .into_iter()
        .enumerate()
    {
        let _ = n;
        press(&mut h, "TAB");
        settle(&mut h);
        save(&mut h, &format!("{name}-{label}.png"));
    }
    // One turn of aiming.
    type_key(&mut h, ".");
    run_until(&mut h, "the aim turn", 60, |h| !h.state().model.busy());
    settle(&mut h);
    save(&mut h, &format!("{name}-05-aimed.png"));
    // Fire: the flight and the hit frame by frame.
    type_key(&mut h, "f");
    run_until(&mut h, "the shot", 60, |h| h.state().playing_effects() > 0);
    for frame in 0..10 {
        save(&mut h, &format!("{name}-06-shot-{frame:02}.png"));
        h.run_steps(3);
    }
    run_until(&mut h, "the shot played", 120, |h| {
        h.state().playing_effects() == 0
    });
    settle(&mut h);
    save(&mut h, &format!("{name}-07-after.png"));
}

const FLAT: &str = r#"{"tiles":true,"tileset":"UltimateCataclysm"}"#;
const ISO: &str = r#"{"tiles":true,"tileset":"Ultica_iso","three_d":false}"#;
const THREE: &str = r#"{"tiles":true,"tileset":"Ultica_iso","three_d":true}"#;
const FIRST: &str = r#"{"tiles":true,"tileset":"Ultica_iso","three_d":true,"first_person":true}"#;
const SMALL: egui::Vec2 = egui::vec2(1290.0, 830.0);
const LARGE: egui::Vec2 = egui::vec2(1920.0, 1080.0);

#[test]
#[ignore = "requires BN_ENGINE pointing at an engine binary"]
fn live_aim_flat() {
    aim_at_zombie("flat-1290", FLAT, SMALL);
    aim_at_zombie("flat-1920", FLAT, LARGE);
}

#[test]
#[ignore = "requires BN_ENGINE pointing at an engine binary"]
fn live_aim_iso() {
    aim_at_zombie("iso-1290", ISO, SMALL);
    aim_at_zombie("iso-1920", ISO, LARGE);
}

#[test]
#[ignore = "requires BN_ENGINE pointing at an engine binary"]
fn live_aim_3d() {
    aim_at_zombie("3d-1290", THREE, SMALL);
    aim_at_zombie("3d-1920", THREE, LARGE);
}

#[test]
#[ignore = "requires BN_ENGINE pointing at an engine binary"]
fn live_aim_first_person() {
    aim_at_zombie("fp-1290", FIRST, SMALL);
    aim_at_zombie("fp-1920", FIRST, LARGE);
}
