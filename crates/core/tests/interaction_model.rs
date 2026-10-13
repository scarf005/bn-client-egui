//! The interaction core on a live model: the 0.2 hello, an invoke from the top screen's affordances, and
//! the pages of a list.

mod common;
use bn_client_core::interaction::InvokeError;
use bn_client_core::protocol::{self, Component, InvokeArgs};
use bn_client_core::transport::{ScriptHandle, ScriptedTransport};
use bn_client_core::{ConnectOptions, EngineModel, GameModel};
use common::fixture;
use serde_json::{Value, json};

fn affordances() -> Value {
    json!([
        {"id": "UP", "label": "Move up", "keys": ["UP", "k"], "enabled": true, "expect": "screen"},
        {"id": "pick", "label": "Choose", "keys": [], "enabled": true, "expect": "boundary",
            "args": {"row": {"component_id": "list"}}}
    ])
}

fn list_root(total: u64) -> Value {
    json!({"id": "list", "kind": "list", "columns": [], "selection": "single", "total": total,
        "rows": [row("a")]})
}

fn row(id: &str) -> Value {
    json!({"id": id, "label": id, "description": "", "denial": "", "enabled": true,
        "selectable": true, "selected": false, "columns": []})
}

/// A subscribe answer whose boundary carries a stack of one screen.
fn subscribe_with_stack(total: u64) -> Vec<Value> {
    let mut subscribe = fixture("subscribe_result");
    subscribe["result"]["interaction"]["stack"] = json!([{
        "screen_id": "screen:3", "role": "dialog", "root": list_root(total),
        "affordances": affordances()}]);
    vec![
        subscribe,
        fixture("snapshot_part_0"),
        fixture("snapshot_part_1"),
    ]
}

fn model(
    total: u64,
    extra: impl FnOnce(ScriptedTransport) -> ScriptedTransport,
) -> (EngineModel, ScriptHandle) {
    let (t, h) = ScriptedTransport::new();
    let t = extra(
        t.on("bn.hello", vec![fixture("hello_result")])
            .on("bn.subscribe", subscribe_with_stack(total)),
    );
    let mut m = EngineModel::open(Box::new(t), ConnectOptions::default());
    m.poll();
    (m, h)
}

fn submits(h: &ScriptHandle) -> Vec<Value> {
    h.sent()
        .into_iter()
        .filter(|m| m["method"] == "bn.command.submit")
        .map(|m| m["params"].clone())
        .collect()
}

#[test]
fn the_hello_names_0_2_the_schema_and_what_the_client_can_do_the_schema_and_its_capabilities() {
    let (_, h) = model(1, |t| t);
    let hello = &h.sent()[0]["params"];
    assert_eq!(hello["versions"], json!(["0.2"]));
    assert_eq!(hello["schema"], json!(protocol::SCHEMA_HASH));
    assert_eq!(hello["delivery"], json!({"credits": 256, "max_hz": 60}));
    assert_eq!(hello["viewport"], json!({"cols": 40, "rows": 25}));
}

#[test]
fn the_stack_of_the_boundary_is_the_models_screens() {
    let (m, _) = model(1, |t| t);
    let [screen] = m.screens() else {
        panic!("one screen: {:?}", m.screens())
    };
    assert_eq!(screen.screen_id, "screen:3");
    assert_eq!(screen.affordances.len(), 2);
}

#[test]
fn a_screen_scoped_invoke_goes_out_expecting_the_screen_and_queues_while_busy() {
    let (mut m, h) = model(1, |t| {
        t.on("bn.command.submit", vec![fixture("submit_result")])
    });
    m.invoke_affordance("UP", InvokeArgs::default()).unwrap();
    assert!(m.busy());
    m.invoke_affordance("UP", InvokeArgs::default()).unwrap();
    assert_eq!(
        submits(&h),
        [
            json!({"epoch": "e7f3", "expect": {"screen_id": "screen:3"},
                "operation": {"kind": "invoke", "affordance_id": "UP", "args": {}}}),
            json!({"epoch": "e7f3", "expect": {"screen_id": "screen:3"},
                "operation": {"kind": "invoke", "affordance_id": "UP", "args": {}}}),
        ]
    );
}

#[test]
fn a_boundary_scoped_invoke_waits_for_the_running_command() {
    let (mut m, h) = model(1, |t| t);
    let args = InvokeArgs {
        row: Some("a".into()),
        ..InvokeArgs::default()
    };
    m.invoke_affordance("pick", args.clone()).unwrap();
    assert_eq!(
        submits(&h)[0]["expect"],
        json!({"boundary_id": "b:12", "revision": "31"})
    );
    assert_eq!(m.invoke_affordance("pick", args), Err(InvokeError::Busy));
    assert_eq!(submits(&h).len(), 1);
}

#[test]
fn an_invoke_the_screen_does_not_offer_is_not_sent() {
    let (mut m, h) = model(1, |t| t);
    assert_eq!(
        m.invoke_affordance("nope", InvokeArgs::default()),
        Err(InvokeError::Unknown("nope".into()))
    );
    assert!(submits(&h).is_empty() && !m.busy());
}

fn rows_list(m: &EngineModel) -> &protocol::List {
    match m.screens().last().and_then(|s| s.root.as_ref()) {
        Some(Component::List(list)) => list,
        other => panic!("a list: {other:?}"),
    }
}

fn page_reply(ids: &[&str], total: u64) -> Vec<Value> {
    vec![
        json!({"jsonrpc": "2.0", "result": {"boundary_id": "b:12", "component_id": "list",
        "total": total, "rows": ids.iter().map(|id| row(id)).collect::<Vec<_>>()}}),
    ]
}

#[test]
fn the_rows_past_the_first_page_are_asked_for_by_component_id() {
    let (mut m, h) = model(3, |t| {
        t.on("bn.interaction.choices", page_reply(&["b", "c"], 3))
    });
    assert_eq!(m.rows(rows_list(&m)).len(), 1);
    m.more_rows("list");
    m.more_rows("list");
    let asked = h
        .sent()
        .into_iter()
        .filter(|x| x["method"] == "bn.interaction.choices")
        .map(|x| x["params"].clone())
        .collect::<Vec<_>>();
    assert_eq!(
        asked,
        [
            json!({"epoch": "e7f3", "boundary_id": "b:12", "component_id": "list",
            "offset": 1, "limit": 200})
        ],
        "once, while the page is on its way"
    );
    m.poll();
    let ids = m
        .rows(rows_list(&m))
        .iter()
        .map(|r| r.id.clone())
        .collect::<Vec<_>>();
    assert_eq!(ids, ["a", "b", "c"]);
    m.more_rows("list");
    assert_eq!(
        h.sent_methods()
            .iter()
            .filter(|x| *x == "bn.interaction.choices")
            .count(),
        1
    );
}

#[test]
fn a_refused_page_is_not_asked_again_in_the_same_boundary() {
    let refusal = json!({"jsonrpc": "2.0", "error": {"code": 1000, "message": "x",
        "data": {"kind": "stale_boundary"}}});
    let (mut m, h) = model(3, |t| {
        t.on("bn.interaction.choices", vec![refusal])
            .on("bn.interaction.choices", page_reply(&["b", "c"], 3))
    });
    m.more_rows("list");
    m.poll();
    m.more_rows("list");
    m.poll();
    m.more_rows("list");
    let asks = h
        .sent_methods()
        .iter()
        .filter(|x| *x == "bn.interaction.choices")
        .count();
    assert_eq!(asks, 1);
    assert_eq!(m.rows(rows_list(&m)).len(), 1);
}

#[test]
fn a_component_that_is_not_a_list_asks_for_nothing() {
    let (mut m, h) = model(3, |t| t);
    m.more_rows("missing");
    assert!(
        !h.sent_methods()
            .contains(&"bn.interaction.choices".to_string())
    );
}

fn accepted(command_id: &str) -> Vec<Value> {
    vec![
        json!({"jsonrpc": "2.0", "result": {"epoch": "e7f3", "command_id": command_id,
        "stage": "received"}}),
    ]
}

fn notice(command_id: &str, stage: &str, text: Option<&str>) -> Value {
    let mut params = json!({"epoch": "e7f3", "command_id": command_id, "stage": stage});
    if let Some(text) = text {
        params["error"] = json!("stale_screen");
        params["error_text"] = json!(text);
    }
    json!({"jsonrpc": "2.0", "method": "bn.command", "params": params})
}

#[test]
fn a_rejected_queued_command_is_reported_while_a_later_one_runs() {
    let (mut m, h) = model(1, |t| {
        t.on("bn.command.submit", accepted("c:5"))
            .on("bn.command.submit", accepted("c:6"))
    });
    m.invoke_affordance("UP", InvokeArgs::default()).unwrap();
    m.invoke_affordance("UP", InvokeArgs::default()).unwrap();
    m.poll();
    h.push(notice("c:5", "rejected", Some("The screen changed.")));
    m.poll();
    assert!(
        m.messages().iter().any(|x| x == "The screen changed."),
        "{:?}",
        m.messages()
    );
    assert!(m.busy(), "c:6 still runs");
    h.push(notice("c:6", "completed", None));
    m.poll();
    assert!(!m.busy());
}

#[test]
fn the_last_of_the_queued_commands_ends_the_busy_state_in_any_order() {
    let (mut m, h) = model(1, |t| {
        t.on("bn.command.submit", accepted("c:5"))
            .on("bn.command.submit", accepted("c:6"))
    });
    m.invoke_affordance("UP", InvokeArgs::default()).unwrap();
    m.invoke_affordance("UP", InvokeArgs::default()).unwrap();
    m.poll();
    h.push(notice("c:6", "completed", None));
    m.poll();
    assert!(m.busy(), "c:5 still runs");
    h.push(notice("c:5", "completed", None));
    m.poll();
    assert!(!m.busy());
}

fn model_with_queue(queue: Option<u64>) -> EngineModel {
    let mut hello = fixture("hello_result");
    if let Some(queue) = queue {
        hello["result"]["limits"]["screen_queue"] = json!(queue);
    }
    let (t, _) = ScriptedTransport::new();
    let mut m = EngineModel::open(
        Box::new(
            t.on("bn.hello", vec![hello])
                .on("bn.subscribe", subscribe_with_stack(1))
                .on("bn.command.submit", accepted("c:1"))
                .on("bn.command.submit", accepted("c:2"))
                .on("bn.command.submit", accepted("c:3")),
        ),
        ConnectOptions::default(),
    );
    m.poll();
    m
}

#[test]
fn the_engines_screen_queue_sets_how_many_screen_commands_wait() {
    let up = |m: &mut EngineModel| m.invoke_affordance("UP", InvokeArgs::default());
    let mut m = model_with_queue(None);
    assert_eq!(up(&mut m), Ok(()));
    assert_eq!(up(&mut m), Ok(()), "one waits behind the running one");
    assert_eq!(
        up(&mut m),
        Err(InvokeError::Busy),
        "a hello without a depth means 1"
    );
    let mut m = model_with_queue(Some(2));
    assert_eq!(up(&mut m), Ok(()));
    assert_eq!(up(&mut m), Ok(()));
    assert_eq!(up(&mut m), Ok(()));
    assert_eq!(up(&mut m), Err(InvokeError::Busy));
}

#[test]
fn a_hello_the_engine_refuses_for_its_schema_fails_with_the_engines_error_and_asks_nothing_more() {
    let refused = json!({"jsonrpc": "2.0", "error": {"code": 1000, "message": "application error",
        "data": {"kind": "negotiation_failed", "action": "hello",
            "message": "The schema does not match.",
            "schema": {"engine": "sha256:0123456789abcdef", "client": "sha256:fedcba9876543210"}}}});
    let (t, h) = ScriptedTransport::new();
    let mut m = EngineModel::open(
        Box::new(t.on("bn.hello", vec![refused])),
        ConnectOptions::default(),
    );
    m.poll();
    let failure = m.failure().expect("the refusal is reported");
    assert_eq!(failure.message, "The schema does not match.");
    assert_eq!(h.sent_methods(), ["bn.hello"], "no fallback to 0.1");
}

/// A model at the map: its stack is the map screen, whose registered actions are screen-scoped affordances.
fn map_model(queue: u64) -> (EngineModel, ScriptHandle) {
    let mut hello = fixture("hello_result");
    hello["result"]["limits"]["screen_queue"] = json!(queue);
    let mut subscribe = subscribe_with_stack(1);
    subscribe[0]["result"]["interaction"]["stack"] = json!([{
        "screen_id": "screen:map:e1", "role": "map",
        "root": {"id": "map:world", "kind": "map", "affordances": [], "mode": "world"},
        "affordances": [
            {"id": "UP", "label": "Move North", "keys": ["UP", "k"], "enabled": true, "expect": "screen"},
            {"id": "open_options", "label": "Options", "keys": [], "enabled": true, "expect": "screen"}]}]);
    let (t, h) = ScriptedTransport::new();
    let t = (0..12).fold(
        t.on("bn.hello", vec![hello]).on("bn.subscribe", subscribe),
        |t, n| t.on("bn.command.submit", accepted(&format!("c:{n}"))),
    );
    let mut m = EngineModel::open(Box::new(t), ConnectOptions::default());
    m.poll();
    (m, h)
}

#[test]
fn a_held_key_at_the_map_queues_screen_scoped_invokes_up_to_the_engines_depth() {
    let (mut m, h) = map_model(8);
    let held = (0..12).filter(|_| m.press("UP")).count();
    assert_eq!(held, 9, "the running move and eight waiting behind it");
    let sent = submits(&h);
    assert_eq!(sent.len(), 9);
    assert!(sent.iter().all(|s| s["expect"] == json!({"screen_id": "screen:map:e1"})
        && s["operation"]["kind"] == "invoke"
        && s["operation"]["affordance_id"] == "UP"));
}

#[test]
fn a_registered_action_the_map_offers_runs_as_its_affordance_while_a_command_runs() {
    let (mut m, h) = map_model(8);
    assert!(m.invoke_action("UP"));
    assert!(m.invoke_action("open_options"), "queued behind the move");
    assert_eq!(submits(&h)[1]["operation"]["affordance_id"], "open_options");
    assert!(!m.invoke_action("pickup"), "an action the map does not offer is not sent");
}
