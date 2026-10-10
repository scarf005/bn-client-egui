//! What a screen reader gets from the AccessKit tree: names, roles, selection and focus.
mod common;
use bn_client_egui::transport::ScriptHandle;
use bn_client_egui::{App, EngineModel};
use common::{fixture, session};
use eframe::egui::{self, accesskit};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use serde_json::json;

type H<'a> = Harness<'a, App<EngineModel>>;

fn harness<'a>() -> (H<'a>, ScriptHandle) {
    let (t, h) = session();
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
fn actions_are_plain_buttons_not_toggle_buttons() {
    let (mut h, _script) = harness();
    h.run_steps(3);
    let action = h.get_by_label("Move right (RIGHT, l)");
    assert_eq!(action.accesskit_node().role(), accesskit::Role::Button);
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
