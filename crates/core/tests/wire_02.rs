//! The 0.2 types against the engine's own examples (`tests/fixtures/0.2.examples/`, copied verbatim).

use bn_client_core::protocol::{self, Text};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

fn examples() -> Vec<(String, Value)> {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/0.2.examples");
    let mut files = std::fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect::<Vec<_>>();
    files.sort();
    files
        .into_iter()
        .map(|path| {
            let text = std::fs::read_to_string(&path).unwrap();
            (
                path.file_name().unwrap().to_string_lossy().into_owned(),
                serde_json::from_str(&text).unwrap(),
            )
        })
        .collect()
}

fn round_trip<T: serde::Serialize + serde::de::DeserializeOwned>(
    v: &Value,
) -> Result<Value, String> {
    let typed: T = serde_json::from_value(v.clone()).map_err(|e| e.to_string())?;
    Ok(serde_json::to_value(&typed).unwrap())
}

fn by_def(def: &str, v: &Value) -> Result<Value, String> {
    use protocol::*;
    match def {
        "hello_params" => round_trip::<HelloParams>(v),
        "hello_result" => round_trip::<Hello>(v),
        "viewport" => round_trip::<Viewport>(v),
        "credit_params" => round_trip::<CreditParams>(v),
        "credit_result" => round_trip::<CreditResult>(v),
        "empty_params" | "hold_params" => {
            round_trip::<std::collections::BTreeMap<String, Value>>(v)
        }
        "subscribe_result" => round_trip::<Snapshot>(v),
        "snapshot_part" => round_trip::<SnapshotPart>(v),
        "events_notification" => round_trip::<EventsParams>(v),
        "resync_notification" => round_trip::<Resync>(v),
        "loading_notification" => round_trip::<LoadingNotice>(v),
        "command_result" | "command_notification" | "submit_result" => {
            round_trip::<CommandNotice>(v)
        }
        "choices_params" => round_trip::<ChoicesParams>(v),
        "choices_result" => round_trip::<ChoicesReply>(v),
        "world_cells_params" => round_trip::<WorldCellsParams>(v),
        "panel_params" => round_trip::<PanelParams>(v),
        "world_cells_result" => round_trip::<WorldCellsResult>(v),
        "describe_params" => round_trip::<DescribeParams>(v),
        "describe_result" => round_trip::<DescribeResult>(v),
        "submit_params" => round_trip::<Submit>(v),
        "application_error" => round_trip::<ApplicationError>(v),
        "interaction_boundary" => round_trip::<Boundary>(v),
        "screen" => round_trip::<Screen>(v),
        "affordance" => round_trip::<Affordance>(v),
        "arg_shape" => round_trip::<ArgShape>(v),
        "row" => round_trip::<Row>(v),
        "form_field" => round_trip::<FormField>(v),
        "list" | "tabs" | "panel" | "form" | "text_component" | "map" | "grid" | "component"
        | "split" | "canvas_component" => round_trip::<Component>(v),
        "text" => round_trip::<Text>(v),
        "pos" => round_trip::<WorldPos>(v),
        "avatar" => round_trip::<Avatar>(v),
        "cell" => round_trip::<Cell>(v),
        "entity" => round_trip::<Entity>(v),
        "environment" => round_trip::<Environment>(v),
        "look" => round_trip::<Look>(v),
        "overmap" => round_trip::<OvermapWire>(v),
        "data_message_logged" => round_trip::<LoggedMessage>(v),
        "event" => round_trip::<Event>(v),
        "mark" => round_trip::<ScreenMark>(v),
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
fn every_02_example_round_trips_exactly() {
    let mut count = 0;
    for (file, doc) in examples() {
        for ex in doc["valid"].as_array().unwrap() {
            let (name, def, value) = (&ex["name"], ex["def"].as_str().unwrap(), &ex["value"]);
            let want = without_empty_changes(value);
            let got = by_def(def, value).map(|v| without_empty_changes(&v));
            assert_eq!(got.as_ref(), Ok(&want), "{file}: {name} ({def})");
            count += 1;
        }
    }
    assert!(count >= 100, "the examples should all be read: {count}");
}

/// The invalid examples the types can tell from the valid ones; the rest only the JSON schema checks
/// (a member no type declares, a colour where a tone belongs).
const REJECTED: &[&str] = &[
    "missing epoch on a command",
    "invoke without an affordance",
    "invoke with a bubble coordinate",
    "invoke with an argument no shape has",
    "invoke with no rows",
    "invoke counting a row below zero",
    "invoke expecting a screen and a boundary",
    "invoke with the 0.1 expectation",
    "0.1 operation with a screen expectation",
    "screen expectation without its screen",
    "choice window above the limit",
    "split with an unknown relation",
    "row with a count bound below zero",
    "component of an unknown kind",
    "list without rows",
    "screen without affordances",
    "affordance without keys",
    "screen without an id",
    "screen of an unknown role",
    "affordance without its expectation scope",
    "affordance with an expectation of its own",
    "form field of an unknown type",
    "map of an unknown mode",
    "pane stat without its value",
    "text as an empty object",
    "text with a boolean argument",
    "text with a plural given as a string",
    "text with a member of its own",
    "text as a number",
    "balance tone that is a color",
    "position without a dimension",
    "hello request with a malformed schema hash",
    "hello delivery without credits",
    "hello delivery with zero credits",
    "hello frame rate of zero",
    "hello frame rate beyond a display",
    "hello frame rate without credits",
    "credit request beyond the window",
    "mark with both squares and a rect",
    "mark of an unknown role",
];

#[test]
fn the_02_invalid_examples_the_types_can_reject_are_rejected() {
    let mut rejected = vec![];
    for (_, doc) in examples() {
        for ex in doc["invalid"].as_array().into_iter().flatten() {
            let (name, def) = (ex["name"].as_str().unwrap(), ex["def"].as_str().unwrap());
            match by_def(def, &ex["value"]) {
                Err(e) if e.starts_with("no type") => panic!("{name}: {e}"),
                Err(_) => rejected.push(name.to_string()),
                Ok(_) => {}
            }
        }
    }
    for name in REJECTED {
        assert!(rejected.contains(&name.to_string()), "{name}: {rejected:?}");
    }
}

#[test]
fn a_message_without_a_catalogue_shows_its_key_with_the_values_filled() {
    let shown = |v: Value| serde_json::from_value::<Text>(v).unwrap().shown();
    assert_eq!(shown(json!("Cancel")), "Cancel");
    assert_eq!(shown(json!({"key": "Yes", "ctx": "confirm"})), "Yes");
    assert_eq!(
        shown(json!({"key": "%d items", "plural": 3, "args": [3]})),
        "3 items"
    );
    assert_eq!(
        shown(json!({"key": "", "args": ["Hit ", {"key": "the zombie"}, 2, "!"]})),
        "Hit the zombie2!",
        "the empty key joins its arguments"
    );
}

#[test]
fn a_boundary_without_a_stack_is_a_0_1_boundary() {
    let boundary: protocol::Boundary = serde_json::from_value(json!({
        "boundary_id": "b:1",
        "readiness": {"phase": "waiting_for_input", "game_ready": true,
            "accepts_interaction_commands": true, "accepts_registered_actions": true},
        "actions": [], "interaction": null}))
    .unwrap();
    assert!(boundary.stack.is_empty());
    assert!(
        serde_json::to_value(&boundary)
            .unwrap()
            .get("stack")
            .is_none()
    );
}

#[test]
fn the_schema_hash_is_the_vendored_schemas_sha256_prefix() {
    let bytes = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/schema/0.2.schema.json"
    ))
    .unwrap();
    let digest = Sha256::digest(bytes);
    let hex = digest
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    assert_eq!(protocol::SCHEMA_HASH, format!("sha256:{}", &hex[..16]));
    assert_eq!(protocol::SCHEMA_HASH, "sha256:442a7d4ffcf46976");
}

#[test]
fn an_invoke_is_refused_by_its_own_arguments_under_a_screen_expectation() {
    let submit = |operation: Value| {
        serde_json::from_value::<protocol::Submit>(json!({
            "epoch": "e:1",
            "operation": operation,
            "expect": {"screen_id": "s:1"}
        }))
    };
    assert!(submit(json!({"kind": "invoke", "affordance_id": "a:1"})).is_ok());
    for args in [
        json!({"nothing": 1}),
        json!({"rows": []}),
        json!({"row": {"id": "r:1", "count": -1}}),
        json!({"pos": {"bubble": 1}}),
    ] {
        let operation = json!({"kind": "invoke", "affordance_id": "a:1", "args": args});
        assert!(submit(operation).is_err(), "{args}");
    }
    assert!(submit(json!({"kind": "invoke", "affordance_id": ""})).is_err());
}

#[test]
fn a_hello_offers_only_0_2_and_names_the_schema_it_was_built_with() {
    let hello = serde_json::to_value(
        protocol::HelloParams::new("bn-client")
            .with_viewport(protocol::Viewport { cols: 40, rows: 25 })
            .with_tiles(true)
            .with_delivery(256, Some(60)),
    )
    .unwrap();
    assert_eq!(hello["versions"], json!(["0.2"]));
    assert_eq!(hello["schema"], json!(protocol::SCHEMA_HASH));
    assert_eq!(hello["viewport"], json!({"cols": 40, "rows": 25}));
    assert_eq!(hello["delivery"], json!({"credits": 256, "max_hz": 60}));
}
