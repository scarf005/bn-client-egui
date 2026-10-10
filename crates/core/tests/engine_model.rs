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
    assert_eq!(m.cell(Pos { x: 21, y: 11 }).desc, "t_wall");
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

fn with_keys(t: ScriptedTransport) -> EngineModel {
    use bn_client_core::keys::KeyMap;
    let json = r#"[{"id":"pickup","category":"DEFAULTMODE","bindings":[{"input_method":"keyboard","key":"g"}]}]"#;
    let mut m =
        EngineModel::new(Box::new(t)).with_keys(KeyMap::from_json(json, "DEFAULTMODE").unwrap());
    m.poll();
    m
}

#[test]
fn actions_list_names_ids_and_keys_from_the_boundary() {
    let (t, _h) = session();
    let m = with_keys(t);
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
    let mut m = with_keys(t.on("bn.command.submit", walk_reply()));
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
    let mut m = with_keys(t);
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
    let mut m = with_keys(t.on("bn.command.submit", walk_reply()));
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
fn travel_that_plans_nothing_and_moves_nothing_says_there_is_no_route() {
    let (t, h) = session();
    let mut m = connected(t.on("bn.command.submit", walk_reply()));
    assert!(m.travel(Pos { x: 22, y: 12 }));
    h.push(json!({"jsonrpc":"2.0","method":"bn.command","params":{
        "epoch":"e7f3","command_id":"c:5","stage":"completed"}}));
    m.poll();
    assert_eq!(m.command().unwrap().stage, "done: no route there");
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
    let mut part = fixture("snapshot_part_1");
    part["params"]["cells"].as_array_mut().unwrap().extend([
        json!({"at": {"dim": "", "x": 9, "y": 8, "z": 0}, "known": "visible", "light": 1,
            "terrain": {"kind": "terrain", "id": "t_wall", "glyph": "|", "color": "light_gray",
                "looks_like": ["t_wall_base"], "subtile": "edge", "rotation": 1},
            "items": [
                {"kind": "item", "id": "corpse", "glyph": "%", "color": "red", "tile": "corpse_mon_zombie",
                 "looks_like": ["corpse"]}
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
    assert_eq!(layers, ["t_wall", "corpse_mon_zombie"]);
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
    assert_eq!(ids, ["t_dirt", "mon_zombie"]);
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
