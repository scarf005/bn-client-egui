//! The engine's hints on an editor: an action's description as its tooltip and accessible description, a list's
//! empty text in place of its rows, a part that starts folded, and a field the form needs filled.
use crate::common::fixture;
use crate::ui_window_class::example;
use bn_client_egui::transport::ScriptHandle;
use bn_client_egui::{App, EngineModel};
use eframe::egui;
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use serde_json::{Value, json};

type H<'a> = Harness<'a, App<EngineModel>>;

const SIZES: [egui::Vec2; 2] = [egui::vec2(1290.0, 830.0), egui::vec2(1920.0, 1080.0)];
const WORLD_VEHICLE: &str = "vehicle editor editing a world vehicle in place";
const CHUNK_OPEN: &str = "map editor with a nested chunk open";
const NOTHING_OPEN: &str = "map editor with nothing open";

fn shown<'a>(screen: &Value, size: egui::Vec2) -> (H<'a>, ScriptHandle) {
    let (app, script) = crate::common::staged(crate::common::over_map(screen.clone()));
    let mut h = crate::common::themed_harness(size, app);
    h.run_steps(7);
    crate::common::settle_windows(&mut h);
    (h, script)
}

/// The engine sends `screen` again on top of the map, as its `n`th change.
fn resent(h: &mut H, script: &ScriptHandle, screen: &Value, n: u32) {
    let map = fixture("subscribe_result")["result"]["interaction"]["stack"][0].clone();
    script.push(json!({"jsonrpc": "2.0", "method": "bn.events", "params": {"epoch": "e7f3", "events": [{
        "sequence": (40 + n).to_string(), "revision": (31 + n).to_string(), "type": "interaction.changed",
        "changes": {"interaction": {"boundary_id": format!("b:{}", 60 + n),
            "readiness": {"phase": "playing", "game_ready": true, "accepts_interaction_commands": true,
                "accepts_registered_actions": true},
            "actions": [], "interaction": null, "stack": [map, screen]}}}]}}));
    h.run_steps(8);
}

fn affordance<'a>(screen: &'a Value, id: &str) -> &'a Value {
    screen["affordances"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["id"] == id)
        .unwrap()
}

fn says(h: &H, text: &str) -> bool {
    h.query_all_by_label(text).next().is_some()
}

/// Moves the pointer off every widget and lets a tooltip's delay pass.
fn pointer_away(h: &mut H) {
    h.hover_at(egui::pos2(1.0, 1.0));
    h.run_steps(40);
}

#[test]
fn a_button_shows_its_actions_description_on_hover_and_on_keyboard_focus_and_reads_it() {
    let screen = crate::common::unfolded(example("vehicle_editor", WORLD_VEHICLE));
    let description = affordance(&screen, "vehicle:copy")["description"]
        .as_str()
        .unwrap()
        .to_string();
    for size in SIZES {
        let (mut h, _) = shown(&screen, size);
        let button = h.get_by_role_and_label(egui::accesskit::Role::Button, "Copy vehicle");
        assert_eq!(
            button.accesskit_node().description().as_deref(),
            Some(description.as_str()),
            "{size:?}: the button reads the description"
        );
        assert!(!says(&h, &description), "{size:?}: no tooltip before");
        button.hover();
        h.run_steps(40);
        assert!(
            says(&h, &description),
            "{size:?}: the tooltip shows on hover"
        );
        pointer_away(&mut h);
        assert!(
            !says(&h, &description),
            "{size:?}: the tooltip leaves with the pointer"
        );
        h.get_by_role_and_label(egui::accesskit::Role::Button, "Copy vehicle")
            .focus();
        h.run_steps(4);
        assert!(
            says(&h, &description),
            "{size:?}: the tooltip shows on focus"
        );
    }
}

#[test]
fn a_menu_item_shows_its_actions_description_on_hover() {
    let screen = example("editor", NOTHING_OPEN);
    let description = affordance(&screen, "editor:open_here")["description"]
        .as_str()
        .unwrap()
        .to_string();
    let (mut h, _) = shown(&screen, SIZES[0]);
    h.get_by_label("World").click();
    h.run_steps(4);
    let item = h.get_by_role_and_label(egui::accesskit::Role::Button, "Open mapgen here");
    assert_eq!(
        item.accesskit_node().description().as_deref(),
        Some(description.as_str())
    );
    item.hover();
    h.run_steps(40);
    assert!(says(&h, &description));
}

#[test]
fn an_empty_list_shows_the_engines_empty_text_and_a_list_with_rows_does_not() {
    let screen = example("editor", CHUNK_OPEN);
    for size in SIZES {
        let (h, _) = shown(&screen, size);
        assert!(says(&h, "No problems."), "{size:?}: the empty list says so");
        assert!(
            !says(&h, "No edits yet."),
            "{size:?}: a list with rows shows them"
        );
    }
}

/// Scrolls the heading `label` into view and clicks it, as a player does.
fn toggle(h: &mut H, label: &str) {
    h.get_by_label(label).scroll_to_me();
    h.run_steps(4);
    h.get_by_label(label).click();
    h.run_steps(8);
}

fn folded(screen: &Value) -> Value {
    let mut screen = screen.clone();
    fn unfold(v: &mut Value) {
        match v {
            Value::Object(o) => {
                o.remove("collapsed");
                o.values_mut().for_each(unfold);
            }
            Value::Array(a) => a.iter_mut().for_each(unfold),
            _ => {}
        }
    }
    unfold(&mut screen);
    screen
}

#[test]
fn a_collapsed_part_starts_folded_to_its_heading_and_unfolds_on_a_click() {
    let screen = example("editor", CHUNK_OPEN);
    for size in SIZES {
        let (mut h, _) = shown(&screen, size);
        assert!(says(&h, "New mapgen file"), "{size:?}: the heading shows");
        assert!(
            !says(&h, "Overmap terrain"),
            "{size:?}: the folded part's fields do not"
        );
        toggle(&mut h, "New mapgen file");
        assert!(says(&h, "Overmap terrain"), "{size:?}: a click unfolds it");
    }
}

#[test]
fn the_players_fold_sticks_while_the_engine_sends_the_part_again() {
    let screen = example("editor", CHUNK_OPEN);
    let (mut h, script) = shown(&screen, SIZES[1]);
    toggle(&mut h, "New mapgen file");
    resent(&mut h, &script, &screen, 1);
    assert!(
        says(&h, "Overmap terrain"),
        "unfolded by the player, it stays"
    );
    toggle(&mut h, "New mapgen file");
    resent(&mut h, &script, &folded(&screen), 2);
    assert!(
        !says(&h, "Overmap terrain"),
        "folded by the player, it stays folded"
    );
}

#[test]
fn a_part_the_engine_stops_folding_unfolds() {
    let screen = example("editor", CHUNK_OPEN);
    let (mut h, script) = shown(&screen, SIZES[1]);
    resent(&mut h, &script, &folded(&screen), 1);
    assert!(says(&h, "Overmap terrain"));
}

/// The places a lone `*` is painted.
fn marks(h: &H) -> Vec<egui::Rect> {
    fn walk(shape: &egui::Shape, out: &mut Vec<egui::Rect>) {
        match shape {
            egui::Shape::Vec(v) => v.iter().for_each(|s| walk(s, out)),
            egui::Shape::Text(t) if t.galley.text() == "*" => {
                out.push(t.galley.rect.translate(t.pos.to_vec2()))
            }
            _ => {}
        }
    }
    let mut out = vec![];
    h.output()
        .shapes
        .iter()
        .for_each(|s| walk(&s.shape, &mut out));
    out
}

#[test]
fn a_required_field_is_marked_beside_its_label_and_read_as_required() {
    let screen = example("editor", NOTHING_OPEN);
    for size in SIZES {
        let (h, _) = shown(&screen, size);
        let marks = marks(&h);
        for (label, required) in [
            ("Path", true),
            ("Overmap terrain", true),
            ("Terrain", true),
            ("Path", false),
        ] {
            // The file form has a Path of its own, which it does not need.
            let fields = h
                .query_all_by_label(label)
                .filter(|n| n.accesskit_node().role() == egui::accesskit::Role::TextInput)
                .filter(|n| n.accesskit_node().is_required() == required)
                .map(|n| n.rect())
                .collect::<Vec<_>>();
            assert!(
                !fields.is_empty(),
                "{size:?}: no field {label} required {required}"
            );
            let marked = |row: &egui::Rect| {
                marks.iter().any(|m| {
                    m.center().y > row.top()
                        && m.center().y < row.bottom()
                        && m.right() <= row.left()
                })
            };
            assert!(
                fields.iter().all(|row| marked(row) == required),
                "{size:?}: {label} marked {marks:?}"
            );
        }
    }
}

/// The start page heads the canvas with the new mapgen form, its `New mod folder` button in reach beside the form's
/// fields, and a click on it runs the engine's affordance.
#[test]
fn the_start_page_puts_the_new_form_and_its_new_mod_folder_on_the_canvas() {
    let screen = example("editor", NOTHING_OPEN);
    for size in SIZES {
        let (mut h, script) = shown(&screen, size);
        // The right column, past the last handle, holds the inspector; the canvas is left of it.
        let column = h
            .get_all_by_label("Resize workspace column")
            .map(|n| n.rect().center().x)
            .fold(0.0, f32::max);
        let folder = h
            .get_by_role_and_label(egui::accesskit::Role::Button, "New mod folder")
            .rect();
        let path = h
            .query_all_by_label("Path")
            .find(|n| n.accesskit_node().role() == egui::accesskit::Role::TextInput)
            .expect("the path box")
            .rect();
        assert!(
            folder.right() < column && path.right() < column,
            "{size:?}: the folder button {folder:?} and the path {path:?} left of the column at {column}"
        );
        assert!(
            h.ctx.content_rect().contains_rect(folder),
            "{size:?}: {folder:?}"
        );
        h.get_by_role_and_label(egui::accesskit::Role::Button, "New mod folder")
            .click();
        h.run_steps(6);
        let sent = script
            .sent()
            .into_iter()
            .filter(|m| m["method"] == "bn.command.submit")
            .map(|m| m["params"]["operation"]["affordance_id"].clone())
            .collect::<Vec<_>>();
        assert_eq!(sent, [json!("editor:new_folder")], "{size:?}");
    }
}

/// A question for an action's number says what the number is: the action's description stands under its name, as a
/// form's field has its description, so the preview's seed is explained where it is typed.
#[test]
fn a_question_for_a_number_shows_the_actions_description_under_its_name() {
    let mut screen = example("editor", "map editor with a drawn room and a new mapping");
    let preview = screen["affordances"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|a| a["id"] == "editor:preview")
        .unwrap();
    preview["enabled"] = json!(true);
    preview.as_object_mut().unwrap().remove("denial");
    let description = preview["description"].as_str().unwrap().to_string();
    for size in SIZES {
        let (mut h, _) = shown(&screen, size);
        h.get_by_role_and_label(egui::accesskit::Role::Button, "View")
            .click();
        h.run_steps(4);
        h.get_by_role_and_label(egui::accesskit::Role::Button, "Preview")
            .click();
        h.run_steps(6);
        pointer_away(&mut h);
        assert!(
            h.query_by_label("OK").is_some(),
            "{size:?}: the question is open"
        );
        assert!(
            says(&h, &description),
            "{size:?}: the question says what its number is"
        );
    }
}
