#![recursion_limit = "512"]
//! The classes a screen's window falls in, in the client's own look: every window lies in the map region (off the
//! top bar, the shortcut bar and the sidebar), a screen is as tall as its content and leaves the map visible around
//! it, a full screen reaches the sidebar, a dialog keeps a readable width, and a side window is wide enough for its
//! footer on one row.
mod common;
use bn_client_egui::transport::ScriptHandle;
use bn_client_egui::{App, EngineModel};
use common::{fixture, session};
use eframe::egui;
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use serde_json::{Value, json};

type H<'a> = Harness<'a, App<EngineModel>>;

const SIZES: [egui::Vec2; 2] = [egui::vec2(1280.0, 800.0), egui::vec2(1920.0, 1080.0)];

/// The space a window keeps from the edge of the map region.
const MARGIN: f32 = 8.0;

fn example(file: &str, name: &str) -> Value {
    let path = format!(
        "{}/../core/tests/fixtures/0.2.examples/{file}.json",
        env!("CARGO_MANIFEST_DIR")
    );
    let all = serde_json::from_str::<Value>(&std::fs::read_to_string(path).unwrap()).unwrap();
    all["valid"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["name"] == name)
        .unwrap_or_else(|| panic!("no example {name}"))["value"]
        .clone()
}

/// A game in progress in a window of `size`, in the client's theme and fonts, with `screen` pushed onto the map,
/// the client's settings kept in `settings`.
fn playing<'a>(
    size: egui::Vec2,
    screen: Value,
    settings: Option<&std::path::Path>,
) -> (H<'a>, ScriptHandle) {
    let (t, script) = session();
    let app = App::new(EngineModel::new(Box::new(t)));
    let app = match settings {
        Some(path) => app.with_settings(path.into()),
        None => app,
    };
    let mut h = Harness::builder()
        .with_size(size)
        .build_ui_state(|ui, app: &mut App<EngineModel>| app.show(ui), app);
    bn_client_egui::kit::tokens::apply(&h.ctx);
    bn_client_egui::fonts::install_bundled(&h.ctx);
    h.run_steps(3);
    script.push(fixture("events_41_walk"));
    h.run_steps(4);
    let map = fixture("subscribe_result")["result"]["interaction"]["stack"][0].clone();
    script.push(json!({"jsonrpc": "2.0", "method": "bn.events", "params": {"epoch": "e7f3", "events": [{
        "sequence": "42", "revision": "33", "type": "interaction.changed", "changes": {"interaction": {
        "boundary_id": "b:60",
        "readiness": {"phase": "playing", "game_ready": true, "accepts_interaction_commands": true,
            "accepts_registered_actions": true},
        "actions": [], "interaction": null, "stack": [map, screen]}}}]}}));
    h.run_steps(6);
    common::settle_windows(&mut h);
    (h, script)
}

/// The window of the screen the engine names `kind`.
fn window_of(h: &H, kind: &str) -> egui::Rect {
    let id = egui::Id::new(("dialog", format!("screen:{kind}")));
    h.ctx
        .memory(|m| m.area_rect(id))
        .unwrap_or_else(|| panic!("no window {kind}"))
}

/// The map region: what the top bar, the shortcut bar and the sidebar (on the right, its resize handle at its
/// left edge) leave.
fn region(h: &H) -> egui::Rect {
    let panel = |id: &str| egui::PanelState::load(&h.ctx, egui::Id::new(id)).map(|p| p.outer_rect);
    let bar = panel("bar").expect("no top bar");
    let left = panel("shortcut-bar").map_or(bar.left(), |r| r.right());
    let sidebar = h.get_by_label("Resize sidebar").rect();
    egui::Rect::from_min_max(
        egui::pos2(left, bar.bottom()),
        egui::pos2(sidebar.center().x, sidebar.bottom()),
    )
}

/// The buttons whose label starts with one of `labels`, in that order.
fn buttons(h: &H, labels: &[&str]) -> Vec<egui::Rect> {
    labels
        .iter()
        .map(|label| {
            h.get_all_by_role(egui::accesskit::Role::Button)
                .find(|b| {
                    b.accesskit_node()
                        .label()
                        .is_some_and(|l| l.starts_with(label))
                })
                .unwrap_or_else(|| panic!("no button {label}"))
                .rect()
        })
        .collect()
}

#[test]
fn every_window_lies_in_the_map_region_off_the_top_bar_toolbar_and_sidebar() {
    let screens = [
        ("info", "message log", "messages"),
        ("character", "character sheet", "character_sheet"),
        ("inventory", "drop selector screen", "inventory"),
        ("pickup", "pickup screen", "pickup"),
        ("crafting", "Crafting screen", "crafting"),
        ("editor", "map editor with nothing open", "editor"),
        ("generic", "query popup screen", "popup"),
        ("look", "look-around screen", "look"),
    ];
    let mut outside = vec![];
    for size in SIZES {
        for (file, name, kind) in screens {
            let (h, _) = playing(size, example(file, name), None);
            let window = window_of(&h, kind);
            let region = region(&h);
            if !region.expand(0.5).contains_rect(window) {
                outside.push(format!("{kind} at {size:?}: {window:?} outside {region:?}"));
            }
        }
    }
    assert!(outside.is_empty(), "{outside:#?}");
}

#[test]
fn a_window_restored_over_the_top_bar_and_toolbar_is_moved_into_the_map_region() {
    for size in SIZES {
        let file = common::game_root("window-class-restored").join("settings.json");
        let windows = json!({"windows": {"screen:messages": {"pos": [0.0, 0.0], "size": [700.0, 300.0],
            "collapsed": false}}});
        std::fs::write(&file, windows.to_string()).unwrap();
        let (h, _) = playing(size, example("info", "message log"), Some(&file));
        let window = window_of(&h, "messages");
        let region = region(&h);
        assert!(
            region.expand(0.5).contains_rect(window),
            "{size:?}: {window:?} outside {region:?}"
        );
    }
}

#[test]
fn a_screen_is_as_tall_as_its_content_and_leaves_the_map_visible() {
    for size in SIZES {
        let (h, _) = playing(size, example("inventory", "drop selector screen"), None);
        let window = window_of(&h, "inventory");
        let region = region(&h);
        // The footer is one row, where the content ends.
        let last = buttons(&h, &["Confirm your selection"])[0];
        assert!(
            window.bottom() - last.bottom() < 40.0,
            "{size:?}: {window:?} runs past its footer at {last:?}"
        );
        assert!(
            window.height() < region.height() - 4.0 * MARGIN,
            "{size:?}: {window:?} hides the map in {region:?}"
        );
        assert!(
            window.width() >= 0.6 * (region.width() - 2.0 * MARGIN) - 1.0,
            "{size:?}: {window:?} narrower than 60% of {region:?}"
        );
        assert!(
            (window.center().x - region.center().x).abs() < 2.0,
            "{size:?}: {window:?} not centred in {region:?}"
        );
    }
}

/// Whether `window` fills the map region inside its margin.
fn fills(h: &H, window: egui::Rect) -> bool {
    let inner = region(h).shrink(MARGIN);
    let near = |a: f32, b: f32| (a - b).abs() <= 2.0;
    near(window.left(), inner.left())
        && near(window.right(), inner.right())
        && near(window.top(), inner.top())
        && near(window.bottom(), inner.bottom())
}

#[test]
fn a_full_screen_reaches_every_edge_of_the_map_region() {
    let mut wrong = vec![];
    for size in SIZES {
        let (h, _) = playing(size, example("overmap", "overmap screen"), None);
        let window = window_of(&h, "overmap");
        if !fills(&h, window) {
            wrong.push(format!(
                "{size:?}: {window:?} does not fill {:?}",
                region(&h)
            ));
        }
    }
    assert!(wrong.is_empty(), "{wrong:#?}");
}

/// The region grows when the sidebar narrows or the window widens; a full screen grows with it rather than
/// leaving a strip of map before the sidebar.
#[test]
fn a_full_screen_follows_the_map_region_when_it_grows() {
    let (mut h, _) = playing(SIZES[0], example("overmap", "overmap screen"), None);
    h.set_size(SIZES[1]);
    h.run_steps(4);
    common::settle_windows(&mut h);
    let window = window_of(&h, "overmap");
    assert!(
        fills(&h, window),
        "{window:?} does not fill {:?}",
        region(&h)
    );
}

/// A screen first shown in a narrower region, before the sidebar or the window settled, is never narrower than
/// one first shown in the region it ends in.
#[test]
fn a_screen_widens_with_the_map_region_to_the_width_it_has_there() {
    let screen = || example("distractions", "distraction manager");
    let (mut grown, _) = playing(SIZES[0], screen(), None);
    grown.set_size(SIZES[1]);
    grown.run_steps(4);
    common::settle_windows(&mut grown);
    let (fresh, _) = playing(SIZES[1], screen(), None);
    let (grown, fresh) = (
        window_of(&grown, "distraction_manager"),
        window_of(&fresh, "distraction_manager"),
    );
    assert!(
        grown.width() >= fresh.width() - 1.0,
        "{grown:?} narrower than {fresh:?}"
    );
}

#[test]
fn a_dialog_is_no_wider_than_a_readable_column() {
    for size in SIZES {
        let (h, _) = playing(size, example("npctalk", "NPC talk screen"), None);
        let window = window_of(&h, "talk");
        let room = region(&h).width() - 2.0 * MARGIN;
        assert!(
            window.width() <= 720.0_f32.min(0.6 * room) + 1.0,
            "{size:?}: {window:?} wider than its column in {room}"
        );
        assert!(window.width() >= 320.0, "{size:?}: {window:?}");
    }
}

/// A table has no words to wrap: a dialog holding one is as wide as its labels beside its columns need, past the
/// reading column, which at 1280x800 leaves the mutation table's labels no room.
#[test]
fn a_dialog_holding_a_table_keeps_each_label_on_one_line() {
    let screen = example("character", "mutation screen");
    let (h, _) = playing(SIZES[0], screen.clone(), None);
    let labels = common::legible::column_list_labels(&screen);
    let wrapped = common::legible::wrapped_labels(&h.output().shapes, &labels);
    assert!(wrapped.is_empty(), "{wrapped:#?}");
}

#[test]
fn a_side_window_is_wide_enough_for_its_footer_on_one_row() {
    let footer = ["Confirm Choice", "Select all", "Filter", "Wear", "Wield"];
    for size in SIZES {
        let (h, _) = playing(size, example("pickup", "pickup screen"), None);
        let window = window_of(&h, "pickup");
        let row = buttons(&h, &footer);
        for button in &row {
            assert!(
                (button.top() - row[0].top()).abs() < 1.0
                    && (button.height() - row[0].height()).abs() < 1.0,
                "{size:?}: footer of {window:?} on more than one row: {row:?}"
            );
            assert!(window.contains_rect(*button), "{size:?}: {button:?} cut");
        }
    }
}

/// A game whose actions include the character sheet (`@`), in a window of `size` in the client's look, with the
/// client's settings kept in `settings`; the sheet's panel window is opened and served.
fn sheet_open<'a>(size: egui::Vec2, settings: &std::path::Path) -> H<'a> {
    let mut subscribed = fixture("subscribe_result");
    subscribed["result"]["interaction"]["actions"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id": "player_data", "name": "View Player Info", "keys": ["@"]}));
    let reply = vec![
        subscribed,
        fixture("snapshot_part_0"),
        fixture("snapshot_part_1"),
    ];
    let (t, script) = bn_client_egui::transport::ScriptedTransport::new();
    let t = t
        .on("bn.hello", vec![fixture("hello_result")])
        .on("bn.subscribe", reply.clone())
        .on("bn.subscribe", reply);
    let app = App::new(EngineModel::new(Box::new(t))).with_settings(settings.into());
    let mut h = Harness::builder()
        .with_size(size)
        .build_ui_state(|ui, app: &mut App<EngineModel>| app.show(ui), app);
    bn_client_egui::kit::tokens::apply(&h.ctx);
    bn_client_egui::fonts::install_bundled(&h.ctx);
    h.run_steps(3);
    h.event(egui::Event::Text("@".into()));
    h.run_steps(2);
    let mut answered = 0;
    for _ in 0..4 {
        let reads = script
            .sent()
            .into_iter()
            .filter(|m| m["method"] == "bn.panel.view")
            .collect::<Vec<_>>();
        for read in &reads[answered..] {
            let panel = read["params"]["panel"].as_str().unwrap();
            script.push(
                json!({"jsonrpc": "2.0", "id": read["id"], "result": common::panel_view(panel)}),
            );
        }
        answered = reads.len();
        h.run_steps(2);
    }
    common::settle_windows(&mut h);
    h
}

#[test]
fn a_panel_window_restored_over_the_top_bar_is_moved_into_the_map_region() {
    for size in SIZES {
        let file = common::game_root("window-class-panel").join("settings.json");
        let windows = json!({"windows": {"panel:player_data": {"pos": [0.0, 0.0], "size": [500.0, 400.0],
            "collapsed": false}}});
        std::fs::write(&file, windows.to_string()).unwrap();
        let h = sheet_open(size, &file);
        let window = h
            .ctx
            .memory(|m| m.area_rect(egui::Id::new(("dialog", "panel:player_data"))))
            .expect("no sheet window");
        let region = region(&h);
        assert!(
            region.expand(0.5).contains_rect(window),
            "{size:?}: {window:?} outside {region:?}"
        );
    }
}

#[test]
fn a_side_window_kept_narrow_by_a_last_run_is_still_wide_enough_for_its_footer() {
    let footer = ["Confirm Choice", "Select all", "Filter", "Wear", "Wield"];
    let file = common::game_root("window-class-narrow").join("settings.json");
    let windows = json!({"windows": {"screen:pickup": {"pos": [20.0, 50.0], "size": [175.0, 500.0],
        "collapsed": false}}});
    std::fs::write(&file, windows.to_string()).unwrap();
    let (h, _) = playing(SIZES[0], example("pickup", "pickup screen"), Some(&file));
    let window = window_of(&h, "pickup");
    let row = buttons(&h, &footer);
    assert!(
        row.iter().all(|b| (b.top() - row[0].top()).abs() < 1.0),
        "footer of {window:?} on more than one row: {row:?}"
    );
}
