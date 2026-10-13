#![recursion_limit = "512"]
//! What a screen reader gets from the AccessKit tree: names, roles, selection and focus.
mod common;
use bn_client_egui::transport::ScriptHandle;
use bn_client_egui::{App, EngineModel, GameModel};
use common::{fixture, session};
use eframe::egui::{self, accesskit};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use serde_json::json;

type H<'a> = Harness<'a, App<EngineModel>>;

fn harness<'a>() -> (H<'a>, ScriptHandle) {
    described(&["Wall", "A zombie is here."])
}

/// A harness whose engine answers every `bn.world.describe` with `lines`.
fn described<'a>(lines: &[&str]) -> (H<'a>, ScriptHandle) {
    let (t, h) = session();
    let t = t
        .on("bn.command.submit", vec![fixture("submit_result")])
        .on("bn.command.submit", vec![fixture("submit_result")])
        .on(
            "bn.world.describe",
            vec![json!({"jsonrpc":"2.0","id":0,"result":{"lines":lines}})],
        );
    let model = EngineModel::new(Box::new(t));
    let harness = Harness::builder()
        .with_size(egui::vec2(1100.0, 800.0))
        .build_ui_state(
            |ui, app: &mut App<EngineModel>| app.show(ui),
            App::new(model),
        );
    (harness, h)
}

fn open_menu(h: &mut H, script: &ScriptHandle) {
    h.run_steps(3);
    for name in ["events_41_walk", "events_42_message", "events_43_menu"] {
        script.push(fixture(name));
    }
    h.run_steps(3);
}

/// Replaces the open menu's choices with a main-menu-like tabbed list.
fn open_tabbed_menu(h: &mut H, script: &ScriptHandle) {
    h.run_steps(3);
    for name in ["events_41_walk", "events_42_message"] {
        script.push(fixture(name));
    }
    let mut menu = fixture("events_43_menu");
    let row = |id: &str, label: &str, selected: bool, pane: Option<&str>| {
        let mut c = json!({"id":id,"label":label,"description":"","denial":"","enabled":true,
            "selectable":true,"selected":selected,"columns":[]});
        if let Some(pane) = pane {
            c["pane_id"] = pane.into();
        }
        c
    };
    menu["params"]["events"][0]["changes"]["interaction"]["interaction"]["choices"] = json!([
        row("tab:new_game", "New Game", true, None),
        row("tab:settings", "Settings", false, None),
        row("new_game:tutorial", "Tutorial", false, Some("new_game")),
    ]);
    script.push(menu);
    h.run_steps(3);
}

fn focused_label(h: &H) -> Option<String> {
    h.root()
        .children_recursive()
        .find(|n| {
            n.accesskit_node().is_focused() && n.accesskit_node().role() != accesskit::Role::Window
        })
        .and_then(|n| n.accesskit_node().label())
}

#[test]
fn the_window_is_named_for_the_screen_reader() {
    let (mut h, _script) = harness();
    h.run_steps(3);
    assert_eq!(
        h.root().accesskit_node().label().as_deref(),
        Some("Cataclysm: Bright Nights client")
    );
}

#[test]
fn engine_menu_is_a_titled_dialog_whose_title_bar_is_named() {
    let (mut h, script) = harness();
    open_menu(&mut h, &script);
    let mut roles: Vec<_> = h
        .get_all_by_label("Pick up")
        .map(|n| n.accesskit_node().role())
        .collect();
    roles.sort_by_key(|r| format!("{r:?}"));
    assert!(roles.contains(&accesskit::Role::Dialog), "{roles:?}");
    assert!(roles.contains(&accesskit::Role::TitleBar), "{roles:?}");
    let unnamed: Vec<_> = h
        .root()
        .children_recursive()
        .filter(|n| {
            n.accesskit_node()
                .data()
                .supports_action(accesskit::Action::Focus)
        })
        .filter(|n| {
            let k = n.accesskit_node();
            k.label().is_none_or(|l| l.trim().is_empty())
                && !(k.role() == accesskit::Role::Label && k.value().is_some())
        })
        .map(|n| format!("{:?}", n.accesskit_node().data()))
        .collect();
    assert!(unnamed.is_empty(), "unnamed focusable nodes: {unnamed:?}");
}

#[test]
fn opening_a_menu_moves_focus_to_its_first_entry() {
    let (mut h, script) = harness();
    open_menu(&mut h, &script);
    assert_eq!(focused_label(&h).as_deref(), Some("combat knife"));
}

#[test]
fn opening_a_tabbed_menu_focuses_the_selected_tab() {
    let (mut h, script) = harness();
    open_tabbed_menu(&mut h, &script);
    assert_eq!(focused_label(&h).as_deref(), Some("New Game"));
}

#[test]
fn tabs_are_selectable_tabs_not_toggle_buttons() {
    let (mut h, script) = harness();
    open_tabbed_menu(&mut h, &script);
    let (on, off) = (h.get_by_label("New Game"), h.get_by_label("Settings"));
    assert_eq!(on.accesskit_node().role(), accesskit::Role::Tab);
    assert_eq!(off.accesskit_node().role(), accesskit::Role::Tab);
    assert_eq!(on.accesskit_node().is_selected(), Some(true));
    assert_eq!(off.accesskit_node().is_selected(), Some(false));
}

#[test]
fn palette_actions_are_menu_items_not_toggle_buttons() {
    let (mut h, _script) = harness();
    h.run_steps(3);
    h.event(egui::Event::Key {
        key: egui::Key::P,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::COMMAND,
    });
    h.run_steps(3);
    let action = h.get_by_label("Move right (RIGHT, l)");
    assert_eq!(action.accesskit_node().role(), accesskit::Role::MenuItem);
    assert_eq!(action.accesskit_node().toggled(), None);
}

#[test]
fn closing_the_menu_returns_focus_to_the_map() {
    let (mut h, script) = harness();
    open_menu(&mut h, &script);
    let mut closed = fixture("events_43_menu");
    let ev = &mut closed["params"]["events"][0];
    ev["sequence"] = "44".into();
    ev["revision"] = "34".into();
    ev["changes"]["interaction"]["interaction"] = serde_json::Value::Null;
    script.push(closed);
    h.run_steps(4);
    assert_eq!(focused_label(&h).as_deref(), Some("Map"));
}

fn map_description(h: &H) -> String {
    h.get_by_label("Map")
        .accesskit_node()
        .description()
        .unwrap_or_default()
}

#[test]
fn the_dialog_message_is_its_description() {
    let (mut h, script) = harness();
    open_menu(&mut h, &script);
    let described = h
        .get_all_by_label("Pick up")
        .find(|n| n.accesskit_node().role() == accesskit::Role::Dialog)
        .and_then(|n| n.accesskit_node().description());
    assert_eq!(described.as_deref(), Some("Choose an item"));
}

#[test]
fn a_menu_opened_as_another_closes_still_takes_focus() {
    let (mut h, script) = harness();
    open_menu(&mut h, &script);
    let mut closed = fixture("events_43_menu");
    let ev = &mut closed["params"]["events"][0];
    ev["sequence"] = "44".into();
    ev["revision"] = "34".into();
    ev["changes"]["interaction"]["interaction"] = serde_json::Value::Null;
    let mut next = fixture("events_43_menu");
    let ev = &mut next["params"]["events"][0];
    ev["sequence"] = "45".into();
    ev["revision"] = "35".into();
    ev["changes"]["interaction"]["interaction"]["title"] = "Other".into();
    script.push(closed);
    h.run_steps(1);
    script.push(next);
    h.run_steps(4);
    assert_eq!(focused_label(&h).as_deref(), Some("combat knife"));
}

#[test]
fn the_map_cursor_starts_on_the_player() {
    let (mut h, _script) = harness();
    h.run_steps(4);
    let desc = map_description(&h);
    let (player, rest) = desc.split_once(". Cursor ").expect(&desc);
    let at = player.strip_prefix("Player at ").expect(&desc);
    assert!(rest.starts_with(at), "{desc}");
}

#[test]
fn tab_and_shift_tab_stay_inside_an_open_menu() {
    let (mut h, script) = harness();
    open_menu(&mut h, &script);
    let mut seen = vec![focused_label(&h)];
    for _ in 0..3 {
        h.key_press(egui::Key::Tab);
        h.run_steps(3);
        seen.push(focused_label(&h));
    }
    let entry = Some("combat knife".to_string());
    let cancel = Some("Cancel".to_string());
    // The disabled "tin can" is skipped; the cycle is knife, Cancel, knife, Cancel.
    assert_eq!(seen, [entry.clone(), cancel.clone(), entry, cancel.clone()]);
    h.key_press_modifiers(egui::Modifiers::SHIFT, egui::Key::Tab);
    h.run_steps(3);
    assert_eq!(focused_label(&h).as_deref(), Some("combat knife"));
    h.key_press_modifiers(egui::Modifiers::SHIFT, egui::Key::Tab);
    h.run_steps(3);
    assert_eq!(focused_label(&h), cancel);
}

#[test]
fn item_rows_have_one_focusable_control_named_with_the_item() {
    use bn_client_egui::FakeModel;
    let mut h = Harness::builder()
        .with_size(egui::vec2(1100.0, 800.0))
        .build_ui_state(
            |ui, app: &mut App<FakeModel>| app.show(ui),
            App::new(FakeModel::default()),
        );
    h.run_steps(3);
    for item in ["Rusty knife", "Tin can", "Flashlight"] {
        let stops: Vec<_> = h
            .get_all_by_label_contains(item)
            .filter(|n| {
                n.accesskit_node()
                    .data()
                    .supports_action(accesskit::Action::Focus)
            })
            .map(|n| n.accesskit_node().label())
            .collect();
        assert_eq!(stops.len(), 1, "{item}: {stops:?}");
    }
}

fn log_line(script: &ScriptHandle, sequence: u32, id: &str, text: &str, count: u32) {
    let label = if count > 1 {
        format!("{text} x {count}")
    } else {
        text.to_string()
    };
    script.push(
        json!({"jsonrpc": "2.0", "method": "bn.events", "params": {"epoch": "e7f3",
        "events": [{"sequence": sequence.to_string(), "revision": "32", "type": "message.logged",
        "data": {"id": id, "text": text, "kind": "bad", "color": "c_red", "count": count, "label": label}}]}}),
    );
}

fn announced(h: &H) -> Vec<String> {
    h.root()
        .children_recursive()
        .filter(|n| n.accesskit_node().live() == accesskit::Live::Polite)
        .filter_map(|n| n.accesskit_node().value())
        .collect()
}

#[test]
fn messages_arriving_in_one_frame_are_announced_together_and_only_once() {
    let (mut h, script) = harness();
    h.run_steps(3);
    script.push(fixture("events_41_walk"));
    log_line(&script, 42, "7", "You open the door.", 1);
    log_line(&script, 43, "8", "Bang!", 1);
    log_line(&script, 44, "8", "Bang!", 2);
    h.run_steps(3);
    let live = announced(&h);
    assert!(
        live.iter().any(|v| v == "You open the door. Bang! x 2"),
        "{live:?}"
    );
    log_line(&script, 45, "9", "Crash!", 1);
    h.run_steps(3);
    let live = announced(&h);
    assert!(live.iter().any(|v| v == "Crash!"), "{live:?}");
    assert!(!live.iter().any(|v| v.contains("door")), "{live:?}");
}

#[test]
fn a_live_announcement_is_not_drawn_as_a_second_copy_of_the_message() {
    let (mut h, script) = harness();
    h.run_steps(3);
    script.push(fixture("events_41_walk"));
    log_line(&script, 42, "7", "You open the door.", 1);
    h.run_steps(3);
    let with_text = || {
        h.root()
            .children_recursive()
            .filter(|n| n.accesskit_node().value().as_deref() == Some("You open the door."))
            .map(|n| {
                let node = n.accesskit_node();
                (node.id(), (node.live(), node.raw_bounds().is_some()))
            })
            .collect::<std::collections::HashMap<_, _>>()
            .into_values()
            .collect::<Vec<_>>()
    };
    let nodes = with_text();
    let live = nodes.iter().filter(|(l, _)| *l == accesskit::Live::Polite);
    assert!(
        live.clone().count() == 1 && live.clone().all(|(_, drawn)| !drawn),
        "the announcement exists for screen readers and paints nothing: {nodes:?}"
    );
}

#[test]
fn stages_reached_within_one_frame_are_announced_together() {
    let (mut h, script) = harness();
    open_menu(&mut h, &script);
    h.get_by_label("combat knife").click();
    h.run_steps(2);
    for stage in ["executing", "completed"] {
        script.push(json!({"jsonrpc":"2.0","method":"bn.command","params":{
            "epoch":"e7f3","command_id":"c:5","stage":stage}}));
    }
    h.run_steps(1);
    let live = announced(&h);
    assert!(
        live.iter().any(|v| v == "combat knife: executing, done"),
        "{live:?}"
    );
}

fn open_palette(h: &mut H) {
    h.run_steps(3);
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::P);
    h.run_steps(3);
}

#[test]
fn the_action_palette_is_a_dialog_with_a_focused_search_and_menu_item_results() {
    let (mut h, _script) = harness();
    open_palette(&mut h);
    let dialog = h
        .get_all_by_label("Actions")
        .find(|n| n.accesskit_node().role() == accesskit::Role::Dialog);
    assert!(dialog.is_some(), "Actions dialog");
    assert_eq!(focused_label(&h).as_deref(), Some("Search actions"));
    let result = h.get_by_label("Move right (RIGHT, l)");
    assert_eq!(result.accesskit_node().role(), accesskit::Role::MenuItem);
}

#[test]
fn tab_walks_the_palette_results_and_wraps_instead_of_leaving() {
    let (mut h, _script) = harness();
    open_palette(&mut h);
    h.key_press(egui::Key::Tab);
    h.run_steps(3);
    let first = focused_label(&h);
    assert_ne!(first.as_deref(), Some("Search actions"));
    h.key_press_modifiers(egui::Modifiers::SHIFT, egui::Key::Tab);
    h.run_steps(3);
    assert_eq!(focused_label(&h).as_deref(), Some("Search actions"));
    h.key_press_modifiers(egui::Modifiers::SHIFT, egui::Key::Tab);
    h.run_steps(3);
    let last = focused_label(&h).expect("focus stays in the palette");
    assert!(
        h.get_all_by_label(&last).count() > 0 && last != "Search actions",
        "{last}"
    );
}

#[test]
fn closing_the_palette_returns_focus_to_the_map() {
    let (mut h, _script) = harness();
    open_palette(&mut h);
    h.key_press(egui::Key::Escape);
    h.run_steps(4);
    assert_eq!(focused_label(&h).as_deref(), Some("Map"));
}

#[test]
fn escape_closes_only_the_palette_opened_over_a_menu() {
    let (mut h, script) = harness();
    open_menu(&mut h, &script);
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::P);
    h.run_steps(3);
    h.key_press(egui::Key::Escape);
    h.run_steps(4);
    assert!(h.state().model.menu().is_some());
    assert!(
        script
            .sent()
            .iter()
            .all(|m| m["method"] != "bn.command.submit"),
        "no cancel was submitted"
    );
}

#[test]
fn tab_in_a_palette_opened_over_a_menu_stays_in_the_palette() {
    let (mut h, script) = harness();
    open_menu(&mut h, &script);
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::P);
    h.run_steps(3);
    for _ in 0..3 {
        h.key_press(egui::Key::Tab);
        h.run_steps(3);
        let at = focused_label(&h).expect("something is focused");
        assert!(
            at == "Search actions" || at.contains('(') || at.starts_with("Move"),
            "focus left the palette: {at}"
        );
    }
}

mod overmap {
    use super::*;
    use serde_json::Value;

    fn open<'a>() -> (H<'a>, ScriptHandle) {
        let (mut h, script) = harness();
        h.run_steps(3);
        let mut ev: Value = common::overmap_event(100);
        ev["params"]["events"][0]["changes"]["interaction"]["interaction"]["overmap"]["legend"] =
            json!([
                "│",
                "│# lake on the northwest",
                "│  outskirts of Bristol",
                "│",
                "│Use movement keys to pan."
            ]);
        script.push(ev);
        h.run_steps(3);
        (h, script)
    }

    #[test]
    fn the_cursor_status_reads_the_squares_description_and_the_view_lists_the_notes() {
        let (h, _script) = open();
        let said = "Overmap (100, 200, 0). lake on the northwest outskirts of Bristol.";
        let live = announced(&h);
        assert!(live.iter().any(|v| v.starts_with(said)), "{live:?}");
        let view = h.get_by_label("Overmap view").accesskit_node();
        let described = view.description().unwrap_or_default();
        assert!(described.starts_with(said), "{described}");
        assert!(
            described.contains("Notes: (99, 200, 0) cache."),
            "{described}"
        );
    }

    #[test]
    fn the_sidebar_frame_is_not_read_and_blank_rows_are_hidden() {
        let (h, _script) = open();
        let rows: Vec<_> = h
            .root()
            .children_recursive()
            .filter(|n| n.accesskit_node().role() == accesskit::Role::Label)
            .filter(|n| !n.accesskit_node().is_hidden())
            .filter_map(|n| n.accesskit_node().value())
            .filter(|v| v.contains('│') || v.trim().is_empty())
            .collect();
        assert!(rows.is_empty(), "frame or blank rows read aloud: {rows:?}");
        h.get_by_label("Use movement keys to pan.");
    }
}

#[test]
fn the_keyboard_cursor_square_is_described_by_the_engines_native_text() {
    let (mut h, script) = harness();
    h.run_steps(3);
    h.get_by_label("Map").focus();
    h.run_steps(6);
    let asked = script
        .sent()
        .iter()
        .filter(|m| m["method"] == "bn.world.describe")
        .count();
    assert_eq!(asked, 1, "the cursor square is asked for without a pointer");
    let described = h
        .get_by_label("Map")
        .accesskit_node()
        .description()
        .unwrap_or_default();
    assert!(
        described.contains("Wall A zombie is here."),
        "the map reads the native text: {described}"
    );
}

#[test]
fn shift_f10_on_the_map_is_the_right_click_on_the_cursor_square() {
    let (mut h, script) = harness();
    h.run_steps(4);
    h.get_by_label("Map").focus();
    h.run_steps(2);
    h.key_press_modifiers(egui::Modifiers::SHIFT, egui::Key::F10);
    h.run_steps(3);
    let contexts: Vec<_> = script
        .sent()
        .into_iter()
        .filter(|m| m["method"] == "bn.command.submit")
        .map(|m| m["params"]["operation"].clone())
        .filter(|o| o["kind"] == "context")
        .collect();
    assert_eq!(contexts.len(), 1, "{contexts:?}");
}

#[test]
fn blank_native_lines_and_a_final_period_do_not_garble_the_cursor_description() {
    let (mut h, _script) = described(&[" ", "Unseen."]);
    h.run_steps(3);
    h.get_by_label("Map").focus();
    h.run_steps(6);
    let said = h
        .get_by_label("Map")
        .accesskit_node()
        .description()
        .unwrap_or_default();
    assert!(said.ends_with(": Unseen."), "{said}");
}
