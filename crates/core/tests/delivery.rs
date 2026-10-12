mod common;
use bn_client_core::transport::{ScriptHandle, ScriptedTransport};
use bn_client_core::{EngineModel, GameModel};
use common::{connected, fixture, subscribe_reply};
use serde_json::{Value, json};

fn credited_session(grant: Option<u64>) -> (ScriptedTransport, ScriptHandle) {
    let mut hello = fixture("hello_result");
    if let Some(credits) = grant {
        hello["result"]["delivery"] =
            json!({"credits": credits, "held_frames": 256, "held_bytes": 1048576});
    }
    let (t, h) = ScriptedTransport::new();
    (
        t.on("bn.hello", vec![hello])
            .on("bn.subscribe", subscribe_reply()),
        h,
    )
}

fn frame(sequence: u32) -> Value {
    json!({"jsonrpc": "2.0", "method": "bn.events", "params": {"epoch": "e7f3", "events": [{
        "sequence": (40 + sequence).to_string(), "revision": "32", "type": "message.logged",
        "data": {"id": format!("m{sequence}"), "text": "tick", "kind": "bad", "color": "c_red", "count": 1}}]}})
}

fn credits(h: &ScriptHandle) -> Vec<u64> {
    h.sent()
        .iter()
        .filter(|m| m["method"] == "bn.credit")
        .map(|m| m["params"]["frames"].as_u64().unwrap())
        .collect()
}

fn feed(h: &ScriptHandle, m: &mut EngineModel, from: u32, n: u32) {
    (from..from + n).for_each(|i| h.push(frame(i)));
    m.poll();
}

#[test]
fn hello_asks_for_delivery_credits() {
    let (t, h) = credited_session(Some(64));
    connected(t);
    assert_eq!(h.sent()[0]["params"]["delivery"], json!({"credits": 64}));
}

#[test]
fn credits_are_paid_back_in_half_windows_as_frames_are_read() {
    let (t, h) = credited_session(Some(64));
    let mut m = connected(t);
    feed(&h, &mut m, 1, 31);
    assert_eq!(credits(&h), Vec::<u64>::new(), "below the refill threshold");
    feed(&h, &mut m, 32, 1);
    assert_eq!(credits(&h), [32]);
    feed(&h, &mut m, 33, 64);
    assert_eq!(credits(&h), [32, 32, 32]);
}

#[test]
fn command_frames_cost_a_credit_too() {
    let (t, h) = credited_session(Some(4));
    let mut m = connected(t);
    h.push(fixture("command_completed"));
    h.push(frame(1));
    m.poll();
    assert_eq!(credits(&h), [2]);
}

#[test]
fn an_engine_without_delivery_gets_no_credit_calls() {
    let (t, h) = credited_session(None);
    let mut m = connected(t);
    feed(&h, &mut m, 1, 200);
    assert_eq!(credits(&h), Vec::<u64>::new());
}

#[test]
fn the_window_follows_what_the_engine_granted() {
    let (t, h) = credited_session(Some(8));
    let mut m = connected(t);
    feed(&h, &mut m, 1, 4);
    assert_eq!(credits(&h), [4]);
}
