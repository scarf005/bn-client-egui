use bn_client_core::protocol::{self, EventsParams, Incoming, Resync, Snapshot, SnapshotPart};
use bn_client_core::replica::{Outcome, Replica};
use serde_json::Value;

fn fixture(name: &str) -> Value {
    let text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/design.json"
    ))
    .unwrap();
    serde_json::from_str::<Value>(&text).unwrap()[name].clone()
}

fn events(name: &str) -> EventsParams {
    match protocol::parse_message(&fixture(name)).unwrap() {
        Incoming::Events(e) => e,
        other => panic!("{other:?}"),
    }
}

fn part(name: &str) -> SnapshotPart {
    match protocol::parse_message(&fixture(name)).unwrap() {
        Incoming::SnapshotPart(p) => p,
        other => panic!("{other:?}"),
    }
}

fn snapshot() -> Snapshot {
    serde_json::from_value(fixture("subscribe_result")["result"].clone()).unwrap()
}

fn loaded() -> Replica {
    let mut r = Replica::default();
    r.begin(snapshot());
    assert_eq!(r.part(part("snapshot_part_0")), Outcome::Pending);
    assert_eq!(r.part(part("snapshot_part_1")), Outcome::Applied);
    r
}

fn avatar_x(r: &Replica) -> i32 {
    r.avatar().unwrap().at.x
}

#[test]
fn snapshot_parts_fill_cells_then_become_ready() {
    let mut r = Replica::default();
    assert!(!r.ready());
    r.begin(snapshot());
    assert!(!r.ready());
    r.part(part("snapshot_part_0"));
    assert!(!r.ready(), "not ready before the last part");
    r.part(part("snapshot_part_1"));
    assert!(r.ready());
    assert_eq!(
        r.cell("", 4, 3, 0).unwrap().terrain.as_ref().unwrap().glyph,
        "#"
    );
    assert_eq!(r.cell("", 3, 4, 0).unwrap().items.len(), 2);
    assert!(r.cell("", 99, 99, 0).is_none(), "absent cells are unknown");
    assert_eq!(r.entities().count(), 1);
    assert_eq!(r.boundary().unwrap().boundary_id, "b:12");
}

#[test]
fn part_with_wrong_index_or_position_requests_resubscribe() {
    let mut r = Replica::default();
    r.begin(snapshot());
    assert!(matches!(
        r.part(part("snapshot_part_1")),
        Outcome::Resubscribe(_)
    ));
    let mut r = Replica::default();
    r.begin(snapshot());
    let mut p = part("snapshot_part_0");
    p.at.sequence = "41".into();
    assert!(matches!(r.part(p), Outcome::Resubscribe(_)));
}

#[test]
fn events_apply_in_sequence_and_move_the_avatar() {
    let mut r = loaded();
    assert_eq!(avatar_x(&r), 3);
    assert_eq!(r.events(events("events_41_walk")), Outcome::Applied);
    assert_eq!(avatar_x(&r), 4);
    assert_eq!(r.at().unwrap().sequence, "41");
    assert_eq!(r.at().unwrap().revision, "32");
    assert_eq!(r.events(events("events_42_message")), Outcome::Applied);
    let lines = r.take_lines();
    assert_eq!(lines.len(), 1);
    assert_eq!(
        (lines[0].text.as_str(), lines[0].count),
        ("You walk east.", 1)
    );
    assert!(r.take_lines().is_empty(), "log is drained once");
    assert_eq!(r.events(events("events_43_menu")), Outcome::Applied);
    assert_eq!(
        r.boundary()
            .unwrap()
            .interaction
            .as_ref()
            .unwrap()
            .choices
            .len(),
        2
    );
}

#[test]
fn duplicate_batch_is_ignored() {
    let mut r = loaded();
    r.events(events("events_41_walk"));
    assert_eq!(r.events(events("events_41_walk")), Outcome::Duplicate);
    assert_eq!(avatar_x(&r), 4);
    assert_eq!(r.at().unwrap().sequence, "41");
}

#[test]
fn gap_requests_resubscribe_and_applies_nothing() {
    let mut r = loaded();
    assert!(matches!(
        r.events(events("events_45_gap")),
        Outcome::Resubscribe(_)
    ));
    assert_eq!(r.at().unwrap().sequence, "40");
    assert!(r.take_log().is_empty());
}

#[test]
fn invalid_suffix_leaves_state_unchanged() {
    let mut r = loaded();
    let mut batch = events("events_41_walk");
    batch.events.extend(events("events_45_gap").events);
    assert!(matches!(r.events(batch), Outcome::Resubscribe(_)));
    assert_eq!(avatar_x(&r), 3, "no prefix applied");
    assert_eq!(r.at().unwrap().sequence, "40");
}

#[test]
fn duplicate_prefix_then_next_event_applies_only_the_new_one() {
    let mut r = loaded();
    r.events(events("events_41_walk"));
    let mut batch = events("events_41_walk");
    batch.events.extend(events("events_42_message").events);
    assert_eq!(r.events(batch), Outcome::Applied);
    assert_eq!(r.at().unwrap().sequence, "42");
}

#[test]
fn epoch_mismatch_requests_resubscribe() {
    let mut r = loaded();
    assert!(matches!(
        r.events(events("events_other_epoch")),
        Outcome::Resubscribe(_)
    ));
    assert_eq!(r.at().unwrap().epoch, "e7f3");
}

#[test]
fn resync_notification_requests_resubscribe() {
    let mut r = loaded();
    let resync = Resync {
        epoch: "e7f3".into(),
        reason: "overflow".into(),
        lost_after: "977".into(),
    };
    assert!(matches!(r.resync(&resync), Outcome::Resubscribe(_)));
}

#[test]
fn events_before_any_snapshot_request_resubscribe() {
    let mut r = Replica::default();
    assert!(matches!(
        r.events(events("events_41_walk")),
        Outcome::Resubscribe(_)
    ));
}

#[test]
fn begin_discards_previous_state() {
    let mut r = loaded();
    r.events(events("events_41_walk"));
    r.begin(snapshot());
    assert!(!r.ready());
    assert!(r.cell("", 4, 3, 0).is_none());
    assert_eq!(avatar_x(&r), 3);
}

#[test]
fn changes_apply_coverage_cells_forgotten_and_gone_in_order() {
    use bn_client_core::protocol::{Changes, Coverage, Event, Gone, WorldPos};
    let pos = |x, y| WorldPos {
        dim: "".into(),
        x,
        y,
        z: 0,
    };
    let mut r = loaded();
    let change = |changes| EventsParams {
        epoch: "e7f3".into(),
        events: vec![Event {
            sequence: "41".into(),
            revision: "32".into(),
            kind: "cells.seen".into(),
            cause: None,
            command: None,
            display: None,
            data: None,
            changes,
        }],
    };
    let changes = Changes {
        forgotten: vec![pos(3, 4)],
        gone: vec![Gone {
            id: "e:12".into(),
            reason: "died".into(),
        }],
        ..Default::default()
    };
    assert_eq!(r.events(change(changes)), Outcome::Applied);
    assert!(r.cell("", 3, 4, 0).is_none());
    assert_eq!(r.entities().count(), 0);

    let mut r = loaded();
    let changes = Changes {
        coverage: Some(Coverage {
            min: pos(2, 2),
            max: pos(5, 5),
        }),
        ..Default::default()
    };
    assert_eq!(r.events(change(changes)), Outcome::Applied);
    assert!(
        r.cell("", 1, 2, 0).is_none(),
        "outside the new coverage is dropped"
    );
    assert!(r.cell("", 4, 3, 0).is_some());
    assert_eq!(
        r.entities().count(),
        0,
        "entity outside coverage is dropped"
    );
}

#[test]
fn an_event_boundary_without_actions_keeps_the_previous_actions() {
    use bn_client_core::protocol::{BoundaryChange, Changes, Event};
    let mut r = loaded();
    let kept = r.boundary().unwrap().actions.clone();
    assert!(!kept.is_empty(), "the snapshot boundary lists actions");
    let event = |sequence: &str, revision: &str, interaction| EventsParams {
        epoch: "e7f3".into(),
        events: vec![Event {
            sequence: sequence.into(),
            revision: revision.into(),
            kind: "interaction.changed".into(),
            cause: None,
            command: None,
            display: None,
            data: None,
            changes: Changes {
                interaction: Some(interaction),
                ..Default::default()
            },
        }],
    };
    let readiness = r.boundary().unwrap().readiness.clone();
    let boundary = |id: &str, actions: Option<Vec<protocol::Action>>| BoundaryChange {
        boundary_id: id.into(),
        readiness: readiness.clone(),
        actions,
        interaction: None,
        stack: vec![],
    };
    let next = boundary("boundary:next", None);
    assert_eq!(r.events(event("41", "32", next)), Outcome::Applied);
    let now = r.boundary().unwrap();
    assert_eq!(now.boundary_id, "boundary:next");
    assert_eq!(now.actions, kept);

    let none = boundary("boundary:none", Some(vec![]));
    assert_eq!(r.events(event("42", "33", none)), Outcome::Applied);
    assert!(
        r.boundary().unwrap().actions.is_empty(),
        "an empty list replaces them"
    );
}

#[test]
fn compact_cell_layers_get_their_kind_and_center_subtile_back() {
    use bn_client_core::protocol::{Cell, Changes, Event};
    let compact: Cell = serde_json::from_value(serde_json::json!({
        "at": {"dim": "", "x": 3, "y": 4, "z": 0},
        "known": "visible",
        "terrain": {"id": "t_floor", "glyph": ".", "color": "c_cyan"},
        "items": [{"id": "rock", "glyph": "*", "color": "c_light_gray"}],
        "memory": {
            "terrain": {"id": "t_wall", "glyph": "|", "color": "c_light_gray", "rotation": 1},
            "overlay": {"kind": "trap", "id": "tr_x", "glyph": "^", "color": "c_red"},
        },
    }))
    .unwrap();
    let mut r = loaded();
    let event = EventsParams {
        epoch: "e7f3".into(),
        events: vec![Event {
            sequence: "41".into(),
            revision: "32".into(),
            kind: "cells.seen".into(),
            cause: None,
            command: None,
            display: None,
            data: None,
            changes: Changes {
                cells: vec![compact],
                ..Default::default()
            },
        }],
    };
    assert_eq!(r.events(event), Outcome::Applied);
    let cell = r.cell("", 3, 4, 0).unwrap();
    let terrain = cell.terrain.as_ref().unwrap();
    assert_eq!(
        (terrain.kind.as_str(), terrain.subtile.as_deref()),
        ("terrain", Some("center"))
    );
    let item = &cell.items[0];
    assert_eq!(
        (item.kind.as_str(), item.subtile.as_deref()),
        ("item", None)
    );
    let memory = cell.memory.as_ref().unwrap();
    assert_eq!(memory.terrain.kind, "terrain");
    assert_eq!(memory.terrain.rotation, Some(1));
    assert_eq!(memory.overlay.as_ref().unwrap().kind, "trap");
    assert_eq!(
        memory.overlay.as_ref().unwrap().subtile.as_deref(),
        Some("center")
    );
}

#[test]
fn an_event_avatar_keeps_the_members_it_names_and_clears_the_others() {
    use bn_client_core::protocol::{AvatarChange, Changes, Event};
    let examples: Value = serde_json::from_str(
        &std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/0.2.examples/world.json"
        ))
        .unwrap(),
    )
    .unwrap();
    let full = examples["valid"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["name"] == "avatar with sidebar")
        .map(|e| e["value"].clone())
        .unwrap();
    let event = |sequence: &str, revision: &str, avatar: AvatarChange| EventsParams {
        epoch: "e7f3".into(),
        events: vec![Event {
            sequence: sequence.into(),
            revision: revision.into(),
            kind: "cells.seen".into(),
            cause: None,
            command: None,
            display: None,
            data: None,
            changes: Changes {
                avatar: Some(avatar),
                ..Default::default()
            },
        }],
    };
    let mut r = loaded();
    let avatar: AvatarChange = serde_json::from_value(full).unwrap();
    assert_eq!(
        r.events(event("41", "32", avatar.clone())),
        Outcome::Applied
    );
    let before = r.avatar().unwrap().clone();
    assert!(before.sidebar.is_some());

    let moved = |keep: &[&str]| AvatarChange {
        at: protocol::WorldPos {
            x: before.at.x + 1,
            ..before.at.clone()
        },
        look: None,
        overlays: vec![],
        traits: Default::default(),
        stats: None,
        inventory: None,
        sidebar: None,
        ground: None,
        keep: keep.iter().map(|k| k.to_string()).collect(),
        ..avatar.clone()
    };
    assert_eq!(
        r.events(event("42", "33", moved(&["stats", "sidebar", "inventory"]))),
        Outcome::Applied
    );
    let kept = r.avatar().unwrap();
    assert_eq!(kept.at.x, before.at.x + 1);
    assert_eq!(kept.stats, before.stats);
    assert_eq!(kept.sidebar, before.sidebar);
    assert_eq!(kept.inventory, before.inventory);

    assert_eq!(r.events(event("43", "34", moved(&[]))), Outcome::Applied);
    let cleared = r.avatar().unwrap();
    assert!(
        cleared.stats.is_empty(),
        "an absent member that is not kept is empty"
    );
    assert!(cleared.sidebar.is_none());
}
