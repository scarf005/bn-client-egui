//! Input handling of the interaction core: gating invokes by their rows, collecting arguments, and keys.

use bn_client_core::interaction::{ArgKind, Interaction, Reason, Refusal, gate};
use bn_client_core::protocol::{Affordance, InvokeArgs, Screen, Text};
use serde_json::{Value, json};

fn row(id: &str, extra: Value) -> Value {
    let mut row = json!({"id": id, "label": id, "description": "", "denial": "", "enabled": true,
        "selectable": true, "selected": false, "columns": []});
    row.as_object_mut()
        .unwrap()
        .extend(extra.as_object().unwrap().clone());
    row
}

fn screen_with(root: Value, affordances: Value) -> Screen {
    serde_json::from_value(
        json!({"screen_id": "screen:1", "role": "screen", "root": root,
        "affordances": affordances}),
    )
    .unwrap()
}

fn list(rows: Vec<Value>) -> Value {
    json!({"id": "list", "kind": "list", "columns": [], "selection": "single",
        "total": rows.len(), "rows": rows, "activate": "pick"})
}

fn pick() -> Value {
    json!({"id": "pick", "label": "Pick", "keys": [], "enabled": true, "expect": "boundary",
        "args": {"row": {"component_id": "list"}}})
}

fn on_row(id: &str) -> InvokeArgs {
    InvokeArgs {
        row: Some(id.into()),
        ..InvokeArgs::default()
    }
}

fn the(screen: &Screen, id: &str) -> Affordance {
    screen
        .affordances
        .iter()
        .find(|a| a.id == id)
        .unwrap()
        .clone()
}

fn refused(reason: Reason, denial: Option<&str>) -> Result<(), Refusal> {
    Err(Refusal {
        reason,
        denial: denial.map(|d| Text::Plain(d.into())),
    })
}

#[test]
fn a_disabled_row_is_not_invoked_and_carries_the_engines_denial() {
    let screen = screen_with(
        list(vec![
            row("a", json!({})),
            row("b", json!({"enabled": false, "denial": "Too heavy"})),
            row("c", json!({"enabled": false})),
        ]),
        json!([pick()]),
    );
    let pick = the(&screen, "pick");
    assert_eq!(gate(&screen, &pick, &on_row("a")), Ok(()));
    assert_eq!(
        gate(&screen, &pick, &on_row("b")),
        refused(Reason::RowDisabled, Some("Too heavy"))
    );
    assert_eq!(
        gate(&screen, &pick, &on_row("c")),
        refused(Reason::RowDisabled, None),
        "an empty denial is no denial"
    );
}

#[test]
fn a_row_allows_only_the_affordances_it_lists_and_those_enabled() {
    let screen = screen_with(
        list(vec![row(
            "a",
            json!({"affordances": [{"id": "pick", "enabled": false, "denial": "Not here"}]}),
        )]),
        json!([
            pick(),
            {"id": "other", "label": "Other", "keys": [], "enabled": true, "expect": "boundary",
                "args": {"row": {"component_id": "list"}}}
        ]),
    );
    assert_eq!(
        gate(&screen, &the(&screen, "pick"), &on_row("a")),
        refused(Reason::RowDisabled, Some("Not here"))
    );
    assert_eq!(
        gate(&screen, &the(&screen, "other"), &on_row("a")),
        refused(Reason::NotOnThisRow, None)
    );
}

#[test]
fn a_disabled_affordance_is_refused_with_its_denial_before_any_row() {
    let screen = screen_with(
        list(vec![row("a", json!({}))]),
        json!([{"id": "pick", "label": "Pick", "keys": [], "enabled": false, "denial": "Later",
            "expect": "boundary", "args": {"row": {"component_id": "list"}}}]),
    );
    assert_eq!(
        gate(&screen, &the(&screen, "pick"), &on_row("a")),
        refused(Reason::Disabled, Some("Later"))
    );
}

#[test]
fn a_cell_and_a_tab_gate_like_a_row() {
    let grid = json!({"id": "grid", "kind": "grid", "cols": 1, "rows": 1, "cells": [
        {"x": 0, "y": 0, "id": "cell", "enabled": false, "denial": "Blocked"}]});
    let screen = screen_with(
        grid,
        json!([{"id": "take", "label": "Take", "keys": [], "enabled": true, "expect": "boundary",
            "args": {"row": {"component_id": "grid"}}}]),
    );
    assert_eq!(
        gate(&screen, &the(&screen, "take"), &on_row("cell")),
        refused(Reason::RowDisabled, Some("Blocked"))
    );
    let tabs = json!({"id": "tabs", "kind": "tabs", "tabs": [
        {"id": "t", "label": "T", "selected": false, "enabled": false}]});
    let screen = screen_with(
        tabs,
        json!([{"id": "go", "label": "Go", "keys": [], "enabled": true, "expect": "boundary",
            "args": {"row": {"component_id": "tabs"}}}]),
    );
    assert_eq!(
        gate(&screen, &the(&screen, "go"), &on_row("t")),
        refused(Reason::RowDisabled, None)
    );
}

#[test]
fn a_count_starts_at_the_default_and_is_asked_for_before_anything_is_sent() {
    let screen = screen_with(
        list(vec![row("a", json!({}))]),
        json!([{"id": "batch", "label": "Batch", "keys": [], "enabled": true, "expect": "boundary",
            "args": {"row": {"component_id": "list"},
                "count": {"min": 1, "max": 50, "default": 3}}}]),
    );
    let batch = the(&screen, "batch");
    let mut ui = Interaction::default();
    ui.focus_row("list", "a");
    assert_eq!(
        ui.begin(&batch),
        [ArgKind::Count],
        "typing digits must not find the command already sent"
    );
    assert_eq!(ui.args(&batch).unwrap().count, Some(3));
}

// Keys, clicks and the ask.

use bn_client_core::interaction::{Outcome, form_fill};
use bn_client_core::protocol::{Component, List, Row, WorldPos};

fn no_page(list: &List) -> Vec<&Row> {
    list.rows.iter().collect()
}

fn press(ui: &mut Interaction, screen: &Screen, key: &str) -> Outcome {
    ui.press(screen, key, 3, &no_page)
}

fn invoked(outcome: &Outcome) -> Option<(&str, Value)> {
    outcome
        .invoke
        .as_ref()
        .map(|(id, args)| (id.as_str(), serde_json::to_value(args).unwrap()))
}

fn plain(id: &str, keys: &[&str]) -> Value {
    json!({"id": id, "label": id, "keys": keys, "enabled": true, "expect": "screen"})
}

fn menu(rows: &[&str], affordances: Vec<Value>) -> Screen {
    let rows = rows.iter().map(|id| row(id, json!({}))).collect();
    screen_with(list(rows), json!(affordances))
}

#[test]
fn a_bound_key_runs_its_affordance_and_a_disabled_one_says_why() {
    let screen = menu(
        &["a"],
        vec![
            plain("go", &["g"]),
            json!({"id": "off", "label": "Off", "keys": ["o"], "enabled": false,
                "denial": "Later", "expect": "screen"}),
        ],
    );
    let mut ui = Interaction::default();
    let out = press(&mut ui, &screen, "g");
    assert_eq!(invoked(&out), Some(("go", json!({}))));
    assert!(out.consumed);
    let out = press(&mut ui, &screen, "o");
    assert_eq!(out.invoke, None);
    assert_eq!(out.denial, Some(Text::Plain("Later".into())));
    assert_eq!(out.reason, Some(Reason::Disabled));
    assert!(out.consumed);
}

#[test]
fn escape_and_return_go_through_roles_and_the_lists_activate_only() {
    let mut ui = Interaction::default();
    let bare = menu(&["a"], vec![]);
    assert!(
        !press(&mut ui, &bare, "ESC").consumed,
        "no cancel affordance: the key is the renderer's"
    );
    let screen = menu(
        &["a", "b"],
        vec![
            json!({"id": "back", "label": "Back", "keys": [], "enabled": true,
                "role": "cancel", "expect": "screen"}),
            pick(),
        ],
    );
    assert_eq!(
        invoked(&press(&mut ui, &screen, "ESC")),
        Some(("back", json!({})))
    );
    assert!(
        !press(&mut ui, &screen, "RETURN").consumed,
        "nothing is focused to activate"
    );
    ui.focus_row("list", "b");
    assert_eq!(
        invoked(&press(&mut ui, &screen, "RETURN")),
        Some(("pick", json!({"row": "b"})))
    );
}

#[test]
fn a_key_the_engine_binds_outranks_the_clients_own_navigation() {
    let screen = menu(&["a", "b"], vec![plain("up", &["UP"])]);
    let mut ui = Interaction::default();
    ui.focus_row("list", "b");
    let out = press(&mut ui, &screen, "UP");
    assert_eq!(invoked(&out), Some(("up", json!({}))));
    assert_eq!(ui.focus().row_id.as_deref(), Some("b"), "the focus stayed");
}

#[test]
fn arrows_and_page_keys_move_the_focus_and_report_the_row_to_reveal() {
    let screen = menu(&["a", "b", "c", "d", "e"], vec![]);
    let mut ui = Interaction::default();
    let out = press(&mut ui, &screen, "DOWN");
    assert_eq!((out.consumed, out.revealed.as_deref()), (true, Some("a")));
    press(&mut ui, &screen, "DOWN");
    assert_eq!(ui.focus().row_id.as_deref(), Some("b"));
    assert_eq!(
        press(&mut ui, &screen, "NPAGE").revealed.as_deref(),
        Some("e"),
        "a page of 3 from b stops at the end"
    );
    assert_eq!(
        press(&mut ui, &screen, "HOME").revealed.as_deref(),
        Some("a")
    );
    assert_eq!(
        press(&mut ui, &screen, "END").revealed.as_deref(),
        Some("e")
    );
    press(&mut ui, &screen, "PPAGE");
    assert_eq!(ui.focus().row_id.as_deref(), Some("b"));
    assert!(!press(&mut ui, &screen, "x").consumed);
}

#[test]
fn a_row_hotkey_chooses_the_row_and_a_disabled_one_is_refused_with_its_denial() {
    let rows = vec![
        row("a", json!({"hotkey": "a"})),
        row(
            "b",
            json!({"hotkey": "b", "enabled": false, "denial": "No room"}),
        ),
    ];
    let screen = screen_with(list(rows), json!([pick()]));
    let mut ui = Interaction::default();
    let out = press(&mut ui, &screen, "a");
    assert_eq!(invoked(&out), Some(("pick", json!({"row": "a"}))));
    assert_eq!(out.revealed.as_deref(), Some("a"));
    let out = press(&mut ui, &screen, "b");
    assert_eq!(out.invoke, None);
    assert_eq!(out.denial, Some(Text::Plain("No room".into())));
    assert_eq!(out.reason, Some(Reason::RowDisabled));
}

#[test]
fn any_key_runs_when_nothing_else_takes_the_key() {
    let screen = screen_with(
        json!({"id": "t", "kind": "text", "markup": "Press"}),
        json!([{"id": "next", "label": "Next", "keys": [], "enabled": true, "role": "any_key",
            "expect": "screen"}]),
    );
    let mut ui = Interaction::default();
    assert_eq!(
        invoked(&press(&mut ui, &screen, "SPACE")),
        Some(("next", json!({})))
    );
}

#[test]
fn tab_cycles_the_components_that_hold_rows() {
    let two = json!({"id": "root", "kind": "split", "relation": "peers", "children": [
        {"id": "one", "kind": "list", "columns": [], "selection": "single", "total": 1,
            "rows": [row("a", json!({}))]},
        {"id": "two", "kind": "list", "columns": [], "selection": "single", "total": 1,
            "rows": [row("b", json!({}))]}]});
    let screen = screen_with(two, json!([]));
    let mut ui = Interaction::default();
    let first = press(&mut ui, &screen, "TAB");
    assert_eq!(first.revealed.as_deref(), Some("a"));
    assert_eq!(ui.focus().component_id.as_deref(), Some("one"));
    press(&mut ui, &screen, "TAB");
    assert_eq!(ui.focus().component_id.as_deref(), Some("two"));
    press(&mut ui, &screen, "BACKTAB");
    assert_eq!(ui.focus().component_id.as_deref(), Some("one"));
    let alone = menu(&["a"], vec![]);
    let mut ui = Interaction::default();
    assert!(press(&mut ui, &alone, "TAB").consumed);
    assert!(
        !press(&mut ui, &alone, "TAB").consumed,
        "nowhere else to go"
    );
}

#[test]
fn left_and_right_switch_to_the_next_enabled_tab() {
    let tabs = json!({"id": "tabs", "kind": "tabs", "select": "goto", "tabs": [
        {"id": "t1", "label": "1", "selected": true, "enabled": true},
        {"id": "t2", "label": "2", "selected": false, "enabled": false},
        {"id": "t3", "label": "3", "selected": false, "enabled": true}]});
    let screen = screen_with(
        tabs,
        json!([{"id": "goto", "label": "Go", "keys": [], "enabled": true, "expect": "boundary",
            "args": {"row": {"component_id": "tabs"}}}]),
    );
    let mut ui = Interaction::default();
    assert_eq!(
        invoked(&press(&mut ui, &screen, "RIGHT")),
        Some(("goto", json!({"row": "t3"}))),
        "the disabled tab is skipped"
    );
    let out = press(&mut ui, &screen, "LEFT");
    assert_eq!(
        invoked(&out),
        None,
        "t1 is the selected one: nothing to the left"
    );
    assert!(out.consumed);
}

#[test]
fn the_grid_cursor_moves_to_the_neighbouring_cell() {
    let grid = json!({"id": "grid", "kind": "grid", "cols": 2, "rows": 2, "cursor": {"x": 0, "y": 0},
        "cells": [
            {"x": 0, "y": 0, "id": "c00", "enabled": true},
            {"x": 1, "y": 0, "id": "c10", "enabled": true},
            {"x": 0, "y": 1, "id": "c01", "enabled": true}]});
    let screen = screen_with(grid, json!([]));
    let mut ui = Interaction::default();
    assert_eq!(
        press(&mut ui, &screen, "RIGHT").revealed.as_deref(),
        Some("c00")
    );
    assert_eq!(
        press(&mut ui, &screen, "RIGHT").revealed.as_deref(),
        Some("c10")
    );
    assert_eq!(
        press(&mut ui, &screen, "RIGHT").revealed,
        None,
        "no cell there"
    );
    assert_eq!(ui.focus().row_id.as_deref(), Some("c10"));
    press(&mut ui, &screen, "LEFT");
    assert_eq!(
        press(&mut ui, &screen, "DOWN").revealed.as_deref(),
        Some("c01")
    );
}

#[test]
fn a_count_is_asked_typed_and_sent_on_return() {
    let screen = menu(
        &["a"],
        vec![
            json!({"id": "batch", "label": "Batch", "keys": ["b"], "enabled": true,
            "expect": "boundary", "args": {"row": {"component_id": "list"},
                "count": {"min": 1, "max": 50, "default": 3}}}),
        ],
    );
    let mut ui = Interaction::default();
    ui.focus_row("list", "a");
    let out = press(&mut ui, &screen, "b");
    assert!(out.consumed && out.invoke.is_none(), "asked, not sent");
    let ask = ui.ask().unwrap();
    assert_eq!((ask.affordance_id.as_str(), ask.count), ("batch", Some(3)));
    press(&mut ui, &screen, "7");
    assert_eq!(
        ui.ask().unwrap().count,
        Some(7),
        "the first digit replaces the default"
    );
    press(&mut ui, &screen, "5");
    assert_eq!(ui.ask().unwrap().count, Some(50), "held to the maximum");
    press(&mut ui, &screen, "BACKSPACE");
    let out = press(&mut ui, &screen, "RETURN");
    assert_eq!(
        invoked(&out),
        Some(("batch", json!({"row": "a", "count": 7})))
    );
    assert!(ui.ask().is_none());
}

#[test]
fn escape_ends_an_ask_and_a_key_the_ask_does_not_use_is_still_its_own() {
    let screen = menu(
        &["a"],
        vec![
            json!({"id": "note", "label": "Note", "keys": ["n"], "enabled": true,
                "expect": "boundary", "args": {"text": {}}}),
            plain("go", &["g"]),
        ],
    );
    let mut ui = Interaction::default();
    press(&mut ui, &screen, "n");
    press(&mut ui, &screen, "g");
    press(&mut ui, &screen, "SPACE");
    press(&mut ui, &screen, "BACKSPACE");
    press(&mut ui, &screen, "i");
    assert_eq!(
        ui.ask().unwrap().text.as_deref(),
        Some("gi"),
        "while asking, g is text, not the affordance bound to it"
    );
    assert!(press(&mut ui, &screen, "ESC").consumed);
    assert!(ui.ask().is_none());
    assert_eq!(
        invoked(&press(&mut ui, &screen, "g")),
        Some(("go", json!({})))
    );
}

fn form(fields: Value, affordances: Value) -> Screen {
    screen_with(
        json!({"id": "form", "kind": "form", "fields": fields,
            "affordances": ["set"]}),
        affordances,
    )
}

fn set() -> Value {
    json!({"id": "set", "label": "Set", "keys": [], "enabled": true, "expect": "boundary",
        "args": {"row": {"component_id": "form"}, "text": {}}})
}

#[test]
fn a_text_field_starts_from_its_value_and_a_number_field_takes_only_numbers() {
    let screen = form(
        json!([
            {"id": "name", "label": "Name", "value": "Ann", "type": "text", "max_length": 4},
            {"id": "age", "label": "Age", "value": "7", "type": "integer"}]),
        json!([set()]),
    );
    let mut ui = Interaction::default();
    ui.focus_row("form", "name");
    ui.activate(&screen, "set");
    assert_eq!(ui.ask().unwrap().text.as_deref(), Some("Ann"));
    press(&mut ui, &screen, "n");
    press(&mut ui, &screen, "a");
    assert_eq!(
        ui.ask().unwrap().text.as_deref(),
        Some("Annn"),
        "four at most"
    );
    let out = press(&mut ui, &screen, "RETURN");
    assert_eq!(
        invoked(&out),
        Some(("set", json!({"row": "name", "text": "Annn"})))
    );
    ui.focus_row("form", "age");
    ui.activate(&screen, "set");
    press(&mut ui, &screen, "x");
    press(&mut ui, &screen, "2");
    assert_eq!(ui.ask().unwrap().text.as_deref(), Some("72"));
}

#[test]
fn a_select_field_moves_among_its_options_and_a_key_field_takes_the_next_key() {
    let screen = form(
        json!([
            {"id": "mode", "label": "Mode", "value": "b", "type": "select",
                "options": [{"id": "a", "label": "A"}, {"id": "b", "label": "B"},
                    {"id": "c", "label": "C"}]},
            {"id": "bind", "label": "Bind", "value": "x", "type": "key"}]),
        json!([set()]),
    );
    let mut ui = Interaction::default();
    ui.focus_row("form", "mode");
    ui.activate(&screen, "set");
    assert_eq!(ui.ask().unwrap().select.as_ref().map(|s| s.at), Some(1));
    press(&mut ui, &screen, "DOWN");
    press(&mut ui, &screen, "DOWN");
    assert_eq!(
        ui.ask().unwrap().select.as_ref().map(|s| s.at),
        Some(2),
        "stops at the end"
    );
    assert_eq!(
        invoked(&press(&mut ui, &screen, "RETURN")),
        Some(("set", json!({"row": "mode", "text": "c"})))
    );
    ui.focus_row("form", "bind");
    ui.activate(&screen, "set");
    assert!(ui.ask().unwrap().capture);
    assert_eq!(
        invoked(&press(&mut ui, &screen, "F5")),
        Some(("set", json!({"row": "bind", "text": "F5"}))),
        "the key pressed is the answer"
    );
}

#[test]
fn a_form_field_is_filled_by_the_affordance_that_names_it() {
    let screen = form(
        json!([{"id": "name", "label": "Name", "value": "", "type": "text"}]),
        json!([set(), plain("other", &[])]),
    );
    let Some(Component::Form(form)) = screen.root.as_ref() else {
        panic!()
    };
    assert_eq!(
        form_fill(&screen, form, "name").map(|a| a.id.as_str()),
        Some("set")
    );
    let mut ui = Interaction::default();
    let out = ui.fill(&screen, "set", "form", "name", "Bo");
    assert_eq!(
        invoked(&out),
        Some(("set", json!({"row": "name", "text": "Bo"})))
    );
    assert_eq!(
        ui.fill(&screen, "set", "form", "ghost", "Bo").reason,
        Some(Reason::Unknown)
    );
}

#[test]
fn a_pane_is_chosen_among_the_panels_and_a_missing_row_is_refused() {
    let split = json!({"id": "root", "kind": "split", "relation": "peers", "children": [
        {"id": "p1", "kind": "panel", "title": "One"},
        {"id": "p2", "kind": "panel", "title": "Two"}]});
    let screen = screen_with(
        split,
        json!([{"id": "move", "label": "Move", "keys": ["m"], "enabled": true,
            "expect": "boundary", "args": {"pane": {"component_id": "root"}}},
            {"id": "take", "label": "Take", "keys": ["t"], "enabled": true, "expect": "boundary",
                "args": {"row": {"component_id": "list"}}}]),
    );
    let mut ui = Interaction::default();
    press(&mut ui, &screen, "m");
    assert_eq!(ui.ask().unwrap().panes, ["p1", "p2"]);
    press(&mut ui, &screen, "DOWN");
    assert_eq!(
        invoked(&press(&mut ui, &screen, "RETURN")),
        Some(("move", json!({"pane": "p2"})))
    );
    let out = press(&mut ui, &screen, "t");
    assert_eq!(out.reason, Some(Reason::Missing(ArgKind::Row)));
    assert!(ui.ask().is_none());
}

#[test]
fn a_position_is_set_by_the_renderer_and_return_waits_for_it() {
    let screen = menu(
        &["a"],
        vec![
            json!({"id": "aim", "label": "Aim", "keys": ["a"], "enabled": true,
            "expect": "boundary", "args": {"pos": {"unit": "square"}}}),
        ],
    );
    let mut ui = Interaction::default();
    press(&mut ui, &screen, "a");
    let out = press(&mut ui, &screen, "RETURN");
    assert_eq!(out.reason, Some(Reason::Missing(ArgKind::Pos)));
    ui.set_pos(WorldPos {
        dim: String::new(),
        x: 1,
        y: 2,
        z: 0,
    });
    assert_eq!(
        invoked(&press(&mut ui, &screen, "RETURN")),
        Some(("aim", json!({"pos": {"dim": "", "x": 1, "y": 2, "z": 0}})))
    );
}

#[test]
fn a_new_screen_starts_with_the_focus_on_the_row_the_list_marks_selected() {
    let rows = vec![row("a", json!({})), row("b", json!({"selected": true}))];
    let screen = screen_with(list(rows), json!([]));
    let mut ui = Interaction::default();
    ui.sync(std::slice::from_ref(&screen));
    assert_eq!(ui.focus().row_id.as_deref(), Some("b"));
}

/// Rows held outside the screen, as a model holds the pages it fetched.
struct Held(Vec<Row>);

impl Held {
    fn rows<'a>(&'a self, list: &'a List) -> Vec<&'a Row> {
        list.rows.iter().chain(self.0.iter()).collect()
    }
}

#[test]
fn keyboard_navigation_reaches_rows_the_model_holds_past_the_first_page() {
    let screen = menu(&["a", "b"], vec![pick()]);
    let held = Held(vec![
        serde_json::from_value(row("c", json!({}))).unwrap(),
        serde_json::from_value(row("d", json!({}))).unwrap(),
    ]);
    let mut ui = Interaction::default();
    let rows = |list| held.rows(list);
    assert_eq!(
        ui.press(&screen, "END", 3, &rows).revealed.as_deref(),
        Some("d")
    );
    ui.press(&screen, "UP", 3, &rows);
    let out = ui.press(&screen, "RETURN", 3, &rows);
    assert_eq!(invoked(&out), Some(("pick", json!({"row": "c"}))));
}

#[test]
fn return_on_a_grid_cell_runs_the_affordance_the_cell_names() {
    let grid = json!({"id": "grid", "kind": "grid", "cols": 2, "rows": 1, "cursor": {"x": 0, "y": 0},
        "cells": [
            {"x": 0, "y": 0, "id": "c0", "enabled": true, "affordance": "take"},
            {"x": 1, "y": 0, "id": "c1", "enabled": false, "denial": "Blocked",
                "affordance": "take"}]});
    let screen = screen_with(
        grid,
        json!([{"id": "take", "label": "Take", "keys": [], "enabled": true, "expect": "boundary",
            "args": {"row": {"component_id": "grid"}}}]),
    );
    let mut ui = Interaction::default();
    press(&mut ui, &screen, "RIGHT");
    assert_eq!(
        invoked(&press(&mut ui, &screen, "RETURN")),
        Some(("take", json!({"row": "c0"})))
    );
    press(&mut ui, &screen, "RIGHT");
    let out = press(&mut ui, &screen, "RETURN");
    assert_eq!(out.invoke, None);
    assert_eq!(out.denial, Some(Text::Plain("Blocked".into())));
}
