mod common;
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
    assert_eq!(h.sent()[0]["params"]["versions"], json!(["1.0"]));
    assert_eq!(m.size(), Pos { x: 40, y: 25 });
    assert_eq!(m.player(), Pos { x: 20, y: 12 });
    assert_eq!(m.cell(Pos { x: 21, y: 11 }).glyph, '#');
    assert_eq!(m.cell(Pos { x: 21, y: 11 }).desc, "plastered wooden wall");
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
    assert!(m.step_toward(Pos { x: 30, y: 12 }));
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
fn diagonal_without_a_matching_action_does_not_submit() {
    let (t, h) = session();
    let mut m = connected(t);
    assert!(!m.step_toward(Pos { x: 25, y: 5 }));
    assert!(submits(&h).is_empty());
    assert!(m.messages().last().unwrap().contains("RIGHTUP"));
}

#[test]
fn second_step_waits_for_the_first_command_to_finish() {
    let (t, h) = session();
    let mut m = connected(
        t.on("bn.command.submit", walk_reply())
            .on("bn.command.submit", walk_reply()),
    );
    m.step_toward(Pos { x: 30, y: 12 });
    m.poll();
    assert!(m.step_toward(Pos { x: 30, y: 12 }), "still walking");
    assert_eq!(submits(&h).len(), 1, "busy: no second submit");
    h.push(fixture("events_41_walk"));
    h.push(fixture("command_completed"));
    m.poll();
    assert_eq!(
        m.player(),
        Pos { x: 21, y: 12 },
        "view stays put until the avatar nears the edge"
    );
    assert_eq!(m.cell(Pos { x: 21, y: 12 }).glyph, '.');
    m.step_toward(Pos { x: 30, y: 12 });
    let sent = submits(&h);
    assert_eq!(sent.len(), 2);
    assert_eq!(sent[1]["params"]["expect"]["revision"], "32");
}

#[test]
fn rejected_command_is_reported_and_blocks_the_walk_once() {
    let (t, h) = session();
    let mut m = connected(t.on("bn.command.submit", walk_reply()));
    m.step_toward(Pos { x: 30, y: 12 });
    h.push(fixture("command_rejected"));
    m.poll();
    assert!(m.messages().last().unwrap().contains("stale_revision"));
    assert!(
        !m.step_toward(Pos { x: 30, y: 12 }),
        "walk stops after a rejection"
    );
    assert!(
        m.step_toward(Pos { x: 30, y: 12 }),
        "next attempt submits again"
    );
}

#[test]
fn a_failed_non_walk_command_does_not_cancel_the_next_step() {
    let (t, h) = session();
    let mut m = connected(
        t.on("bn.command.submit", walk_reply())
            .on("bn.command.submit", walk_reply()),
    );
    m.pick_up(0);
    h.push(fixture("command_rejected"));
    m.poll();
    assert!(
        m.step_toward(Pos { x: 30, y: 12 }),
        "only a rejected step stops the walk"
    );
    assert_eq!(submits(&h).len(), 2, "the step was submitted");
}

#[test]
fn error_response_to_submit_frees_the_queue_and_reports_the_kind() {
    let (t, _h) = session();
    let err = json!({"jsonrpc":"2.0","id":null,"error":{"code":1000,"message":"busy","data":{"kind":"command_busy"}}});
    let mut m = connected(t.on("bn.command.submit", vec![err]));
    m.step_toward(Pos { x: 30, y: 12 });
    m.poll();
    assert!(m.messages().last().unwrap().contains("command_busy"));
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
    serde_json::json!({"jsonrpc": "2.0", "method": "bn.events", "params": {"epoch": "e7f3", "events": [{
        "sequence": sequence.to_string(), "revision": "32", "type": "message.logged",
        "data": {"id": id, "text": text, "kind": "bad", "color": "c_red", "count": count}}]}})
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
    assert_eq!(shown, ["You open the door.", "Bang! x2"]);
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
fn pick_up_and_drop_invoke_registered_actions() {
    let (t, h) = session();
    let mut m = connected(t.on("bn.command.submit", walk_reply()));
    m.drop_item(0);
    assert!(
        m.messages().last().unwrap().contains("drop"),
        "missing action is reported"
    );
    assert!(submits(&h).is_empty());
    m.pick_up(0);
    assert_eq!(
        submits(&h)[0]["params"]["operation"],
        json!({"kind":"action","action_id":"pickup"})
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

#[test]
fn main_menu_snapshot_without_a_world_shows_the_menu_and_no_map() {
    let text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/1.0.examples.json"
    ))
    .unwrap();
    let examples: Value = serde_json::from_str(&text).unwrap();
    let menu = examples["valid"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["name"] == "subscribe result at a menu")
        .unwrap()["value"]
        .clone();
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
    let text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/1.0.examples.json"
    ))
    .unwrap();
    let examples: Value = serde_json::from_str(&text).unwrap();
    let menu = examples["valid"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["name"] == "subscribe result at the main menu")
        .unwrap()["value"]
        .clone();
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
    assert!(m.travel(Pos { x: 22, y: 12 }));
    let sent = submits(&h);
    assert_eq!(
        sent[0]["params"]["operation"],
        json!({"kind":"travel","pos":pos(5, 4)})
    );
    assert_eq!(sent[0]["params"]["expect"]["schema_id"], Value::Null);
    assert!(m.busy());
    assert!(
        m.travel(Pos { x: 23, y: 12 }),
        "handled, but not sent twice"
    );
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
    assert!(m.travel(Pos { x: 22, y: 12 }));
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
    assert!(m.travel(Pos { x: 22, y: 12 }));
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
    assert!(m.travel(Pos { x: 22, y: 12 }));
    h.push(json!({"jsonrpc":"2.0","method":"bn.command","params":{
        "epoch":"e7f3","command_id":"c:5","stage":"interrupted","error":error}}));
    m.poll();
    assert!(!m.busy());
    let messages = m.messages().to_vec();
    (m, messages)
}

#[test]
fn interrupted_command_shows_the_engine_error_reason() {
    let (_, messages) = interrupted_by("not_ready");
    assert_eq!(messages.last().unwrap(), "Command interrupted: not_ready.");
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
fn clicking_an_adjacent_closed_door_bumps_it_open_like_tiles() {
    let (t, h) = session();
    let mut m = connected(t.on("bn.command.submit", walk_reply()));
    with_door(&h, 4);
    m.poll();
    assert!(m.travel(Pos { x: 21, y: 12 }));
    assert_eq!(
        submits(&h)[0]["params"]["operation"],
        json!({"kind":"action","action_id":"RIGHT"})
    );
}

#[test]
fn clicking_a_distant_closed_door_still_asks_the_engine_for_a_route() {
    let (t, h) = session();
    let mut m = connected(t.on("bn.command.submit", walk_reply()));
    with_door(&h, 6);
    m.poll();
    assert!(m.travel(Pos { x: 23, y: 12 }));
    assert_eq!(
        submits(&h)[0]["params"]["operation"],
        json!({"kind":"travel","pos":pos(6, 4)})
    );
}

#[test]
fn travel_that_plans_nothing_and_moves_nothing_says_the_native_cant_travel_message() {
    let (t, h) = session();
    let mut m = connected(t.on("bn.command.submit", walk_reply()));
    assert!(m.travel(Pos { x: 22, y: 12 }));
    h.push(json!({"jsonrpc":"2.0","method":"bn.command","params":{
        "epoch":"e7f3","command_id":"c:5","stage":"completed"}}));
    m.poll();
    assert_eq!(m.command().unwrap().stage, "done: You can't travel there.");
}

#[test]
fn travel_that_planned_a_route_is_simply_done() {
    let (t, h) = session();
    let mut m = connected(t.on("bn.command.submit", walk_reply()));
    assert!(m.travel(Pos { x: 22, y: 12 }));
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
    assert_eq!(m.cell(view).desc, "f_bench", "seen furniture draws");

    h.push(cell_event(
        42,
        json!({"at": at, "known": "remembered", "memory": {"terrain": floor, "overlay": bench}}),
    ));
    m.poll();
    let remembered = m.cell(view);
    assert_eq!(
        (remembered.glyph, remembered.desc.as_str()),
        ('{', "f_bench")
    );

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
        assert!(m.travel(Pos { x: 39, y: 12 }));
        h.push(json!({"jsonrpc":"2.0","method":"bn.command","params":{
            "epoch":"e7f3","command_id":id,"stage":"rejected","error":"validation_failed"}}));
        m.poll();
        assert!(!m.busy());
    }
    assert_eq!(
        m.messages(),
        ["Travel rejected: that square is outside the map window the engine can resolve."]
    );
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

#[test]
fn a_click_outside_the_engines_view_is_ignored_without_a_message() {
    let (t, h) = session();
    let mut m = connected(t.on("bn.command.submit", walk_reply()));
    h.push(view_event(41, -15, 20));
    m.poll();
    // View column 36 would be absolute x 21, one past the window; column 35 is its last square.
    assert!(m.travel(Pos { x: 39, y: 12 }));
    assert!(submits(&h).is_empty());
    assert!(m.messages().is_empty(), "{:?}", m.messages());
    assert!(m.travel(Pos { x: 35, y: 12 }));
    assert_eq!(submits(&h).len(), 1);
}

fn not_ready() -> Value {
    json!({"jsonrpc":"2.0","id":null,"error":{"code":1000,"message":"x","data":{"kind":"not_ready"}}})
}

#[test]
fn interrupt_sends_bn_interrupt_while_a_command_is_in_flight() {
    let (t, h) = session();
    let mut m = connected(t.on("bn.command.submit", walk_reply()).on(
        "bn.interrupt",
        vec![json!({"jsonrpc":"2.0","id":null,"result":{}})],
    ));
    assert!(m.step_toward(Pos { x: 30, y: 12 }));
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
fn a_not_ready_interrupt_is_explained_once() {
    let (t, _h) = session();
    let mut m = connected(
        t.on("bn.command.submit", walk_reply())
            .on("bn.interrupt", vec![not_ready()])
            .on("bn.interrupt", vec![not_ready()]),
    );
    assert!(m.step_toward(Pos { x: 30, y: 12 }));
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
        "tile": "overlay_female_worn_jacket", "looks_like": ["overlay_worn_jacket"]
    }]);
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
                "looks_like": ["t_wall_base"], "subtile": "edge", "rotation": 1},
            "items": [
                {"kind": "item", "id": "corpse", "glyph": "%", "color": "red", "tile": "corpse_mon_zombie",
                 "looks_like": ["corpse"], "stack": 2}
            ]}),
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
    assert_eq!(layers, ["t_wall", "corpse_mon_zombie", "highlight_item"]);
    assert_eq!(cell.layers[2].glyph, '\0');
    let wall = &cell.layers[0];
    assert_eq!((wall.subtile.as_deref(), wall.rotation), (Some("edge"), 1));
    assert_eq!(wall.fallbacks, ["t_wall_base"]);
    assert_eq!(cell.layers[1].fallbacks, ["corpse"]);
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
            "overlay_female_worn_jacket"
        ]
    );
    assert!(!me.layers[2].mirrored && me.layers[2].creature);
    assert_eq!(me.layers[2].glyph, '@');
    assert_eq!(me.layers[3].glyph, '\0');
    assert_eq!(me.layers[3].fallbacks, ["overlay_worn_jacket"]);
    // A square without sprite facts keeps its ASCII look.
    let plain = at(2, 2);
    assert_eq!((plain.glyph, plain.layers.len()), ('.', 1));
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
}

fn avatar_event(inventory: Value, ground: Value) -> Value {
    let mut event = fixture("events_41_walk");
    let avatar = &mut event["params"]["events"][0]["changes"]["avatar"];
    avatar["inventory"] = inventory;
    avatar["ground"] = ground;
    event
}

fn item(name: &str, count: u64, slot: Option<&str>) -> Value {
    let mut row = json!({"look": {"kind": "item", "id": "x", "glyph": "/", "color": "white"}, "name": name, "count": count});
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
            item("combat knife", 1, Some("wielded")),
            item("fur scarf", 1, Some("worn")),
            item("hammer", 2, Some("carried"))
        ]),
        json!([item("light disposable battery (12/25)", 3, None)]),
    ));
    m.poll();
    assert_eq!(
        m.inventory(),
        ["combat knife (wielded)", "fur scarf (worn)", "hammer x2"]
    );
    assert_eq!(m.ground(), ["light disposable battery (12/25) x3"]);
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

#[test]
fn picking_up_a_ground_item_marks_that_item_in_the_native_menu_and_confirms() {
    let (t, h) = session();
    let mut m = connected(
        t.on("bn.command.submit", walk_reply())
            .on("bn.command.submit", walk_reply())
            .on("bn.command.submit", walk_reply()),
    );
    h.push(avatar_event(
        json!([]),
        json!([item("combat knife", 1, None), item("tin can", 1, None)]),
    ));
    m.poll();
    m.pick_up(1);
    let mut menu = fixture("events_43_menu");
    menu["params"]["events"][0]["sequence"] = json!("42");
    let interaction = &mut menu["params"]["events"][0]["changes"]["interaction"];
    interaction["actions"] =
        json!([{"id": "pickup_all", "name": "x"}, {"id": "CONFIRM", "name": "Confirm"}]);
    interaction["interaction"]["choices"][1]["enabled"] = json!(true);
    interaction["interaction"]["choices"][1]["selectable"] = json!(true);
    interaction["interaction"]["choices"][1]["denial"] = json!("");
    interaction["interaction"]["choices"][1]["label"] = json!("tin can");
    // The engine publishes the menu before it reports the command done.
    h.push(menu);
    h.push(fixture("command_completed"));
    m.poll();
    let operations = submits(&h)
        .into_iter()
        .map(|s| s["params"]["operation"].clone())
        .collect::<Vec<_>>();
    assert_eq!(
        operations[0]["action_id"], "pickup",
        "the fixture offers no pickup_all"
    );
    assert_eq!(
        operations[1],
        json!({"kind": "choose", "choice_id": "c:2"}),
        "the clicked item, not the first row"
    );
}

fn crafting(name: &str) -> Value {
    let text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/crafting.json"
    ))
    .unwrap();
    serde_json::from_str::<Value>(&text).unwrap()[name].clone()
}

#[test]
fn crafting_menu_exposes_context_field_details_and_sends_fill_and_set_count() {
    let (t, h) = session();
    let mut m = connected(t.on("bn.command.submit", walk_reply()));
    h.push(change(41, json!({"interaction": crafting("weapon")})));
    m.poll();
    let menu = m.menu().unwrap();
    assert_eq!(menu.context, "CRAFTING");
    assert!(menu.can_set_count);
    let field = menu.field.unwrap();
    assert_eq!((field.id.as_str(), field.value.as_str()), ("filter", ""));
    let club = menu.choices.iter().find(|c| c.id == "recipe:aa").unwrap();
    assert!(club.description.starts_with("Time to complete"));
    assert_eq!(
        club.columns,
        [("Skill".to_string(), "fabrication 1".into())]
    );

    m.fill("nope", "club", true);
    assert!(submits(&h).is_empty(), "unknown fields are never submitted");
    m.set_count("recipe:bb", 2);
    assert!(submits(&h).is_empty(), "disabled recipes take no count");
    m.fill("filter", "club", true);
    assert_eq!(
        submits(&h)[0]["params"]["operation"],
        json!({"kind":"fill","field_id":"filter","value":"club","submit":true})
    );
}

#[test]
fn set_count_submits_for_an_enabled_recipe_and_batch_mode_has_no_field() {
    let (t, h) = session();
    let mut m = connected(t.on("bn.command.submit", walk_reply()));
    h.push(change(41, json!({"interaction": crafting("weapon")})));
    m.poll();
    m.set_count("recipe:aa", 3);
    assert_eq!(
        submits(&h)[0]["params"]["operation"],
        json!({"kind":"set_count","choice_id":"recipe:aa","count":3})
    );
    let (t, h) = session();
    let mut m = connected(t.on("bn.command.submit", walk_reply()));
    h.push(change(41, json!({"interaction": crafting("batch")})));
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
            "allow_cancel":false,"choices":[ack],"choice_total":1,"allow_set_count":false})
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
fn the_selected_tab_picks_the_pane_and_a_target_interaction_is_a_cursor_not_a_menu() {
    let (t, h) = session();
    let mut m = connected(t);
    let row = |id: &str, label: &str, pane: Option<&str>, selected: bool| {
        let mut c = json!({"id":id,"label":label,"description":"d","denial":"","enabled":true,
            "selectable":true,"selected":selected,"columns":[{"label":"Base","value":"8"}]});
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
    assert_eq!(
        menu.entries()[0].columns,
        [("Base".to_string(), "8".to_string())]
    );
    h.push(show(
        42,
        json!([]),
        json!({"schema_id":"s:l","kind":"target","context":"LOOK","title":"Look Around","message":"",
            "allow_cancel":true,"choices":[],"choice_total":0,"allow_set_count":false,
            "target":{"current":{"dim":"","x":5,"y":4,"z":0}}}),
    ));
    m.poll();
    assert!(m.menu().is_none());
    let origin = m.origin();
    assert_eq!(
        m.focus(),
        Some(Pos {
            x: 5 - origin.x,
            y: 4 - origin.y
        })
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
    fn silence_with_a_command_in_flight_is_a_stall_that_a_message_clears() {
        let (t, h) = session();
        let (mut m, clock) = with_clock(t.on("bn.command.submit", walk_reply()));
        assert!(m.step_toward(Pos { x: 30, y: 12 }));
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
        assert!(m.step_toward(Pos { x: 30, y: 12 }));
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
        assert!(m.step_toward(Pos { x: 30, y: 12 }));
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

#[test]
fn returning_to_the_main_menu_clears_the_previous_games_log_and_command_lines() {
    let (t, h) = session();
    let mut m = connected(t);
    h.push(fixture("events_41_walk"));
    h.push(logged(42, "7", "You open the door.", 1));
    m.poll();
    m.say("Something blocks the way.".into());
    assert_eq!(m.native_log().len(), 1);
    let mut menu = crafting("favorites");
    menu["interaction"]["context"] = json!("MAIN_MENU");
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
fn pickup_rows_carry_the_native_highlight_marks_and_counts() {
    let text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/pickup.json"
    ))
    .unwrap();
    let boundary = serde_json::from_str::<Value>(&text).unwrap()["pickup"].clone();
    let (t, h) = session();
    let mut m = connected(t);
    h.push(change(41, json!({"interaction": boundary})));
    m.poll();
    let menu = m.menu().unwrap();
    assert_eq!(menu.context, "PICKUP");
    let rows = menu
        .choices
        .iter()
        .map(|c| {
            (
                c.highlighted,
                c.selected,
                c.selected_count,
                c.available_count,
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        rows,
        [
            (false, false, None, Some(1)),
            (true, true, Some(2), Some(3))
        ]
    );
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
    let mut menu = crafting("favorites");
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
    let mut real = crafting("weapon");
    real["actions"] = json!([{"id": "YES", "name": "Yes"}]);
    h.push(change(42, json!({"interaction": real})));
    m.poll();
    let kept = m
        .menu()
        .unwrap()
        .choices
        .into_iter()
        .find(|c| c.id == "recipe:aa")
        .unwrap();
    assert!(kept.description.starts_with("Time to complete"));
}

#[test]
fn a_menu_banner_is_published_line_by_line() {
    let mut menu = crafting("favorites");
    menu["interaction"]["banner"] = json!(["<color_light_cyan>art</color>", "", "Version: x"]);
    let (t, h) = session();
    let mut m = connected(t);
    h.push(change(41, json!({"interaction": menu})));
    m.poll();
    assert_eq!(
        m.menu().unwrap().banner,
        ["<color_light_cyan>art</color>", "", "Version: x"]
    );
}
