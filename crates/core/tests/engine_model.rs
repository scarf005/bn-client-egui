#![recursion_limit = "512"]
mod common;
use bn_client_core::colors::ColorMap;
use bn_client_core::transport::ScriptedTransport;
use bn_client_core::{EngineModel, GameModel, Pos, Rgb};
use common::{connected, fixture, session, subscribe_reply};
use serde_json::{Value, json};

fn submits(h: &bn_client_core::transport::ScriptHandle) -> Vec<Value> {
    h.sent()
        .into_iter()
        .filter(|m| m["method"] == "bn.command.submit")
        .collect()
}

fn walk_reply() -> Vec<Value> {
    vec![fixture("submit_result")]
}

#[test]
fn connects_with_hello_then_subscribe_and_exposes_the_map() {
    let (t, h) = session();
    let m = connected(t);
    assert_eq!(h.sent_methods(), ["bn.hello", "bn.subscribe"]);
    assert_eq!(h.sent()[0]["params"]["versions"], json!(["0.2"]));
    assert_eq!(m.size(), Pos { x: 40, y: 25 });
    assert_eq!(m.player(), Pos { x: 20, y: 12 });
    assert_eq!(m.cell(Pos { x: 21, y: 11 }).glyph, '#');
    assert_eq!(m.cell(Pos { x: 21, y: 11 }).desc, "plastered wooden wall");
    assert_eq!(
        m.cell(Pos { x: 21, y: 11 }).id,
        "t_wall",
        "the id stays for code that matches"
    );
    assert_eq!(
        m.cell(Pos { x: 23, y: 12 }).glyph,
        'Z',
        "entities draw over terrain"
    );
    assert_eq!(m.cell(Pos { x: 0, y: 0 }).desc, "unknown");
    assert_eq!(m.ground(), ["knife_combat", "tin_can"]);
    assert_eq!(m.inventory(), ["flashlight"]);
}

#[test]
fn model_is_not_ready_until_the_last_snapshot_part() {
    let (t, h) = ScriptedTransport::new();
    let t = t.on("bn.hello", vec![fixture("hello_result")]).on(
        "bn.subscribe",
        vec![fixture("subscribe_result"), fixture("snapshot_part_0")],
    );
    let mut m = EngineModel::new(Box::new(t));
    m.poll();
    assert_eq!(m.size(), Pos { x: 0, y: 0 });
    h.push(fixture("snapshot_part_1"));
    m.poll();
    assert_eq!(m.size(), Pos { x: 40, y: 25 });
}

#[test]
fn step_submits_a_move_action_against_the_current_boundary() {
    let (t, h) = session();
    let mut m = connected(t.on("bn.command.submit", walk_reply()));
    assert!(m.invoke_action("RIGHT"));
    let sent = submits(&h);
    assert_eq!(sent.len(), 1);
    assert_eq!(
        sent[0]["params"],
        json!({
            "epoch": "e7f3",
            "expect": { "revision": "31", "boundary_id": "b:12", "schema_id": null },
            "operation": { "kind": "action", "action_id": "RIGHT" }
        })
    );
}

#[test]
fn an_action_the_engine_does_not_offer_is_not_submitted() {
    let (t, h) = session();
    let mut m = connected(t);
    assert!(!m.invoke_action("RIGHTUP"));
    assert!(submits(&h).is_empty());
    assert!(m.messages().is_empty(), "the client writes no text for it");
}

#[test]
fn second_step_waits_for_the_first_command_to_finish() {
    let (t, h) = session();
    let mut m = connected(
        t.on("bn.command.submit", walk_reply())
            .on("bn.command.submit", walk_reply()),
    );
    m.invoke_action("RIGHT");
    m.poll();
    assert!(!m.invoke_action("RIGHT"), "busy");
    assert_eq!(submits(&h).len(), 1, "busy: no second submit");
    h.push(fixture("events_41_walk"));
    h.push(fixture("command_completed"));
    m.poll();
    assert_eq!(
        m.player(),
        Pos { x: 20, y: 12 },
        "the view follows the avatar"
    );
    assert_eq!(m.origin(), Pos { x: -16, y: -8 });
    assert_eq!(m.cell(Pos { x: 20, y: 12 }).glyph, '.');
    m.invoke_action("RIGHT");
    let sent = submits(&h);
    assert_eq!(sent.len(), 2);
    assert_eq!(sent[1]["params"]["expect"]["revision"], "32");
}

#[test]
fn a_rejected_command_is_reported_in_the_engines_words() {
    let (t, h) = session();
    let mut m = connected(t.on("bn.command.submit", walk_reply()));
    m.invoke_action("RIGHT");
    h.push(fixture("command_rejected"));
    m.poll();
    assert_eq!(
        m.messages().last().unwrap(),
        "The screen changed, try again."
    );
}

#[test]
fn a_failed_command_does_not_cancel_the_next_one() {
    let (t, h) = session();
    let mut m = connected(
        t.on("bn.command.submit", walk_reply())
            .on("bn.command.submit", walk_reply()),
    );
    m.invoke_action("RIGHT");
    h.push(fixture("command_rejected"));
    m.poll();
    assert!(m.invoke_action("RIGHT"));
    assert_eq!(submits(&h).len(), 2, "the next command was submitted");
}

#[test]
fn error_response_to_submit_frees_the_queue_and_reports_the_engines_words() {
    let (t, _h) = session();
    let err = json!({"jsonrpc":"2.0","id":null,"error":{"code":1000,"message":"busy","data":{"kind":"command_busy","message":"Another command is still running."}}});
    let mut m = connected(t.on("bn.command.submit", vec![err]));
    m.invoke_action("RIGHT");
    m.poll();
    assert_eq!(
        m.messages().last().unwrap(),
        "Another command is still running."
    );
}

#[test]
fn gap_triggers_a_second_subscribe_and_state_is_rebuilt() {
    let (t, h) = session();
    let mut m = connected(t.on("bn.subscribe", subscribe_reply()));
    h.push(fixture("events_45_gap"));
    m.poll();
    assert_eq!(
        h.sent_methods(),
        ["bn.hello", "bn.subscribe", "bn.subscribe"]
    );
    assert_eq!(m.size(), Pos { x: 40, y: 25 });
    assert_eq!(m.player(), Pos { x: 20, y: 12 });
}

#[test]
fn events_arriving_while_resubscribing_are_ignored() {
    let (t, h) = session();
    let mut m = connected(t.on("bn.subscribe", subscribe_reply()));
    h.push(fixture("events_45_gap"));
    h.push(fixture("events_45_gap"));
    m.poll();
    assert_eq!(
        h.sent_methods()
            .iter()
            .filter(|s| *s == "bn.subscribe")
            .count(),
        2
    );
}

#[test]
fn epoch_mismatch_and_resync_resubscribe() {
    for notice in ["events_other_epoch", "resync"] {
        let (t, h) = session();
        let mut m = connected(t.on("bn.subscribe", subscribe_reply()));
        h.push(fixture(notice));
        m.poll();
        assert_eq!(h.sent_methods().last().unwrap(), "bn.subscribe", "{notice}");
        assert_eq!(h.sent_methods().len(), 3, "{notice}");
    }
}

#[test]
fn backpressure_resync_resubscribes() {
    let (t, h) = session();
    let mut m = connected(t.on("bn.subscribe", subscribe_reply()));
    let mut notice = fixture("resync");
    notice["params"]["reason"] = json!("backpressure");
    h.push(notice);
    m.poll();
    assert_eq!(h.sent_methods().last().unwrap(), "bn.subscribe");
    assert_eq!(h.sent_methods().len(), 3);
}

#[test]
fn resync_mid_command_rebuilds_the_mirror_and_keeps_the_command_outcome() {
    let (t, h) = session();
    let fresh = connected(session().0);
    let mut m = connected(
        t.on("bn.command.submit", walk_reply())
            .on("bn.subscribe", subscribe_reply()),
    );
    assert!(m.invoke_action("RIGHT"));
    h.push(fixture("events_41_walk"));
    h.push(fixture("events_42_message"));
    m.poll();
    assert_ne!(m.origin(), fresh.origin(), "the stream moved the avatar");
    // The engine resyncs and, in the same publication, reports how the command ended.
    h.push(fixture("resync"));
    h.push(fixture("command_completed"));
    m.poll();
    assert_eq!(h.sent_methods().last().unwrap(), "bn.subscribe");
    assert_eq!(m.origin(), fresh.origin());
    assert_eq!(m.player(), fresh.player());
    assert_eq!(m.size(), fresh.size());
    for at in [Pos { x: 21, y: 11 }, Pos { x: 23, y: 12 }] {
        assert_eq!(
            (m.cell(at).glyph, m.cell(at).desc),
            (fresh.cell(at).glyph, fresh.cell(at).desc)
        );
    }
    assert_eq!(m.ground(), fresh.ground());
    assert_eq!(m.inventory(), fresh.inventory());
    let shown: Vec<_> = m.native_log().iter().map(|l| l.shown()).collect();
    assert_eq!(
        shown,
        ["You walk east."],
        "the log read so far is kept once"
    );
    assert!(!m.busy(), "the finished command no longer blocks input");
    let status = m.command().unwrap();
    assert_eq!((status.stage.as_str(), status.finished), ("done", true));
}

#[test]
fn duplicate_events_do_not_resubscribe_or_duplicate_messages() {
    let (t, h) = session();
    let mut m = connected(t);
    h.push(fixture("events_41_walk"));
    h.push(fixture("events_42_message"));
    h.push(fixture("events_42_message"));
    m.poll();
    assert_eq!(h.sent_methods().len(), 2);
    let shown: Vec<_> = m.native_log().iter().map(|l| l.shown()).collect();
    assert_eq!(shown, ["You walk east."]);
    assert!(
        m.messages().is_empty(),
        "native lines are not command lines"
    );
}

fn logged(sequence: u32, id: &str, text: &str, count: u32) -> serde_json::Value {
    let label = if count > 1 {
        format!("{text} x {count}")
    } else {
        text.to_string()
    };
    serde_json::json!({"jsonrpc": "2.0", "method": "bn.events", "params": {"epoch": "e7f3", "events": [{
        "sequence": sequence.to_string(), "revision": "32", "type": "message.logged",
        "data": {"id": id, "text": text, "kind": "bad", "color": "c_red", "count": count, "label": label}}]}})
}

#[test]
fn native_log_keeps_order_colors_and_replaces_a_repeat_in_place() {
    let (t, h) = session();
    let mut m = connected(t);
    h.push(fixture("events_41_walk"));
    h.push(logged(42, "7", "You open the door.", 1));
    h.push(logged(43, "8", "Bang!", 1));
    h.push(logged(44, "8", "Bang!", 2));
    m.poll();
    let shown: Vec<_> = m.native_log().iter().map(|l| l.shown()).collect();
    assert_eq!(shown, ["You open the door.", "Bang! x 2"]);
    assert!(m.native_log().iter().all(|l| l.color.is_some()));
    assert_ne!(m.native_log()[0].color, Some(Rgb(0, 0, 0)));
}

#[test]
fn interaction_menu_lists_choices_and_choose_cancel_go_through_submit() {
    let (t, h) = session();
    let mut m = connected(t.on("bn.command.submit", walk_reply()));
    assert!(m.menu().is_none());
    h.push(fixture("events_41_walk"));
    h.push(fixture("events_42_message"));
    h.push(fixture("events_43_menu"));
    m.poll();
    let menu = m.menu().unwrap();
    assert_eq!(menu.title, "Pick up");
    assert!(menu.can_cancel);
    assert_eq!(menu.choices.len(), 2);
    assert_eq!(
        (menu.choices[1].enabled, menu.choices[1].denial.as_str()),
        (false, "Too heavy")
    );

    m.choose("c:2");
    assert!(
        submits(&h).is_empty(),
        "disabled choices are never submitted"
    );
    m.choose("c:1");
    let sent = submits(&h);
    assert_eq!(
        sent[0]["params"]["operation"],
        json!({"kind":"choose","choice_id":"c:1"})
    );
    assert_eq!(sent[0]["params"]["expect"]["schema_id"], "s:pickup");
    assert_eq!(sent[0]["params"]["expect"]["boundary_id"], "b:13");
}

#[test]
fn cancel_menu_submits_cancel_only_when_allowed() {
    let (t, h) = session();
    let mut m = connected(t.on("bn.command.submit", walk_reply()));
    m.cancel_menu();
    assert!(submits(&h).is_empty(), "no menu, nothing to cancel");
    h.push(fixture("events_41_walk"));
    h.push(fixture("events_42_message"));
    h.push(fixture("events_43_menu"));
    m.poll();
    m.cancel_menu();
    assert_eq!(
        submits(&h)[0]["params"]["operation"],
        json!({"kind":"cancel"})
    );
}

#[test]
fn pick_up_and_drop_send_the_engines_row_id_and_count() {
    let (t, h) = session();
    let mut m = connected(
        t.on("bn.command.submit", walk_reply())
            .on("bn.command.submit", walk_reply()),
    );
    let mut ground = item("tin can", 2, None, "2 tin cans");
    ground["id"] = json!("item:g1");
    let mut carried = item("flashlight", 1, Some("carried"), "flashlight");
    carried["id"] = json!("item:i1");
    h.push(avatar_event(
        json!([carried]),
        json!([ground, item("no id", 1, None, "no id")]),
    ));
    m.poll();
    m.pick_up(1);
    assert!(submits(&h).is_empty(), "a row without an id names nothing");
    m.pick_up(0);
    assert_eq!(
        submits(&h)[0]["params"]["operation"],
        json!({"kind":"pickup","item_id":"item:g1","count":2})
    );
    h.push(fixture("command_completed"));
    m.poll();
    m.drop_item(0);
    assert_eq!(
        submits(&h)[1]["params"]["operation"],
        json!({"kind":"drop","item_id":"item:i1","count":1})
    );
}

#[test]
fn unknown_notifications_are_ignored() {
    let (t, h) = session();
    let mut m = connected(t);
    h.push(json!({"jsonrpc":"2.0","method":"bn.future","params":{}}));
    m.poll();
    assert!(m.live(), "unknown notifications are ignored");
}

/// A subscribe result at a menu: the 0.2 example with the `interaction` the bespoke menus still read.
fn menu_snapshot(name: &str) -> Value {
    let text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/menu-interaction.json"
    ))
    .unwrap();
    serde_json::from_str::<Value>(&text).unwrap()[name].clone()
}

#[test]
fn main_menu_snapshot_without_a_world_shows_the_menu_and_no_map() {
    let menu = menu_snapshot("subscribe result at a menu");
    let (t, _h) = ScriptedTransport::new();
    let t = t.on("bn.hello", vec![fixture("hello_result")]).on(
        "bn.subscribe",
        vec![json!({"jsonrpc":"2.0","id":0,"result":menu})],
    );
    let m = connected(t);
    assert_eq!(m.size(), Pos { x: 0, y: 0 });
    let menu = m.menu().unwrap();
    assert_eq!(menu.title, "Cataclysm: Bright Nights");
    assert_eq!(menu.choices[0].label, "New Game");
}

#[test]
fn tabbed_main_menu_separates_tab_rows_from_the_selected_tabs_entries() {
    let menu = menu_snapshot("subscribe result at the main menu");
    let (t, _h) = ScriptedTransport::new();
    let t = t.on("bn.hello", vec![fixture("hello_result")]).on(
        "bn.subscribe",
        vec![json!({"jsonrpc":"2.0","id":0,"result":menu})],
    );
    let menu = connected(t).menu().unwrap();
    let ids = |rows: Vec<&bn_client_core::MenuChoice>| {
        rows.iter().map(|c| c.id.clone()).collect::<Vec<_>>()
    };
    assert_eq!(ids(menu.tabs()), ["tab:new_game", "tab:load", "tab:quit"]);
    assert_eq!(
        ids(menu.entries()),
        ["new_game:custom", "new_game:tutorial"]
    );
    assert!(menu.tabs()[0].selected && !menu.tabs()[1].selected);
}

#[test]
fn origin_reports_the_view_offset_so_targets_survive_recentering() {
    let (t, _h) = session();
    let m = connected(t);
    // Avatar (3, 4) is drawn at view (20, 12).
    assert_eq!(m.origin(), Pos { x: -17, y: -8 });
}

#[test]
fn cell_colors_come_from_the_game_palette() {
    use bn_client_core::colors::ColorMap;
    let json = r#"[{"type":"colordef","BLACK":[0,0,0],"RED":[1,0,0],"GREEN":[2,0,0],"BROWN":[3,0,0],
    "BLUE":[4,0,0],"MAGENTA":[5,0,0],"CYAN":[6,0,0],"GRAY":[7,0,0],"DGRAY":[8,0,0],"LRED":[9,0,0],
    "LGREEN":[10,0,0],"YELLOW":[11,0,0],"LBLUE":[12,0,0],"LMAGENTA":[13,0,0],"LCYAN":[14,0,0],"WHITE":[15,0,0]}]"#;
    let (t, _h) = session();
    let mut m = EngineModel::new(Box::new(t)).with_colors(ColorMap::from_json(json).unwrap());
    m.poll();
    // The zombie is light_green, palette slot 10.
    assert_eq!(m.cell(Pos { x: 23, y: 12 }).color, Rgb(10, 0, 0));
    assert_eq!(
        m.colors().index(10),
        Rgb(10, 0, 0),
        "renderers resolve names with the palette the model loaded"
    );
}

fn polled(t: ScriptedTransport) -> EngineModel {
    let mut m = EngineModel::new(Box::new(t));
    m.poll();
    m
}

#[test]
fn actions_list_names_ids_and_keys_from_the_boundary() {
    let (t, _h) = session();
    let m = polled(t);
    let a = m.actions();
    let right = a.iter().find(|a| a.id == "RIGHT").unwrap();
    assert_eq!(
        (right.name.as_str(), right.keys.as_slice()),
        (
            "Move right",
            ["RIGHT".to_string(), "l".to_string()].as_slice()
        )
    );
    assert_eq!(a.iter().find(|a| a.id == "pickup").unwrap().keys, ["g"]);
}

#[test]
fn pressing_a_bound_key_submits_the_same_action_id() {
    let (t, h) = session();
    let mut m = polled(t.on("bn.command.submit", walk_reply()));
    assert!(m.press("g"));
    assert_eq!(
        submits(&h)[0]["params"]["operation"],
        json!({"kind":"action","action_id":"pickup"})
    );
    assert!(!m.press("l"), "busy: the key is not consumed");
    assert_eq!(submits(&h).len(), 1);
}

#[test]
fn keys_without_an_offered_action_are_not_consumed() {
    let (t, h) = session();
    let mut m = polled(t);
    assert!(!m.press("q"));
    assert!(
        !m.press("x"),
        "bound in the game but not offered by this boundary"
    );
    assert!(submits(&h).is_empty());
}

#[test]
fn clicking_an_action_invokes_it_by_id_and_unknown_ids_are_refused() {
    let (t, h) = session();
    let mut m = polled(t.on("bn.command.submit", walk_reply()));
    assert!(!m.invoke_action("nope"));
    assert!(m.invoke_action("LEFT"));
    assert_eq!(
        submits(&h)[0]["params"]["operation"],
        json!({"kind":"action","action_id":"LEFT"})
    );
}

fn pos(x: i32, y: i32) -> Value {
    json!({"dim": "", "x": x, "y": y, "z": 0})
}

/// An event carrying only `changes`, numbered after the fixture session (sequence 40).
fn change(sequence: u32, changes: Value) -> Value {
    json!({"jsonrpc":"2.0","method":"bn.events","params":{"epoch":"e7f3","events":[{
        "sequence": sequence.to_string(), "revision": (sequence - 9).to_string(),
        "type": "cells.seen", "command": "c:5", "changes": changes}]}})
}

fn boundary_without_actions() -> Value {
    let mut b = fixture("subscribe_result")["result"]["interaction"].clone();
    b["actions"] = json!([]);
    b
}

#[test]
fn travel_click_submits_the_absolute_square_at_a_free_boundary() {
    let (t, h) = session();
    let mut m = connected(t.on("bn.command.submit", walk_reply()));
    // Avatar (3, 4) is drawn at view (20, 12).
    m.travel(Pos { x: 22, y: 12 });
    let sent = submits(&h);
    assert_eq!(
        sent[0]["params"]["operation"],
        json!({"kind":"travel","pos":pos(5, 4)})
    );
    assert_eq!(sent[0]["params"]["expect"]["schema_id"], Value::Null);
    assert!(m.busy());
    m.travel(Pos { x: 23, y: 12 });
    assert_eq!(submits(&h).len(), 1, "one command at a time");
}

#[test]
fn travel_click_is_ignored_while_a_menu_is_open() {
    let (t, h) = session();
    let mut m = connected(t.on("bn.command.submit", walk_reply()));
    for name in ["events_41_walk", "events_42_message", "events_43_menu"] {
        h.push(fixture(name));
    }
    m.poll();
    assert!(m.menu().is_some());
    m.travel(Pos { x: 22, y: 12 });
    assert!(submits(&h).is_empty());
}

#[test]
fn route_is_replaced_by_each_event_and_cleared_by_an_empty_one() {
    let (t, h) = session();
    let mut m = connected(t);
    assert!(m.route().is_empty());
    h.push(change(41, json!({"route": [pos(4, 4), pos(5, 4)]})));
    m.poll();
    assert_eq!(m.route(), [Pos { x: 21, y: 12 }, Pos { x: 22, y: 12 }]);
    h.push(change(42, json!({"cells": []})));
    m.poll();
    assert_eq!(m.route().len(), 2, "events without route keep the plan");
    h.push(change(43, json!({"route": []})));
    m.poll();
    assert!(m.route().is_empty());
}

#[test]
fn route_in_the_snapshot_header_is_decoded() {
    let mut sub = fixture("subscribe_result");
    sub["result"]["route"] = json!([pos(4, 4), pos(4, 5)]);
    let (t, _h) = ScriptedTransport::new();
    let t = t.on("bn.hello", vec![fixture("hello_result")]).on(
        "bn.subscribe",
        vec![sub, fixture("snapshot_part_0"), fixture("snapshot_part_1")],
    );
    let m = connected(t);
    assert_eq!(m.route(), [Pos { x: 21, y: 12 }, Pos { x: 21, y: 13 }]);
}

#[test]
fn empty_action_list_between_auto_move_steps_keeps_the_controls_until_idle() {
    let (t, h) = session();
    let mut m = connected(t.on("bn.command.submit", walk_reply()));
    let before = m.actions();
    assert!(!before.is_empty());
    m.travel(Pos { x: 22, y: 12 });
    h.push(change(
        41,
        json!({"interaction": boundary_without_actions()}),
    ));
    m.poll();
    assert!(m.busy());
    assert_eq!(m.actions(), before, "no flicker while a step is in flight");
    h.push(json!({"jsonrpc":"2.0","method":"bn.command","params":{
        "epoch":"e7f3","command_id":"c:5","stage":"completed"}}));
    m.poll();
    assert!(!m.busy());
    assert!(
        m.actions().is_empty(),
        "an idle boundary with none shows none"
    );
}

fn interrupted_by(error: &str) -> (EngineModel, Vec<String>) {
    let (t, h) = session();
    let mut m = connected(t.on("bn.command.submit", walk_reply()));
    m.travel(Pos { x: 22, y: 12 });
    h.push(json!({"jsonrpc":"2.0","method":"bn.command","params":{
        "epoch":"e7f3","command_id":"c:5","stage":"interrupted","error":error,"error_text":"The game is not ready."}}));
    m.poll();
    assert!(!m.busy());
    let messages = m.messages().to_vec();
    (m, messages)
}

#[test]
fn interrupted_command_shows_the_engine_error_reason() {
    let (_, messages) = interrupted_by("not_ready");
    assert_eq!(messages.last().unwrap(), "The game is not ready.");
}

#[test]
fn world_swap_interruption_is_not_reported_as_a_failure() {
    let (_, messages) = interrupted_by("stale_epoch");
    assert!(messages.is_empty(), "{messages:?}");
}

fn with_door(h: &bn_client_core::transport::ScriptHandle, x: i32) {
    let look = json!({"kind":"terrain","id":"t_door_c","glyph":"+","color":"brown"});
    let cell = json!({"at": pos(x, 4), "known": "visible", "terrain": look});
    h.push(change(41, json!({"cells": [cell]})));
}

#[test]
fn clicking_an_adjacent_closed_door_sends_the_same_travel_as_any_square() {
    let (t, h) = session();
    let mut m = connected(t.on("bn.command.submit", walk_reply()));
    m.travel(Pos { x: 21, y: 12 });
    assert_eq!(submits(&h)[0]["params"]["operation"]["kind"], "travel");
}

#[test]
fn clicking_a_distant_closed_door_still_asks_the_engine_for_a_route() {
    let (t, h) = session();
    let mut m = connected(t.on("bn.command.submit", walk_reply()));
    with_door(&h, 6);
    m.poll();
    m.travel(Pos { x: 23, y: 12 });
    assert_eq!(
        submits(&h)[0]["params"]["operation"],
        json!({"kind":"travel","pos":pos(6, 4)})
    );
}

#[test]
fn travel_that_plans_nothing_is_silent_like_native_where_a_failed_click_route_prints_no_message() {
    let (t, h) = session();
    let mut m = connected(t.on("bn.command.submit", walk_reply()));
    m.travel(Pos { x: 22, y: 12 });
    h.push(json!({"jsonrpc":"2.0","method":"bn.command","params":{
        "epoch":"e7f3","command_id":"c:5","stage":"completed"}}));
    m.poll();
    assert_eq!(m.command().unwrap().stage, "done");
    assert!(m.messages().is_empty());
}

#[test]
fn travel_that_planned_a_route_is_simply_done() {
    let (t, h) = session();
    let mut m = connected(t.on("bn.command.submit", walk_reply()));
    m.travel(Pos { x: 22, y: 12 });
    h.push(change(41, json!({"route": [pos(4, 4), pos(5, 4)]})));
    h.push(json!({"jsonrpc":"2.0","method":"bn.command","params":{
        "epoch":"e7f3","command_id":"c:5","stage":"completed"}}));
    m.poll();
    assert_eq!(m.command().unwrap().stage, "done");
}

fn cell_event(sequence: u64, cell: Value) -> Value {
    json!({"jsonrpc":"2.0","method":"bn.events","params":{"epoch":"e7f3","events":[{
        "sequence": sequence.to_string(), "revision": (sequence - 9).to_string(),
        "type": "cells.seen", "changes": { "cells": [cell] }}]}})
}

#[test]
fn remembered_furniture_stays_drawn_from_its_memory_overlay() {
    let (t, h) = session();
    let mut m = connected(t);
    let at = json!({"dim":"","x":5,"y":4,"z":0});
    let view = Pos { x: 22, y: 12 };
    let floor = json!({"kind":"terrain","id":"t_floor","glyph":".","color":"c_cyan"});
    let bench = json!({"kind":"furniture","id":"f_bench","glyph":"{","color":"c_light_gray"});

    h.push(cell_event(
        41,
        json!({"at": at, "known": "visible", "terrain": floor, "furniture": bench}),
    ));
    m.poll();
    assert_eq!(m.cell(view).id, "f_bench", "seen furniture draws");

    h.push(cell_event(
        42,
        json!({"at": at, "known": "remembered", "memory": {"terrain": floor, "overlay": bench}}),
    ));
    m.poll();
    let remembered = m.cell(view);
    assert_eq!((remembered.glyph, remembered.id.as_str()), ('{', "f_bench"));

    h.push(cell_event(
        43,
        json!({"at": at, "known": "remembered", "memory": {"terrain": floor}}),
    ));
    m.poll();
    assert_eq!(
        m.cell(view).glyph,
        '.',
        "bare remembered floor draws terrain"
    );
}

#[test]
fn travel_to_a_square_the_engine_cannot_resolve_is_reported_once_in_words() {
    let (t, h) = session();
    let mut m = connected(
        t.on("bn.command.submit", walk_reply())
            .on("bn.command.submit", walk_reply()),
    );
    for id in ["c:5", "c:5"] {
        m.travel(Pos { x: 39, y: 12 });
        h.push(json!({"jsonrpc":"2.0","method":"bn.command","params":{
            "epoch":"e7f3","command_id":id,"stage":"rejected","error":"validation_failed",
            "error_text":"Travel rejected: outside the map window."}}));
        m.poll();
        assert!(!m.busy());
    }
    assert_eq!(m.messages(), ["Travel rejected: outside the map window."]);
}

fn view_event(sequence: u64, min_x: i32, max_x: i32) -> Value {
    json!({"jsonrpc":"2.0","method":"bn.events","params":{"epoch":"e7f3","events":[{
        "sequence": sequence.to_string(), "revision": (sequence - 9).to_string(),
        "type": "cells.seen", "changes": { "view": {
            "min": {"dim":"","x":min_x,"y":-8,"z":0}, "max": {"dim":"","x":max_x,"y":16,"z":0}}}}]}})
}

#[test]
fn hello_declares_the_map_size_the_client_wants() {
    let (t, h) = session();
    let _ = connected(t);
    assert_eq!(
        h.sent()[0]["params"]["viewport"],
        json!({"cols": 40, "rows": 25})
    );
}

#[test]
fn a_resized_panel_is_sent_once_after_it_settles() {
    let (t, h) = session();
    let clock = bn_client_core::Clock::manual();
    let mut m = connected(t.on(
        "bn.viewport",
        vec![json!({"jsonrpc":"2.0","id":null,"result":{}})],
    ))
    .with_clock(clock.clone());
    let viewports = |h: &bn_client_core::transport::ScriptHandle| {
        h.sent()
            .into_iter()
            .filter(|s| s["method"] == "bn.viewport")
            .map(|s| s["params"].clone())
            .collect::<Vec<_>>()
    };
    m.set_viewport(Pos { x: 50, y: 30 });
    m.set_viewport(Pos { x: 52, y: 30 });
    m.poll();
    assert!(viewports(&h).is_empty(), "still changing: nothing sent yet");
    clock.advance(std::time::Duration::from_millis(300));
    m.poll();
    m.poll();
    assert_eq!(viewports(&h), [json!({"cols": 52, "rows": 30})]);
    m.set_viewport(Pos { x: 52, y: 30 });
    clock.advance(std::time::Duration::from_millis(300));
    m.poll();
    assert_eq!(
        viewports(&h).len(),
        1,
        "an unchanged size is not sent again"
    );
}

#[test]
fn the_shown_map_is_the_middle_of_the_engines_view_and_never_larger() {
    let (t, h) = session();
    let mut m = connected(t);
    assert_eq!(
        m.size(),
        Pos { x: 40, y: 25 },
        "no view reported yet: as asked"
    );
    // A 36-wide engine window around the avatar at (3, 4): the map shrinks to it.
    h.push(view_event(41, -15, 20));
    m.poll();
    assert_eq!(m.size(), Pos { x: 36, y: 25 });
    assert_eq!(m.origin(), Pos { x: -15, y: -8 });
    assert_eq!(m.player(), Pos { x: 18, y: 12 });
    // A larger window than asked for is clipped to the middle.
    m.set_viewport(Pos { x: 20, y: 10 });
    h.push(view_event(42, -15, 20));
    m.poll();
    assert_eq!(m.size(), Pos { x: 20, y: 10 });
    assert_eq!(m.origin(), Pos { x: -7, y: -1 });
    let me = m.player();
    assert!(
        (0..20).contains(&me.x) && (0..10).contains(&me.y),
        "avatar stays in view: {me:?}"
    );
}

fn not_ready() -> Value {
    json!({"jsonrpc":"2.0","id":null,"error":{"code":1000,"message":"x","data":{"kind":"not_ready","message":"There is nothing to interrupt."}}})
}

#[test]
fn interrupt_sends_bn_interrupt_while_a_command_is_in_flight() {
    let (t, h) = session();
    let mut m = connected(t.on("bn.command.submit", walk_reply()).on(
        "bn.interrupt",
        vec![json!({"jsonrpc":"2.0","id":null,"result":{}})],
    ));
    assert!(m.invoke_action("RIGHT"));
    assert!(m.busy());
    assert!(m.interrupt());
    m.poll();
    assert_eq!(h.sent_methods().last().unwrap(), "bn.interrupt");
    assert_eq!(h.sent().last().unwrap()["params"], json!({}));
    assert!(m.messages().is_empty(), "{:?}", m.messages());
}

#[test]
fn interrupt_with_no_command_in_flight_sends_nothing() {
    let (t, h) = session();
    let mut m = connected(t);
    assert!(!m.interrupt());
    assert!(!h.sent_methods().contains(&"bn.interrupt".to_string()));
}

#[test]
fn a_not_ready_interrupt_settles_a_command_whose_completion_was_never_heard() {
    let (t, h) = session();
    let mut m = connected(
        t.on("bn.command.submit", walk_reply())
            .on("bn.interrupt", vec![not_ready()])
            .on(
                "bn.command.result",
                vec![json!({"jsonrpc":"2.0","id":null,"result":{
                    "epoch":"e7f3","command_id":"c:5","stage":"completed"}})],
            ),
    );
    assert!(m.invoke_action("RIGHT"));
    assert!(m.busy());
    assert!(m.interrupt());
    m.poll();
    m.poll();
    assert!(h.sent_methods().contains(&"bn.command.result".to_string()));
    assert!(!m.busy(), "the engine's answer ends the command");
    assert_eq!(m.command().unwrap().stage, "done");
}

#[test]
fn a_not_ready_interrupt_keeps_a_command_the_engine_still_runs() {
    let (t, _h) = session();
    let mut m = connected(
        t.on("bn.command.submit", walk_reply())
            .on("bn.interrupt", vec![not_ready()])
            .on(
                "bn.command.result",
                vec![json!({"jsonrpc":"2.0","id":null,"result":{
                    "epoch":"e7f3","command_id":"c:5","stage":"executing"}})],
            ),
    );
    assert!(m.invoke_action("RIGHT"));
    assert!(m.interrupt());
    m.poll();
    m.poll();
    assert!(m.busy());
}

#[test]
fn a_not_ready_interrupt_is_explained_once() {
    let (t, _h) = session();
    let mut m = connected(
        t.on("bn.command.submit", walk_reply())
            .on("bn.interrupt", vec![not_ready()])
            .on("bn.interrupt", vec![not_ready()]),
    );
    assert!(m.invoke_action("RIGHT"));
    assert!(m.interrupt());
    m.poll();
    assert!(m.interrupt());
    m.poll();
    let said: Vec<_> = m
        .messages()
        .iter()
        .filter(|l| l.contains("nothing to interrupt"))
        .collect();
    assert_eq!(said.len(), 1, "{:?}", m.messages());
    assert!(!m.messages().iter().any(|l| l.contains("not_ready")));
}

/// The fixture world with sprite facts: a connected wall with a light level, a remembered square, a
/// zombie facing left and an avatar wearing a jacket.
fn tiled_session() -> EngineModel {
    let mut subscribe = fixture("subscribe_result");
    subscribe["result"]["environment"]["season"] = json!("winter");
    subscribe["result"]["avatar"]["look"] = json!({
        "kind": "avatar", "id": "avatar", "glyph": "@", "color": "white",
        "tile": "player_female", "facing": "right"
    });
    subscribe["result"]["avatar"]["overlays"] = json!([{
        "kind": "overlay", "id": "worn_jacket", "glyph": "", "color": "",
        "tile": "overlay_female_worn_jacket", "looks_like": ["overlay_worn_jacket"],
        "tint_keys": ["VARSIZE", "jacket_light"]
    }, {
        "kind": "overlay", "id": "mutation_hair_long", "glyph": "", "color": "",
        "tile": "overlay_female_mutation_hair_long",
        "mutation": {"id": "hair_long", "types": ["hair_style"], "flags": []}
    }]);
    subscribe["result"]["avatar"]["traits"] = json!({"hair_color": ["hair_black"]});
    subscribe["result"]["entities"][0]["look"]
        .as_object_mut()
        .unwrap()
        .extend(
            json!({"facing": "left", "looks_like": ["mon_zombie_base"]})
                .as_object()
                .unwrap()
                .clone(),
        );
    subscribe["result"]["entities"][0]["attitude"] = json!("hostile");
    subscribe["result"]["entities"][0]["aware"] = json!(true);
    let mut part = fixture("snapshot_part_1");
    part["params"]["cells"].as_array_mut().unwrap().extend([
        json!({"at": {"dim": "", "x": 9, "y": 8, "z": 0}, "known": "visible", "light": 1,
            "terrain": {"kind": "terrain", "id": "t_wall", "glyph": "|", "color": "light_gray",
                "looks_like": ["t_wall_base"], "subtile": "edge", "rotation": 1,
                "tint_keys": ["WALL", "t_wall"], "tint_vars": {"fg": {"color": "#102030"}}},
            "reviving": true, "cargo": true,
            "vehicle": {"kind": "vehicle_part", "id": "door", "glyph": "+", "color": "white",
                "tile": "vp_door", "tint": {"fg": "#445566"}},
            "items": [
                {"kind": "item", "id": "corpse", "glyph": "%", "color": "red", "tile": "corpse_mon_zombie",
                 "looks_like": ["corpse"], "stack": 2, "tint_keys": ["corpse"],
                 "tint_vars": {"fg": {"color": "#ff3030", "blend_mode": "multiply", "brightness": 0.5}}}
            ]}),
        json!({"at": {"dim": "", "x": 9, "y": 6, "z": -1}, "known": "visible", "light": 3,
            "terrain": {"kind": "terrain", "id": "t_dirt", "glyph": ".", "color": "brown"}}),
        json!({"at": {"dim": "", "x": 10, "y": 6, "z": -1}, "known": "visible", "light": 3,
            "vehicle": {"kind": "vehicle_part", "id": "roof", "glyph": "#", "color": "white",
                "tile": "vp_roof", "tint": {"fg": "#f7fbf5"}}}),
        json!({"at": {"dim": "", "x": 11, "y": 8, "z": 0}, "known": "visible", "light": 3,
            "fields": [{"look": {"kind": "field", "id": "fd_acid_water", "glyph": "~", "color": "green",
                "tint": {"bg": "#00aa00", "fg": "#00aa00"}}, "intensity": 1}]}),
        json!({"at": {"dim": "", "x": 9, "y": 6, "z": 0}, "known": "visible", "below": 1}),
        json!({"at": {"dim": "", "x": 10, "y": 6, "z": 0}, "known": "visible", "below": 1}),
        json!({"at": {"dim": "", "x": 9, "y": 5, "z": 0}, "known": "visible", "below": 1}),
        json!({"at": {"dim": "", "x": 12, "y": 6, "z": 0}, "known": "visible", "below": 1,
            "terrain": {"kind": "terrain", "id": "t_open_air", "glyph": " ", "color": "c_dark_gray"},
            "fields": [{"look": {"kind": "field", "id": "fd_smoke", "glyph": "8", "color": "c_dark_gray"},
                "intensity": 1, "hides_items": true}]}),
        json!({"at": {"dim": "", "x": 12, "y": 6, "z": -1}, "known": "visible", "light": 3,
            "terrain": {"kind": "terrain", "id": "t_dirt", "glyph": ".", "color": "brown"}}),
        json!({"at": {"dim": "", "x": 14, "y": 6, "z": 0}, "known": "visible", "below": 2}),
        json!({"at": {"dim": "", "x": 14, "y": 6, "z": -1}, "known": "visible", "light": 2,
            "fields": [{"look": {"kind": "field", "id": "fd_smoke", "name": "smoke", "glyph": "8",
                "color": "c_dark_gray"}, "intensity": 1, "hides_items": true}]}),
        json!({"at": {"dim": "", "x": 14, "y": 6, "z": -2}, "known": "visible", "light": 3,
            "terrain": {"kind": "terrain", "id": "t_rock_floor", "glyph": ".", "color": "light_gray"}}),
        json!({"at": {"dim": "", "x": 13, "y": 6, "z": -1}, "known": "visible", "light": 3,
            "terrain": {"kind": "terrain", "id": "t_dirt", "glyph": ".", "color": "brown"}}),
        json!({"at": {"dim": "", "x": 9, "y": 5, "z": -1}, "known": "remembered",
            "memory": {"terrain": {"kind": "terrain", "id": "t_grass", "glyph": "\"", "color": "green"}}}),
        json!({"at": {"dim": "", "x": 9, "y": 7, "z": 0}, "known": "remembered",
            "memory": {"terrain": {"kind": "terrain", "id": "t_grass", "glyph": "\"", "color": "green",
                "subtile": "center", "rotation": 0},
                "overlay": {"kind": "furniture", "id": "f_bench", "glyph": "H", "color": "brown",
                "subtile": "unconnected", "rotation": 2}}}),
    ]);
    let (t, _) = ScriptedTransport::new();
    connected(t.on("bn.hello", vec![fixture("hello_result")]).on(
        "bn.subscribe",
        vec![subscribe, fixture("snapshot_part_0"), part],
    ))
}

#[test]
fn a_cell_lists_its_sprites_bottom_first_with_the_engines_tile_selection() {
    let m = tiled_session();
    assert_eq!(m.season().as_deref(), Some("winter"));
    let origin = m.origin();
    let cell = m.cell(Pos {
        x: 9 - origin.x,
        y: 8 - origin.y,
    });
    let layers = cell
        .layers
        .iter()
        .map(|l| l.id.as_str())
        .collect::<Vec<_>>();
    // The item is named by its sprite id, the corpse of the monster, not the item type.
    // A pile of two is highlighted over its displayed item, with a sprite that has no glyph.
    // The paint of a vehicle part reaches its foreground sprite only; a loaded cargo part is highlighted over it
    // and a corpse that can rise is marked last.
    assert_eq!(
        layers,
        [
            "t_wall",
            "corpse_mon_zombie",
            "highlight_item",
            "vp_door",
            "highlight_item",
            "zombie_revival_indicator"
        ]
    );
    assert_eq!(
        cell.layers[3].tint_fg,
        Some(bn_client_core::Rgb(0x44, 0x55, 0x66))
    );
    assert_eq!(cell.layers[3].tint_bg, None);
    assert_eq!(cell.layers[0].tint_fg, None);
    assert_eq!(cell.layers[5].glyph, '\0');
    assert_eq!(cell.layers[4].glyph, '\0');
    assert_eq!(cell.layers[2].glyph, '\0');
    let wall = &cell.layers[0];
    assert_eq!((wall.subtile.as_deref(), wall.rotation), (Some("edge"), 1));
    assert_eq!(wall.fallbacks, ["t_wall_base"]);
    assert_eq!(cell.layers[1].fallbacks, ["corpse"]);
    // The keys of a terrain and the variables of an item reach the layers the tileset paints.
    assert_eq!(wall.tint_keys, ["WALL", "t_wall"]);
    let vars = cell.layers[1].tint_vars.as_ref().unwrap();
    assert_eq!(vars.fg.color.as_deref(), Some("#ff3030"));
    assert_eq!(vars.fg.brightness, Some(0.5));
    assert_eq!(vars.bg.color, None);
    assert_eq!(
        wall.tint_vars.as_ref().unwrap().fg.color.as_deref(),
        Some("#102030")
    );
    assert_eq!(cell.light, Some(1));
    assert!(!cell.remembered);
}

#[test]
fn a_remembered_cell_lists_its_memorized_terrain_and_overlay_and_is_marked_remembered() {
    let m = tiled_session();
    let origin = m.origin();
    let cell = m.cell(Pos {
        x: 9 - origin.x,
        y: 7 - origin.y,
    });
    assert!(cell.remembered);
    assert_eq!(cell.light, None);
    let layers = cell
        .layers
        .iter()
        .map(|l| (l.id.as_str(), l.subtile.as_deref(), l.rotation))
        .collect::<Vec<_>>();
    assert_eq!(
        layers,
        [
            ("t_grass", Some("center"), 0),
            ("f_bench", Some("unconnected"), 2)
        ]
    );
}

#[test]
fn creatures_and_the_avatar_draw_over_the_terrain_with_their_overlays() {
    let m = tiled_session();
    let at = |x: i32, y: i32| {
        let origin = m.origin();
        m.cell(Pos {
            x: x - origin.x,
            y: y - origin.y,
        })
    };
    // The zombie faces left: mirrored, with its data's fallbacks, over the dirt it stands on.
    let zombie = at(6, 4);
    let ids = zombie
        .layers
        .iter()
        .map(|l| l.id.as_str())
        .collect::<Vec<_>>();
    // The native view marks a monster that is aware of the avatar with its attitude.
    assert_eq!(ids, ["t_dirt", "mon_zombie", "overlay_hostile_sees_player"]);
    let sprite = &zombie.layers[1];
    assert!(sprite.mirrored && sprite.creature);
    assert_eq!(sprite.fallbacks, ["mon_zombie_base"]);
    // The avatar stands over the items on the ground: its gendered sprite, then the jacket, which has
    // no glyph to fall back on.
    let me = at(3, 4);
    let ids = me.layers.iter().map(|l| l.id.as_str()).collect::<Vec<_>>();
    assert_eq!(
        ids,
        [
            "t_dirt",
            "tin_can",
            "player_female",
            "overlay_female_worn_jacket",
            "overlay_female_mutation_hair_long"
        ]
    );
    // A mutation overlay carries what the tileset picks its paint by; a worn item does not.
    let hair = me.layers[4].mutation.as_ref().unwrap();
    assert_eq!(
        (hair.id.as_str(), hair.types.as_slice()),
        ("hair_long", ["hair_style".to_string()].as_slice())
    );
    assert_eq!(hair.traits["hair_color"], ["hair_black"]);
    assert!(me.layers[3].mutation.is_none());
    assert_eq!(me.layers[3].tint_keys, ["VARSIZE", "jacket_light"]);
    assert!(!me.layers[2].mirrored && me.layers[2].creature);
    assert_eq!(me.layers[2].glyph, '@');
    assert_eq!(me.layers[3].glyph, '\0');
    assert_eq!(me.layers[3].fallbacks, ["overlay_worn_jacket"]);
    // A square without sprite facts keeps its ASCII look.
    let plain = at(2, 2);
    assert_eq!((plain.glyph, plain.layers.len()), ('.', 1));
}

#[test]
fn a_field_that_hides_items_draws_no_items_or_highlight_under_it() {
    let (t, h) = session();
    let mut m = connected(t);
    let at = json!({"dim":"","x":5,"y":4,"z":0});
    let floor = json!({"kind":"terrain","id":"t_floor","glyph":".","color":"c_cyan"});
    let rock = json!({"kind":"item","id":"rock","glyph":"*","color":"c_light_gray","stack":2});
    let field = |id: &str, hides: bool| {
        let mut f = json!({"look": {"kind":"field","id":id,"glyph":"8","color":"c_dark_gray"}, "intensity": 1});
        if hides {
            f["hides_items"] = json!(true);
        }
        f
    };
    let ids = |m: &EngineModel| {
        m.cell(Pos { x: 22, y: 12 })
            .layers
            .iter()
            .map(|l| l.id.clone())
            .collect::<Vec<_>>()
    };
    h.push(cell_event(
        41,
        json!({"at": at, "known": "visible", "terrain": floor, "items": [rock],
            "fields": [field("fd_smoke", true)]}),
    ));
    m.poll();
    assert_eq!(ids(&m), ["t_floor", "fd_smoke"]);
    h.push(cell_event(
        42,
        json!({"at": at, "known": "visible", "terrain": floor, "items": [rock],
            "fields": [field("fd_acid_water", false)]}),
    ));
    m.poll();
    assert_eq!(
        ids(&m),
        ["t_floor", "fd_acid_water", "rock", "highlight_item"]
    );
}

#[test]
fn a_look_with_a_display_name_is_described_by_it_instead_of_its_id() {
    let (t, h) = session();
    let mut m = connected(t);
    let at = json!({"dim":"","x":5,"y":4,"z":0});
    let floor =
        json!({"kind":"terrain","id":"t_floor","name":"floor","glyph":".","color":"c_cyan"});
    h.push(cell_event(
        41,
        json!({"at": at, "known": "visible", "terrain": floor}),
    ));
    m.poll();
    assert_eq!(m.cell(Pos { x: 22, y: 12 }).desc, "floor");
    assert_eq!(m.cell(Pos { x: 22, y: 12 }).id, "t_floor");
}

fn avatar_event(inventory: Value, ground: Value) -> Value {
    let mut event = fixture("events_41_walk");
    let avatar = &mut event["params"]["events"][0]["changes"]["avatar"];
    avatar["inventory"] = inventory;
    avatar["ground"] = ground;
    event
}

fn item(name: &str, count: u64, slot: Option<&str>, label: &str) -> Value {
    let mut row = json!({"look": {"kind": "item", "id": "x", "glyph": "/", "color": "white"}, "name": name, "count": count, "label": label});
    if let Some(slot) = slot {
        row["slot"] = json!(slot);
    }
    row
}

#[test]
fn panels_list_display_names_with_counts_and_where_items_are_kept() {
    let (t, h) = session();
    let mut m = connected(t);
    h.push(avatar_event(
        json!([
            item("combat knife", 1, Some("wielded"), "combat knife (wielded)"),
            item("fur scarf", 1, Some("worn"), "fur scarf (worn)"),
            item("hammer", 2, Some("carried"), "2 hammers")
        ]),
        json!([item(
            "light disposable battery (12/25)",
            3,
            None,
            "3 light disposable batteries (12/25)"
        )]),
    ));
    m.poll();
    assert_eq!(
        m.inventory(),
        ["combat knife (wielded)", "fur scarf (worn)", "2 hammers"]
    );
    assert_eq!(m.ground(), ["3 light disposable batteries (12/25)"]);
}

#[test]
fn a_condition_marker_keeps_its_native_colour_and_every_row_has_its_own_look() {
    let (t, h) = session();
    let mut m = connected(t);
    let mut worn = item(
        "||\u{a0}bra",
        1,
        Some("worn"),
        "<color_light_green>||</color>\u{a0}bra (worn)",
    );
    worn["durability"] = json!({"symbol": "||", "color": "light_green"});
    worn["look"] =
        json!({"kind": "item", "id": "bra", "tile": "bra_tile", "glyph": "[", "color": "pink"});
    h.push(avatar_event(
        json!([worn, item("hammer", 2, Some("carried"), "2 hammers")]),
        json!([item("rock", 1, None, "rock")]),
    ));
    m.poll();
    let runs = m.styled(&m.inventory()[0]);
    assert_eq!(
        runs.iter()
            .map(|r| (r.text.as_str(), r.color))
            .collect::<Vec<_>>(),
        [
            ("||", Some(ColorMap::default().resolve("light_green").0)),
            ("\u{a0}bra (worn)", None)
        ]
    );
    assert_eq!(
        m.styled(&m.inventory()[1]).len(),
        1,
        "no marker, no colour run"
    );
    let cells = m.inventory_cells();
    assert_eq!(cells.len(), 2);
    assert_eq!(cells[0].layers[0].id, "bra_tile");
    assert_eq!(cells[0].glyph, '[');
    assert_eq!(cells[0].color, ColorMap::default().resolve("pink").0);
    assert_eq!(m.ground_cells().len(), 1);
}

#[test]
fn an_avatar_that_carries_nothing_empties_the_inventory_panel() {
    let (t, h) = session();
    let mut m = connected(t);
    assert_eq!(m.inventory(), ["flashlight"]);
    let mut event = avatar_event(json!([]), json!([]));
    event["params"]["events"][0]["changes"]["avatar"]
        .as_object_mut()
        .unwrap()
        .retain(|k, _| k != "inventory" && k != "ground");
    h.push(event);
    m.poll();
    assert!(m.inventory().is_empty());
}

fn counted(name: &str) -> Value {
    let text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/menu-counts.json"
    ))
    .unwrap();
    serde_json::from_str::<Value>(&text).unwrap()[name].clone()
}

#[test]
fn counted_menu_exposes_context_field_details_and_sends_fill_and_set_count() {
    let (t, h) = session();
    let mut m = connected(t.on("bn.command.submit", walk_reply()));
    h.push(change(41, json!({"interaction": counted("counted")})));
    m.poll();
    let menu = m.menu().unwrap();
    assert_eq!(menu.context, "COUNTED");
    assert!(menu.can_set_count);
    let field = menu.field.unwrap();
    assert_eq!((field.id.as_str(), field.value.as_str()), ("filter", ""));
    let club = menu.choices.iter().find(|c| c.id == "item:aa").unwrap();
    assert!(club.description.starts_with("Time to complete"));
    assert_eq!(club.column("skill"), "fabrication 1");
    assert_eq!(club.column_label("skill"), "Skill");

    m.fill("nope", "club", true);
    assert!(submits(&h).is_empty(), "unknown fields are never submitted");
    m.set_count("item:bb", 2);
    assert!(submits(&h).is_empty(), "disabled rows take no count");
    m.fill("filter", "club", true);
    assert_eq!(
        submits(&h)[0]["params"]["operation"],
        json!({"kind":"fill","field_id":"filter","value":"club","submit":true})
    );
}

#[test]
fn set_count_submits_for_an_enabled_row_and_batch_mode_has_no_field() {
    let (t, h) = session();
    let mut m = connected(t.on("bn.command.submit", walk_reply()));
    h.push(change(41, json!({"interaction": counted("counted")})));
    m.poll();
    m.set_count("item:aa", 3);
    assert_eq!(
        submits(&h)[0]["params"]["operation"],
        json!({"kind":"set_count","choice_id":"item:aa","count":3})
    );
    let (t, h) = session();
    let mut m = connected(t.on("bn.command.submit", walk_reply()));
    h.push(change(41, json!({"interaction": counted("batch")})));
    m.poll();
    assert!(m.menu().unwrap().field.is_none());
    m.fill("filter", "x", true);
    assert!(submits(&h).is_empty());
}

/// An `interaction.changed` event at `sequence` with these actions and interaction.
fn show(sequence: u64, actions: Value, interaction: Value) -> Value {
    json!({"jsonrpc":"2.0","method":"bn.events","params":{"epoch":"e7f3","events":[{
        "sequence": sequence.to_string(), "revision": (sequence - 9).to_string(),
        "type": "interaction.changed",
        "changes": {"interaction": {
            "boundary_id": format!("b:{sequence}"),
            "readiness": {"phase":"playing","game_ready":true,
                "accepts_interaction_commands":true,"accepts_registered_actions":true},
            "actions": actions,
            "interaction": interaction}}}]}})
}

/// A boundary of the map with a target screen over it: the title, the cursor at `x`, what the screen says in
/// words, the facts of the cursor's square, and the affordance that moves the cursor.
fn target_stack(
    sequence: u64,
    title: &str,
    x: i32,
    status_text: Option<&str>,
    parts: Value,
) -> Value {
    let mut target = json!({"source":{"dim":"","x":2,"y":4,"z":0},"current":{"dim":"","x":x,"y":6,"z":0},
        "range":8,"distance_metric":"square","distance":(x - 2).abs(),"status":"look",
        "candidates":[],"parts":parts});
    if let Some(text) = status_text {
        target["status_text"] = text.into();
    }
    let mut event = show(sequence, json!([]), Value::Null);
    event["params"]["events"][0]["changes"]["interaction"]["stack"] = json!([
        {"screen_id":"screen:map","role":"map","root":null,"affordances":[]},
        {"screen_id":"screen:7","role":"dialog","title":title,
         "root":{"id":"target","kind":"panel","title":title,"child":{"id":"target:map","kind":"map",
             "affordances":["target:move"],"mode":"world","target":target}},
         "affordances":[{"id":"target:move","label":"Select","keys":[],"enabled":true,
             "args":{"pos":{"unit":"square"}},"expect":"boundary"}]}]);
    event
}

fn operations(h: &bn_client_core::transport::ScriptHandle) -> Vec<Value> {
    submits(h)
        .iter()
        .map(|s| s["params"]["operation"].clone())
        .collect()
}

#[test]
fn a_pressed_key_runs_the_action_the_engine_binds_to_it_and_nothing_else() {
    let (t, h) = session();
    let mut m = connected(t.on("bn.command.submit", walk_reply()));
    h.push(show(
        41,
        json!([{"id":"LEVEL_DOWN","name":"Descend","keys":[">"]},
               {"id":"QUIT","name":"Quit","keys":["ESC","SPACE"]},
               {"id":"COORDINATE","name":"COORDINATE","keys":[]}]),
        Value::Null,
    ));
    m.poll();
    assert!(!m.press("<"), "no action binds <");
    assert!(!m.press("COORDINATE"));
    assert!(m.press("SPACE"));
    assert_eq!(
        operations(&h),
        [json!({"kind":"action","action_id":"QUIT"})]
    );
    let names: Vec<_> = m.actions().into_iter().map(|a| a.name).collect();
    assert_eq!(
        names,
        ["Descend", "Quit"],
        "an action without keys is not offered"
    );
}

#[test]
fn any_key_acknowledges_a_press_any_key_popup_and_only_that() {
    let popup = |anykey: bool| {
        let mut ack = json!({"id":"acknowledge","label":"x","description": if anykey {"ANY_INPUT"} else {"QUIT"},
            "denial":"","enabled":true,"selectable":true,"selected":true,"columns":[]});
        ack["label"] = "Press any key".into();
        json!({"schema_id":"s:p","kind":"choices","context":"POPUP_WAIT","title":"","message":"Nothing.",
            "allow_cancel":false,"choices":[ack],"choice_total":1,"allow_set_count":false,
            "any_key_choice_id": if anykey {json!("acknowledge")} else {Value::Null}})
    };
    for (anykey, sent) in [(true, 1), (false, 0)] {
        let (t, h) = session();
        let mut m = connected(t.on("bn.command.submit", walk_reply()));
        h.push(show(41, json!([]), popup(anykey)));
        m.poll();
        assert_eq!(m.press("w"), anykey);
        assert_eq!(submits(&h).len(), sent);
    }
}

#[test]
fn any_key_continues_a_terminal_prompt_but_does_not_answer_its_security_query() {
    let terminal = |ids: &[&str]| {
        let choices: Vec<_> = ids
            .iter()
            .map(|id| {
                json!({"id":id,"label":id,"description":"","denial":"","enabled":true,"selectable":true,
                    "selected":false,"columns":[],"group":"terminal"})
            })
            .collect();
        let context = if ids == ["continue"] {
            "COMPUTER_TERMINAL"
        } else {
            "YESNOQUIT"
        };
        json!({"schema_id":"s:term","kind":"choices","context":context,"title":"Evac shelter computer",
            "message":"Login successful.  Press any key…","allow_cancel":false,"choices":choices,
            "choice_total":ids.len(),"allow_set_count":false,
            "any_key_choice_id": if ids == ["continue"] {json!("continue")} else {Value::Null}})
    };
    for (ids, sent) in [(&["continue"][..], 1), (&["yes", "no", "quit"][..], 0)] {
        let (t, h) = session();
        let mut m = connected(t.on("bn.command.submit", walk_reply()));
        h.push(show(41, json!([]), terminal(ids)));
        m.poll();
        assert_eq!(m.press("w"), sent == 1, "{ids:?}");
        assert_eq!(submits(&h).len(), sent, "{ids:?}");
    }
}

#[test]
fn the_selected_tab_picks_the_pane_and_a_target_interaction_is_a_cursor_not_a_menu() {
    let (t, h) = session();
    let mut m = connected(t);
    let row = |id: &str, label: &str, pane: Option<&str>, selected: bool| {
        let mut c = json!({"id":id,"label":label,"description":"d","denial":"","enabled":true,
            "selectable":true,"selected":selected,"columns":[{"key":"base","label":"Base","value":"8"}]});
        if let Some(p) = pane {
            c["pane_id"] = p.into();
        }
        c
    };
    h.push(show(
        41,
        json!([]),
        json!({"schema_id":"s:t","kind":"choices","context":"PLAYER_INFO","title":"Info","message":"",
            "allow_cancel":true,"choice_total":3,"allow_set_count":false,
            "choices":[row("t:1","STATS",None,false), row("t:2","SKILLS",None,true),
                       row("s:1","Melee",Some("skills"),false)],
            "compat":{"panes":[{"id":"stats","label":"STATS"},{"id":"skills","label":"SKILLS"}]}}),
    ));
    m.poll();
    let menu = m.menu().unwrap();
    assert_eq!(menu.pane.as_deref(), Some("skills"));
    assert_eq!(menu.tabs().len(), 2);
    assert_eq!(menu.entries()[0].label, "Melee");
    assert_eq!(menu.entries()[0].description, "d");
    assert_eq!(menu.entries()[0].column("base"), "8");
    h.push(target_stack(42, "Look Around", 5, None, json!([])));
    m.poll();
    assert!(m.menu().is_none());
    let origin = m.origin();
    assert_eq!(
        m.focus(),
        Some(Pos {
            x: 5 - origin.x,
            y: 6 - origin.y
        })
    );
}

#[test]
fn aiming_says_the_screens_range_line_and_the_creature_under_the_cursor() {
    let (t, h) = session();
    let mut m = connected(t);
    for (sequence, (x, words, parts, want)) in (41..).zip([
        (
            6,
            Some("range 4/8, ready to fire"),
            json!([]),
            Some("Firing Uzi 9mm: range 4/8, ready to fire"),
        ),
        (
            12,
            Some("range 10/8, out of range"),
            json!([]),
            Some("Firing Uzi 9mm: range 10/8, out of range"),
        ),
        (
            3,
            Some("range 2/8, out of ammo"),
            json!([{"kind":"creature","text":"<color_c_red>zombie</color>","detail":"A walking corpse."}]),
            Some("Firing Uzi 9mm: range 2/8, out of ammo / zombie / A walking corpse."),
        ),
        (6, None, json!([]), None),
    ]) {
        h.push(target_stack(sequence, "Firing Uzi 9mm", x, words, parts));
        m.poll();
        assert_eq!(m.aiming().as_deref(), want, "{words:?}");
    }
}

#[test]
fn looking_says_the_facts_of_the_cursor_square_and_moving_it_invokes_the_screens_move() {
    let (t, h) = session();
    let mut m = connected(t.on("bn.command.submit", walk_reply()));
    let parts = json!([{"kind":"terrain","text":"<color_c_green>grass</color>"},
        {"kind":"item","text":"rock [2]"}]);
    h.push(target_stack(
        41,
        "Look Around",
        3,
        Some("Select first point."),
        parts,
    ));
    m.poll();
    assert_eq!(
        m.aiming().as_deref(),
        Some("Look Around: Select first point. / grass / rock [2]")
    );
    let origin = m.origin();
    // The cursor is already on the square: a hover leaves it, and nothing is sent.
    assert!(m.aim(
        Pos {
            x: 3 - origin.x,
            y: 6 - origin.y
        },
        false
    ));
    assert!(operations(&h).is_empty());
    // Another square is the screen's move, with the square as its argument.
    assert!(m.aim(
        Pos {
            x: 5 - origin.x,
            y: 6 - origin.y
        },
        false
    ));
    let sent = submits(&h);
    assert_eq!(sent.len(), 1);
    let operation = &sent[0]["params"]["operation"];
    assert_eq!(operation["kind"], "invoke");
    assert_eq!(operation["affordance_id"], "target:move");
    assert_eq!(operation["args"]["pos"]["x"], 5);
}

#[test]
fn the_map_editors_square_and_keys_are_said_as_a_note() {
    let row = |id: &str, pane: &str, label: &str, description: &str, hotkey: &str| {
        json!({"id":id,"label":label,"description":description,"denial":"","hotkey":hotkey,
            "pane_id":pane,"enabled":true,"selectable":hotkey.is_empty() == false,"selected":false,
            "columns":[]})
    };
    let (t, h) = session();
    let mut m = connected(t);
    let editor = json!([
        row("tile:terrain", "tile", "Terrain", "grass", ""),
        row("tile:trap", "tile", "Trap", "bear trap", ""),
        row("action:EDIT_TERRAIN", "actions", "Edit terrain", "", "e"),
        row("action:LEVEL_UP", "actions", "Up", "", ""),
    ]);
    h.push(show(
        41,
        json!([]),
        json!({"schema_id":"s:n","kind":"target","context":"EDITMAP","title":"Map editor","message":"",
            "allow_cancel":true,"choices":editor,"choice_total":0,"allow_set_count":false,
            "target":{"source":{"dim":"","x":2,"y":4,"z":0},"current":{"dim":"","x":3,"y":4,"z":0},
                "range":0,"distance_metric":"square","status":"editmap","candidates":[]}}),
    ));
    m.poll();
    assert_eq!(
        m.aiming().as_deref(),
        Some("Map editor: Terrain grass, Trap bear trap. e Edit terrain")
    );
}

#[test]
fn fill_sets_an_editable_row_or_the_key_field_and_ignores_anything_else() {
    let (t, h) = session();
    let mut m = connected(t.on("bn.command.submit", walk_reply()));
    h.push(fixture("events_41_walk"));
    h.push(fixture("events_42_message"));
    let mut menu = fixture("events_43_menu");
    let interaction = &mut menu["params"]["events"][0]["changes"]["interaction"]["interaction"];
    interaction["choices"][0]["editor"] =
        json!({"type":"integer","value":"7","values":[],"minimum":0,"maximum":60,"max_length":-1});
    interaction["field"] = json!({"id":"field:key","label":"Key","description":"","value":"",
        "type":"key","max_length":-1,"printable":false});
    h.push(menu);
    m.poll();
    let menu = m.menu().unwrap();
    assert_eq!(menu.field.as_ref().map(|f| f.kind.as_str()), Some("key"));
    let editor = menu.choices[0].editor.as_ref().unwrap();
    assert_eq!(
        (
            editor.kind.as_str(),
            editor.maximum.as_ref().and_then(|n| n.as_f64())
        ),
        ("integer", Some(60.0))
    );
    assert!(menu.choices[1].editor.is_none());

    m.fill("nowhere", "1", true);
    m.fill("c:2", "1", true);
    assert!(
        submits(&h).is_empty(),
        "unknown and disabled rows are never filled"
    );
    m.fill("c:1", "8", true);
    assert_eq!(
        submits(&h)[0]["params"]["operation"],
        json!({"kind":"fill","field_id":"c:1","value":"8","submit":true})
    );
}

#[test]
fn control_function_and_numpad_keys_run_the_actions_the_engine_binds_to_them() {
    let (t, h) = session();
    let mut m = connected(
        t.on("bn.command.submit", walk_reply())
            .on("bn.command.submit", walk_reply()),
    );
    h.push(show(
        41,
        json!([{"id":"save","name":"Save","keys":["CTRL+S"]},
               {"id":"help","name":"Help","keys":["F1"]},
               {"id":"pause","name":"Pause","keys":[".","NUMPAD_5"]},
               {"id":"back","name":"Back","keys":["BACKTAB"]}]),
        Value::Null,
    ));
    m.poll();
    assert!(m.press("CTRL+S"));
    h.push(json!({"jsonrpc":"2.0","method":"bn.command","params":{"epoch":"e7f3","command_id":"c:5","stage":"completed"}}));
    m.poll();
    assert!(
        m.press("5"),
        "a digit with no binding may be the numpad key"
    );
    assert!(!m.press("CTRL+Q"), "no other control letter is bound");
    assert_eq!(
        operations(&h),
        [
            json!({"kind":"action","action_id":"save"}),
            json!({"kind":"action","action_id":"pause"})
        ]
    );
}

mod failures {
    use super::*;
    use bn_client_core::{Clock, FailureKind};
    use std::time::Duration;

    fn with_clock(t: ScriptedTransport) -> (EngineModel, Clock) {
        let clock = Clock::manual();
        let mut m = EngineModel::new(Box::new(t)).with_clock(clock.clone());
        m.poll();
        (m, clock)
    }

    #[test]
    fn an_exit_is_a_failure_with_the_last_stderr_lines() {
        let (t, h) = session();
        let mut m = connected(t);
        assert!(m.failure().is_none());
        h.close(&["loading data", "terminate called after throwing"]);
        m.poll();
        let failure = m.failure().expect("the exit is reported");
        assert_eq!(failure.kind, FailureKind::Exited);
        assert_eq!(
            failure.stderr,
            ["loading data", "terminate called after throwing"]
        );
        assert!(!m.busy());
    }

    #[test]
    fn the_failure_names_how_the_engine_process_ended() {
        let (t, h) = session();
        let mut m = connected(t);
        h.exit_with("exit code 0");
        h.close(&["mcp: the engine is exiting: the client closed its end of stdin"]);
        m.poll();
        let failure = m.failure().expect("the exit is reported");
        assert_eq!(failure.message, "The engine stopped (exit code 0).");
        assert_eq!(
            failure.stderr,
            ["mcp: the engine is exiting: the client closed its end of stdin"]
        );
    }

    #[test]
    fn an_engine_that_refuses_the_protocol_version_is_a_failure_with_its_reason() {
        let (t, h) = ScriptedTransport::new();
        let refused = json!({"jsonrpc": "2.0", "error": {"code": 1000, "message": "application error",
            "data": {"kind": "negotiation_failed", "action": "hello",
                "message": "The protocol version is not supported."}}});
        let engine = std::path::PathBuf::from("/opt/bn/cataclysm-bn-tiles");
        let mut m =
            EngineModel::new(Box::new(t.on("bn.hello", vec![refused]))).with_engine(engine.clone());
        m.poll();
        let failure = m.failure().expect("the refusal is reported");
        assert_eq!(failure.kind, FailureKind::Incompatible);
        assert_eq!(failure.message, "The protocol version is not supported.");
        assert_eq!(failure.engine, Some(engine));
        assert_eq!(
            h.sent_methods(),
            ["bn.hello"],
            "nothing follows a refused hello"
        );
    }

    #[test]
    fn an_engine_without_bn_hello_is_incompatible() {
        let (t, h) = ScriptedTransport::new();
        let unknown =
            json!({"jsonrpc": "2.0", "error": {"code": -32601, "message": "Method not found"}});
        let mut m = EngineModel::new(Box::new(t.on("bn.hello", vec![unknown])));
        m.poll();
        let failure = m.failure().expect("the refusal is reported");
        assert_eq!(failure.kind, FailureKind::Incompatible);
        assert_eq!(failure.message, "Method not found");
        assert_eq!(h.sent_methods(), ["bn.hello"]);
    }

    #[test]
    fn silence_with_a_command_in_flight_is_a_stall_that_a_message_clears() {
        let (t, h) = session();
        let (mut m, clock) = with_clock(t.on("bn.command.submit", walk_reply()));
        assert!(m.invoke_action("RIGHT"));
        m.poll();
        clock.advance(Duration::from_secs(119));
        m.poll();
        assert!(m.failure().is_none(), "within the watchdog");
        clock.advance(Duration::from_secs(2));
        m.poll();
        assert_eq!(m.failure().unwrap().kind, FailureKind::Stalled);
        h.push(json!({"jsonrpc":"2.0","method":"bn.command","params":{
            "epoch":"e7f3","command_id":"c:5","stage":"executing"}}));
        m.poll();
        assert!(m.failure().is_none(), "the engine answered after all");
    }

    #[test]
    fn heartbeats_keep_a_long_activity_from_counting_as_stalled() {
        let (t, h) = session();
        let (mut m, clock) = with_clock(t.on("bn.command.submit", walk_reply()));
        assert!(m.invoke_action("RIGHT"));
        m.poll();
        // Ten minutes of activity, with a heartbeat every two seconds.
        for _ in 0..300 {
            clock.advance(Duration::from_secs(2));
            h.push(json!({"jsonrpc":"2.0","method":"bn.progress","params":{"epoch":"e7f3"}}));
            m.poll();
            assert!(m.failure().is_none(), "alive engines are not stalled");
        }
        assert!(m.busy(), "the command is still running");
        // Then the heartbeats stop.
        clock.advance(Duration::from_secs(119));
        m.poll();
        assert!(m.failure().is_none());
        clock.advance(Duration::from_secs(2));
        m.poll();
        assert_eq!(m.failure().unwrap().kind, FailureKind::Stalled);
    }

    #[test]
    fn loading_notifications_also_count_as_signs_of_life() {
        let (t, h) = session();
        let (mut m, clock) = with_clock(t.on("bn.command.submit", walk_reply()));
        assert!(m.invoke_action("RIGHT"));
        m.poll();
        for _ in 0..100 {
            clock.advance(Duration::from_secs(60));
            h.push(json!({"jsonrpc":"2.0","method":"bn.loading","params":{
                "epoch":"e7f3","title":"Loading","entries":["a"],"index":0}}));
            m.poll();
            assert!(m.failure().is_none());
        }
    }

    #[test]
    fn an_idle_engine_may_stay_quiet() {
        let (t, _h) = session();
        let (mut m, clock) = with_clock(t);
        clock.advance(Duration::from_secs(3600));
        m.poll();
        assert!(m.failure().is_none());
    }

    #[test]
    fn restart_starts_a_fresh_session_through_the_respawn_hook() {
        let (t, h) = session();
        let (t2, h2) = session();
        let mut spare = Some(t2);
        let mut m = EngineModel::new(Box::new(t)).with_respawn(Box::new(move || {
            Ok(Box::new(spare.take().expect("one restart"))
                as Box<dyn bn_client_core::transport::Transport>)
        }));
        m.poll();
        h.close(&["boom"]);
        m.poll();
        assert!(m.failure().is_some());
        assert!(m.restart());
        assert!(m.failure().is_none());
        m.poll();
        assert_eq!(h2.sent_methods(), ["bn.hello", "bn.subscribe"]);
        assert_eq!(m.size(), Pos { x: 40, y: 25 }, "the new session is live");
    }

    #[test]
    fn restart_without_a_respawn_hook_does_nothing() {
        let (t, h) = session();
        let mut m = connected(t);
        h.close(&[]);
        m.poll();
        assert!(!m.restart());
        assert!(m.failure().is_some());
    }
}

fn fact_event(kind: &str, data: Value, duration_ms: u64) -> Value {
    json!({"jsonrpc": "2.0", "method": "bn.events", "params": {"epoch": "e7f3", "events": [
        {"sequence": "41", "revision": "32", "type": kind, "data": data, "display": {"duration_ms": duration_ms}}
    ]}})
}

fn square(x: i32, y: i32, z: i32) -> Value {
    json!({"dim": "", "x": x, "y": y, "z": z})
}

#[test]
fn animation_facts_become_effects_in_view_coordinates() {
    use bn_client_core::{Pos, Shape};
    let (t, h) = session();
    let mut m = connected(t);
    let me = m.player();
    h.push(fact_event(
        "projectile.moved",
        json!({"projectile": "projectile:1", "path": [square(4, 4, 0), square(5, 4, 0), square(5, 4, 1)],
            "look": {"kind": "projectile", "id": null, "glyph": "*", "color": "red"}}),
        30,
    ));
    m.poll();
    let effects = m.take_effects();
    assert_eq!(effects.len(), 1);
    assert_eq!(effects[0].duration_ms, 30);
    let Shape::Projectile { path, glyph, .. } = &effects[0].shape else {
        panic!("{:?}", effects[0]);
    };
    assert_eq!(*glyph, '*');
    assert_eq!(
        *path,
        [
            Pos {
                x: me.x + 1,
                y: me.y
            },
            Pos {
                x: me.x + 2,
                y: me.y
            }
        ],
        "squares on another level are dropped"
    );
    assert!(m.take_effects().is_empty(), "each fact is handed out once");
}

#[test]
fn explosion_and_floating_text_facts_carry_their_colors() {
    use bn_client_core::Shape;
    let (t, h) = session();
    let mut m = connected(t);
    h.push(fact_event(
        "explosion.started",
        json!({"explosion": "explosion:1", "at": square(3, 4, 0), "radius": 3, "color": "c_red"}),
        40,
    ));
    m.poll();
    let effects = m.take_effects();
    let Shape::Burst { radius, color, .. } = &effects[0].shape else {
        panic!("{:?}", effects[0]);
    };
    assert_eq!(*radius, 3);
    assert_eq!(
        *color,
        bn_client_core::Rgb(170, 40, 40),
        "c_red resolves to the game's red"
    );
    let mut text = fact_event(
        "combat_text.shown",
        json!({"at": square(3, 4, 0), "segments": [{"text": "-12", "color": "light_red"}]}),
        80,
    );
    text["params"]["events"][0]["sequence"] = json!("42");
    h.push(text);
    m.poll();
    let effects = m.take_effects();
    let Shape::Text { segments, .. } = &effects[0].shape else {
        panic!("{:?}", effects[0]);
    };
    assert_eq!(segments[0].0, "-12");
}

#[test]
fn open_air_draws_the_square_the_engine_names_below_with_its_depth() {
    let m = tiled_session();
    let origin = m.origin();
    let at = |x: i32, y: i32| {
        m.cell(Pos {
            x: x - origin.x,
            y: y - origin.y,
        })
    };
    // Open air on the avatar's level at (9, 6) names the level below the game draws there.
    let open = at(9, 6);
    let layers = open
        .layers
        .iter()
        .map(|l| l.id.as_str())
        .collect::<Vec<_>>();
    assert_eq!((layers, open.depth), (vec!["t_dirt"], 1));
    // A vehicle roof seen through open air has no terrain of its own and is drawn all the same.
    let roof = at(10, 6);
    let layers = roof
        .layers
        .iter()
        .map(|l| l.id.as_str())
        .collect::<Vec<_>>();
    assert_eq!((layers, roof.depth), (vec!["vp_roof"], 1));
    // A remembered square below draws its memory, as memory, when the engine names it.
    let grass = at(9, 5);
    assert_eq!((grass.layers[0].id.as_str(), grass.depth), ("t_grass", 1));
    assert!(grass.remembered);
    // Smoke on the open air draws over the level below, but the open air itself does not.
    let smoke = at(12, 6);
    let layers = smoke
        .layers
        .iter()
        .map(|l| l.id.as_str())
        .collect::<Vec<_>>();
    assert_eq!((layers, smoke.light), (vec!["t_dirt", "fd_smoke"], Some(3)));
    // A square in sight below that the engine does not name is not drawn, and the avatar's own level has
    // no depth.
    assert!(at(13, 6).layers.is_empty());
    // Two levels down, the smoke on the open air between draws over the floor and describes the square.
    let deep = at(14, 6);
    let layers = deep
        .layers
        .iter()
        .map(|l| l.id.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        (layers, deep.depth, deep.light),
        (vec!["t_rock_floor", "fd_smoke"], 2, Some(3))
    );
    assert_eq!(deep.desc, "smoke");
    assert_eq!(at(9, 8).depth, 0);
    assert_eq!(at(3, 4).depth, 0);
}

#[test]
fn returning_to_the_main_menu_clears_the_previous_games_log_and_command_lines() {
    let (t, h) = session();
    let mut m = connected(t);
    h.push(fixture("events_41_walk"));
    h.push(logged(42, "7", "You open the door.", 1));
    m.poll();
    m.say("Something blocks the way.".into());
    assert_eq!(m.native_log().len(), 1);
    let mut menu = counted("base");
    // The title screen is a stack with no map in it.
    menu["stack"] = json!([{"screen_id": "screen:1", "role": "screen", "title": "Title",
        "root": {"id": "banner", "kind": "text", "markup": "Title"}, "affordances": []}]);
    h.push(change(43, json!({"interaction": menu})));
    m.poll();
    assert!(
        m.native_log().is_empty(),
        "the game's own lines belong to that game"
    );
    assert!(m.messages().is_empty(), "so do the client's command lines");
    h.push(logged(44, "8", "Welcome back.", 1));
    m.poll();
    assert_eq!(m.native_log().len(), 1, "a later game logs again");
}

#[test]
fn menu_rows_carry_the_engines_hotkey_and_empty_when_absent() {
    let text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/pickup.json"
    ))
    .unwrap();
    let mut boundary = serde_json::from_str::<Value>(&text).unwrap()["pickup"].clone();
    boundary["interaction"]["choices"][0]["hotkey"] = json!("e");
    let (t, h) = session();
    let mut m = connected(t);
    h.push(change(41, json!({"interaction": boundary})));
    m.poll();
    let keys = m
        .menu()
        .unwrap()
        .choices
        .iter()
        .map(|c| c.hotkey.clone())
        .collect::<Vec<_>>();
    assert_eq!(keys, ["e", ""]);
}

/// The pickup fixture with a hotkey on each row, `edit` applied to its boundary.
fn hotkeyed(
    edit: impl FnOnce(&mut Value),
) -> (EngineModel, bn_client_core::transport::ScriptHandle) {
    let text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/pickup.json"
    ))
    .unwrap();
    let mut boundary = serde_json::from_str::<Value>(&text).unwrap()["pickup"].clone();
    boundary["interaction"]["choices"][0]["hotkey"] = json!("e");
    boundary["interaction"]["choices"][1]["hotkey"] = json!("4");
    edit(&mut boundary);
    let (t, h) = session();
    let mut m = connected(t.on("bn.command.submit", walk_reply()));
    h.push(change(41, json!({"interaction": boundary})));
    m.poll();
    (m, h)
}

#[test]
fn a_typed_hotkey_chooses_the_row_that_shows_it_even_when_the_digit_is_a_numpad_binding() {
    // The digit is also the numpad key of a menu action, as in every native list.
    let (mut m, h) =
        hotkeyed(|b| b["actions"] = json!([{"id":"LEFT","name":"Left","keys":["NUMPAD_4"]}]));
    assert!(m.press("4"));
    assert_eq!(
        submits(&h)[0]["params"]["operation"],
        json!({"kind":"choose","choice_id":"pickup:b"})
    );
}

#[test]
fn a_hotkey_no_row_shows_or_a_disabled_row_does_not_choose() {
    let (mut m, h) = hotkeyed(|b| b["interaction"]["choices"][1]["enabled"] = json!(false));
    assert!(!m.press("4"), "disabled row");
    assert!(!m.press("z"), "no row has it");
    assert!(submits(&h).is_empty());
}

#[test]
fn a_hotkey_typed_into_a_text_prompt_is_not_a_choice() {
    let (mut m, h) = hotkeyed(|b| {
        b["interaction"]["field"] =
            json!({"id":"filter","label":"Filter","description":"","value":"","type":"text"})
    });
    assert!(!m.press("e"));
    assert!(submits(&h).is_empty());
}

#[test]
fn a_client_line_repeating_the_newest_one_is_not_added_again() {
    let (t, _h) = session();
    let mut m = connected(t);
    m.say("You see a wall at (4, 5).".into());
    m.say("You see a wall at (4, 5).".into());
    m.say("You see a door at (5, 5).".into());
    m.say("You see a wall at (4, 5).".into());
    assert_eq!(m.messages().len(), 3, "{:?}", m.messages());
}

#[test]
fn query_popup_rows_do_not_carry_the_action_id_as_a_description() {
    let mut menu = counted("base");
    menu["actions"] = json!([{"id": "YES", "name": "Yes", "keys": ["y"]}, {"id": "NO", "name": "No", "keys": ["n"]}]);
    let rows = ["Yes", "No"].map(|label| {
        let id = label.to_uppercase();
        json!({"id": format!("option:{id}"), "label": label, "description": id, "denial": "",
            "enabled": true, "selectable": true, "selected": false, "columns": []})
    });
    menu["interaction"]["context"] = json!("YESNO");
    menu["interaction"]["choices"] = json!(rows);
    let (t, h) = session();
    let mut m = connected(t);
    h.push(change(41, json!({"interaction": menu})));
    m.poll();
    let menu = m.menu().unwrap();
    assert!(
        menu.choices.iter().all(|c| c.description.is_empty()),
        "{:?}",
        menu.choices
    );
    // Text that is not an action id stays.
    let mut real = counted("counted");
    real["actions"] = json!([{"id": "YES", "name": "Yes"}]);
    h.push(change(42, json!({"interaction": real})));
    m.poll();
    let kept = m
        .menu()
        .unwrap()
        .choices
        .into_iter()
        .find(|c| c.id == "item:aa")
        .unwrap();
    assert!(kept.description.starts_with("Time to complete"));
}

fn subscribe_with_log(lines: &[(&str, &str, u32)]) -> Vec<serde_json::Value> {
    let mut reply = subscribe_reply();
    reply[0]["result"]["messages"] = lines
        .iter()
        .map(|(id, text, count)| {
            let label = if *count > 1 {
                format!("{text} x {count}")
            } else {
                text.to_string()
            };
            json!({"id": id, "text": text, "kind": "bad", "color": "c_red", "count": count, "label": label})
        })
        .collect();
    reply
}

#[test]
fn the_log_a_snapshot_carries_is_the_native_log_before_any_event() {
    let (t, _h) = ScriptedTransport::new();
    let t = t.on("bn.hello", vec![fixture("hello_result")]).on(
        "bn.subscribe",
        subscribe_with_log(&[("3", "You wake up.", 1), ("7", "Bang!", 2)]),
    );
    let m = connected(t);
    let shown: Vec<_> = m.native_log().iter().map(|l| l.shown()).collect();
    assert_eq!(shown, ["You wake up.", "Bang! x 2"]);
    assert!(m.native_log().iter().all(|l| l.color.is_some()));
}

#[test]
fn a_second_snapshot_with_the_same_log_replaces_lines_instead_of_duplicating_them() {
    let (t, h) = ScriptedTransport::new();
    let t = t
        .on("bn.hello", vec![fixture("hello_result")])
        .on(
            "bn.subscribe",
            subscribe_with_log(&[("3", "You wake up.", 1)]),
        )
        .on(
            "bn.subscribe",
            subscribe_with_log(&[
                ("3", "You wake up.", 1),
                ("7", "Bang!", 1),
                ("8", "Crash!", 1),
            ]),
        );
    let mut m = connected(t);
    h.push(fixture("events_45_gap"));
    m.poll();
    let shown: Vec<_> = m.native_log().iter().map(|l| l.shown()).collect();
    assert_eq!(shown, ["You wake up.", "Bang!", "Crash!"]);
}

#[test]
fn the_engines_display_options_from_hello_are_the_default_map_mode() {
    let mut hello = fixture("hello_result");
    hello["result"]["engine"]["display"] = json!({"tiles": false, "tileset": "MSX"});
    let (t, _h) = ScriptedTransport::new();
    let t = t
        .on("bn.hello", vec![hello])
        .on("bn.subscribe", subscribe_reply());
    let m = connected(t);
    assert_eq!(m.display_default(), Some((false, "MSX".to_string())));
    let (t, _h) = session();
    assert_eq!(
        connected(t).display_default(),
        None,
        "older engines publish none"
    );
}

#[test]
fn a_field_is_painted_with_the_tint_of_its_type_on_both_layers() {
    let m = tiled_session();
    let origin = m.origin();
    let cell = m.cell(Pos {
        x: 11 - origin.x,
        y: 8 - origin.y,
    });
    let field = cell
        .layers
        .iter()
        .find(|l| l.id == "fd_acid_water")
        .unwrap();
    assert_eq!(field.tint_bg, Some(Rgb(0, 170, 0)));
    assert_eq!(field.tint_fg, Some(Rgb(0, 170, 0)));
}

#[test]
fn a_rejected_command_is_logged_in_words_not_as_the_engines_error_name() {
    let (t, h) = session();
    let mut m = connected(t.on("bn.command.submit", walk_reply()));
    assert!(m.invoke_action("RIGHT"));
    h.push(json!({"jsonrpc":"2.0","method":"bn.command","params":{
        "epoch":"e7f3","command_id":"c:5","stage":"rejected","error":"validation_failed","error_text":"Not possible here."}}));
    m.poll();
    assert_eq!(m.messages(), ["Not possible here."]);
}

/// The walk fixture moved to `to`, with the engine's window of `window` squares around `to` when it has one.
fn step_event(n: i32, from: (i32, i32), to: (i32, i32), window: Option<(i32, i32)>) -> Value {
    let at = |x: i32, y: i32| json!({"dim": "", "x": x, "y": y, "z": 0});
    let mut message = fixture("events_41_walk");
    let event = &mut message["params"]["events"][0];
    event["sequence"] = (41 + n).to_string().into();
    event["revision"] = (32 + n).to_string().into();
    event["data"]["from"] = at(from.0, from.1);
    event["data"]["to"] = at(to.0, to.1);
    event["changes"]["avatar"]["at"] = at(to.0, to.1);
    if let Some((w, h)) = window {
        let (x, y) = (to.0 - w / 2, to.1 - h / 2);
        event["changes"]["view"] = json!({"min": at(x, y), "max": at(x + w - 1, y + h - 1)});
    }
    message
}

#[test]
fn the_player_square_stays_at_the_native_centre_after_steps_in_every_direction() {
    // Without the engine's window, with ones of the same size, larger, odd-sized and smaller.
    for window in [
        None,
        Some((40, 25)),
        Some((61, 33)),
        Some((50, 30)),
        Some((30, 20)),
    ] {
        let (t, h) = session();
        let mut m = connected(t);
        let mut at = (3, 4);
        let mut n = -1;
        for (dx, dy) in [
            (1, 0),
            (-1, 0),
            (0, 1),
            (0, -1),
            (1, 1),
            (-1, -1),
            (1, -1),
            (-1, 1),
        ] {
            for _ in 0..10 {
                n += 1;
                let from = at;
                at = (at.0 + dx, at.1 + dy);
                h.push(step_event(n, from, at, window));
                m.poll();
                let want = Pos {
                    x: m.size().x / 2,
                    y: m.size().y / 2,
                };
                assert_eq!(m.player(), want, "{window:?} step {n} toward {dx},{dy}");
                assert_eq!(
                    (m.origin().x + m.player().x, m.origin().y + m.player().y),
                    at,
                    "{window:?} step {n}: the view is on the avatar"
                );
            }
        }
    }
}

fn paned(kind: &str, context: &str, panes: Value, rows: &[(&str, &str, bool)]) -> Value {
    let choices = rows
        .iter()
        .map(|(id, pane, selectable)| {
            json!({"id":id,"label":id,"description":"","denial":"","hotkey":"","pane_id":pane,
            "enabled":true,"selectable":selectable,"selected":false,"columns":[]})
        })
        .collect::<Vec<_>>();
    json!({"schema_id":"s:p","kind":kind,"context":context,"title":"Screen","message":"",
        "allow_cancel":true,"choices":choices,"choice_total":choices.len(),"allow_set_count":false,
        "compat":{"panes":panes},
        "target":{"source":{"dim":"","x":2,"y":4,"z":0},"current":{"dim":"","x":3,"y":4,"z":0},
            "range":0,"distance_metric":"square","status":"editmap","candidates":[]}})
}

#[test]
fn a_list_of_named_panes_starts_each_pane_under_its_heading() {
    let (t, h) = session();
    let mut m = connected(t);
    let panes = json!([{"id":"zones","label":"Zones","role":"current"},
        {"id":"options","label":"Options","role":"inactive"},
        {"id":"actions","label":"Actions","role":"controls"}]);
    let rows = [
        ("zone:0", "zones", true),
        ("zone:1", "zones", true),
        ("option:0", "options", false),
        ("action:ADD_ZONE", "actions", true),
    ];
    h.push(show(
        41,
        json!([]),
        paned("choices", "ZONES_MANAGER", panes, &rows),
    ));
    m.poll();
    let menu = m.menu().unwrap();
    let rows = menu.entries();
    let headings = rows
        .iter()
        .enumerate()
        .map(|(i, c)| menu.heading(i.checked_sub(1).map(|p| rows[p]), c))
        .collect::<Vec<_>>();
    assert_eq!(
        headings,
        [Some("Zones"), None, Some("Options"), Some("Actions")]
    );
}

fn snake_boundary(id: &str, refresh: bool) -> Value {
    let mut boundary = counted("base");
    boundary["boundary_id"] = json!(id);
    boundary["actions"] = json!([{"id": "UP", "name": "Up", "keys": ["w"]}]);
    let i = &mut boundary["interaction"];
    i["kind"] = json!("custom");
    i["context"] = json!("SNAKE");
    i["choices"] = json!([]);
    i["choice_total"] = json!(0);
    if refresh {
        i["refresh_ms"] = json!(100);
    }
    boundary
}

fn stale_boundary() -> Value {
    json!({"jsonrpc":"2.0","id":null,"error":{"code":1000,"message":"stale","data":{"kind":"stale_boundary","message":"The screen changed, try again."}}})
}

#[test]
fn a_key_rejected_stale_on_a_timed_screen_is_sent_once_to_the_next_boundary() {
    let (t, h) = session();
    let mut m = connected(
        t.on("bn.command.submit", vec![stale_boundary()])
            .on("bn.command.submit", vec![stale_boundary()]),
    );
    h.push(change(
        41,
        json!({"interaction": snake_boundary("b:20", true)}),
    ));
    m.poll();
    assert!(m.press("w"));
    m.poll();
    assert_eq!(submits(&h).len(), 1, "the new boundary has not arrived");
    h.push(change(
        42,
        json!({"interaction": snake_boundary("b:21", true)}),
    ));
    m.poll();
    let sent = submits(&h);
    assert_eq!(sent.len(), 2);
    assert_eq!(
        sent[1]["params"]["operation"],
        json!({"kind":"action","action_id":"UP"})
    );
    assert_eq!(sent[1]["params"]["expect"]["boundary_id"], "b:21");
    h.push(change(
        43,
        json!({"interaction": snake_boundary("b:22", true)}),
    ));
    m.poll();
    assert_eq!(submits(&h).len(), 2, "a resend is not repeated");
}

#[test]
fn a_key_rejected_stale_on_a_screen_without_refresh_is_not_resent() {
    let (t, h) = session();
    let mut m = connected(t.on("bn.command.submit", vec![stale_boundary()]));
    h.push(change(
        41,
        json!({"interaction": snake_boundary("b:20", false)}),
    ));
    m.poll();
    assert!(m.press("w"));
    m.poll();
    h.push(change(
        42,
        json!({"interaction": snake_boundary("b:21", false)}),
    ));
    m.poll();
    assert_eq!(submits(&h).len(), 1);
    assert_eq!(
        m.messages().last().unwrap(),
        "The screen changed, try again."
    );
}

fn heard(sequence: u32, text: &str, logged: Option<bool>) -> serde_json::Value {
    let mut data = serde_json::json!({"text": text, "volume": 52});
    if let Some(logged) = logged {
        data["logged"] = logged.into();
    }
    serde_json::json!({"jsonrpc": "2.0", "method": "bn.events", "params": {"epoch": "e7f3", "events": [{
        "sequence": sequence.to_string(), "revision": "32", "type": "sound.heard", "data": data}]}})
}

fn sound_texts(m: &EngineModel) -> Vec<String> {
    m.sounds().into_iter().map(|s| s.text).collect()
}

#[test]
fn a_heard_sound_the_engine_logged_is_not_repeated_as_a_sound_line() {
    let (t, h) = session();
    let mut m = connected(t);
    h.push(fixture("events_41_walk"));
    h.push(heard(42, "Zoe saying \"Look out!\"", Some(true)));
    h.push(logged(
        43,
        "7",
        "From the northwest you hear Zoe saying \"Look out!\".",
        1,
    ));
    m.poll();
    assert_eq!(m.native_log().len(), 1);
    assert!(m.messages().is_empty(), "{:?}", m.messages());
    assert!(m.sounds().is_empty(), "{:?}", m.sounds());
}

#[test]
fn a_heard_sound_the_engine_did_not_log_is_a_sound_not_a_command_line() {
    let (t, h) = session();
    let mut m = connected(t);
    h.push(fixture("events_41_walk"));
    h.push(heard(42, "Tick.", Some(false)));
    h.push(heard(43, "Tock.", None));
    m.poll();
    let texts = sound_texts(&m);
    assert_eq!(texts, ["Tick.", "Tock."]);
    assert!(m.messages().is_empty(), "{:?}", m.messages());
}

#[test]
fn a_logged_sound_is_judged_by_its_flag_not_by_matching_native_text() {
    let (t, h) = session();
    let mut m = connected(t);
    h.push(fixture("events_41_walk"));
    h.push(heard(42, "Tick.", Some(false)));
    h.push(logged(43, "7", "Tick. Tick. You open the door.", 1));
    m.poll();
    let texts = sound_texts(&m);
    assert_eq!(texts, ["Tick."]);
}

fn describes(h: &bn_client_core::transport::ScriptHandle) -> usize {
    h.sent()
        .iter()
        .filter(|m| m["method"] == "bn.world.describe")
        .count()
}

#[test]
fn a_hovered_square_is_described_again_when_its_answer_is_for_an_older_revision() {
    let (t, h) = session();
    let reply =
        json!({"jsonrpc":"2.0","id":0,"result":{"parts":[{"kind":"terrain","text":"Wall"}]}});
    let mut m = connected(
        t.on("bn.world.describe", vec![reply.clone()])
            .on("bn.world.describe", vec![reply]),
    );
    // Avatar (3, 4) is drawn at view (20, 12).
    m.hover(Pos { x: 22, y: 12 });
    m.poll();
    assert_eq!(describes(&h), 1);
    assert_eq!(
        m.hover_text(Pos { x: 22, y: 12 }),
        Some(vec!["Wall".into()])
    );
    m.poll();
    assert_eq!(describes(&h), 1, "an unchanged world asks nothing more");
    h.push(change(41, json!({"route": [pos(4, 4)]})));
    m.poll();
    assert_eq!(
        describes(&h),
        2,
        "a new revision asks for the same square again"
    );
    m.poll();
    assert_eq!(describes(&h), 2, "once per revision");
    assert!(m.hover_text(Pos { x: 22, y: 12 }).is_some());
}

#[test]
fn no_command_leaves_for_the_old_world_while_the_new_one_is_being_fetched() {
    let (t, h) = session();
    // The engine has not answered the new subscription yet.
    let mut m = connected(t.on("bn.command.submit", walk_reply()));
    h.push(fixture("resync"));
    m.poll();
    assert_eq!(h.sent_methods().last().unwrap(), "bn.subscribe");
    assert!(!m.invoke_action("RIGHT"), "the mirror is the old world's");
    assert!(submits(&h).is_empty());
}

#[test]
fn only_the_terrain_layer_of_a_square_says_it_is_terrain() {
    let m = tiled_session();
    let origin = m.origin();
    let cell = m.cell(Pos {
        x: 6 - origin.x,
        y: 4 - origin.y,
    });
    let terrain = cell.layers.iter().map(|l| l.terrain).collect::<Vec<_>>();
    assert_eq!(
        cell.layers
            .iter()
            .map(|l| l.id.as_str())
            .collect::<Vec<_>>(),
        ["t_dirt", "mon_zombie", "overlay_hostile_sees_player"]
    );
    assert_eq!(terrain, [true, false, false]);
}

#[test]
fn the_map_version_changes_with_what_the_engine_publishes_and_with_nothing_else() {
    let (t, h) = session();
    let mut m = connected(t);
    let first = m.map_version().expect("an engine model tells");
    assert_eq!(
        m.map_version(),
        Some(first),
        "reading the map does not change it"
    );
    h.push(fixture("events_41_walk"));
    m.poll();
    assert_ne!(m.map_version(), Some(first), "an event changed the map");
}
