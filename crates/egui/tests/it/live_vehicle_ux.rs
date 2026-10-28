//! Against a real engine: the vehicle editor's New vehicle with a few parts, and the map editor's canvas, saved at both
//! window sizes for a look at how the canvas squares are sized.
//!
//! `BN_ENGINE=<engine> SHOT_DIR=<dir> SHOT_TAG=<tag> cargo test --release -p bn-client-egui --test it live_vehicle_ux::
//! -- --ignored --nocapture --test-threads 1`
use crate::live_editor::{invoke, row, save, start, text, wait_kind};
use crate::live_editor_walk::{click, key};
use eframe::egui;
use egui_kittest::kittest::Queryable;

fn tag(name: &str) -> String {
    format!("{}-{name}", std::env::var("SHOT_TAG").unwrap_or_default())
}

#[test]
#[ignore = "requires BN_ENGINE pointing at an engine binary"]
fn the_vehicle_editor_canvas_fits_a_new_vehicle() {
    let dir = std::env::temp_dir().join(format!("bn-vux-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let (mut h, _root) = start(&dir);
    click(&mut h, "Editor");
    click(&mut h, "Vehicle editor");
    key(&mut h, egui::Key::Escape);
    click(&mut h, "Yes");
    wait_kind(&mut h, "vehicle_editor");
    let mods = crate::common::live::profile_dir().join("mods");
    let path = mods.join("my_mod/demo_van.json");
    invoke(&mut h, "vehicle:new", text(&path.to_string_lossy()));
    if crate::live_editor_walk::labels(&h)
        .iter()
        .any(|l| l.ends_with(" Continue"))
    {
        click(&mut h, "Continue");
    }
    wait_kind(&mut h, "vehicle_editor");
    save(&mut h, &tag("vehicle-new"));
    for (id, at) in [
        ("frame_cross", "0,0"),
        ("frame_cross", "-1,0"),
        ("frame_cross", "1,0"),
        ("seat", "0,0"),
    ] {
        invoke(&mut h, "vehicle:part_filter", text(id));
        invoke(&mut h, "vehicle:choose", row(&format!("vpart:{id}")));
        invoke(&mut h, "vehicle:mount", row(&format!("mount:{at}")));
    }
    invoke(&mut h, "vehicle:part_filter", text(""));
    save(&mut h, &tag("vehicle-parts"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
#[ignore = "requires BN_ENGINE pointing at an engine binary"]
fn the_map_editor_canvas_keeps_its_squares() {
    let dir = std::env::temp_dir().join(format!("bn-vux-map-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let (mut h, root) = start(&dir);
    click(&mut h, "Editor");
    click(&mut h, "Map editor");
    key(&mut h, egui::Key::Escape);
    click(&mut h, "Yes");
    wait_kind(&mut h, "editor");
    let house = root.join("data/json/mapgen/house/2Story01.json");
    invoke(&mut h, "editor:open", text(&house.to_string_lossy()));
    save(&mut h, &tag("map-house"));
    let _ = std::fs::remove_dir_all(&dir);
}

use bn_client_egui::GameModel;
use bn_client_egui::protocol::Component;
use bn_client_egui::protocol::InvokeArgs;

type Tab = (String, String);

/// The tabs (id, label) of the part palette, and the rows (id, label) of the group shown.
fn palette(h: &crate::live_editor::H) -> (Vec<Tab>, Vec<Tab>) {
    let top = h.state().model.screens().last().unwrap();
    let root = top.root.as_ref().unwrap();
    let tabs = match root.find("vehicle:part_groups") {
        Some(Component::Tabs(t)) => t
            .tabs
            .iter()
            .map(|t| (t.id.clone(), t.label.shown()))
            .collect(),
        _ => vec![],
    };
    let rows = match root.find("vehicle:parts") {
        Some(Component::List(l)) => l
            .rows
            .iter()
            .map(|r| (r.id.clone(), r.label.shown()))
            .collect(),
        _ => vec![],
    };
    (tabs, rows)
}

/// Picks the part whose label contains `part` from the palette group whose label contains `group`, as a player does
/// with the tabs, and installs it on `at`.
fn install(h: &mut crate::live_editor::H, group: &str, part: &str, at: &str) {
    let (tabs, _) = palette(h);
    let tab = tabs
        .iter()
        .find(|t| t.1.contains(group))
        .unwrap_or_else(|| panic!("no tab {group}: {tabs:?}"));
    invoke(h, "vehicle:part_group", row(&tab.0));
    let (_, rows) = palette(h);
    println!("GROUP {group}: {rows:?}");
    let found = rows
        .iter()
        .find(|r| r.1 == part)
        .or_else(|| rows.iter().find(|r| r.1.contains(part)))
        .unwrap_or_else(|| panic!("no part {part} in {group}: {rows:?}"));
    invoke(h, "vehicle:choose", row(&found.0));
    invoke(h, "vehicle:mount", row(&format!("mount:{at}")));
}

#[test]
#[ignore = "requires BN_ENGINE pointing at an engine binary"]
fn a_first_time_walk_builds_fuels_saves_and_spawns_a_vehicle() {
    let dir = std::env::temp_dir().join(format!("bn-vwalk-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let (mut h, _root) = start(&dir);
    click(&mut h, "Editor");
    click(&mut h, "Vehicle editor");
    key(&mut h, egui::Key::Escape);
    click(&mut h, "Yes");
    wait_kind(&mut h, "vehicle_editor");
    save(&mut h, "01-editor-opened");
    let path = crate::common::live::profile_dir().join("mods/my_mod/walk_van.json");
    invoke(&mut h, "vehicle:new", text(&path.to_string_lossy()));
    if crate::live_editor_walk::labels(&h)
        .iter()
        .any(|l| l.ends_with(" Continue"))
    {
        click(&mut h, "Continue");
    }
    save(&mut h, "02-new-vehicle");
    for (n, group, part, at, name) in [
        (3, "Frames", "frame", "1,0", "frame-east"),
        (4, "Frames", "frame", "-1,0", "frame-west"),
        (5, "Wheels", "wheel", "-1,0", "wheel-without-hub"),
        (6, "Wheels", "wheel hub assembly", "-1,0", "hub"),
        (7, "Wheels", "wheel", "-1,0", "wheel-with-hub"),
        (8, "Wheels", "wheel hub assembly", "1,0", "hub-east"),
        (9, "Wheels", "wheel", "1,0", "wheel-east"),
        (10, "Seats", "seat", "0,0", "seat"),
        (11, "Seats", "controls", "0,0", "controls"),
        (12, "Frames", "frame", "0,1", "frame-south"),
        (13, "Engines", "1-cylinder engine", "0,1", "engine"),
        (14, "Frames", "frame", "0,-1", "frame-north"),
        (15, "Fuel", "vehicle tank (60L)", "0,-1", "tank"),
    ] {
        install(&mut h, group, part, at);
        save(&mut h, &format!("{n:02}-{name}"));
    }
    // The tank's fuel: the part picked in the stack, then the Fuel field of its settings.
    invoke(&mut h, "vehicle:mount", row("mount:0,-1"));
    save(&mut h, "16-tank-mount-picked");
    let tank = {
        let top = h.state().model.screens().last().unwrap();
        let Some(Component::List(stack)) = top.root.as_ref().unwrap().find("vehicle:stack") else {
            panic!("no stack");
        };
        stack
            .rows
            .iter()
            .find(|r| r.label.shown().contains("tank"))
            .map(|r| r.id.clone())
    };
    println!("WALK tank row {tank:?}");
    if let Some(tank) = tank {
        invoke(&mut h, "vehicle:pick", row(&tank));
    }
    save(&mut h, "17-tank-picked");
    let combo = |h: &crate::live_editor::H| {
        h.query_by_role_and_label(egui::accesskit::Role::ComboBox, "Fuel")
            .map(|n| n.rect())
    };
    println!("WALK fuel combo {:?}", combo(&h));
    if let Some(at) = combo(&h) {
        crate::live_editor_walk::click_at_pub(&mut h, at.center());
        save(&mut h, "18-fuel-popup");
        h.event(egui::Event::Text("gaso".into()));
        h.run_steps(5);
        save(&mut h, "19-fuel-filtered");
        key(&mut h, egui::Key::Enter);
        save(&mut h, "20-fuel-set");
    }
    // A right click on the frame square removes its top part.
    let at = crate::live_editor_walk::square_pub(&h, "wheel");
    println!("WALK wheel square {at:?}");
    save(&mut h, "21-before-right-click");
    if let Some(at) = at {
        crate::live_editor_walk::right_click_pub(&mut h, at);
        save(&mut h, "22-right-clicked");
    }
    invoke(&mut h, "vehicle:save", InvokeArgs::default());
    save(&mut h, "23-saved");
    invoke(&mut h, "vehicle:spawn", InvokeArgs::default());
    save(&mut h, "24-spawned");
    let _ = std::fs::remove_dir_all(&dir);
}
