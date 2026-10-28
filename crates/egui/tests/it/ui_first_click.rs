//! A press and release on a control act on the first try, also when the engine refreshes the screen between them
//! and every control sweeps once: tabs, rows and buttons of the engine's example screens.
use crate::ui_prompt_click::{first_choice, invoked, push_screen, screens, showing};
use bn_client_egui::protocol::{Affordance, AffordanceRole, Component, Screen};
use eframe::egui;
use egui_kittest::kittest::Queryable;
use serde_json::json;

fn button(h: &mut crate::ui_prompt_click::H, pos: egui::Pos2, pressed: bool) {
    h.event(egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    });
    h.run_steps(1);
}

#[test]
fn a_row_still_answers_when_the_engine_refreshes_the_screen_between_press_and_release() {
    let mut tried = 0;
    for (name, screen) in screens() {
        let Some((activate, row, label, 1)) = first_choice(&screen) else {
            continue;
        };
        let detail = screen["root"]
            .to_string()
            .contains(r#""relation":"detail""#);
        if detail
            || screen["root"]
                .to_string()
                .contains(r#""selection":"multi""#)
        {
            continue;
        }
        let (mut h, script) = showing(screen.clone());
        let Some(node) = h.query_all_by_label_contains(&label).next() else {
            continue;
        };
        let at = node.rect().center();
        h.hover_at(at);
        h.run_steps(2);
        button(&mut h, at, true);
        // The same screen again at a later revision: every widget is rebuilt under the held button.
        push_screen(&script, 42, screen.clone());
        h.run_steps(1);

        button(&mut h, at, false);
        h.run_steps(3);
        tried += 1;
        let ran = invoked(&script)
            .iter()
            .any(|(a, r)| *a == json!(activate) && *r == json!(row));
        assert!(
            ran,
            "{name}: a refresh between press and release lost the click on {row}: {:?}",
            invoked(&script)
        );
    }
    assert!(tried > 5, "{tried} screens");
}

/// The tabs of `c`, with the affordance that chooses one.
fn tabs<'a>(c: &'a Component, found: &mut Vec<(&'a bn_client_egui::protocol::Tabs, String)>) {
    if let Component::Tabs(t) = c {
        found.extend(t.select.clone().map(|s| (t, s)));
    }
    c.children().into_iter().for_each(|c| tabs(c, found));
}

#[test]
fn one_click_on_a_tab_or_a_button_of_any_example_screen_sends_its_affordance() {
    let mut clicked = 0;
    for (name, value) in screens() {
        let value = crate::common::unfolded(value);
        let screen = serde_json::from_value::<Screen>(value.clone()).unwrap();
        let mut found = vec![];
        tabs(screen.root.as_ref().unwrap(), &mut found);
        let mut wanted = vec![];
        // One of each kind per screen keeps the sweep short.
        let first_tab = found.iter().find_map(|(t, select)| {
            let tab = t.tabs.iter().find(|t| t.enabled && !t.selected)?;
            Some((tab.label.shown(), select.clone()))
        });
        wanted.extend(first_tab);
        let offered = |a: &&Affordance| {
            a.enabled
                && a.keys.is_empty()
                && matches!(
                    a.role,
                    Some(AffordanceRole::Cancel | AffordanceRole::Confirm)
                )
        };
        wanted.extend(
            screen
                .affordances
                .iter()
                .find(offered)
                .map(|a| (a.label.shown(), a.id.clone())),
        );
        for (label, id) in wanted {
            let (mut h, script) = showing(value.clone());
            let label = bn_client_egui::markup::plain(&label);
            let nodes = h.query_all_by_label(&label).collect::<Vec<_>>();
            let [node] = nodes.as_slice() else {
                continue;
            };
            let at = node.rect().center();
            h.hover_at(at);
            h.run_steps(2);
            button(&mut h, at, true);
            button(&mut h, at, false);
            h.run_steps(3);
            clicked += 1;
            let sent = invoked(&script).iter().any(|(a, _)| *a == json!(id));
            assert!(sent, "{name}: one click on {label} did not send {id}");
        }
    }
    assert!(clicked > 10, "{clicked} controls");
}
