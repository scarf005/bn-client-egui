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

fn open_palette(h: &mut H) {
    h.event(egui::Event::Key {
        key: egui::Key::P,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::COMMAND,
    });
    h.run_steps(3);
}

fn harness<'a>() -> (H<'a>, ScriptHandle) {
    let (t, h) = session();
    let t = (0..6).fold(t, |t, _| {
        t.on("bn.command.submit", vec![fixture("submit_result")])
    });
    // A manual clock: the settle delay never elapses on a loaded machine.
    let clock = bn_client_core::Clock::manual();
    let model = EngineModel::new(Box::new(t)).with_clock(clock.clone());
    let harness = Harness::builder()
        .with_size(egui::vec2(1100.0, 800.0))
        .build_ui_state(
            |ui, app: &mut App<EngineModel>| app.show(ui),
            App::new(model).with_clock(clock),
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
    {
        let text = "w";
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
    open_palette(&mut h);
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
    open_palette(&mut h);
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
    // The status line names squares by their absolute position, not their place in the window.
    let at = h.state().model.origin();
    let status = format!("Cursor ({}, {})", focus.x + at.x, focus.y + at.y);
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
        json!({"schema_id":"s:info","kind":"choices","context":"INFO_LIST",
            "title":"Character information","message":"","allow_cancel":true,
            "choices":[strength],"choice_total":1,"allow_set_count":false}),
    ));
    h.run_steps(3);
    assert!(h.get_all_by_label("Character information").count() >= 1);
    h.get_by_label("Strength affects your melee damage.");
    // The values sit in the entry's own row, so its highlight covers them.
    h.get_by_label("Strength:  Current: 8  Base: 9");
}

#[test]
fn a_value_that_repeats_the_entry_text_is_not_printed_twice() {
    let (mut h, script) = harness();
    h.run_steps(3);
    let mut item = choice("c:1", "lighter (3/25)");
    item["columns"] = json!([{"label":"lighter (3/25)","value":"lighter (3/25)"}]);
    script.push(show(
        41,
        json!([]),
        json!({"schema_id":"s:m","kind":"choices","context":"UILIST","title":"Wield item",
            "message":"","allow_cancel":true,"choices":[item],
            "choice_total":1,"allow_set_count":false}),
    ));
    h.run_steps(3);
    h.get_by_label("lighter (3/25)");
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
        json!([{"id":"NEXT_TAB","name":"Next tab","keys":["TAB"]},
               {"id":"PREV_TAB","name":"Previous tab","keys":["BACKTAB"]}]),
        json!({"schema_id":"s:tabs","kind":"choices","context":"INFO_TABS","title":"Info",
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
fn tab_and_shift_tab_are_the_engines_tab_keys_on_a_tabbed_screen() {
    let (mut h, script) = harness();
    h.run_steps(3);
    script.push(tabbed(41, "stats"));
    h.run_steps(3);
    h.key_press(egui::Key::Tab);
    h.run_steps(2);
    script.push(json!({"jsonrpc":"2.0","method":"bn.command","params":{
        "epoch":"e7f3","command_id":"c:5","stage":"completed"}}));
    script.push(tabbed(42, "skills"));
    h.run_steps(3);
    h.key_press_modifiers(egui::Modifiers::SHIFT, egui::Key::Tab);
    h.run_steps(2);
    assert_eq!(
        submits(&script),
        ["NEXT_TAB", "PREV_TAB"].map(|id| json!({"kind":"action","action_id":id}))
    );
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
    open_palette(&mut h);
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
    // Absolute: the square keeps its name while the window scrolls under the cursor.
    let at = h.state().model.origin();
    let named = format!("Cursor ({}, {})", 10 + at.x, 5 + at.y);
    assert!(description(&h).contains(&named), "{}", description(&h));
    script.push(view_event(42, -13, 26));
    h.run_steps(3);
    assert!(description(&h).contains(&named), "{}", description(&h));
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

fn screen(sequence: u32) -> Value {
    show(
        sequence,
        json!([{"id":"NEXT_TAB","name":"Next tab","keys":["TAB"]},
               {"id":"QUIT","name":"Cancel","keys":["ESC"]}]),
        json!({"schema_id":"s:screen","kind":"custom","context":"NEW_CHAR_STATS","title":"",
            "message":"Strength:    8\nDexterity:   8","allow_cancel":false,"choices":[],
            "choice_total":0,"allow_set_count":false}),
    )
}

#[test]
fn a_composed_screen_is_shown_as_text_and_every_key_including_tab_is_the_engines() {
    let (mut h, script) = harness();
    h.run_steps(3);
    script.push(screen(41));
    h.run_steps(4);
    assert!(h.state().model.menu().unwrap().screen);
    h.get_by_label_contains("Strength:    8");
    h.key_press(egui::Key::Tab);
    h.run_steps(2);
    assert_eq!(
        submits(&script),
        [json!({"kind":"action","action_id":"NEXT_TAB"})]
    );
}

#[test]
fn leaving_look_mode_puts_the_cursor_back_on_the_player() {
    let (mut h, script) = harness();
    h.run_steps(3);
    script.push(look_mode(41, 9));
    h.run_steps(3);
    script.push(show(42, json!([]), Value::Null));
    h.run_steps(3);
    let me = h.state().model.player();
    let at = h.state().model.origin();
    let status = format!("Cursor ({}, {})", me.x + at.x, me.y + at.y);
    let map = h.get_by_label("Map");
    let description = map.accesskit_node().description().unwrap_or_default();
    assert!(description.contains(&status), "{status} in {description}");
}

fn text_prompt(value: &str) -> Value {
    json!({"schema_id":"s:filter","kind":"field","context":"STRING_INPUT","title":"Set filter",
        "message":"","allow_cancel":true,"choices":[],"choice_total":0,"allow_set_count":false,
        "field":{"id":"field:text","label":"Filter:","description":"","value":value,
            "type":"text","max_length":30,"printable":false}})
}

#[test]
fn a_text_prompt_is_typed_into_a_box_and_enter_sends_it() {
    let (mut h, script) = harness();
    h.run_steps(3);
    script.push(show(41, json!([]), text_prompt("old")));
    h.run_steps(3);
    let edit = h.get_by_label("Filter:");
    edit.click();
    h.run_steps(2);
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::A);
    h.run_steps(1);
    h.get_by_label("Filter:").type_text("shirt");
    h.run_steps(2);
    // Every change is sent as a fill that keeps the prompt open; the engine takes the next one once
    // the command in flight is done, and Enter submits the newest text.
    let typed = submits(&script);
    assert!(!typed.is_empty());
    assert!(
        typed
            .iter()
            .all(|s| s["kind"] == "fill" && s["submit"] == false),
        "{typed:?}"
    );
    script.push(json!({"jsonrpc":"2.0","method":"bn.command","params":{
        "epoch":"e7f3","command_id":"c:5","stage":"completed"}}));
    h.run_steps(2);
    h.key_press(egui::Key::Enter);
    h.run_steps(3);
    let last = submits(&script).pop().unwrap();
    assert_eq!(
        last,
        json!({"kind":"fill","field_id":"field:text","value":"shirt","submit":true})
    );
}

#[test]
fn typing_in_a_text_prompt_runs_no_game_action() {
    let (mut h, script) = harness();
    h.run_steps(3);
    script.push(show(
        41,
        json!([{"id":"WIELD","name":"Wield","keys":["w"]}]),
        text_prompt(""),
    ));
    h.run_steps(3);
    h.get_by_label("Filter:").click();
    h.get_by_label("Filter:").type_text("w");
    h.run_steps(3);
    assert!(
        submits(&script).iter().all(|s| s["kind"] == "fill"),
        "typing sends fills, never the game's own action"
    );
}

fn popup_with(message: &str) -> Value {
    let mut ack = choice("acknowledge", "Cancel popup");
    ack["description"] = "QUIT".into();
    popup(true, message, ack)
}

#[test]
fn a_popups_confirm_button_is_where_it_was_whatever_its_text_length() {
    let mut tops = vec![];
    for message in [
        "Short.",
        "A much longer popup.\nIt has\nmany\nlines\nof text.",
    ] {
        let (mut h, script) = harness();
        h.run_steps(3);
        script.push(show(
            41,
            json!([{"id":"QUIT","name":"Cancel popup","keys":["ESC"]}]),
            popup_with(message),
        ));
        h.run_steps(4);
        let button = h
            .get_all_by_role_and_label(egui::accesskit::Role::MenuItem, "Cancel popup")
            .next()
            .expect("the confirm row");
        tops.push(button.rect().min.y);
    }
    assert_eq!(tops[0], tops[1], "{tops:?}");
}

#[test]
fn space_right_after_a_popup_closes_does_not_reach_the_game_until_the_hold_ends() {
    let (mut h, script) = harness();
    h.run_steps(3);
    script.push(show(
        41,
        json!([{"id":"jump","name":"Jump","keys":["SPACE"]}]),
        popup_with("Welcome."),
    ));
    h.run_steps(3);
    h.key_press(egui::Key::Space);
    h.run_steps(2);
    assert_eq!(submits(&script).len(), 1, "the popup took the Space");
    script.push(json!({"jsonrpc":"2.0","method":"bn.command","params":{
        "epoch":"e7f3","command_id":"c:5","stage":"completed"}}));
    script.push(show(
        42,
        json!([{"id":"jump","name":"Jump","keys":["SPACE"]}]),
        Value::Null,
    ));
    h.run_steps(3);
    h.event(egui::Event::Text(" ".into()));
    h.run_steps(2);
    assert_eq!(
        submits(&script).len(),
        1,
        "held back while the player is still dismissing"
    );
    // A burst keeps the hold alive: each Space within 500 ms of the last held one is held too.
    for _ in 0..4 {
        h.state()
            .clock()
            .advance(std::time::Duration::from_millis(300));
        h.event(egui::Event::Text(" ".into()));
        h.run_steps(2);
    }
    assert_eq!(
        submits(&script).len(),
        1,
        "a steady burst never reaches the game"
    );
    h.state()
        .clock()
        .advance(std::time::Duration::from_millis(600));
    h.event(egui::Event::Text(" ".into()));
    h.run_steps(2);
    assert_eq!(
        submits(&script).last(),
        Some(&json!({"kind":"action","action_id":"jump"}))
    );
}

fn choices_menu(choices: Vec<Value>) -> Value {
    json!({"schema_id":"s:list","kind":"choices","context":"UILIST","title":"Options",
        "message":"","allow_cancel":true,"choices":choices,"choice_total":0,
        "allow_set_count":false})
}

#[test]
fn an_entry_described_only_by_its_action_name_shows_no_description_row() {
    let (mut h, script) = harness();
    h.run_steps(3);
    let mut no = choice("no", "No");
    no["selected"] = true.into();
    no["description"] = "NO".into();
    script.push(show(
        41,
        json!([]),
        choices_menu(vec![choice("yes", "Yes"), no]),
    ));
    h.run_steps(4);
    h.get_by_label("No");
    assert!(h.query_by_label("NO").is_none());
    // Real prose is still shown.
    let mut helped = choice("help", "Help");
    helped["selected"] = true.into();
    helped["description"] = "Shows what the keys do.".into();
    script.push(show(42, json!([]), choices_menu(vec![helped])));
    h.run_steps(4);
    h.get_by_label("Shows what the keys do.");
}

#[test]
fn rows_refused_for_the_same_reason_say_it_once_and_each_carries_it_as_its_description() {
    let (mut h, script) = harness();
    h.run_steps(3);
    let refused = |id: &str, label: &str, denial: &str| {
        let mut c = choice(id, label);
        c["enabled"] = false.into();
        c["denial"] = denial.into();
        c
    };
    script.push(show(
        41,
        json!([]),
        choices_menu(vec![
            choice("a", "Auto pickup enabled"),
            refused("b", "Adjacent", "Needs auto pickup"),
            refused("c", "Weight limit", "Needs auto pickup"),
            refused("d", "Volume limit", "Needs auto pickup"),
            refused("e", "Other", "Needs something else"),
        ]),
    ));
    h.run_steps(4);
    assert_eq!(h.get_all_by_label("Needs auto pickup").count(), 1);
    h.get_by_label("Needs something else");
    for row in ["Adjacent", "Weight limit", "Volume limit"] {
        let node = h.get_by_label(row);
        assert_eq!(
            node.accesskit_node().description().as_deref(),
            Some("Needs auto pickup"),
            "{row}"
        );
    }
}

#[test]
fn the_status_line_gives_absolute_coordinates_that_do_not_change_when_the_view_scrolls() {
    let (mut h, script) = harness();
    h.run_steps(5);
    h.get_by_label_contains("Player at (3, 4).");
    // The engine's window moves while the avatar stays where it is.
    script.push(view_event(41, -16, 23));
    h.run_steps(4);
    h.get_by_label_contains("Player at (3, 4).");
}

/// The look cursor of a zone's area, or the map editor, as the engine publishes it.
fn target_screen(context: &str, title: &str, message: &str, choices: Value) -> Value {
    json!({"schema_id":"s:z","kind":"target","context":context,"title":title,"message":message,
        "allow_cancel":true,"choices":choices,"choice_total":0,"allow_set_count":false,
        "target":{"source":{"dim":"","x":3,"y":4,"z":0},"current":{"dim":"","x":4,"y":4,"z":0},
            "range":0,"distance_metric":"square","status":"zone_first","candidates":[]}})
}

#[test]
fn the_status_line_tells_the_step_of_a_zones_area_beside_the_cursor() {
    let (mut h, script) = harness();
    h.run_steps(5);
    script.push(show(
        41,
        json!([]),
        target_screen("LOOK", "Look Around", "Select first point.", json!([])),
    ));
    h.run_steps(4);
    h.get_by_label_contains("Look Around: Select first point.");
}

#[test]
fn the_status_line_lists_the_map_editors_square_and_its_keys() {
    let (mut h, script) = harness();
    h.run_steps(5);
    let mut terrain = choice("tile:terrain", "Terrain");
    terrain["pane_id"] = "tile".into();
    terrain["description"] = "grass".into();
    let mut edit = choice("action:EDIT_TERRAIN", "Edit terrain");
    edit["pane_id"] = "actions".into();
    edit["hotkey"] = "e".into();
    script.push(show(
        41,
        json!([]),
        target_screen("EDITMAP", "Map editor", "", json!([terrain, edit])),
    ));
    h.run_steps(4);
    h.get_by_label_contains("Map editor: Terrain grass. e Edit terrain");
}

#[test]
fn the_cell_tooltip_does_not_hang_over_a_dialog() {
    let (mut h, script) = harness();
    h.run_steps(5);
    let map = h.get_by_label("Map").rect();
    // The avatar's own square is tin_can in the fixture, as is a ground row; the tooltip is one more.
    let me = h.state().model.player();
    let over = map.min + egui::vec2(me.x as f32 + 0.5, me.y as f32 + 0.5) * CELL;
    let named = |h: &H| h.query_all_by_label("tin_can").count();
    let rows = named(&h);
    h.hover_at(over);
    h.run_steps(3);
    assert_eq!(named(&h), rows + 1, "the tooltip shows without a dialog");
    script.push(show(
        41,
        json!([]),
        choices_menu(vec![choice("a", "Alpha")]),
    ));
    h.run_steps(4);
    h.hover_at(over + egui::vec2(1.0, 1.0));
    h.run_steps(3);
    assert_eq!(named(&h), rows, "no tooltip over a dialog");
}

#[test]
fn the_interrupt_button_appearing_does_not_move_the_map_or_the_sidebar() {
    let (mut h, _script) = harness();
    // The real theme: its button padding is what made the bar one pixel taller.
    bn_client_egui::theme::apply(&h.ctx);
    h.run_steps(5);
    let top = |h: &H| {
        (
            h.get_by_label("Map").rect().min.y,
            h.get_by_label("Ground").rect().min.y,
        )
    };
    let idle = top(&h);
    assert!(h.query_by_label("Interrupt").is_none());
    h.event(egui::Event::Text("k".into()));
    h.run_steps(3);
    assert!(
        h.query_by_label("Interrupt").is_some(),
        "a command is running"
    );
    assert_eq!(top(&h), idle);
}

#[test]
fn a_long_menu_scrolls_inside_the_window_instead_of_running_off_it() {
    let (mut h, script) = harness();
    h.run_steps(3);
    let rows = (0..60)
        .map(|n| choice(&format!("r{n}"), &format!("Row {n}")))
        .collect();
    script.push(show(41, json!([]), choices_menu(rows)));
    h.run_steps(5);
    let last_shown = (0..60)
        .filter(|n| {
            h.query_by_label(&format!("Row {n}"))
                .is_some_and(|row| row.rect().max.y <= 800.0)
        })
        .count();
    assert!(
        last_shown >= 20,
        "{last_shown} rows are fully inside the window"
    );
    assert!(
        h.query_by_label("Row 59")
            .is_none_or(|row| row.rect().min.y >= 800.0 * 0.85),
        "the rest waits behind the scroll bar"
    );
}

#[test]
fn a_popup_that_ignored_a_key_still_takes_the_next_one() {
    let (mut h, script) = harness();
    h.run_steps(3);
    script.push(show(
        41,
        json!([{"id":"QUIT","name":"Cancel popup","keys":["x","y"]}]),
        popup_with("Welcome."),
    ));
    h.run_steps(3);
    // The first key is sent but the engine keeps the popup open; the next key must still be sent.
    h.event(egui::Event::Text("x".into()));
    h.run_steps(3);
    script.push(json!({"jsonrpc":"2.0","method":"bn.command","params":{
        "epoch":"e7f3","command_id":"c:5","stage":"completed"}}));
    h.run_steps(3);
    h.event(egui::Event::Text("y".into()));
    h.run_steps(3);
    assert_eq!(submits(&script).len(), 2, "{:?}", submits(&script));
}

#[test]
fn space_closes_a_popup_while_the_focus_is_still_on_the_map() {
    let (mut h, script) = harness();
    h.run_steps(3);
    h.get_by_label("Map").focus();
    h.run_steps(2);
    script.push(show(
        41,
        json!([{"id":"QUIT","name":"Cancel popup","keys":["ESC","SPACE"]}]),
        popup_with("Close the door with c."),
    ));
    // A frame without the popup's controls in focus: the map still has it.
    h.event(egui::Event::Text(" ".into()));
    h.run_steps(3);
    assert!(!submits(&script).is_empty(), "Space reaches the engine");
}

#[test]
fn an_arrow_key_with_nothing_focused_leaves_the_toolbar_alone() {
    let (mut h, _script) = harness();
    h.run_steps(3);
    h.key_press(egui::Key::ArrowLeft);
    h.run_steps(3);
    assert!(
        !h.get_by_label("Reduced motion")
            .accesskit_node()
            .is_focused(),
        "arrows walk; they do not move the toolkit focus"
    );
    assert!(h.get_by_label("Map").accesskit_node().is_focused());
}

#[test]
fn an_arrow_key_on_a_popup_keeps_the_focus_on_its_button() {
    let (mut h, script) = harness();
    h.run_steps(3);
    script.push(show(41, json!([]), popup_with("First.")));
    h.run_steps(4);
    h.key_press(egui::Key::ArrowLeft);
    h.run_steps(4);
    let button = h
        .get_all_by_role_and_label(egui::accesskit::Role::MenuItem, "Cancel popup")
        .next()
        .unwrap();
    assert!(button.accesskit_node().is_focused());
}

#[test]
fn a_popup_right_after_another_takes_the_focus_too() {
    let (mut h, script) = harness();
    h.run_steps(3);
    script.push(show(41, json!([]), popup_with("First.")));
    h.run_steps(4);
    script.push(show(42, json!([]), popup_with("Second.")));
    h.run_steps(4);
    let button = h
        .get_all_by_role_and_label(egui::accesskit::Role::MenuItem, "Cancel popup")
        .next()
        .unwrap();
    assert!(button.accesskit_node().is_focused());
}

#[test]
fn a_popups_dismiss_button_tells_screen_readers_which_keys_close_it() {
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
    let button = h.get_by_role_and_label(egui::accesskit::Role::MenuItem, "Cancel popup");
    let hint = egui_kittest::kittest::NodeT::accesskit_node(&button)
        .description()
        .unwrap_or_default();
    assert_eq!(hint, "Space or Enter closes this popup");
}

#[test]
fn list_menu_rows_fill_the_window_like_the_native_highlight_bar() {
    let (mut h, script) = harness();
    h.run_steps(3);
    script.push(show(
        41,
        json!([]),
        choices_menu(vec![choice("a", "Alpha"), choice("b", "Longer beta row")]),
    ));
    h.run_steps(4);
    let (alpha, beta) = (
        h.get_by_label("Alpha").rect(),
        h.get_by_label("Longer beta row").rect(),
    );
    assert_eq!(alpha.width(), beta.width(), "{alpha:?} {beta:?}");
    assert!(alpha.width() >= 500.0, "{alpha:?}");
}

fn finished_command(h: &mut H, script: &ScriptHandle) {
    script.push(json!({"jsonrpc":"2.0","method":"bn.command","params":{
        "epoch":"e7f3","command_id":"c:5","stage":"completed"}}));
    h.run_steps(4);
}

#[test]
fn options_is_in_the_action_palette_and_opens_the_native_options_through_the_main_menu() {
    let (mut h, script) = harness();
    h.run_steps(3);
    script.push(show(
        41,
        json!([{"id":"main_menu","name":"Main Menu","keys":["ESC"]}]),
        Value::Null,
    ));
    h.run_steps(3);
    open_palette(&mut h);
    h.get_by_label("Search actions").focus();
    h.run_steps(2);
    h.get_by_label("Search actions").type_text("option");
    h.run_steps(2);
    h.get_by_label("Options").click();
    h.run_steps(3);
    assert_eq!(
        submits(&script),
        [json!({"kind":"action","action_id":"main_menu"})]
    );
    finished_command(&mut h, &script);
    let mut menu = choice("main:options", "Options");
    menu["selected"] = false.into();
    script.push(show(
        42,
        json!([]),
        json!({"schema_id":"s:mm","kind":"choices","context":"UILIST","title":"",
            "message":"MAIN MENU","allow_cancel":true,"choices":[choice("main:wiki","Open Wiki"), menu],
            "choice_total":2,"allow_set_count":false}),
    ));
    h.run_steps(5);
    assert_eq!(
        submits(&script).last(),
        Some(&json!({"kind":"choose","choice_id":"main:options"}))
    );
}

#[test]
fn an_action_row_of_a_tabbed_screen_shows_its_key_and_alt_down_then_enter_sends_the_action() {
    let (mut h, script) = harness();
    h.run_steps(3);
    let tab = |id: &str, label: &str, on: bool| {
        let mut c = choice(id, label);
        c["selected"] = on.into();
        c
    };
    let mut finish = choice("action:NEXT_TAB", "Finish the character");
    finish["pane_id"] = "tab0".into();
    let mut row = choice("row", "A row");
    row["pane_id"] = "tab0".into();
    row["selected"] = true.into();
    script.push(show(
        41,
        json!([{"id":"NEXT_TAB","name":"Next tab","keys":["TAB"]},
               {"id":"QUIT","name":"Cancel","keys":["ESC"]}]),
        json!({"schema_id":"s:newchar","kind":"choices","context":"NEW_CHAR_DESCRIPTION",
            "title":"Character creation","message":"","allow_cancel":true,
            "choices":[tab("tab:0", "OVERVIEW", true), tab("tab:1", "SKILLS", false), row, finish],
            "choice_total":4,"allow_set_count":false,
            "compat":{"focus":{"choice_id":"row"},
                "panes":[{"id":"tab0","label":"OVERVIEW","role":"focused"},
                         {"id":"tab1","label":"SKILLS","role":"category"}]}}),
    ));
    h.run_steps(4);
    h.get_by_label("Finish the character");
    h.get_by(|n| n.description().as_deref() == Some("[TAB]"));
    // The engine's keys never land on the row; Alt+Down does.
    h.key_press_modifiers(egui::Modifiers::ALT, egui::Key::ArrowDown);
    h.run_steps(2);
    assert!(submits(&script).is_empty(), "Alt+Down is not the engine's");
    h.key_press(egui::Key::Enter);
    h.run_steps(3);
    assert_eq!(
        submits(&script),
        [json!({"kind":"choose","choice_id":"action:NEXT_TAB"})],
        "Enter on the focused row sends the action, not CONFIRM"
    );
}

fn list_with(rows: usize) -> Value {
    let choices = (0..rows)
        .map(|i| choice(&format!("r:{i}"), &format!("row {i}")))
        .collect::<Vec<_>>();
    json!({"schema_id":"s:list","kind":"choices","context":"UILIST","title":"","message":"Pick",
        "allow_cancel":true,"choices":choices,"choice_total":rows,"allow_set_count":false})
}

#[test]
fn the_filter_button_is_for_long_lists_not_for_a_two_row_question() {
    let filter = json!([{"id":"FILTER","name":"Filter","keys":["/"]}]);
    for (rows, shown) in [(2, false), (9, false), (30, true)] {
        let (mut h, script) = harness();
        h.run_steps(3);
        script.push(show(41, filter.clone(), list_with(rows)));
        h.run_steps(4);
        assert_eq!(h.query_by_label("Filter").is_some(), shown, "{rows} rows");
    }
}

#[test]
fn a_filter_prompt_that_follows_an_untitled_list_takes_the_typing_at_once() {
    let (mut h, script) = harness();
    h.run_steps(3);
    let filter = json!([{"id":"FILTER","name":"Filter","keys":["/"]}]);
    script.push(show(41, filter.clone(), list_with(30)));
    h.run_steps(4);
    let mut prompt = text_prompt("");
    prompt["title"] = "".into();
    script.push(show(42, json!([]), prompt));
    h.run_steps(4);
    h.get_by_label("Filter:").type_text("zombie");
    h.run_steps(3);
    let sent = submits(&script);
    assert!(
        sent.iter().any(|s| s["kind"] == "fill"),
        "typed text reaches the engine without a click first: {sent:?}"
    );
}
