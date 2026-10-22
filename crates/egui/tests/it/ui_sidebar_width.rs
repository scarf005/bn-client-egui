//! The sidebar is as wide as the game's panel options say, whatever its lines hold: a long item name is cut at
//! the panel's edge, it never pushes the panel over the map.
use crate::common::{fixture, session};
use bn_client_egui::{App, EngineModel};
use eframe::egui;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use serde_json::json;

type H<'a> = Harness<'a, App<EngineModel>>;

/// A window of `width` points whose avatar carries one item named `name`.
fn carrying<'a>(width: f32, name: &str) -> H<'a> {
    let (t, script) = session();
    let mut h = Harness::builder()
        .with_size(egui::vec2(width, 800.0))
        .build_ui_state(
            |ui, app: &mut App<EngineModel>| app.show(ui),
            App::new(EngineModel::new(Box::new(t))),
        );
    h.run_steps(3);
    let mut event = fixture("events_41_walk");
    event["params"]["events"][0]["changes"]["avatar"]["inventory"] = json!([{
        "look": {"kind": "item", "id": "rock", "glyph": "*", "color": "light_gray"},
        "name": name, "label": name, "slot": "carried"
    }]);
    script.push(event);
    h.run_steps(8);
    h
}

#[test]
fn a_long_item_name_does_not_move_the_sidebars_left_edge() {
    let long = "MBR vest (ceramic plates) with 2 items, 40x46mm M433 HEDP (grenade ammo pouch), hand grenade pouch with 2 items";
    for width in [1280.0, 1920.0] {
        let short = carrying(width, "rock").get_by_label("Ground").rect().min.x;
        let h = carrying(width, long);
        assert_eq!(h.get_by_label("Ground").rect().min.x, short, "at {width}");
    }
}

#[test]
fn a_long_item_name_is_cut_before_its_button_leaves_the_panel() {
    let long = "MBR vest (ceramic plates) with 2 items, 40x46mm M433 HEDP (grenade ammo pouch), hand grenade pouch with 2 items";
    let h = carrying(1280.0, long);
    let drop = h.get_by_label_contains("Drop MBR vest").rect();
    assert!(drop.max.x <= 1280.0, "the button is on screen: {drop:?}");
}
