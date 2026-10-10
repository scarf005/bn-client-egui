#![recursion_limit = "512"]
mod common;
use bn_client_egui::app::CELL;
use bn_client_egui::transport::ScriptHandle;
use bn_client_egui::{App, EngineModel, GameModel};
use common::{fixture, session};
use eframe::egui;
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use serde_json::{Value, json};

type H<'a> = Harness<'a, App<EngineModel>>;

fn harness<'a>() -> (H<'a>, ScriptHandle) {
    let (t, h) = session();
    let t = (0..6).fold(t, |t, _| {
        t.on("bn.command.submit", vec![fixture("submit_result")])
    });
    let model = EngineModel::new(Box::new(t));
    let harness = Harness::builder()
        .with_size(egui::vec2(1100.0, 800.0))
        .build_ui_state(
            |ui, app: &mut App<EngineModel>| app.show(ui),
            App::new(model),
        );
    (harness, h)
}

fn submits(h: &ScriptHandle) -> Vec<Value> {
    h.sent()
        .into_iter()
        .filter(|m| m["method"] == "bn.command.submit")
        .map(|m| m["params"]["operation"].clone())
        .collect()
}

/// An `interaction.changed` event at `sequence` carrying these actions and interaction.
fn show(sequence: u32, actions: Value, interaction: Value) -> Value {
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

fn choice(id: &str, label: &str) -> Value {
    json!({"id":id,"label":label,"enabled":true,"description":"","denial":"",
        "selectable":true,"selected":false,"columns":[]})
}

fn popup(allow_cancel: bool, message: &str, ack: Value) -> Value {
    json!({"schema_id":"s:popup","kind":"choices","context":"POPUP_WAIT","title":"",
        "message":message,"allow_cancel":allow_cancel,"choices":[ack],"choice_total":1,
        "allow_set_count":false})
}

fn any_key_popup() -> Value {
    let mut ack = choice("acknowledge", "Press any key");
    ack["description"] = "ANY_INPUT".into();
    popup(false, "You have nothing to wield.", ack)
}

#[test]
fn any_key_popup_closes_on_escape_space_enter_and_letters() {
    for key in [egui::Key::Escape, egui::Key::Enter] {
        let (mut h, script) = harness();
        h.run_steps(3);
        script.push(show(
            41,
            json!([{"id":"ANY_INPUT","name":"ANY_INPUT","keys":[]}]),
            any_key_popup(),
        ));
        h.run_steps(3);
        h.key_press(key);
        h.run_steps(2);
        assert_eq!(
            submits(&script),
            [json!({"kind":"choose","choice_id":"acknowledge"})],
            "{key:?}"
        );
    }
    // Space acts on the focused button, as in any toolkit; a letter goes to the engine.
    {
        let (mut h, script) = harness();
        h.run_steps(3);
        script.push(show(41, json!([]), any_key_popup()));
        h.run_steps(3);
        h.key_press(egui::Key::Space);
        h.run_steps(2);
        assert_eq!(
            submits(&script),
            [json!({"kind":"choose","choice_id":"acknowledge"})]
        );
    }
    for text in ["w"] {
        let (mut h, script) = harness();
        h.run_steps(3);
        script.push(show(41, json!([]), any_key_popup()));
        h.run_steps(3);
        h.event(egui::Event::Text(text.into()));
        h.run_steps(2);
        assert_eq!(
            submits(&script),
            [json!({"kind":"choose","choice_id":"acknowledge"})],
            "{text:?}"
        );
    }
}

#[test]
fn a_popup_shows_one_dismiss_control_and_no_context_name_as_title() {
    let (mut h, script) = harness();
    h.run_steps(3);
    let mut ack = choice("acknowledge", "Cancel popup");
    ack["description"] = "QUIT".into();
    script.push(show(
        41,
        json!([{"id":"QUIT","name":"Cancel popup","keys":["ESC","SPACE","RETURN"]}]),
        popup(true, "Welcome to the tutorial!", ack),
    ));
    h.run_steps(3);
    h.get_by_label("Welcome to the tutorial!");
    assert!(h.query_by_label("POPUP_WAIT").is_none());
    assert!(
        h.query_by_label("Cancel").is_none(),
        "only the engine's own row dismisses"
    );
    assert_eq!(
        h.get_all_by_label_contains("Cancel popup").count(),
        2,
        "row and action"
    );
}

#[test]
fn space_closes_a_cancel_popup_through_its_focused_button() {
    let (mut h, script) = harness();
    h.run_steps(3);
    let mut ack = choice("acknowledge", "Cancel popup");
    ack["description"] = "QUIT".into();
    script.push(show(
        41,
        json!([{"id":"QUIT","name":"Cancel popup","keys":["ESC","SPACE","RETURN"]}]),
        popup(true, "Welcome", ack),
    ));
    h.run_steps(3);
    h.key_press(egui::Key::Space);
    h.run_steps(2);
    assert_eq!(
        submits(&script),
        [json!({"kind":"choose","choice_id":"acknowledge"})]
    );
}

#[test]
fn greater_than_runs_the_level_action_the_engine_binds_to_it() {
    let (mut h, script) = harness();
    h.run_steps(3);
    script.push(show(
        41,
        json!([{"id":"LEVEL_DOWN","name":"Descend Stairs","keys":[">"]},
               {"id":"LEVEL_UP","name":"Ascend Stairs","keys":["<"]}]),
        Value::Null,
    ));
    h.run_steps(3);
    h.event(egui::Event::Text(">".into()));
    h.run_steps(2);
    assert_eq!(
        submits(&script),
        [json!({"kind":"action","action_id":"LEVEL_DOWN"})]
    );
}

#[test]
fn actions_the_engine_gives_no_key_are_not_listed() {
    let (mut h, script) = harness();
    h.run_steps(3);
    script.push(show(
        41,
        json!([{"id":"pickup","name":"Pick up","keys":["g"]},
               {"id":"COORDINATE","name":"COORDINATE","keys":[]}]),
        Value::Null,
    ));
    h.run_steps(3);
    h.get_by_label("Pick up (g)");
    assert!(h.query_by_label("COORDINATE").is_none());
}

fn look_mode(sequence: u32, x: i32) -> Value {
    show(
        sequence,
        json!([{"id":"QUIT","name":"Quit","keys":["ESC"]},
               {"id":"RIGHT","name":"Move right","keys":["RIGHT","l"]}]),
        json!({"schema_id":"s:look","kind":"target","context":"LOOK","title":"Look Around",
            "message":"","allow_cancel":true,"choices":[],"choice_total":0,"allow_set_count":false,
            "target":{"source":{"dim":"","x":3,"y":4,"z":0},"current":{"dim":"","x":x,"y":4,"z":0},
                "range":0,"distance_metric":"square","status":"look","candidates":[]}}),
    )
}

#[test]
fn escape_leaves_look_mode_through_the_engines_quit_action_and_no_window_opens() {
    let (mut h, script) = harness();
    h.run_steps(3);
    script.push(look_mode(41, 6));
    h.run_steps(3);
    assert!(h.state().model.menu().is_none(), "the cursor is not a list");
    h.key_press(egui::Key::Escape);
    h.run_steps(2);
    assert_eq!(
        submits(&script),
        [json!({"kind":"action","action_id":"QUIT"})]
    );
}

#[test]
fn the_engines_look_cursor_is_the_map_cursor_and_arrows_do_not_move_it_twice() {
    let (mut h, script) = harness();
    h.run_steps(3);
    script.push(look_mode(41, 6));
    h.run_steps(3);
    let focus = h.state().model.focus().expect("look cursor");
    let status = format!("Cursor ({}, {})", focus.x, focus.y);
    let map = h.get_by_label("Map");
    assert!(
        map.accesskit_node()
            .description()
            .unwrap_or_default()
            .contains(&status),
        "{status}"
    );
}

#[test]
fn a_description_and_columns_of_the_selected_entry_are_shown() {
    let (mut h, script) = harness();
    h.run_steps(3);
    let mut strength = choice("stat:str", "Strength:");
    strength["selected"] = true.into();
    strength["description"] = "Strength affects your melee damage.".into();
    strength["columns"] = json!([{"label":"Current","value":"8"},{"label":"Base","value":"9"}]);
    script.push(show(
        41,
        json!([]),
        json!({"schema_id":"s:info","kind":"choices","context":"PLAYER_INFO",
            "title":"Character information","message":"","allow_cancel":true,
            "choices":[strength],"choice_total":1,"allow_set_count":false}),
    ));
    h.run_steps(3);
    assert!(h.get_all_by_label("Character information").count() >= 1);
    h.get_by_label("Strength affects your melee damage.");
    h.get_by_label("Current: 8");
    h.get_by_label("Base: 9");
}

#[test]
fn colour_tags_in_a_label_are_drawn_not_printed() {
    let (mut h, script) = harness();
    h.run_steps(3);
    script.push(show(
        41,
        json!([]),
        json!({"schema_id":"s:m","kind":"choices","context":"UILIST","title":"Drop",
            "message":"","allow_cancel":true,
            "choices":[choice("c:1","lighter<color_c_light_green> (23/25)</color>")],
            "choice_total":1,"allow_set_count":false}),
    ));
    h.run_steps(3);
    h.get_by_label("lighter (23/25)");
    assert!(h.query_by_label_contains("<color_").is_none());
}

fn tabbed(sequence: u32, selected: &str) -> Value {
    let tab = |id: &str, label: &str| {
        let mut c = choice(&format!("tab:{id}"), label);
        c["selected"] = (id == selected).into();
        c
    };
    let entry = |id: &str, pane: &str, label: &str| {
        let mut c = choice(id, label);
        c["pane_id"] = pane.into();
        c
    };
    let mut choices = vec![
        tab("motd", "MOTD"),
        tab("stats", "STATS"),
        tab("skills", "SKILLS"),
    ];
    choices.extend([
        entry("str", "stats", "Strength"),
        entry("melee", "skills", "Melee"),
    ]);
    let panes: Vec<Value> = [("motd", "MOTD"), ("stats", "STATS"), ("skills", "SKILLS")]
        .iter()
        .map(|(id, label)| json!({"id":id,"label":label,"role":"category"}))
        .collect();
    show(
        sequence,
        json!([]),
        json!({"schema_id":"s:tabs","kind":"choices","context":"PLAYER_INFO","title":"Info",
            "message":"","allow_cancel":true,"choices":choices,"choice_total":5,
            "allow_set_count":false,"compat":{"focus":{},"panes":panes}}),
    )
}

#[test]
fn only_the_selected_tabs_entries_are_listed_and_an_entryless_tab_keeps_its_tab_row() {
    let (mut h, script) = harness();
    h.run_steps(3);
    script.push(tabbed(41, "stats"));
    h.run_steps(3);
    h.get_by_label("Strength");
    assert!(h.query_by_label("Melee").is_none(), "another tab's entry");
    script.push(tabbed(42, "motd"));
    h.run_steps(3);
    assert!(h.query_by_label("Strength").is_none());
    let menu = h.state().model.menu().unwrap();
    assert_eq!(
        menu.tabs().len(),
        3,
        "tabs stay tabs when the tab has no entries"
    );
    assert!(menu.entries().is_empty());
}

#[test]
fn a_click_during_a_command_is_sent_when_it_finishes_instead_of_being_lost() {
    let (mut h, script) = harness();
    h.run_steps(3);
    script.push(show(
        41,
        json!([{"id":"UP","name":"Move up","keys":["k"]},
               {"id":"LEVEL_UP","name":"Ascend Stairs","keys":["<"]}]),
        Value::Null,
    ));
    h.run_steps(3);
    h.event(egui::Event::Text("k".into()));
    h.run_steps(1);
    h.get_by_label("Ascend Stairs (<)").click();
    h.run_steps(2);
    assert_eq!(submits(&script).len(), 1, "the engine is still busy");
    script.push(json!({"jsonrpc":"2.0","method":"bn.command","params":{
        "epoch":"e7f3","command_id":"c:5","stage":"completed"}}));
    script.push(show(
        42,
        json!([
        {"id":"UP","name":"Move up","keys":["k"]},
        {"id":"LEVEL_UP","name":"Ascend Stairs","keys":["<"]}]),
        Value::Null,
    ));
    h.run_steps(4);
    let sent = submits(&script);
    assert_eq!(sent.len(), 2);
    assert_eq!(sent[1], json!({"kind":"action","action_id":"LEVEL_UP"}));
}

fn view_event(sequence: u32, min_x: i32, max_x: i32) -> Value {
    json!({"jsonrpc":"2.0","method":"bn.events","params":{"epoch":"e7f3","events":[{
        "sequence": sequence.to_string(), "revision": (sequence - 9).to_string(),
        "type": "cells.seen", "changes": { "view": {
            "min": {"dim":"","x":min_x,"y":-8,"z":0}, "max": {"dim":"","x":max_x,"y":16,"z":0}}}}]}})
}

#[test]
fn the_cursor_stays_on_its_map_square_when_the_view_scrolls() {
    let (mut h, script) = harness();
    h.run_steps(3);
    script.push(view_event(41, -16, 23));
    h.run_steps(3);
    let origin = h.get_by_label("Map").rect().min;
    h.hover_at(origin + egui::vec2(10.5 * CELL, 5.5 * CELL));
    h.run_steps(3);
    let description = |h: &H| {
        h.get_by_label("Map")
            .accesskit_node()
            .description()
            .unwrap_or_default()
            .to_string()
    };
    assert!(
        description(&h).contains("Cursor (10, 5)"),
        "{}",
        description(&h)
    );
    script.push(view_event(42, -13, 26));
    h.run_steps(3);
    assert!(
        description(&h).contains("Cursor (7, 5)"),
        "{}",
        description(&h)
    );
}

#[test]
fn control_function_and_shift_tab_keys_run_the_engines_actions_and_a_digit_may_be_numpad() {
    let (mut h, script) = harness();
    h.run_steps(3);
    script.push(show(
        41,
        json!([{"id":"save","name":"Save","keys":["CTRL+S"]},
               {"id":"help","name":"Help","keys":["F1"]},
               {"id":"back","name":"Back","keys":["BACKTAB"]},
               {"id":"pause","name":"Pause","keys":[".","NUMPAD_5"]}]),
        Value::Null,
    ));
    h.run_steps(3);
    let finish = |h: &mut H, script: &ScriptHandle, n: u32| {
        script.push(json!({"jsonrpc":"2.0","method":"bn.command","params":{
            "epoch":"e7f3","command_id":"c:5","stage":"completed"}}));
        script.push(show(
            n,
            json!([
            {"id":"save","name":"Save","keys":["CTRL+S"]},
            {"id":"help","name":"Help","keys":["F1"]},
            {"id":"back","name":"Back","keys":["BACKTAB"]},
            {"id":"pause","name":"Pause","keys":[".","NUMPAD_5"]}]),
            Value::Null,
        ));
        h.run_steps(3);
    };
    h.key_press_modifiers(egui::Modifiers::CTRL, egui::Key::S);
    h.run_steps(2);
    finish(&mut h, &script, 42);
    h.key_press(egui::Key::F1);
    h.run_steps(2);
    finish(&mut h, &script, 43);
    h.key_press_modifiers(egui::Modifiers::SHIFT, egui::Key::Tab);
    h.run_steps(2);
    finish(&mut h, &script, 44);
    h.event(egui::Event::Text("5".into()));
    h.run_steps(2);
    assert_eq!(
        submits(&script),
        ["save", "help", "back", "pause"].map(|id| json!({"kind":"action","action_id":id}))
    );
}

fn list_menu(sequence: u32, focus: &str) -> Value {
    show(
        sequence,
        json!([{"id":"UP","name":"Previous","keys":["UP"]},
               {"id":"DOWN","name":"Next","keys":["j"]},
               {"id":"CONFIRM","name":"Confirm","keys":["x"]},
               {"id":"QUIT","name":"Cancel","keys":["ESC"]}]),
        json!({"schema_id":"s:list","kind":"choices","context":"UILIST","title":"Pick","message":"",
            "allow_cancel":true,
            "choices":[choice("c:1","first"),choice("c:2","second")],
            "choice_total":2,"allow_set_count":false,
            "compat":{"focus":{"choice_id":focus},"panes":[]}}),
    )
}

#[test]
fn a_menus_rebound_keys_move_and_confirm_through_the_engine_and_focus_follows_its_cursor() {
    let (mut h, script) = harness();
    h.run_steps(3);
    script.push(list_menu(41, "c:1"));
    h.run_steps(4);
    // The player rebound DOWN to j and CONFIRM to x: the client has no key of its own for either.
    h.event(egui::Event::Text("j".into()));
    h.run_steps(2);
    // The engine moved its cursor to the second row: keyboard focus follows it.
    script.push(json!({"jsonrpc":"2.0","method":"bn.command","params":{
        "epoch":"e7f3","command_id":"c:5","stage":"completed"}}));
    script.push(list_menu(42, "c:2"));
    h.run_steps(4);
    assert!(
        h.get_by_label("second").accesskit_node().is_focused(),
        "focus on the engine's cursor row"
    );
    h.event(egui::Event::Text("x".into()));
    h.run_steps(2);
    assert_eq!(
        submits(&script),
        ["DOWN", "CONFIRM"].map(|id| json!({"kind":"action","action_id":id}))
    );
}
