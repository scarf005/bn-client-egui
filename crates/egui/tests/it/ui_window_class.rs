//! The classes a screen's window falls in, in the client's own look: every window lies in the map region (off the
//! top bar, the shortcut bar and the sidebar), a screen opens at least 60% as tall as the region and leaves the map
//! visible around it, a full screen reaches the sidebar, a dialog keeps a readable width, and a side window is wide enough for its
//! footer on one row.
use crate::common::{fixture, session};
use bn_client_egui::transport::ScriptHandle;
use bn_client_egui::{App, EngineModel};
use eframe::egui;
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use serde_json::{Value, json};

type H<'a> = Harness<'a, App<EngineModel>>;

const SIZES: [egui::Vec2; 2] = [egui::vec2(1280.0, 800.0), egui::vec2(1920.0, 1080.0)];

/// The sizes the first-height tests run at: the user's, the two above, and a short, narrow client window.
const HEIGHT_SIZES: [egui::Vec2; 4] = [
    egui::vec2(1290.0, 830.0),
    egui::vec2(1280.0, 800.0),
    egui::vec2(1920.0, 1080.0),
    egui::vec2(900.0, 480.0),
];

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
    crate::common::settle_windows(&mut h);
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
fn a_window_restored_off_the_screen_is_moved_onto_it() {
    for size in SIZES {
        let file = crate::common::game_root("window-class-restored").join("settings.json");
        let windows = json!({"windows": {"screen:messages": {"pos": [5000.0, 3000.0], "size": [700.0, 0.7 * size.y],
            "collapsed": false}}});
        std::fs::write(&file, windows.to_string()).unwrap();
        let (h, _) = playing(size, example("info", "message log"), Some(&file));
        let window = window_of(&h, "messages");
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, size);
        assert!(
            screen.expand(0.5).contains_rect(window)
                && window.size() == egui::vec2(700.0, (0.7 * size.y).round()),
            "{size:?}: {window:?} outside {screen:?}"
        );
    }
}

#[test]
fn a_screen_opens_at_least_60_percent_as_tall_as_the_map_region_and_leaves_the_map_visible() {
    for size in SIZES {
        let (h, _) = playing(size, example("inventory", "drop selector screen"), None);
        let window = window_of(&h, "inventory");
        let region = region(&h);
        assert!(
            window.height() >= 0.6 * region.height(),
            "{size:?}: {window:?} shorter than 60% of {region:?}"
        );
        assert!(
            window.height() < region.height() - 2.0 * MARGIN,
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

/// The direction prompt as the engine publishes it: its question over the compass of the squares around the
/// avatar, each running the prompt's choose.
fn direction_prompt() -> Value {
    let names = [["NW", "N", "NE"], ["W", "CE", "E"], ["SW", "S", "SE"]];
    let cells = (0..3)
        .flat_map(|y| (0..3).map(move |x| (x, y)))
        .map(|(x, y)| {
            json!({"x": x, "y": y, "id": format!("direction:{x}:{y}"), "glyph": names[y][x],
                "label": names[y][x], "affordance": "direction:choose", "enabled": true})
        })
        .collect::<Vec<_>>();
    let prompt =
        json!({"id": "text:message", "kind": "text", "markup": "Close where?", "role": "prompt"});
    let compass = json!({"id": "direction:compass", "kind": "grid", "affordances": ["direction:choose"],
        "cols": 3, "rows": 3, "cells": cells});
    let choose = json!({"id": "direction:choose", "label": "Choose direction", "keys": [], "enabled": true,
        "args": {"row": {"component_id": "direction:compass"}}, "expect": "boundary"});
    let cancel = json!({"id": "bn:cancel", "label": "Cancel", "keys": ["ESC"], "enabled": true,
        "role": "cancel", "expect": "screen"});
    json!({"screen_id": "screen:77", "role": "dialog", "kind": "direction", "title": "Choose direction",
        "root": {"id": "direction", "kind": "split", "relation": "peers", "children": [prompt, compass]},
        "affordances": [choose, cancel], "focus": {"component_id": "direction:compass"}})
}

/// A grid the player picks one square of is no canvas, even where it names the affordance its cells run: the
/// direction prompt is a dialog of its size and the advanced inventory sits where it does without the name; the
/// map editor, whose canvas the engine marks, fills the region.
#[test]
fn only_a_grid_the_engine_marks_as_a_canvas_fills_the_region() {
    let aim = example("aim", "advanced inventory screen");
    let mut named = aim.clone();
    for pane in named["root"]["children"].as_array_mut().unwrap() {
        pane["child"]["children"][0]["affordances"] = json!(["aim:set_area"]);
    }
    for size in SIZES {
        let (h, _) = playing(size, direction_prompt(), None);
        let window = window_of(&h, "direction");
        assert!(!fills(&h, window), "{size:?}: {window:?} fills the region");
        let [plain, named] = [&aim, &named].map(|screen| {
            let (h, _) = playing(size, screen.clone(), None);
            window_of(&h, "advanced_inventory")
        });
        assert_eq!(plain, named, "{size:?}");
        let editor = example("editor", "map editor with a drawn room and a new mapping");
        let (h, _) = playing(size, editor, None);
        let window = window_of(&h, "editor");
        assert!(
            fills(&h, window),
            "{size:?}: {window:?} does not fill the region"
        );
    }
}

/// The region grows when the sidebar narrows or the window widens; a full screen grows with it rather than
/// leaving a strip of map before the sidebar.
#[test]
fn a_full_screen_follows_the_map_region_when_it_grows() {
    let (mut h, _) = playing(SIZES[0], example("overmap", "overmap screen"), None);
    h.set_size(SIZES[1]);
    h.run_steps(4);
    crate::common::settle_windows(&mut h);
    let window = window_of(&h, "overmap");
    assert!(
        fills(&h, window),
        "{window:?} does not fill {:?}",
        region(&h)
    );
}

/// A window changes size only when the player resizes it: not when the app window, and so the map region, does.
#[test]
fn a_screen_keeps_its_size_when_the_map_region_grows() {
    let screen = || example("distractions", "distraction manager");
    let (mut h, _) = playing(SIZES[0], screen(), None);
    let before = window_of(&h, "distraction_manager");
    h.set_size(SIZES[1]);
    h.run_steps(4);
    crate::common::settle_windows(&mut h);
    let after = window_of(&h, "distraction_manager");
    assert_eq!(before.size(), after.size());
}

#[test]
fn a_dialog_first_opens_a_readable_column_wide() {
    for size in SIZES {
        let (h, _) = playing(size, example("tutorial", "wait menu screen"), None);
        let window = window_of(&h, "uilist");
        let room = region(&h).width() - 2.0 * MARGIN;
        assert!(
            window.width() <= 960.0_f32.min(0.6 * room) + 1.0,
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
    let labels = crate::common::legible::column_list_labels(&screen);
    let wrapped = crate::common::legible::wrapped_labels(&h.output().shapes, &labels);
    assert!(wrapped.is_empty(), "{wrapped:#?}");
}

/// Panes side by side have no words to wrap across them: a dialog holding them is as wide as they need, past the
/// reading column, which at 1280x800 wraps the NPC's replies word by word.
#[test]
fn a_dialog_holding_panes_keeps_each_reply_on_one_line() {
    let screen = example("npctalk", "NPC talk screen");
    let replies = screen["root"]["children"][1]["child"]["children"][0]["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["label"].as_str().unwrap().to_string())
        .collect::<Vec<_>>();
    assert!(replies.len() > 5);
    let (h, _) = playing(SIZES[0], screen, None);
    let wrapped = crate::common::legible::wrapped_labels(&h.output().shapes, &replies);
    assert!(wrapped.is_empty(), "{wrapped:#?}");
}

#[test]
fn a_side_window_keeps_every_footer_button_it_shows_inside_itself() {
    let footer = ["Confirm Choice", "Select all", "Filter", "Wear", "Wield"];
    for size in SIZES {
        let (h, _) = playing(size, example("pickup", "pickup screen"), None);
        let window = window_of(&h, "pickup");
        let shown = h
            .get_all_by_role(egui::accesskit::Role::Button)
            .filter(|b| {
                let label = b.accesskit_node().label();
                footer
                    .iter()
                    .any(|f| label.as_ref().is_some_and(|l| l.starts_with(f)))
            })
            .map(|b| b.rect())
            .collect::<Vec<_>>();
        assert!(!shown.is_empty(), "{size:?}: no footer button");
        for button in shown {
            assert!(
                window.contains_rect(button),
                "{size:?}: {button:?} cut by {window:?}"
            );
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
                json!({"jsonrpc": "2.0", "id": read["id"], "result": crate::common::panel_view(panel)}),
            );
        }
        answered = reads.len();
        h.run_steps(2);
    }
    crate::common::settle_windows(&mut h);
    h
}

#[test]
fn a_panel_window_restored_off_the_screen_is_moved_onto_it() {
    for size in SIZES {
        let file = crate::common::game_root("window-class-panel").join("settings.json");
        let windows = json!({"windows": {"panel:player_data": {"pos": [5000.0, 3000.0], "size": [500.0, 400.0],
            "collapsed": false}}});
        std::fs::write(&file, windows.to_string()).unwrap();
        let h = sheet_open(size, &file);
        let window = h
            .ctx
            .memory(|m| m.area_rect(egui::Id::new(("dialog", "panel:player_data"))))
            .expect("no sheet window");
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, size);
        assert!(
            screen.expand(0.5).contains_rect(window),
            "{size:?}: {window:?} outside {screen:?}"
        );
    }
}

#[test]
fn a_side_window_kept_narrow_by_a_last_run_opens_as_narrow() {
    let file = crate::common::game_root("window-class-narrow").join("settings.json");
    let windows = json!({"windows": {"screen:pickup": {"pos": [20.0, 50.0], "size": [175.0, 500.0],
        "collapsed": false}}});
    std::fs::write(&file, windows.to_string()).unwrap();
    let (h, _) = playing(SIZES[0], example("pickup", "pickup screen"), Some(&file));
    assert_eq!(window_of(&h, "pickup").size(), egui::vec2(175.0, 500.0));
}

/// Writes the direction prompt over the map at both sizes to `$SHOT_DIR/<width>x<height>/direction.png`.
#[test]
#[ignore = "needs a Vulkan adapter and SHOT_DIR"]
fn direction_shots() {
    for size in SIZES {
        let name = format!("{}x{}/direction.png", size.x, size.y);
        crate::common::shot_stack(crate::common::over_map(direction_prompt()), size, &name);
    }
}

/// The labels of the first list of `screen`, which is the one that has rows.
fn row_labels(screen: &Value) -> Vec<String> {
    fn find(v: &Value) -> Option<Vec<String>> {
        match v {
            Value::Object(o) => o
                .get("rows")
                .and_then(Value::as_array)
                .filter(|rows| !rows.is_empty())
                .map(|rows| {
                    rows.iter()
                        .filter_map(|r| r["label"].as_str().map(str::to_string))
                        .collect()
                })
                .or_else(|| o.values().find_map(find)),
            Value::Array(a) => a.iter().find_map(find),
            _ => None,
        }
    }
    find(screen).expect("a list")
}

/// `screen` with its first list given `n` rows, the last ones copies of the first with their own names.
fn with_rows(mut screen: Value, n: usize) -> Value {
    fn rows(v: &mut Value) -> Option<&mut Vec<Value>> {
        match v {
            Value::Object(o) => {
                if o.get("rows")
                    .and_then(Value::as_array)
                    .is_some_and(|r| !r.is_empty())
                {
                    return o.get_mut("rows").and_then(Value::as_array_mut);
                }
                o.values_mut().find_map(rows)
            }
            Value::Array(a) => a.iter_mut().find_map(rows),
            _ => None,
        }
    }
    let rows = rows(&mut screen).expect("a list");
    let first = rows[0].clone();
    let at = rows.len();
    rows.truncate(n);
    rows.extend((at..n).map(|i| {
        let mut row = first.clone();
        row["id"] = json!(format!("entry:extra{i}"));
        row["label"] = json!(format!("Extra row number {i}"));
        row
    }));
    screen
}

/// Every list window opens showing all its rows, or fills its room when the rows do not fit, at the sizes
/// of a client window and whether or not a short size was remembered by an earlier session for its kind.
fn first_height_misses(case: &str, screen: &Value, kind: &str, remembered: bool) -> Vec<String> {
    let last = row_labels(screen).pop().expect("rows");
    let mut misses = vec![];
    for size in HEIGHT_SIZES {
        let file = crate::common::game_root("window-first-height").join("settings.json");
        let windows = match remembered {
            true => json!({"windows": {format!("screen:{kind}"): {"pos": [60.0, 120.0],
                "size": [size.x * 0.6, 210.0], "collapsed": false}}}),
            false => json!({}),
        };
        std::fs::write(&file, windows.to_string()).unwrap();
        let (mut h, _) = playing(size, screen.clone(), Some(&file));
        h.run_steps(8);
        let window = window_of(&h, kind);
        let room = region(&h).shrink(MARGIN);
        let row = h
            .query_all_by_label_contains(&last)
            .last()
            .map(|n| n.rect());
        let shows = row.is_some_and(|r| window.contains_rect(r.shrink(1.0)));
        let fills = window.height() >= 0.9 * room.height() - 2.0;
        if !(shows || fills) {
            misses.push(format!(
                "{case} at {size:?}, remembered {remembered}: {window:?} in {room:?} does not show {last:?} ({row:?})"
            ));
        }
    }
    misses
}

#[test]
fn a_list_window_opens_tall_enough_for_its_rows_or_the_room() {
    let wait = example("tutorial", "wait menu screen");
    let cases = [
        ("wait menu", wait.clone(), "uilist"),
        ("20 rows", with_rows(wait.clone(), 20), "uilist"),
        ("26 rows", with_rows(wait.clone(), 26), "uilist"),
        ("60 rows", with_rows(wait, 60), "uilist"),
        ("spell menu", example("character", "spell menu"), "uilist"),
        (
            "filterable list",
            example("generic", "filterable list screen"),
            "uilist",
        ),
        ("help topics", example("info", "help topics"), "help"),
        ("message log", example("info", "message log"), "messages"),
        ("options", example("options", "options screen"), "options"),
    ];
    let misses = [false, true]
        .into_iter()
        .flat_map(|remembered| {
            cases.iter().flat_map(move |(case, screen, kind)| {
                first_height_misses(case, screen, kind, remembered)
            })
        })
        .collect::<Vec<_>>();
    assert!(misses.is_empty(), "{misses:#?}");
}

#[test]
fn a_remembered_height_the_player_left_readable_is_kept() {
    for size in HEIGHT_SIZES {
        let file = crate::common::game_root("window-kept-height").join("settings.json");
        let height = (0.8 * size.y).round();
        let windows = json!({"windows": {"screen:uilist": {"pos": [60.0, 40.0], "size": [500.0, height],
            "collapsed": false}}});
        std::fs::write(&file, windows.to_string()).unwrap();
        let (mut h, _) = playing(
            size,
            example("generic", "filterable list screen"),
            Some(&file),
        );
        h.run_steps(8);
        assert_eq!(window_of(&h, "uilist").height(), height, "{size:?}");
    }
}
