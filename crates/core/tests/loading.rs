mod common;
use bn_client_core::GameModel;
use bn_client_core::protocol::{self, Incoming};
use common::{connected, session};
use serde_json::{Value, json};

fn progress() -> Value {
    json!({"jsonrpc":"2.0","method":"bn.loading","params":{"epoch":"e7f3","title":"Loading world","entries":["Loading items","Loading monsters","Finalizing"],"index":1,"image":{"path":"gfx/loading/a.png","author":"Ada"}}})
}

fn done() -> Value {
    json!({"jsonrpc":"2.0","method":"bn.loading","params":{"epoch":"e7f3","done":true}})
}

#[test]
fn loading_notifications_round_trip_exactly() {
    for msg in [progress(), done()] {
        let Incoming::Loading(n) = protocol::parse_message(&msg).unwrap() else {
            panic!("not a loading notice")
        };
        assert_eq!(serde_json::to_value(&n).unwrap(), msg["params"]);
    }
}

#[test]
fn progress_without_image_parses() {
    let msg = json!({"jsonrpc":"2.0","method":"bn.loading","params":{"epoch":"e","title":"T","entries":[],"index":0}});
    assert!(
        matches!(protocol::parse_message(&msg).unwrap(), Incoming::Loading(n) if n.image.is_none())
    );
}

#[test]
fn model_exposes_progress_and_clears_it_when_done() {
    let (t, h) = session();
    let mut m = connected(t);
    assert!(m.loading().is_none());
    h.push(progress());
    m.poll();
    let l = m.loading().unwrap();
    assert_eq!((l.title.as_str(), l.index), ("Loading world", 1));
    assert_eq!(
        l.entries,
        ["Loading items", "Loading monsters", "Finalizing"]
    );
    assert_eq!(l.image.as_deref(), Some("gfx/loading/a.png"));
    h.push(done());
    m.poll();
    assert!(m.loading().is_none());
}

#[test]
fn a_later_progress_replaces_the_earlier_one() {
    let (t, h) = session();
    let mut m = connected(t);
    h.push(progress());
    let mut next = progress();
    next["params"]["index"] = json!(2);
    next["params"].as_object_mut().unwrap().remove("image");
    h.push(next);
    m.poll();
    let l = m.loading().unwrap();
    assert_eq!(l.index, 2);
    assert!(l.image.is_none());
}
