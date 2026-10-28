//! The engine's news on an editor (a toned text in its log) is shown where the player acted: under the form they
//! sent, else in the status strip; a form value the client refuses marks its field, with the refusal beside it.
use bn_client_egui::{App, EngineModel};
use eframe::egui;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use serde_json::{Value, json};

type H<'a> = Harness<'a, App<EngineModel>>;

const SIZES: [egui::Vec2; 2] = [egui::vec2(1290.0, 830.0), egui::vec2(1920.0, 1080.0)];
const NOTICE: &str = "The pasted text is not JSON.";

/// A map editor with nothing open: files on the canvas, history in the log, a form to stamp text and one with a
/// bounded number, and `notice` as the engine's news when given.
fn editor(notice: Option<&str>) -> Value {
    let rows = |id: &str, labels: &[&str]| {
        labels
            .iter()
            .enumerate()
            .map(|(n, l)| {
                json!({"id": format!("{id}:{n}"), "label": l, "description": "", "denial": "",
                "enabled": true, "selectable": true, "selected": false, "columns": []})
            })
            .collect::<Vec<_>>()
    };
    let files = rows("file", &["a.json", "b.json", "c.json"]);
    let steps = rows("step", &["Opened", "Drew a wall"]);
    let mut children = vec![
        json!({"id": "ed:files", "kind": "list", "label": "Files", "slot": "canvas", "columns": [],
            "rows": files, "total": 3, "selection": "single"}),
        json!({"id": "ed:history", "kind": "list", "label": "History", "slot": "log", "columns": [],
            "rows": steps, "total": 2, "selection": "single"}),
        json!({"id": "ed:stamp", "kind": "form", "label": "Stamp mapgen text", "slot": "inspector",
            "affordances": ["ed:stamp_set", "ed:stamp"], "submit": "ed:stamp",
            "fields": [{"id": "text", "label": "Mapgen to stamp", "value": "{ no", "type": "text"}]}),
        json!({"id": "ed:spawn", "kind": "form", "label": "Test spawn", "slot": "inspector",
            "affordances": ["ed:fuel"],
            "fields": [{"id": "fuel", "label": "Fuel %", "value": "50", "type": "integer", "min": 0, "max": 100}]}),
        json!({"id": "ed:document", "kind": "text", "slot": "status", "markup": "No file open"}),
    ];
    if let Some(notice) = notice {
        children.push(json!({"id": "ed:notice", "kind": "text", "slot": "log", "markup": notice, "tone": "bad"}));
    }
    json!({"screen_id": "screen:5", "role": "workspace", "kind": "editor", "title": "Map Editor",
        "root": {"id": "ed:body", "kind": "split", "relation": "peers", "children": children},
        "affordances": [
            {"id": "ed:stamp_set", "label": "Mapgen to stamp", "keys": [], "enabled": true, "expect": "boundary",
                "args": {"row": {"component_id": "ed:stamp"}, "text": {}}},
            {"id": "ed:stamp", "label": "Stamp text", "keys": [], "enabled": true, "expect": "screen"},
            {"id": "ed:fuel", "label": "Fuel %", "keys": [], "enabled": true, "expect": "boundary",
                "args": {"row": {"component_id": "ed:spawn"}, "text": {}}},
        ],
        "context": "EDITOR"})
}

fn interaction(stack: Value) -> Value {
    json!({"jsonrpc": "2.0", "method": "bn.events", "params": {"epoch": "e7f3", "events": [{
        "sequence": "42", "revision": "33", "type": "interaction.changed", "changes": {"interaction": {
        "boundary_id": "b:61",
        "readiness": {"phase": "playing", "game_ready": true, "accepts_interaction_commands": true,
            "accepts_registered_actions": true},
        "actions": [], "interaction": null, "stack": stack}}}]}})
}

fn shown<'a>(
    size: egui::Vec2,
    notice: Option<&str>,
) -> (H<'a>, bn_client_egui::transport::ScriptHandle) {
    let (app, script) = crate::common::staged(crate::common::over_map(editor(notice)));
    let mut h = crate::common::themed_harness(size, app);
    h.run_steps(7);
    crate::common::settle_windows(&mut h);
    (h, script)
}

fn rect(h: &H, label: &str) -> egui::Rect {
    h.get_by_label(label).rect()
}

#[test]
fn the_engines_news_with_no_form_sent_is_in_the_status_strip_not_over_the_history() {
    for size in SIZES {
        let (h, _) = shown(size, Some(NOTICE));
        let (notice, history, files) = (rect(&h, NOTICE), rect(&h, "History"), rect(&h, "a.json"));
        assert!(
            notice.top() > files.bottom() && notice.top() > history.bottom(),
            "{size:?}: news {notice:?} over History {history:?}"
        );
        let status = rect(&h, "No file open");
        assert!(
            (notice.bottom() - status.top()).abs() < 40.0,
            "{size:?}: news {notice:?} is not by the status line {status:?}"
        );
    }
}

#[test]
fn the_engines_news_after_a_form_is_sent_is_under_that_form() {
    for size in SIZES {
        let (mut h, script) = shown(size, None);
        h.get_by_label("Stamp text").click();
        h.run_steps(3);
        assert!(
            script
                .sent()
                .iter()
                .any(|m| m["params"]["operation"]["affordance_id"] == "ed:stamp")
        );
        let stack = crate::common::over_map(editor(Some(NOTICE)));
        script.push(crate::common::fixture("events_41_walk"));
        h.run_steps(4);
        script.push(interaction(stack));
        h.run_steps(6);
        let (notice, button, fuel) = (
            rect(&h, NOTICE),
            h.get_by_role_and_label(egui::accesskit::Role::Button, "Stamp text")
                .rect(),
            field(&h, "Fuel %"),
        );
        assert!(
            notice.top() >= button.bottom() - 1.0
                && notice.top() - button.bottom() < 60.0
                && notice.left() >= button.left() - 40.0,
            "{size:?}: news {notice:?} is not under Stamp text {button:?}"
        );
        assert!(
            notice.bottom() <= fuel.top() || notice.top() >= fuel.bottom(),
            "{size:?}"
        );
    }
}

/// The text box of the field labelled `label`.
fn field(h: &H, label: &str) -> egui::Rect {
    h.get_by_role_and_label(egui::accesskit::Role::TextInput, label)
        .rect()
}
