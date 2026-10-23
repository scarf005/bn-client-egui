//! The screens every uilist, query popup and text field shares, drawn from the engine's generated
//! `generic.json` examples: the list filter and its prompt, the rows left by what is typed, switches, refused rows.
use crate::common::fixture;
use bn_client_egui::transport::{ScriptHandle, ScriptedTransport};
use bn_client_egui::{App, Clock, ConnectOptions, EngineModel};
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
    harness_in(screen, egui::vec2(1100.0, 800.0))
}

fn harness_in<'a>(screen: Value, size: egui::Vec2) -> (H<'a>, ScriptHandle) {
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
    let mut harness = Harness::builder().with_size(size).build_ui_state(
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

/// The engine shows `screen` alone, as revision `revision`.
fn screen_event(screen: Value, revision: u32) -> Value {
    json!({"jsonrpc": "2.0", "method": "bn.events", "params": {"epoch": "e7f3", "events": [{
        "sequence": (revision + 9).to_string(), "revision": revision.to_string(),
        "type": "interaction.changed", "changes": {"interaction": boundary(screen, revision)}}]}})
}

/// The prompt of the example with the engine showing `value`, as revision `revision`.
fn prompt_event(value: &str, revision: u32) -> Value {
    let mut screen = example("filter prompt before typing");
    screen["root"]["children"][0]["fields"][0]["value"] = value.into();
    screen_event(screen, revision)
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

/// The invoke of the list's filter affordance that sets the text, as the search box sends it.
fn search_for(text: &str) -> Value {
    json!({"kind": "invoke", "affordance_id": "bn:filter:list:choices", "args": {"text": text}})
}

fn search<'a>(h: &'a H) -> egui_kittest::Node<'a> {
    h.get_by_role_and_label(egui::accesskit::Role::TextInput, "Search")
}

/// The example list with the engine's filter text `text`.
fn filtered(text: &str) -> Value {
    let mut screen = example("filterable list screen");
    screen["root"]["children"][1]["filter"] = text.into();
    screen
}

#[test]
fn a_filterable_list_has_a_search_box_and_no_filter_button() {
    let (h, _) = harness(example("filterable list screen"));
    h.get_by_label("Teleport…");
    assert!(
        search(&h)
            .accesskit_node()
            .value()
            .unwrap_or_default()
            .is_empty()
    );
    assert!(h.query_by_label("Filter").is_none());
}

#[test]
fn the_search_box_shows_the_filter_text_the_engine_holds() {
    let (h, _) = harness(filtered("tele"));
    assert_eq!(search(&h).accesskit_node().value().as_deref(), Some("tele"));
}

#[test]
fn typing_in_the_search_box_sends_each_text_as_a_filter_invoke_one_at_a_time_and_the_newest_wins() {
    let (mut h, script) = harness(example("filterable list screen"));
    search(&h).click();
    h.run_steps(2);
    search(&h).type_text("t");
    h.run_steps(2);
    search(&h).type_text("el");
    h.run_steps(3);
    // The engine is busy with the first text: nothing else goes out yet.
    assert_eq!(operations(&script), [search_for("t")]);
    finish(&script, 1, "c:5");
    h.run_steps(3);
    assert_eq!(operations(&script), [search_for("t"), search_for("tel")]);
}

#[test]
fn slash_and_ctrl_f_focus_the_search_box_and_the_slash_is_not_typed_into_it() {
    for press in ["/", "ctrl+f"] {
        let (mut h, script) = harness(example("filterable list screen"));
        assert!(!search(&h).accesskit_node().is_focused());
        match press {
            "/" => h.event(egui::Event::Text("/".into())),
            _ => h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::F),
        }
        h.run_steps(3);
        assert!(search(&h).accesskit_node().is_focused(), "{press}");
        assert!(
            search(&h)
                .accesskit_node()
                .value()
                .unwrap_or_default()
                .is_empty()
        );
        assert!(operations(&script).is_empty(), "{:?}", operations(&script));
    }
}

#[test]
fn keys_typed_in_the_search_box_run_no_game_action() {
    let (mut h, script) = harness(example("filterable list screen"));
    search(&h).click();
    h.run_steps(2);
    search(&h).type_text("q/");
    h.run_steps(3);
    assert!(
        operations(&script)
            .iter()
            .all(|op| op["affordance_id"] == "bn:filter:list:choices"),
        "{:?}",
        operations(&script)
    );
}

#[test]
fn escape_in_a_filled_search_box_clears_it_and_the_next_escape_leaves_it_without_closing_the_list()
{
    let (mut h, script) = harness(example("filterable list screen"));
    search(&h).click();
    h.run_steps(2);
    search(&h).type_text("tele");
    h.run_steps(3);
    finish(&script, 1, "c:5");
    h.run_steps(3);
    h.key_press(egui::Key::Escape);
    h.run_steps(3);
    assert_eq!(operations(&script).last(), Some(&search_for("")));
    assert!(search(&h).accesskit_node().is_focused());
    finish(&script, 2, "c:6");
    h.run_steps(3);
    h.key_press(egui::Key::Escape);
    h.run_steps(3);
    assert!(!search(&h).accesskit_node().is_focused());
    assert!(
        operations(&script)
            .iter()
            .all(|op| op["affordance_id"] != "bn:cancel"),
        "{:?}",
        operations(&script)
    );
    // With the focus on the list, Escape closes it as it always did.
    h.key_press(egui::Key::Escape);
    h.run_steps(3);
    assert!(
        operations(&script)
            .iter()
            .any(|op| op["affordance_id"] == "bn:cancel")
    );
}

#[test]
fn enter_and_down_in_the_search_box_leave_it_for_the_first_row_and_send_nothing() {
    for leave in [egui::Key::Enter, egui::Key::ArrowDown] {
        let (mut h, script) = harness(filtered("tele"));
        search(&h).click();
        h.run_steps(2);
        h.key_press(leave);
        h.run_steps(3);
        assert!(!search(&h).accesskit_node().is_focused(), "{leave:?}");
        assert!(operations(&script).is_empty(), "{:?}", operations(&script));
    }
}

#[test]
fn a_list_that_is_not_filterable_offers_no_filter() {
    let (h, _) = harness(example("query popup screen"));
    // The row and the registered action's button read the same.
    assert!(h.query_all_by_label("Yes").next().is_some());
    assert!(h.query_by_label_contains("Filter").is_none());
}

#[test]
fn the_filter_prompt_shows_the_rows_the_text_leaves() {
    let (h, _) = harness(example("filter prompt after typing"));
    h.get_by_label("Teleport…");
    assert!(h.query_by_label("Info…").is_none());
}

/// The player drags the window's bottom edge up by `by` points.
fn shrink_from_below(h: &mut H, by: f32) {
    let window = h.get_by_role(egui::accesskit::Role::Window).rect();
    let bottom = window.center_bottom() - egui::vec2(0.0, 2.0);
    for (at, pressed) in [(bottom, true), (bottom - egui::vec2(0.0, by), false)] {
        h.hover_at(at);
        h.run_steps(2);
        h.event(egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        });
        h.run_steps(2);
    }
}

#[test]
fn the_description_of_the_selected_row_shows_in_full_under_a_list_in_a_window_of_the_first_size() {
    let mut screen = example("filterable list screen");
    let text = (1..=8)
        .map(|n| format!("detail line {n}"))
        .collect::<Vec<_>>();
    // A long list, as the spawn menu's: more rows than its share of the window holds.
    let list = &mut screen["root"]["children"][1];
    list["rows"][0]["description"] = text.join("\n").into();
    let row = list["rows"][7].clone();
    let more = (0..22).map(|n| {
        let mut row = row.clone();
        row["id"] = format!("entry:{}", 100 + n).into();
        row["selected"] = false.into();
        row
    });
    list["rows"].as_array_mut().unwrap().extend(more);
    list["total"] = 30.into();
    let (mut h, _) = harness_in(screen, egui::vec2(800.0, 800.0));
    shrink_from_below(&mut h, 100.0);
    let first = h.get_by_label_contains("detail line 1").rect();
    let window = h.get_by_role(egui::accesskit::Role::Window).rect();
    // Room for at least five of the description's lines (a line is about 18 points) above the window's end.
    assert!(
        window.bottom() - first.top() >= 5.0 * 18.0,
        "{first:?} in {window:?}"
    );
}

#[test]
fn the_filter_prompt_has_no_buttons_for_the_keys_that_edit_its_text() {
    let (h, _) = harness(example("filter prompt before typing"));
    assert!(h.query_by_label_contains("Remove previous").is_none());
    assert!(h.query_by_label_contains("Remove current").is_none());
    assert!(h.query_by_label_contains("Clear text").is_none());
}

#[test]
fn the_filter_prompt_is_the_window_of_the_list_it_filters_with_its_box_above_the_results() {
    let (mut h, script) = harness(example("filterable list screen"));
    let window = |h: &H| h.get_by_role(egui::accesskit::Role::Window).rect();
    shrink_from_below(&mut h, 120.0);
    let before = window(&h);
    assert!(before.height() < 680.0, "{before:?}");
    let mut prompt = example("filter prompt after typing");
    prompt["screen_id"] = "screen:13".into();
    let mut stacked = boundary(example("filterable list screen"), 32);
    stacked["stack"].as_array_mut().unwrap().push(prompt);
    script.push(
        json!({"jsonrpc": "2.0", "method": "bn.events", "params": {"epoch": "e7f3", "events": [{
        "sequence": "41", "revision": "32", "type": "interaction.changed",
        "changes": {"interaction": stacked}}]}}),
    );
    h.run_steps(4);
    assert_eq!(
        window(&h),
        before,
        "typing a filter keeps the window as it was"
    );
    let result = h.get_by_label("Teleport…").rect();
    assert!(field(&h).rect().bottom() <= result.top());
}

#[test]
fn a_click_on_a_filter_result_confirms_the_text_and_then_chooses_that_row() {
    let (mut h, script) = harness(example("filter prompt after typing"));
    h.get_by_label("Teleport…").click();
    h.run_steps(3);
    assert_eq!(typed(&script), [("tele".to_string(), true)]);
    let mut list = example("filterable list screen");
    list["screen_id"] = "screen:13".into();
    script.push(screen_event(list, 32));
    finish(&script, 1, "c:5");
    h.run_steps(4);
    let ops = operations(&script);
    assert_eq!(
        ops.last().unwrap()["affordance_id"],
        "bn:choose:list:choices",
        "{ops:?}"
    );
    assert_eq!(ops.last().unwrap()["args"]["row"], "entry:4", "{ops:?}");
}

#[test]
fn a_click_on_a_result_of_a_prompt_whose_filter_matches_nothing_chooses_nothing() {
    let (mut h, script) = harness(example("filter prompt after typing"));
    h.get_by_label("Teleport…").click();
    h.run_steps(3);
    let mut list = example("filterable list screen");
    list["screen_id"] = "screen:13".into();
    list["root"]["children"][1]["rows"] = json!([]);
    script.push(screen_event(list, 32));
    finish(&script, 1, "c:5");
    h.run_steps(4);
    assert_eq!(operations(&script).len(), 1, "{:?}", operations(&script));
}

#[test]
fn a_filter_result_click_whose_prompt_gives_way_to_the_map_chooses_nothing_in_a_later_list() {
    let (mut h, script) = harness(example("filter prompt after typing"));
    h.get_by_label("Teleport…").click();
    h.run_steps(3);
    let map = json!({"screen_id": "screen:map", "kind": "map", "role": "map", "root": null,
        "affordances": []});
    script.push(screen_event(map, 32));
    finish(&script, 1, "c:5");
    h.run_steps(4);
    let mut list = example("filterable list screen");
    list["screen_id"] = "screen:13".into();
    script.push(screen_event(list, 33));
    h.run_steps(4);
    assert_eq!(operations(&script).len(), 1, "{:?}", operations(&script));
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

#[test]
fn escape_in_a_filled_prompt_empties_it_and_a_second_escape_cancels() {
    let (mut h, script) = harness(example("filter prompt before typing"));
    field(&h).focus();
    h.run_steps(2);
    field(&h).type_text("tele");
    h.run_steps(3);
    finish(&script, 1, "c:5");
    h.run_steps(3);
    h.key_press(egui::Key::Escape);
    h.run_steps(3);
    let ops = operations(&script);
    assert_eq!(ops.last().unwrap()["args"]["text"], "", "{ops:?}");
    assert!(
        !operations(&script)
            .iter()
            .any(|op| op["affordance_id"] == "bn:cancel"),
        "the first Escape only clears: {:?}",
        operations(&script)
    );
    finish(&script, 2, "c:6");
    h.run_steps(3);
    h.key_press(egui::Key::Escape);
    h.run_steps(3);
    assert!(
        operations(&script)
            .iter()
            .any(|op| op["affordance_id"] == "bn:cancel")
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
