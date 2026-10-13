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
