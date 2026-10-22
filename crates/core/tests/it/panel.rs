//! The character sheet and the inventory read as panels (`bn.panel.view`) while the game goes on: once per
//! revision while their window is open, with the row its focus is on, and never once it is closed.
use crate::common::{connected, panel_view, session};
use bn_client_core::protocol::PanelKind;
use bn_client_core::transport::ScriptHandle;
use bn_client_core::{GameModel, PanelWatch};
use serde_json::{Value, json};

fn answer(panel: &str) -> Value {
    json!({"jsonrpc": "2.0", "id": 0, "result": panel_view(panel)})
}

fn reads(h: &ScriptHandle) -> Vec<Value> {
    h.sent()
        .into_iter()
        .filter(|m| m["method"] == "bn.panel.view")
        .collect()
}

/// An event that only moves the world on: revision `sequence - 9`.
fn change(sequence: u32) -> Value {
    json!({"jsonrpc":"2.0","method":"bn.events","params":{"epoch":"e7f3","events":[{
        "sequence": sequence.to_string(), "revision": (sequence - 9).to_string(),
        "type": "cells.seen", "command": "c:5", "changes": {"route": []}}]}})
}

fn watch(row: Option<&str>) -> Option<PanelWatch> {
    Some(PanelWatch {
        row: row.map(str::to_string),
    })
}

#[test]
fn an_open_panel_is_read_once_per_revision_and_a_closed_one_not_at_all() {
    let (t, h) = session();
    let mut m = connected(
        t.on("bn.panel.view", vec![answer("character")])
            .on("bn.panel.view", vec![answer("character")]),
    );
    m.poll();
    assert!(reads(&h).is_empty(), "no window, no read");
    m.watch_panel(PanelKind::Character, watch(None));
    m.poll();
    m.poll();
    let sent = reads(&h);
    assert_eq!(sent.len(), 1);
    assert_eq!(
        sent[0]["params"],
        json!({"epoch": "e7f3", "panel": "character"})
    );
    let screen = m
        .watched_panel(PanelKind::Character)
        .expect("the sheet was read");
    assert_eq!(screen.screen_id, "panel:character");
    assert!(m.watched_panel(PanelKind::Inventory).is_none());
    m.watch_panel(PanelKind::Character, watch(None));
    m.poll();
    assert_eq!(reads(&h).len(), 1, "an unchanged world asks nothing more");
    h.push(change(41));
    m.poll();
    assert_eq!(reads(&h).len(), 2, "a new revision reads again");
    m.watch_panel(PanelKind::Character, None);
    assert!(m.watched_panel(PanelKind::Character).is_none());
    h.push(change(42));
    m.poll();
    assert_eq!(reads(&h).len(), 2, "a closed window reads nothing");
}

#[test]
fn a_panel_is_read_again_with_the_row_its_focus_moved_to() {
    let (t, h) = session();
    let mut m = connected(
        t.on("bn.panel.view", vec![answer("inventory")])
            .on("bn.panel.view", vec![answer("inventory")]),
    );
    m.watch_panel(PanelKind::Inventory, watch(None));
    m.poll();
    m.watch_panel(PanelKind::Inventory, watch(Some("item:2")));
    m.poll();
    let sent = reads(&h);
    assert_eq!(sent.len(), 2);
    assert_eq!(
        sent[1]["params"],
        json!({"epoch": "e7f3", "panel": "inventory", "row": "item:2"})
    );
    m.poll();
    assert_eq!(reads(&h).len(), 2, "the same row is not asked for again");
}

#[test]
fn an_answer_to_a_read_of_a_window_since_closed_is_ignored() {
    let (t, h) = session();
    let mut m = connected(t);
    m.watch_panel(PanelKind::Character, watch(None));
    m.poll();
    m.watch_panel(PanelKind::Character, None);
    m.watch_panel(PanelKind::Character, watch(None));
    m.poll();
    let sent = reads(&h);
    assert_eq!(sent.len(), 2, "the window opened again reads afresh");
    let reply = |id: &Value| json!({"jsonrpc": "2.0", "id": id, "result": panel_view("character")});
    h.push(reply(&sent[0]["id"]));
    m.poll();
    assert!(
        m.watched_panel(PanelKind::Character).is_none(),
        "the answer to the closed window's read is stale"
    );
    h.push(reply(&sent[1]["id"]));
    m.poll();
    assert!(m.watched_panel(PanelKind::Character).is_some());
}

#[test]
fn a_refused_read_keeps_the_last_screen_and_the_next_revision_reads_again() {
    let (t, h) = session();
    let refusal = json!({"jsonrpc": "2.0", "id": 0, "error": {"code": 1000, "message": "x",
        "data": {"kind": "stale_epoch", "message": "The world changed."}}});
    let mut m = connected(
        t.on("bn.panel.view", vec![answer("character")])
            .on("bn.panel.view", vec![refusal])
            .on("bn.panel.view", vec![answer("character")]),
    );
    m.watch_panel(PanelKind::Character, watch(None));
    m.poll();
    h.push(change(41));
    m.poll();
    m.poll();
    assert_eq!(reads(&h).len(), 2);
    assert!(
        m.watched_panel(PanelKind::Character).is_some(),
        "the refusal keeps the last screen"
    );
    h.push(change(42));
    m.poll();
    assert_eq!(reads(&h).len(), 3);
}
