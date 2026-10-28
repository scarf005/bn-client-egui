//! Against a real engine: the vehicle editor from the main menu's Editor tab, drawn from its generic components: a
//! prototype opened from the filtered list, its mounts drawn with the tileset on the canvas, a tank's fuel and the
//! vehicle's name in their own fields, a part installed on a mount and one the game does not mount refused, the
//! install undone, the design spawned and copied with its state beside the avatar, a part installed on the spawned
//! vehicle in place and undone, and the editor left for the map.
//!
//! `BN_ENGINE=<engine> SHOT_DIR=<dir> cargo test --release -p bn-client-egui --test it live_vehicle_editor:: --
//! --ignored --nocapture`; `BN_ROOT` names the game root when it is not beside the engine.
use crate::live_editor::{
    H, field, glyphs, invoke, row, rows, run_until, save, start, text, wait_kind,
};
use bn_client_egui::GameModel;
use bn_client_egui::protocol::{Component, InvokeArgs, ScreenRole};

/// The row `id` of a list of the screen on top.
fn list_row(h: &H, list: &str, id: &str) -> bn_client_egui::protocol::Row {
    let top = h.state().model.screens().last().unwrap();
    let Some(Component::List(found)) = top.root.as_ref().unwrap().find(list) else {
        panic!("no {list}");
    };
    found
        .rows
        .iter()
        .find(|r| r.id == id)
        .cloned()
        .unwrap_or_else(|| panic!("no row {id}"))
}

/// How many cells of the grid draw a sprite.
fn drawn_cells(h: &H, id: &str) -> usize {
    let top = h.state().model.screens().last().unwrap();
    let Some(Component::Grid(grid)) = top.root.as_ref().unwrap().find(id) else {
        panic!("no {id}");
    };
    grid.cells.iter().filter(|c| !c.looks.is_empty()).count()
}

#[test]
#[ignore = "requires BN_ENGINE pointing at an engine binary"]
fn the_vehicle_editor_opens_installs_and_undoes() {
    let dir = std::env::temp_dir().join(format!("bn-vehicle-editor-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let (mut h, _root) = start(&dir);
    invoke(&mut h, "menu:tab", row("editor"));
    invoke(&mut h, "menu:entry", row("editor:vehicle"));
    // The mod list first, left with the mods as they are.
    wait_kind(&mut h, "world_creation");
    crate::live_editor_walk::key(&mut h, eframe::egui::Key::Escape);
    crate::live_editor_walk::click(&mut h, "Yes");
    run_until(&mut h, "the vehicle editor", 900, |h| {
        !h.state().model.busy()
            && h.state()
                .model
                .screens()
                .last()
                .and_then(|s| s.kind.as_deref())
                == Some("vehicle_editor")
    });
    wait_kind(&mut h, "vehicle_editor");
    save(&mut h, "vehicle-editor-empty");

    invoke(&mut h, "vehicle:filter", text("car"));
    invoke(&mut h, "vehicle:open", row("prototype:car"));
    assert!(drawn_cells(&h, "vehicle:grid") > 10);
    save(&mut h, "vehicle-editor-car");
    let mount = {
        let top = h.state().model.screens().last().unwrap();
        let Some(Component::Grid(grid)) = top.root.as_ref().unwrap().find("vehicle:grid") else {
            panic!("no vehicle:grid");
        };
        grid.cells
            .iter()
            .find(|c| c.label.as_ref().is_some_and(|l| l.shown().contains("tank")))
            .and_then(|c| c.id.clone())
            .expect("the car's tank mount")
    };
    invoke(&mut h, "vehicle:mount", row(&mount));
    let tank = rows(&h, "vehicle:stack")
        .into_iter()
        .find(|id| {
            list_row(&h, "vehicle:stack", id)
                .label
                .shown()
                .contains("tank")
        })
        .expect("the car's tank");
    invoke(&mut h, "vehicle:pick", row(&tank));
    save(&mut h, "vehicle-editor-stack");
    invoke(&mut h, "vehicle:tool", row("tool:part"));
    assert_eq!(field(&h, "vehicle:part_form", "fuel"), "gasoline");
    save(&mut h, "vehicle-editor-part");
    invoke(&mut h, "vehicle:tool", row("tool:vehicle"));
    assert_eq!(field(&h, "vehicle:props", "name"), "Car");
    save(&mut h, "vehicle-editor-props");
    invoke(&mut h, "vehicle:tool", row("tool:mount"));

    invoke(&mut h, "vehicle:filter", text("bicycle"));
    save(&mut h, "vehicle-editor-list");
    invoke(&mut h, "vehicle:open", row("prototype:bicycle"));
    let opened = glyphs(&h, "vehicle:grid");
    assert_eq!((opened.len(), opened[0].chars().count()), (5, 7));
    assert_eq!(drawn_cells(&h, "vehicle:grid"), 3);

    invoke(&mut h, "vehicle:mount", row("mount:1,0"));
    let before = rows(&h, "vehicle:stack");
    invoke(&mut h, "vehicle:part_filter", text("headlight"));
    invoke(&mut h, "vehicle:choose", row("vpart:headlight"));
    invoke(&mut h, "vehicle:mount", row("mount:1,0"));
    assert_eq!(rows(&h, "vehicle:stack").len(), before.len() + 1);
    save(&mut h, "vehicle-editor-installed");

    invoke(&mut h, "vehicle:part_filter", text("xlframe_vertical"));
    let frame = list_row(&h, "vehicle:parts", "vpart:xlframe_vertical");
    assert!(!frame.enabled);
    save(&mut h, "vehicle-editor-denied");

    invoke(&mut h, "vehicle:undo", InvokeArgs::default());
    invoke(&mut h, "vehicle:tool", row("tool:mount"));
    assert_eq!(rows(&h, "vehicle:stack"), before);

    invoke(&mut h, "vehicle:spawn", InvokeArgs::default());
    invoke(&mut h, "vehicle:source", row("source:world"));
    assert_eq!(rows(&h, "vehicle:world").len(), 1);
    invoke(&mut h, "vehicle:copy", row("world:0"));
    invoke(&mut h, "vehicle:paste", InvokeArgs::default());
    assert_eq!(rows(&h, "vehicle:world").len(), 2);
    save(&mut h, "vehicle-editor-world");

    invoke(&mut h, "vehicle:edit", row("world:0"));
    invoke(&mut h, "vehicle:mount", row("mount:1,0"));
    let plain = rows(&h, "vehicle:stack");
    invoke(&mut h, "vehicle:part_filter", text("headlight"));
    invoke(&mut h, "vehicle:choose", row("vpart:headlight"));
    invoke(&mut h, "vehicle:mount", row("mount:1,0"));
    assert_eq!(rows(&h, "vehicle:stack").len(), plain.len() + 1);
    save(&mut h, "vehicle-editor-live");
    invoke(&mut h, "vehicle:undo", InvokeArgs::default());
    assert_eq!(rows(&h, "vehicle:stack").len(), plain.len());
    invoke(&mut h, "vehicle:done", InvokeArgs::default());
    invoke(&mut h, "QUIT", InvokeArgs::default());
    run_until(&mut h, "the map", 120, |h| {
        !h.state().model.busy()
            && h.state()
                .model
                .screens()
                .last()
                .is_some_and(|s| s.role == ScreenRole::Map)
    });
    let _ = std::fs::remove_dir_all(&dir);
}
