use bn_client_core::protocol::{self, Incoming, Snapshot};
use serde_json::{Value, json};

fn fixtures() -> serde_json::Map<String, Value> {
    let text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/design.json"
    ))
    .unwrap();
    serde_json::from_str(&text).unwrap()
}

fn to_value<T: serde::Serialize>(v: &T) -> Value {
    serde_json::to_value(v).unwrap()
}

/// Re-encodes a parsed message the way the engine would have written it.
fn reencode(msg: &Value, parsed: &Incoming) -> Value {
    let (head, body) = match parsed {
        Incoming::SnapshotPart(p) => (json!({"method": "bn.snapshot.part"}), to_value(p)),
        Incoming::Events(p) => (json!({"method": "bn.events"}), to_value(p)),
        Incoming::Command(p) => (json!({"method": "bn.command"}), to_value(p)),
        Incoming::Resync(p) => (json!({"method": "bn.resync"}), to_value(p)),
        other => panic!("not a notification: {other:?}"),
    };
    let mut out = json!({"jsonrpc": msg["jsonrpc"], "method": head["method"]});
    out["params"] = body;
    out
}

#[test]
fn every_fixture_notification_round_trips_exactly() {
    let all = fixtures();
    let mut count = 0;
    for (name, msg) in &all {
        if msg.get("method").is_none() {
            continue;
        }
        let parsed = protocol::parse_message(msg).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(reencode(msg, &parsed), *msg, "{name}");
        count += 1;
    }
    assert!(count >= 8, "fixtures should cover the notifications");
}

#[test]
fn subscribe_result_round_trips_exactly() {
    let msg = &fixtures()["subscribe_result"];
    let snapshot: Snapshot = serde_json::from_value(msg["result"].clone()).unwrap();
    assert_eq!(to_value(&snapshot), msg["result"]);
}

#[test]
fn hello_result_and_submit_result_parse_as_responses() {
    let all = fixtures();
    for (name, id) in [("hello_result", 1), ("submit_result", 3)] {
        let parsed = protocol::parse_message(&all[name]).unwrap();
        assert!(matches!(parsed, Incoming::Response { id: got, result: Ok(_) } if got == id));
    }
}

#[test]
fn error_response_keeps_code_and_kind() {
    let line = r#"{"jsonrpc":"2.0","id":9,"error":{"code":1000,"message":"stale","data":{"kind":"stale_revision"}}}"#;
    let Incoming::Response {
        id: 9,
        result: Err(e),
    } = protocol::parse_line(line)
    else {
        panic!("expected error response")
    };
    assert_eq!(
        (e.code, e.data.unwrap()["kind"].clone()),
        (1000, json!("stale_revision"))
    );
}

#[test]
fn malformed_and_unknown_lines_are_reported_not_fatal() {
    assert!(matches!(
        protocol::parse_line("{nope"),
        Incoming::Malformed(_)
    ));
    assert!(matches!(
        protocol::parse_line(r#"{"jsonrpc":"2.0","method":"bn.future","params":{}}"#),
        Incoming::Unknown(m) if m == "bn.future"
    ));
    assert!(matches!(
        protocol::parse_line(r#"{"jsonrpc":"2.0","method":"bn.events","params":{"epoch":1}}"#),
        Incoming::Malformed(_)
    ));
}

#[test]
fn operations_serialize_with_the_kind_tag() {
    use protocol::Operation::*;
    assert_eq!(
        to_value(&Action {
            action_id: "pickup".into()
        }),
        json!({"kind":"action","action_id":"pickup"})
    );
    assert_eq!(
        to_value(&Choose {
            choice_id: "c:1".into()
        }),
        json!({"kind":"choose","choice_id":"c:1"})
    );
    assert_eq!(to_value(&Cancel), json!({"kind":"cancel"}));
}

fn overmap(
    cols: usize,
    rows: usize,
    target: Option<(i32, i32)>,
) -> bn_client_core::protocol::OvermapWire {
    serde_json::from_value(serde_json::json!({
        "origin": {"dim": "", "x": 0, "y": 0, "z": 0},
        "player": {"dim": "", "x": 0, "y": 0, "z": 0},
        "cols": cols, "rows": rows,
        "cells": vec![vec![(".", 7, 0); cols]; rows],
        "legend": [], "notes": [],
        "mission_target": target.map(|(x, y)| serde_json::json!({"dim": "", "x": x, "y": y, "z": 0})),
    }))
    .unwrap()
}

#[test]
fn an_overmap_window_shows_nothing_where_no_square_fits() {
    let at = bn_client_core::protocol::WorldPos::default();
    assert_eq!(overmap(0, 0, Some((5, 5))).window(&at, (10, 10)), None);
    assert_eq!(overmap(20, 20, Some((5, 5))).window(&at, (0, 10)), None);
}

#[test]
fn an_overmap_window_keeps_the_cursor_inside_and_marks_a_target_beyond_its_edge() {
    use bn_client_core::protocol::{OvermapWindow, WorldPos};
    let om = overmap(40, 30, Some((39, 2)));
    let cursor = WorldPos {
        x: 10,
        y: 15,
        ..Default::default()
    };
    let window = om.window(&cursor, (8, 6)).unwrap();
    assert_eq!(
        window,
        OvermapWindow {
            left: 6,
            top: 12,
            cols: 8,
            rows: 6
        }
    );
    let mark = om.target_mark(&window, &cursor).unwrap();
    assert_eq!((mark.col, mark.row, mark.arrow), (13, 12, "/"));
    let near = WorldPos {
        x: 36,
        y: 3,
        ..Default::default()
    };
    let window = om.window(&near, (8, 6)).unwrap();
    assert_eq!(
        om.target_mark(&window, &near),
        None,
        "a target inside the window is drawn, not marked"
    );
}
