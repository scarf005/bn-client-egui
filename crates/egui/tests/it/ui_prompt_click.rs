//! The choices of a dialog that holds one list are answered by one click, as its buttons are; the rows of any other
//! screen are only focused by one, and used by a double click.
use crate::common::{fixture, session};
use bn_client_egui::transport::ScriptHandle;
use bn_client_egui::{App, EngineModel};
use eframe::egui;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use serde_json::{Value, json};

type H<'a> = Harness<'a, App<EngineModel>>;

/// The engine's example screens that hold a list with an `activate`, with their role.
fn screens() -> Vec<(String, Value)> {
    let dir = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../core/tests/fixtures/0.2.examples"
    );
    let mut found = vec![];
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        let doc = serde_json::from_str::<Value>(&std::fs::read_to_string(&path).unwrap()).unwrap();
        for ex in doc["valid"].as_array().into_iter().flatten() {
            let text = ex["value"]["root"].to_string();
            if ex["def"] == "screen"
                && text.contains(r#""activate":"#)
                && text.contains(r#""kind":"list""#)
            {
                found.push((
                    format!("{}: {}", path.display(), ex["name"]),
                    ex["value"].clone(),
                ));
            }
        }
    }
    found
}

fn showing<'a>(screen: Value) -> (H<'a>, ScriptHandle) {
    let (t, script) = session();
    let mut h = Harness::builder()
        .with_size(egui::vec2(1600.0, 1000.0))
        .build_ui_state(
            |ui, app: &mut App<EngineModel>| app.show(ui),
            App::new(EngineModel::new(Box::new(t))),
        );
    h.run_steps(3);
    let map = fixture("subscribe_result")["result"]["interaction"]["stack"][0].clone();
    script.push(json!({"jsonrpc": "2.0", "method": "bn.events", "params": {"epoch": "e7f3", "events": [{
        "sequence": "41", "revision": "32", "type": "interaction.changed", "changes": {"interaction": {
        "boundary_id": "b:60",
        "readiness": {"phase": "playing", "game_ready": true, "accepts_interaction_commands": true,
            "accepts_registered_actions": true},
        "actions": [], "interaction": null, "stack": [map, screen]}}}]}}));
    h.run_steps(8);
    (h, script)
}

/// The first list of `screen` and its first row that can be chosen: the list's `activate`, the row's id and label.
fn first_choice(screen: &Value) -> Option<(String, String, String, usize)> {
    fn lists<'a>(c: &'a Value, found: &mut Vec<&'a Value>) {
        if c["kind"] == "list" {
            found.push(c);
        }
        for child in c["children"]
            .as_array()
            .into_iter()
            .flatten()
            .chain(c["child"].as_array().into_iter().flatten())
        {
            lists(child, found);
        }
        if c["child"].is_object() {
            lists(&c["child"], found);
        }
    }
    let mut found = vec![];
    lists(&screen["root"], &mut found);
    let count = found.len();
    let offered = |id: &str| {
        screen["affordances"]
            .as_array()
            .is_some_and(|all| all.iter().any(|a| a["id"] == id && a["enabled"] == true))
    };
    let list = found
        .into_iter()
        .find(|l| l["activate"].as_str().is_some_and(offered))?;
    let row = list["rows"]
        .as_array()?
        .iter()
        .find(|r| r["enabled"] == true && r["selectable"] == true && r["header"] != true)?;
    Some((
        list["activate"].as_str()?.into(),
        row["id"].as_str()?.into(),
        row["label"].as_str()?.into(),
        count,
    ))
}

fn invoked(script: &ScriptHandle) -> Vec<(Value, Value)> {
    script
        .sent()
        .into_iter()
        .filter(|m| m["method"] == "bn.command.submit")
        .map(|m| {
            (
                m["params"]["operation"]["affordance_id"].clone(),
                m["params"]["operation"]["args"]["row"].clone(),
            )
        })
        .collect()
}

#[test]
fn one_click_on_the_one_list_of_a_dialog_runs_the_choice_and_one_on_another_screens_row_only_focuses_it()
 {
    let (mut dialogs, mut others) = (0, 0);
    let named = [
        "wait menu screen",
        "query popup screen",
        "character sheet",
        "armor layers screen",
    ];
    for (name, screen) in screens() {
        if !named.iter().any(|n| name.ends_with(&format!("\"{n}\""))) {
            continue;
        }
        let Some((activate, row, label, lists)) = first_choice(&screen) else {
            continue;
        };
        let (mut h, script) = showing(screen.clone());
        let Some(node) = h.query_all_by_label_contains(&label).next() else {
            continue;
        };
        let at = node.rect().center();
        h.hover_at(at);
        h.run_steps(2);
        h.event(egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        });
        h.run_steps(1);
        h.event(egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        });
        h.run_steps(3);
        let ran = invoked(&script).contains(&(json!(activate), json!(row)));
        match screen["role"] == "dialog" && lists == 1 {
            true => {
                dialogs += 1;
                assert!(
                    ran,
                    "{name}: a click on {label} of a dialog's one list ran nothing"
                );
            }
            false => {
                others += 1;
                assert!(
                    !ran,
                    "{name}: a click on {label} used the row of a screen with more lists, or none a dialog"
                );
            }
        }
    }
    assert!(
        dialogs > 0 && others > 0,
        "{dialogs} dialogs, {others} screens"
    );
}
