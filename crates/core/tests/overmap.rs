#![recursion_limit = "512"]
mod common;
use bn_client_core::{EngineModel, GameModel, Rgb};
use common::{connected, session};
use serde_json::{Value, json};

/// A 4x3 overmap centred on (100, 200, 0) with the player two squares west of the cursor.
fn overmap_event(cursor_x: i32) -> Value {
    let cell = |g: &str, fg: u8| json!([g, fg, 0]);
    let row = |g: &str| json!([cell(g, 7), cell(g, 2), cell("@", 11), cell(g, 1)]);
    json!({"jsonrpc":"2.0","method":"bn.events","params":{"epoch":"e7f3","events":[{
    "sequence":"41","revision":"32","type":"interaction.changed","changes":{"interaction":{
    "boundary_id":"b:60",
    "readiness":{"phase":"playing","game_ready":true,"accepts_interaction_commands":true,"accepts_registered_actions":true},
    "actions":[{"id":"CHOOSE_DESTINATION","name":"Travel","keys":["_"]},{"id":"QUIT","name":"Quit","keys":["ESC"]}],
    "interaction":{
        "schema_id":format!("s:om{cursor_x}"),"kind":"target","context":"OVERMAP","title":"Overmap",
        "message":"","allow_cancel":true,"allow_set_count":false,"choices":[],"choice_total":0,
        "target":{"source":{"dim":"","x":98,"y":200,"z":0},"current":{"dim":"","x":cursor_x,"y":200,"z":0},
            "range":100,"distance_metric":"square","unit":"omt","status":"","candidates":[]},
        "overmap":{
            "origin":{"dim":"","x":cursor_x - 2,"y":199,"z":0},
            "player":{"dim":"","x":98,"y":200,"z":0},
            "cols":4,"rows":3,
            "cells":[row("F"), row("f"), row("r")],
            "legend":["Overmap","  forest"],
            "notes":[{"pos":{"dim":"","x":cursor_x - 1,"y":200,"z":0},"text":"cache"}],
                "path":[{"dim":"","x":cursor_x + 1,"y":200,"z":0},{"dim":"","x":cursor_x + 2,"y":200,"z":0}]
        },
        "compat":{"panes":[]}
    }}}}]}})
}

/// The overmap event for a client that said it draws tiles: every square is `t_floor` but the
/// top-right one, a `forest` that falls back to `t_dirt`.
fn with_tiles(mut event: Value) -> Value {
    let rows: Vec<Value> = (0..3)
        .map(|row| {
            let cells: Vec<Value> = (0..4)
                .map(|col| match (col, row) {
                    (3, 0) => json!(["forest", 1, ""]),
                    _ => json!(["t_floor", 0, ""]),
                })
                .collect();
            json!(cells)
        })
        .collect();
    event["params"]["events"][0]["changes"]["interaction"]["interaction"]["overmap"]["tiles"] =
        json!({"cells": rows, "looks_like": {"forest": ["t_dirt"], "t_floor": []}});
    event
}

fn open() -> (EngineModel, bn_client_core::transport::ScriptHandle) {
    let (t, h) = session();
    let mut m = connected(t.on("bn.command.submit", vec![common::fixture("submit_result")]));
    h.push(overmap_event(100));
    m.poll();
    (m, h)
}

#[test]
fn the_overmap_interaction_becomes_a_grid_with_colours_cursor_and_notes() {
    let (m, _h) = open();
    let om = m.overmap().expect("overmap is open");
    assert_eq!((om.cols, om.rows), (4, 3));
    assert_eq!(om.cells.len(), 12);
    assert_eq!(om.cell(2, 0).unwrap().glyph, "@");
    assert_eq!(om.cell(3, 1).unwrap().color, Rgb(170, 40, 40), "curses red");
    assert_eq!(om.cursor, (100, 200, 0));
    assert_eq!(om.player, (98, 200, 0));
    assert_eq!(om.square(2, 1), (100, 200, 0), "cell (2, 1) is the cursor");
    assert_eq!(om.legend, ["Overmap", "  forest"]);
    assert_eq!(om.note((99, 200, 0)), Some("cache"));
    assert!(om.cell(4, 0).is_none() && om.cell(0, 3).is_none());
    assert!(m.menu().is_none(), "the overmap is not a list menu");
    assert!(m.focus().is_none(), "its cursor is not a map square");
}

#[test]
fn moving_the_cursor_submits_set_target_with_an_overmap_square() {
    let (mut m, h) = open();
    m.overmap_cursor_to((103, 198, 0));
    let sent = h
        .sent()
        .into_iter()
        .rfind(|s| s["method"] == "bn.command.submit")
        .expect("a command was submitted");
    assert_eq!(
        sent["params"]["operation"],
        json!({"kind":"set_target","pos":{"dim":"","x":103,"y":198,"z":0}})
    );
    assert_eq!(sent["params"]["expect"]["schema_id"], "s:om100");
}

#[test]
fn closing_the_overmap_submits_cancel_and_a_closed_one_does_nothing() {
    let (mut m, h) = open();
    m.close_overmap();
    assert_eq!(
        h.sent().last().unwrap()["params"]["operation"],
        json!({"kind":"cancel"})
    );
    let (t, h2) = session();
    let mut closed = connected(t);
    closed.close_overmap();
    closed.overmap_cursor_to((1, 2, 3));
    assert!(!h2.sent_methods().contains(&"bn.command.submit".to_string()));
}

#[test]
fn the_planned_route_and_the_tile_ids_come_with_the_overmap() {
    let (m, _h) = open();
    let om = m.overmap().unwrap();
    assert_eq!(om.path, [(101, 200, 0), (102, 200, 0)]);
    assert!(
        om.tiles.is_empty() && om.tile(0, 0).is_none(),
        "no tiles unless asked for"
    );

    let (t, h) = session();
    let mut tiled = EngineModel::connect(Box::new(t), true);
    tiled.poll();
    h.push(with_tiles(overmap_event(100)));
    tiled.poll();
    let om = tiled.overmap().unwrap();
    let forest = om.tile(3, 0).unwrap();
    assert_eq!((forest.id.as_str(), forest.rotation), ("forest", 1));
    assert_eq!(forest.fallbacks, ["t_dirt"]);
    assert_eq!(om.tile(0, 0).unwrap().id, "t_floor");
    assert!(om.tile(4, 0).is_none());
}

#[test]
fn hello_says_tiles_only_for_a_client_that_draws_them() {
    let hello_of = |tiles: bool| {
        let (t, h) = session();
        let _ = EngineModel::connect(Box::new(t), tiles);
        h.sent()[0]["params"].clone()
    };
    assert_eq!(hello_of(true)["tiles"], true);
    assert!(hello_of(false).get("tiles").is_none());
}

mod switching {
    use super::*;
    use bn_client_core::Clock;
    use std::time::Duration;

    fn hellos(h: &bn_client_core::transport::ScriptHandle) -> Vec<Value> {
        h.sent()
            .into_iter()
            .filter(|m| m["method"] == "bn.hello")
            .map(|m| m["params"].clone())
            .collect()
    }

    #[test]
    fn switching_tiles_on_sends_a_hello_and_a_same_size_viewport_once_idle() {
        let (t, h) = session();
        let mut m = connected(t);
        m.set_tiles(false);
        m.poll();
        assert_eq!(hellos(&h).len(), 1, "no change, nothing sent");
        m.set_tiles(true);
        m.poll();
        let sent = hellos(&h);
        assert_eq!(sent.len(), 2);
        assert_eq!(sent[1]["tiles"], true);
        assert!(
            sent[1].get("viewport").is_none(),
            "only the preference changes"
        );
        let methods = h.sent_methods();
        assert_eq!(&methods[methods.len() - 2..], ["bn.hello", "bn.viewport"]);
        m.poll();
        assert_eq!(hellos(&h).len(), 2, "told once");
        m.set_tiles(false);
        m.poll();
        assert!(
            hellos(&h)[2].get("tiles").is_none(),
            "switching back says so"
        );
    }

    #[test]
    fn the_preference_waits_while_a_command_runs() {
        let (t, h) = session();
        let mut m = connected(t.on("bn.command.submit", vec![common::fixture("submit_result")]));
        assert!(m.invoke_action("RIGHT"));
        m.set_tiles(true);
        m.poll();
        assert_eq!(hellos(&h).len(), 1);
        h.push(json!({"jsonrpc":"2.0","method":"bn.command","params":{
            "epoch":"e7f3","command_id":"c:5","stage":"completed"}}));
        m.poll();
        assert_eq!(hellos(&h).len(), 2);
    }

    #[test]
    fn a_refused_hello_is_tried_again_after_a_moment() {
        let (t, h) = session();
        let clock = Clock::manual();
        let refuse = json!({"jsonrpc":"2.0","id":null,"error":{"code":1000,"message":"x","data":{"kind":"command_busy"}}});
        let mut m =
            EngineModel::new(Box::new(t.on("bn.hello", vec![refuse]))).with_clock(clock.clone());
        m.poll();
        m.set_tiles(true);
        m.poll();
        m.poll();
        assert_eq!(hellos(&h).len(), 2, "refused");
        m.poll();
        assert_eq!(hellos(&h).len(), 2, "not at once");
        clock.advance(Duration::from_secs(2));
        m.poll();
        assert_eq!(hellos(&h).len(), 3);
    }
}
