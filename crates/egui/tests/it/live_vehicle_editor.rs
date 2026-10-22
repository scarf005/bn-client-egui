//! Against a real engine: the vehicle editor from the main menu's Editor tab, drawn from its generic components: a
//! prototype opened from the filtered list, its mounts drawn with the tileset, a part installed on a mount and one the
//! game does not mount refused, the install undone, the design spawned and copied with its state beside the avatar,
//! a part installed on the spawned vehicle in place and undone, and the editor left for the map.
//!
//! `BN_ENGINE=<engine> SHOT_DIR=<dir> cargo test --release -p bn-client-egui --test it live_vehicle_editor:: --
//! --ignored --nocapture`; `BN_ROOT` names the game root when it is not beside the engine.
use crate::live_editor::{H, glyphs, invoke, row, rows, run_until, save, start, text, wait_kind};
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

    invoke(&mut h, "vehicle:filter", text("bicycle"));
    save(&mut h, "vehicle-editor-list");
    invoke(&mut h, "vehicle:open", row("prototype:bicycle"));
    let opened = glyphs(&h, "vehicle:grid");
    assert_eq!((opened.len(), opened[0].chars().count()), (3, 5));
    assert_eq!(drawn_cells(&h, "vehicle:grid"), 3);

    invoke(&mut h, "vehicle:mount", row("mount:1,0"));
    let before = rows(&h, "vehicle:stack");
    invoke(&mut h, "vehicle:part_filter", text("headlight"));
    invoke(&mut h, "vehicle:install", row("vpart:headlight"));
    assert_eq!(rows(&h, "vehicle:stack").len(), before.len() + 1);
    save(&mut h, "vehicle-editor-installed");

    invoke(&mut h, "vehicle:part_filter", text("xlframe_vertical"));
    let frame = list_row(&h, "vehicle:parts", "vpart:xlframe_vertical");
    assert!(!frame.enabled);
    save(&mut h, "vehicle-editor-denied");

    invoke(&mut h, "vehicle:undo", InvokeArgs::default());
    assert_eq!(rows(&h, "vehicle:stack"), before);

    invoke(&mut h, "vehicle:spawn", InvokeArgs::default());
    assert_eq!(rows(&h, "vehicle:world").len(), 1);
    invoke(&mut h, "vehicle:copy", row("world:0"));
    invoke(&mut h, "vehicle:paste", InvokeArgs::default());
    assert_eq!(rows(&h, "vehicle:world").len(), 2);
    save(&mut h, "vehicle-editor-world");

    invoke(&mut h, "vehicle:edit", row("world:0"));
    invoke(&mut h, "vehicle:mount", row("mount:1,0"));
    let plain = rows(&h, "vehicle:stack");
    invoke(&mut h, "vehicle:part_filter", text("headlight"));
    invoke(&mut h, "vehicle:install", row("vpart:headlight"));
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
