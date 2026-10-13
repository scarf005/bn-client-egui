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
        "data": {"id": format!("m{sequence}"), "text": "tick", "kind": "bad", "color": "c_red", "count": 1, "label": "tick"}}]}})
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
    assert_eq!(
        h.sent()[0]["params"]["delivery"],
        json!({"credits": 256, "max_hz": 60})
    );
}

fn hello_delivery(max_hz: u16) -> Value {
    let (t, h) = credited_session(Some(64));
    let _ = EngineModel::connect_with(Box::new(t), false, max_hz);
    h.sent()[0]["params"]["delivery"].clone()
}

#[test]
fn hello_carries_the_clients_frame_rate() {
    assert_eq!(hello_delivery(30), json!({"credits": 256, "max_hz": 30}));
}

#[test]
fn a_frame_rate_outside_1_to_240_is_never_sent() {
    assert_eq!(hello_delivery(0)["max_hz"], 1);
    assert_eq!(hello_delivery(241)["max_hz"], 240);
    assert_eq!(hello_delivery(u16::MAX)["max_hz"], 240);
}

#[test]
fn a_later_hello_keeps_the_frame_rate() {
    let (t, h) = credited_session(Some(64));
    let mut m = EngineModel::connect_with(Box::new(t), false, 30);
    m.poll();
    m.set_tiles(true);
    m.poll();
    let hellos = h
        .sent()
        .into_iter()
        .filter(|x| x["method"] == "bn.hello")
        .map(|x| x["params"]["delivery"]["max_hz"].clone())
        .collect::<Vec<_>>();
    assert_eq!(hellos, [json!(30), json!(30)]);
}

#[test]
fn credits_work_whether_or_not_the_engine_echoes_a_frame_rate() {
    for echo in [Some(60), None] {
        let mut hello = fixture("hello_result");
        hello["result"]["delivery"] =
            json!({"credits": 8, "held_frames": 256, "held_bytes": 1048576});
        if let Some(hz) = echo {
            hello["result"]["delivery"]["max_hz"] = json!(hz);
        }
        let (t, h) = ScriptedTransport::new();
        let t = t
            .on("bn.hello", vec![hello])
            .on("bn.subscribe", subscribe_reply());
        let mut m = connected(t);
        feed(&h, &mut m, 1, 4);
        assert_eq!(credits(&h), [4], "{echo:?}");
    }
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

fn invalid_params() -> Value {
    json!({"jsonrpc": "2.0", "error": {"code": -32602, "message": "unknown member"}})
}

fn hello_calls(h: &ScriptHandle) -> Vec<Value> {
    h.sent()
        .into_iter()
        .filter(|x| x["method"] == "bn.hello")
        .map(|x| x["params"]["delivery"].clone())
        .collect()
}

fn granted(max_hz: Option<u16>) -> Value {
    let mut hello = fixture("hello_result");
    hello["result"]["delivery"] = json!({"credits": 8, "held_frames": 256, "held_bytes": 1048576});
    if let Some(hz) = max_hz {
        hello["result"]["delivery"]["max_hz"] = json!(hz);
    }
    hello
}

#[test]
fn an_engine_that_rejects_only_the_frame_rate_gets_credits_without_it() {
    let (t, h) = ScriptedTransport::new();
    let t = t
        .on("bn.hello", vec![invalid_params()])
        .on("bn.hello", vec![granted(None)])
        .on("bn.subscribe", subscribe_reply());
    let mut m = connected(t);
    assert_eq!(
        hello_calls(&h),
        [
            json!({"credits": 256, "max_hz": 60}),
            json!({"credits": 256})
        ]
    );
    assert!(m.failure().is_none() && m.live());
    feed(&h, &mut m, 1, 4);
    assert_eq!(credits(&h), [4], "credits stay on");
}

#[test]
fn an_engine_that_rejects_delivery_gets_a_hello_without_it_and_no_credits() {
    let (t, h) = ScriptedTransport::new();
    let t = t
        .on("bn.hello", vec![invalid_params()])
        .on("bn.hello", vec![invalid_params()])
        .on("bn.hello", vec![fixture("hello_result")])
        .on("bn.subscribe", subscribe_reply());
    let mut m = connected(t);
    assert_eq!(
        hello_calls(&h),
        [
            json!({"credits": 256, "max_hz": 60}),
            json!({"credits": 256}),
            Value::Null
        ]
    );
    assert!(m.messages().is_empty(), "{:?}", m.messages());
    feed(&h, &mut m, 1, 200);
    assert_eq!(credits(&h), Vec::<u64>::new());
}

#[test]
fn the_tiles_hello_uses_what_the_engine_accepted() {
    let (t, h) = ScriptedTransport::new();
    let t = t
        .on("bn.hello", vec![invalid_params()])
        .on("bn.hello", vec![invalid_params()])
        .on("bn.hello", vec![fixture("hello_result")])
        .on("bn.hello", vec![fixture("hello_result")])
        .on("bn.subscribe", subscribe_reply());
    let mut m = connected(t);
    m.set_tiles(true);
    m.poll();
    assert_eq!(hello_calls(&h).last(), Some(&Value::Null));
    assert_eq!(hello_calls(&h).len(), 4);
}

#[test]
fn a_hello_without_delivery_that_still_fails_is_reported() {
    let (t, h) = ScriptedTransport::new();
    let t = t
        .on("bn.hello", vec![invalid_params()])
        .on("bn.hello", vec![invalid_params()])
        .on("bn.hello", vec![invalid_params()]);
    let mut m = EngineModel::new(Box::new(t));
    m.poll();
    assert_eq!(hello_calls(&h).len(), 3, "no fourth try");
    assert!(!m.messages().is_empty());
}
