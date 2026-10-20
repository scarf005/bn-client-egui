//! The screens every uilist, query popup and text field shares, drawn from the engine's generated
//! `generic.json` examples: the list filter and its prompt, the rows left by what is typed, switches, refused rows.
mod common;
use bn_client_egui::transport::{ScriptHandle, ScriptedTransport};
use bn_client_egui::{App, Clock, ConnectOptions, EngineModel};
use common::fixture;
use eframe::egui;
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use serde_json::{Value, json};

type H<'a> = Harness<'a, App<EngineModel>>;

/// The screen of the example `name` in `generic.json`.
fn example(name: &str) -> Value {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../core/tests/fixtures/0.2.examples/generic.json"
    );
    let all = serde_json::from_str::<Value>(&std::fs::read_to_string(path).unwrap()).unwrap();
    let found = all["valid"].as_array().unwrap().iter();
    found
        .into_iter()
        .find(|e| e["name"] == name)
        .unwrap_or_else(|| panic!("no example {name}"))["value"]
        .clone()
}

/// The boundary of a session whose only screen is `screen`, as revision `revision`.
fn boundary(screen: Value, revision: u32) -> Value {
    let mut boundary = fixture("subscribe_result")["result"]["interaction"].clone();
    boundary["stack"] = json!([screen]);
    boundary["boundary_id"] = format!("b:{revision}").into();
    boundary
}

fn harness<'a>(screen: Value) -> (H<'a>, ScriptHandle) {
    let mut subscribe = fixture("subscribe_result");
    subscribe["result"]["interaction"] = boundary(screen, 12);
    let reply = vec![
        subscribe,
        fixture("snapshot_part_0"),
        fixture("snapshot_part_1"),
    ];
    let (t, script) = ScriptedTransport::new();
    let t = t
        .on("bn.hello", vec![fixture("hello_result")])
        .on("bn.subscribe", reply.clone())
        .on("bn.subscribe", reply);
    let clock = Clock::manual();
    let model = EngineModel::open(Box::new(t), ConnectOptions::default()).with_clock(clock.clone());
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1100.0, 800.0))
        .build_ui_state(
            |ui, app: &mut App<EngineModel>| app.show(ui),
            App::new(model).with_clock(clock),
        );
    harness.run_steps(4);
    (harness, script)
}

fn operations(script: &ScriptHandle) -> Vec<Value> {
    script
        .sent()
        .into_iter()
        .filter(|m| m["method"] == "bn.command.submit")
        .map(|m| m["params"]["operation"].clone())
        .collect()
}

/// What was typed into the prompt, in the order it went out, with whether each was a submit.
fn typed(script: &ScriptHandle) -> Vec<(String, bool)> {
    operations(script)
        .iter()
        .map(|o| {
            (
                o["args"]["text"].as_str().unwrap().to_string(),
                o["affordance_id"] == "bn:submit:form:field",
            )
        })
        .collect()
}

/// The prompt of the example with the engine showing `value`, as revision `revision`.
fn prompt_event(value: &str, revision: u32) -> Value {
    let mut screen = example("filter prompt before typing");
    screen["root"]["children"][0]["fields"][0]["value"] = value.into();
    json!({"jsonrpc": "2.0", "method": "bn.events", "params": {"epoch": "e7f3", "events": [{
        "sequence": (revision + 9).to_string(), "revision": revision.to_string(),
        "type": "interaction.changed", "changes": {"interaction": boundary(screen, revision)}}]}})
}

/// The engine takes the `n`th command sent (from 1) and finishes it.
fn finish(script: &ScriptHandle, n: usize, command: &str) {
    let id = script
        .sent()
        .into_iter()
        .filter(|m| m["method"] == "bn.command.submit")
        .nth(n - 1)
        .map(|m| m["id"].as_u64().unwrap())
        .expect("the command was sent");
    script.push(json!({"jsonrpc":"2.0","id":id,"result":
        {"epoch":"e7f3","command_id":command,"stage":"received"}}));
    script.push(json!({"jsonrpc":"2.0","method":"bn.command","params":
        {"epoch":"e7f3","command_id":command,"stage":"completed"}}));
}

fn field<'a>(h: &'a H) -> egui_kittest::Node<'a> {
    h.get_by_role(egui::accesskit::Role::TextInput)
}

#[test]
fn a_filterable_list_offers_the_engines_filter_and_the_click_runs_it() {
    let (mut h, script) = harness(example("filterable list screen"));
    h.get_by_label("Teleport…");
    h.get_by_label("Filter [/]").click();
    h.run_steps(3);
    assert_eq!(
        operations(&script),
        [json!({"kind": "invoke", "affordance_id": "FILTER", "args": {}})]
    );
}

#[test]
fn a_list_that_is_not_filterable_offers_no_filter() {
    let (h, _) = harness(example("query popup screen"));
    h.get_by_label("Yes");
    assert!(h.query_by_label_contains("Filter").is_none());
}

#[test]
fn the_filter_prompt_shows_the_rows_the_text_leaves_and_they_are_not_choices() {
    let (mut h, script) = harness(example("filter prompt after typing"));
    h.get_by_label("Teleport…");
    assert!(h.query_by_label("Info…").is_none());
    h.get_by_label("Teleport…").click();
    h.run_steps(3);
    assert!(operations(&script).is_empty(), "{:?}", operations(&script));
}

#[test]
fn typing_in_the_prompt_fills_the_field_and_enter_submits_it() {
    let (mut h, script) = harness(example("filter prompt before typing"));
    field(&h).focus();
    h.run_steps(2);
    field(&h).type_text("tele");
    h.run_steps(3);
    assert_eq!(typed(&script).last(), Some(&("tele".to_string(), false)));
    finish(&script, 1, "c:5");
    h.run_steps(3);
    field(&h).focus();
    h.key_press(egui::Key::Enter);
    h.run_steps(3);
    assert_eq!(typed(&script).last(), Some(&("tele".to_string(), true)));
}

#[test]
fn the_prompt_has_the_typing_when_it_opens() {
    let (h, _) = harness(example("filter prompt before typing"));
    assert!(field(&h).accesskit_node().is_focused());
}

#[test]
fn the_prompt_keeps_what_is_typed_while_the_engine_echoes_an_older_value() {
    let (mut h, script) = harness(example("filter prompt before typing"));
    field(&h).focus();
    h.run_steps(2);
    field(&h).type_text("m");
    h.run_steps(2);
    field(&h).type_text("od");
    h.run_steps(2);
    // The first fill is done and the engine shows the text it had then, while "mod" waits to go out.
    script.push(prompt_event("m", 32));
    finish(&script, 1, "c:5");
    h.run_steps(3);
    field(&h).type_text("e");
    h.run_steps(2);
    script.push(prompt_event("mod", 33));
    finish(&script, 2, "c:6");
    h.run_steps(3);
    finish(&script, 3, "c:7");
    script.push(prompt_event("mode", 34));
    h.run_steps(3);
    let sent = typed(&script).into_iter().map(|t| t.0).collect::<Vec<_>>();
    assert_eq!(sent, ["m", "mod", "mode"]);
}

#[test]
fn a_value_the_prompt_did_not_send_replaces_its_text() {
    let (mut h, script) = harness(example("filter prompt before typing"));
    assert_eq!(field(&h).accesskit_node().value().as_deref(), Some(""));
    script.push(prompt_event("kept", 32));
    h.run_steps(3);
    assert_eq!(field(&h).accesskit_node().value().as_deref(), Some("kept"));
}

#[test]
fn a_composition_goes_out_once_committed_after_the_engine_echoes_an_older_value() {
    let (mut h, script) = harness(example("filter prompt before typing"));
    field(&h).focus();
    h.run_steps(2);
    field(&h).type_text("a");
    h.run_steps(2);
    field(&h).type_text("b");
    h.event(egui::Event::Ime(egui::ImeEvent::Preedit {
        text: "\u{314e}".into(),
        active_range_chars: None,
    }));
    h.run_steps(2);
    script.push(prompt_event("a", 32));
    finish(&script, 1, "c:5");
    h.run_steps(3);
    h.event(egui::Event::Ime(egui::ImeEvent::Preedit {
        text: "\u{d55c}".into(),
        active_range_chars: None,
    }));
    h.run_steps(2);
    h.event(egui::Event::Ime(egui::ImeEvent::Commit("\u{d55c}".into())));
    h.run_steps(3);
    finish(&script, 2, "c:6");
    h.run_steps(3);
    let sent = typed(&script).into_iter().map(|t| t.0).collect::<Vec<_>>();
    assert_eq!(sent, ["a", "ab\u{d55c}"], "no half-made syllable goes out");
}

#[test]
fn escape_in_the_prompt_cancels_it() {
    let (mut h, script) = harness(example("filter prompt before typing"));
    field(&h).focus();
    h.run_steps(2);
    h.key_press(egui::Key::Escape);
    h.run_steps(3);
    assert_eq!(
        operations(&script),
        [json!({"kind": "invoke", "affordance_id": "bn:cancel", "args": {}})]
    );
}

fn refused(id: &str, label: &str, denial: &str) -> Value {
    json!({"id": id, "label": label, "description": "", "denial": denial, "enabled": false,
        "selectable": true, "selected": false, "columns": []})
}

#[test]
fn hovering_a_refused_row_gives_the_engines_reason_and_clicking_it_sends_nothing() {
    let screen = json!({"screen_id": "screen:9", "role": "dialog", "title": "Options", "affordances": [],
        "root": {"id": "list", "kind": "list", "columns": [], "total": 2, "selection": "single",
            "rows": [refused("b", "Adjacent", "Needs auto pickup"), refused("c", "Weight limit", "Needs auto pickup")]}});
    let (mut h, script) = harness(screen);
    h.get_by_label("Weight limit").hover();
    h.run_steps(6);
    h.get_by_label("Needs auto pickup");
    h.get_by_label("Weight limit").click();
    h.run_steps(3);
    assert!(operations(&script).is_empty());
}

#[test]
fn tab_on_a_screen_that_takes_no_tab_moves_the_focus_to_a_button() {
    let (mut h, _) = harness(example("query popup screen"));
    h.ctx
        .memory_mut(|m| m.focused().map(|id| m.surrender_focus(id)));
    h.key_press(egui::Key::Tab);
    h.run_steps(4);
    let focused = h
        .get_all_by_role(egui::accesskit::Role::Button)
        .any(|b| b.accesskit_node().is_focused());
    assert!(focused);
}
