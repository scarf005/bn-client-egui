mod common;
use bn_client_egui::transport::ScriptedTransport;
use bn_client_egui::{EngineModel, GameModel, Pos};
use common::{connected, fixture, session, subscribe_reply};
use serde_json::{Value, json};

fn submits(h: &bn_client_egui::transport::ScriptHandle) -> Vec<Value> {
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
    assert_eq!(m.messages(), ["You walk east."]);
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
fn origin_reports_the_view_offset_so_targets_survive_recentering() {
    let (t, _h) = session();
    let m = connected(t);
    // Avatar (3, 4) is drawn at view (20, 12).
    assert_eq!(m.origin(), Pos { x: -17, y: -8 });
}

#[test]
fn cell_colors_come_from_the_game_palette() {
    use bn_client_egui::colors::ColorMap;
    let json = r#"[{"type":"colordef","BLACK":[0,0,0],"RED":[1,0,0],"GREEN":[2,0,0],"BROWN":[3,0,0],
    "BLUE":[4,0,0],"MAGENTA":[5,0,0],"CYAN":[6,0,0],"GRAY":[7,0,0],"DGRAY":[8,0,0],"LRED":[9,0,0],
    "LGREEN":[10,0,0],"YELLOW":[11,0,0],"LBLUE":[12,0,0],"LMAGENTA":[13,0,0],"LCYAN":[14,0,0],"WHITE":[15,0,0]}]"#;
    let (t, _h) = session();
    let mut m = EngineModel::new(Box::new(t)).with_colors(ColorMap::from_json(json).unwrap());
    m.poll();
    // The zombie is light_green, palette slot 10.
    assert_eq!(
        m.cell(Pos { x: 23, y: 12 }).color,
        eframe::egui::Color32::from_rgb(10, 0, 0)
    );
}

fn with_keys(t: ScriptedTransport) -> EngineModel {
    use bn_client_egui::keys::KeyMap;
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
