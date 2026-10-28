//! Against a real engine: the vehicle editor's New vehicle with a few parts, and the map editor's canvas, saved at both
//! window sizes for a look at how the canvas squares are sized.
//!
//! `BN_ENGINE=<engine> SHOT_DIR=<dir> SHOT_TAG=<tag> cargo test --release -p bn-client-egui --test it live_vehicle_ux::
//! -- --ignored --nocapture --test-threads 1`
use crate::live_editor::{invoke, row, save, start, text, wait_kind};
use crate::live_editor_walk::{click, key};
use eframe::egui;

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
