use bn_client_core::protocol::{self, Incoming, Snapshot};
use serde_json::{Value, json};

fn fixtures() -> serde_json::Map<String, Value> {
    let text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/design-0.1.json"
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

fn examples() -> Value {
    let text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/0.1.examples.json"
    ))
    .unwrap();
    serde_json::from_str(&text).unwrap()
}

fn round_trip<T: serde::Serialize + serde::de::DeserializeOwned>(
    v: &Value,
) -> Result<Value, String> {
    let typed: T = serde_json::from_value(v.clone()).map_err(|e| e.to_string())?;
    Ok(to_value(&typed))
}

fn by_def(def: &str, v: &Value) -> Result<Value, String> {
    use protocol::*;
    match def {
        "hello_params" => round_trip::<HelloParams>(v),
        "viewport" => round_trip::<Viewport>(v),
        "credit_params" => round_trip::<CreditParams>(v),
        "credit_result" => round_trip::<CreditResult>(v),
        "hello_result" => round_trip::<Hello>(v),
        "empty_params" => round_trip::<std::collections::BTreeMap<String, Value>>(v),
        "subscribe_result" => round_trip::<Snapshot>(v),
        "choices_params" => round_trip::<ChoicesParams>(v),
        "choices_result" => round_trip::<ChoicesResult>(v),
        "world_cells_params" => round_trip::<WorldCellsParams>(v),
        "world_cells_result" => round_trip::<WorldCellsResult>(v),
        "submit_params" => round_trip::<Submit>(v),
        "submit_result" | "command_result" | "command_notification" => {
            round_trip::<CommandNotice>(v)
        }
        "result_params" => round_trip::<ResultParams>(v),
        "describe_params" => round_trip::<DescribeParams>(v),
        "describe_result" => round_trip::<DescribeResult>(v),
        "snapshot_part" => round_trip::<SnapshotPart>(v),
        "events_notification" => round_trip::<EventsParams>(v),
        "resync_notification" => round_trip::<Resync>(v),
        "application_error" => round_trip::<ApplicationError>(v),
        "pos" => round_trip::<WorldPos>(v),
        "avatar" => round_trip::<Avatar>(v),
        "loading_notification" => round_trip::<LoadingNotice>(v),
        "choice" => round_trip::<Choice>(v),
        "interaction" => round_trip::<Interaction>(v),
        "canvas" => round_trip::<CanvasWire>(v),
        "vehicle" => round_trip::<protocol::Vehicle>(v),
        "overmap" => round_trip::<protocol::OvermapWire>(v),
        // The world values are typed by the replica tests; here they only have to be valid JSON.
        "environment" => round_trip::<protocol::Environment>(v),
        "entity" => round_trip::<protocol::Entity>(v),
        "look" => round_trip::<protocol::Look>(v),
        "field" | "cell" | "pane" => Ok(v.clone()),
        other => Err(format!("no type for {other}")),
    }
}

/// The schema makes `changes` optional, so an event's empty `{}` and an omitted one are the same message.
fn without_empty_changes(v: &Value) -> Value {
    match v {
        Value::Object(m) => Value::Object(
            m.iter()
                .filter(|(k, v)| !(*k == "changes" && v.as_object().is_some_and(|c| c.is_empty())))
                .map(|(k, v)| (k.clone(), without_empty_changes(v)))
                .collect(),
        ),
        Value::Array(a) => Value::Array(a.iter().map(without_empty_changes).collect()),
        other => other.clone(),
    }
}

#[test]
fn contract_examples_round_trip_exactly() {
    let all = examples();
    let valid = all["valid"].as_array().unwrap();
    assert!(valid.len() >= 20);
    for ex in valid {
        let (name, def, value) = (&ex["name"], ex["def"].as_str().unwrap(), &ex["value"]);
        // Requests that this client never sends or parses still have to be well typed.
        let want = without_empty_changes(value);
        let got = by_def(def, value).map(|v| without_empty_changes(&v));
        assert_eq!(got.as_ref(), Ok(&want), "{name} ({def})");
    }
}

#[test]
fn contract_invalid_examples_the_types_can_reject_are_rejected() {
    // Structural checks only the JSON schema enforces (closed event catalogue) are not repeated here.
    let all = examples();
    let rejected = all["invalid"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|ex| by_def(ex["def"].as_str().unwrap(), &ex["value"]).is_err())
        .map(|ex| ex["name"].as_str().unwrap().to_string())
        .collect::<Vec<_>>();
    assert!(
        rejected.contains(&"bubble coordinate in a target".to_string()),
        "{rejected:?}"
    );
    assert!(
        rejected.contains(&"position without a dimension".to_string()),
        "{rejected:?}"
    );
    for name in [
        "travel with a bubble coordinate",
        "travel without a position",
        "hello delivery without credits",
        "hello delivery with zero credits",
        "hello frame rate of zero",
        "hello frame rate beyond a display",
        "hello frame rate without credits",
        "credit request beyond the window",
    ] {
        assert!(rejected.contains(&name.to_string()), "{name}: {rejected:?}");
    }
}
