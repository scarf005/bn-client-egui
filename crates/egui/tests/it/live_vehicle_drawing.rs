//! Against a real engine: a vehicle built the way a player draws it, with the pointer on the canvas: a filled
//! rectangle of frames by one drag, mirrored wheels by clicks, seats by a line, a right drag erasing a strip, the
//! undo of it, the view turned twice and a pencil stroke in the turned view, saved. Counts the gestures.
//!
//! `BN_ENGINE=<engine> SHOT_DIR=<dir> cargo test --release -p bn-client-egui --test it live_vehicle_drawing:: --
//! --ignored --nocapture --test-threads 1`
use crate::live_editor::{H, invoke, save, start, text, wait_kind};
use crate::live_editor_walk::{click, key};
use crate::live_vehicle_ux::pick;
use bn_client_egui::GameModel;
use bn_client_egui::protocol::{Component, InvokeArgs};
use eframe::egui;
use egui_kittest::kittest::NodeT;

/// The drawn square of the cell `id`, found by the cells' own `x` and `y`: the canvas's squares are the lattice of
/// square buttons, the first of them the grid's first cell.
fn square(h: &H, id: &str) -> egui::Pos2 {
    let top = h.state().model.screens().last().unwrap();
    let Some(Component::Grid(grid)) = top.root.as_ref().unwrap().find("vehicle:grid") else {
        panic!("no vehicle:grid");
    };
    let first = (
        grid.cells.iter().map(|c| c.x).min().unwrap(),
        grid.cells.iter().map(|c| c.y).min().unwrap(),
    );
    let cell = grid
        .cells
        .iter()
        .find(|c| c.id.as_deref() == Some(id))
        .unwrap_or_else(|| panic!("no cell {id}"));
    let squares = h
        .root()
        .children_recursive()
        .filter(|n| n.accesskit_node().role() == egui::accesskit::Role::Button)
        .filter(|n| n.accesskit_node().bounding_box().is_some())
        .filter(|n| {
            let r = n.rect();
            (r.width() - r.height()).abs() < 1.0 && r.width() >= 14.0
        })
        .map(|n| n.rect())
        .collect::<Vec<_>>();
    let side = squares[0].width();
    let lattice = squares
        .iter()
        .filter(|r| (r.width() - side).abs() < 1.0)
        .collect::<Vec<_>>();
    let min = egui::pos2(
        lattice.iter().map(|r| r.min.x).fold(f32::MAX, f32::min),
        lattice.iter().map(|r| r.min.y).fold(f32::MAX, f32::min),
    );
    min + egui::vec2(
        (cell.x - first.0) as f32 + 0.5,
        (cell.y - first.1) as f32 + 0.5,
    ) * side
}

fn button(h: &mut H, at: egui::Pos2, button: egui::PointerButton, pressed: bool) {
    h.event(egui::Event::PointerButton {
        pos: at,
        button,
        pressed,
        modifiers: egui::Modifiers::NONE,
    });
    h.run_steps(3);
}

/// One gesture: the pointer down on the cell `from`, along the squares between to the cell `to`, and up.
fn drag(h: &mut H, by: egui::PointerButton, from: &str, to: &str) {
    let (a, b) = (square(h, from), square(h, to));
    h.hover_at(a);
    h.run_steps(3);
    button(h, a, by, true);
    for step in 1..=12 {
        h.hover_at(a + (b - a) * (step as f32 / 12.0));
        h.run_steps(2);
    }
    button(h, b, by, false);
    h.run_steps(6);
}

/// A letter typed on the keyboard, which the engine's hotkeys take, and the engine's answer.
fn typed(h: &mut H, letter: &str) {
    h.event(egui::Event::Text(letter.into()));
    h.run_steps(3);
    crate::common::live::settle(h);
}

fn mount(h: &mut H, id: &str) {
    let at = square(h, id);
    crate::live_editor_walk::click_at_pub(h, at);
}

fn parts_at(h: &H, id: &str) -> usize {
    let top = h.state().model.screens().last().unwrap();
    let Some(Component::Grid(grid)) = top.root.as_ref().unwrap().find("vehicle:grid") else {
        panic!("no vehicle:grid");
    };
    grid.cells
        .iter()
        .find(|c| c.id.as_deref() == Some(id))
        .map_or(0, |c| c.looks.len())
}

#[test]
#[ignore = "requires BN_ENGINE pointing at an engine binary"]
fn a_vehicle_is_drawn_with_the_canvas_tools_in_a_few_strokes() {
    let dir = std::env::temp_dir().join(format!("bn-vdraw-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let (mut h, _root) = start(&dir);
    let mut gestures = Vec::<String>::new();
    click(&mut h, "Editor");
    click(&mut h, "Vehicle editor");
    key(&mut h, egui::Key::Escape);
    click(&mut h, "Yes");
    wait_kind(&mut h, "vehicle_editor");
    let path = crate::common::live::profile_dir().join("mods/my_mod/draw_van.json");
    invoke(&mut h, "vehicle:new", text(&path.to_string_lossy()));
    if crate::live_editor_walk::labels(&h)
        .iter()
        .any(|l| l.ends_with(" Continue"))
    {
        click(&mut h, "Continue");
    }
    wait_kind(&mut h, "vehicle_editor");
    save(&mut h, "01-new-vehicle");

    typed(&mut h, "u");
    pick(&mut h, "Frames", "frame");
    gestures.push("pick frame".into());
    save(&mut h, "02-filled-rectangle-picked");
    drag(
        &mut h,
        egui::PointerButton::Primary,
        "mount:-3,-1",
        "mount:3,1",
    );
    gestures.push("drag: filled rectangle of frames".into());
    save(&mut h, "03-frame-drawn");

    click(&mut h, "Mirror left and right");
    gestures.push("mirror sides on".into());
    save(&mut h, "04-mirror-on");

    typed(&mut h, "b");
    for (part, label) in [("wheel hub assembly", "hub"), ("wheel", "wheel")] {
        pick(&mut h, "Wheels", part);
        gestures.push(format!("pick {label}"));
        mount(&mut h, "mount:-3,1");
        gestures.push(format!("click {label} mount (mirrored)"));
        save(&mut h, &format!("05-{label}"));
    }

    typed(&mut h, "l");
    pick(&mut h, "Seats", "seat");
    gestures.push("pick seat".into());
    drag(
        &mut h,
        egui::PointerButton::Primary,
        "mount:-1,0",
        "mount:1,0",
    );
    gestures.push("drag: line of seats".into());
    save(&mut h, "06-seats-by-line");

    typed(&mut h, "b");
    pick(&mut h, "Engines", "1-cylinder engine");
    mount(&mut h, "mount:0,1");
    gestures.push("pick + click engine".into());
    pick(&mut h, "Fuel", "vehicle tank (60L)");
    mount(&mut h, "mount:0,-1");
    gestures.push("pick + click tank".into());
    save(&mut h, "07-engine-and-tank");

    let before = parts_at(&h, "mount:-1,1");
    drag(
        &mut h,
        egui::PointerButton::Secondary,
        "mount:-2,1",
        "mount:2,1",
    );
    gestures.push("right drag: erase a strip".into());
    save(&mut h, "08-strip-erased");
    println!(
        "DRAW parts on mount:-1,1 before {before} after {}",
        parts_at(&h, "mount:-1,1")
    );

    h.key_press_modifiers(egui::Modifiers::CTRL, egui::Key::Z);
    h.run_steps(30);
    gestures.push("undo".into());
    save(&mut h, "09-undone");
    println!(
        "DRAW parts on mount:-1,1 after undo {}",
        parts_at(&h, "mount:-1,1")
    );

    typed(&mut h, "z");
    typed(&mut h, "z");
    gestures.push("rotate view x2".into());
    save(&mut h, "10-view-turned-twice");

    typed(&mut h, "b");
    pick(&mut h, "Seats", "seat");
    drag(
        &mut h,
        egui::PointerButton::Primary,
        "mount:-1,-1",
        "mount:1,-1",
    );
    gestures.push("pick seat + pencil stroke in the turned view".into());
    save(&mut h, "11-pencil-in-turned-view");

    invoke(&mut h, "vehicle:save", InvokeArgs::default());
    save(&mut h, "12-saved");
    println!("DRAW gestures {}: {gestures:#?}", gestures.len());
    let _ = std::fs::remove_dir_all(&dir);
}
