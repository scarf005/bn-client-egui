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
            "notes":[{"pos":{"dim":"","x":cursor_x - 1,"y":200,"z":0},"text":"cache"}]
        },
        "compat":{"panes":[]}
    }}}}]}})
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
