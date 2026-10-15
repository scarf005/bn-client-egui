mod common;
use bn_client_egui::transport::ScriptHandle;
use bn_client_egui::{App, Clock, EngineModel, GameModel};
use common::{fixture, session};
use eframe::egui;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use serde_json::{Value, json};

type H<'a> = Harness<'a, App<EngineModel>>;

fn harness<'a>(extra: Vec<Value>) -> (H<'a>, ScriptHandle) {
    let (t, h) = session();
    let t = t
        .on("bn.command.submit", vec![fixture("submit_result")])
        .on("bn.interaction.choices", extra);
    let clock = Clock::manual();
    let model = EngineModel::new(Box::new(t)).with_clock(clock.clone());
    let harness = Harness::builder()
        .with_size(egui::vec2(1100.0, 800.0))
        .build_ui_state(
            |ui, app: &mut App<EngineModel>| app.show(ui),
            App::new(model).with_clock(clock),
        );
    (harness, h)
}

/// The fixture pickup menu with its interaction edited by `edit`.
fn menu_event(edit: impl FnOnce(&mut Value)) -> Value {
    let mut menu = fixture("events_43_menu");
    edit(&mut menu["params"]["events"][0]["changes"]["interaction"]["interaction"]);
    menu
}

fn open(h: &mut H, script: &ScriptHandle, menu: Value) {
    h.run_steps(3);
    script.push(fixture("events_41_walk"));
    script.push(fixture("events_42_message"));
    script.push(menu);
    h.run_steps(3);
}

fn sent(script: &ScriptHandle, method: &str) -> Vec<Value> {
    script
        .sent()
        .into_iter()
        .filter(|m| m["method"] == method)
        .collect()
}

#[test]
fn a_menu_with_more_choices_than_listed_fetches_the_next_page_and_shows_it() {
    let reply = json!({"jsonrpc":"2.0","id":4,"result":{"boundary_id":"b:13","total":3,"choices":[
        {"id":"c:3","label":"glass jar","enabled":true,"description":"","denial":"","selectable":true,
         "selected":false,"columns":[]}]}});
    let (mut h, script) = harness(vec![reply]);
    let menu = menu_event(|i| i["choice_total"] = json!(3));
    open(&mut h, &script, menu);
    assert!(
        sent(&script, "bn.interaction.choices").is_empty(),
        "a long list does not flood the window by itself"
    );
    h.get_by_label("Load more (2 of 3)").click();
    h.run_steps(3);
    let asked = sent(&script, "bn.interaction.choices");
    assert_eq!(asked.len(), 1, "the missing page is asked for once");
    assert_eq!(asked[0]["params"]["offset"], 2);
    assert_eq!(asked[0]["params"]["boundary_id"], "b:13");
    h.run_steps(3);
    h.get_by_label("glass jar");
    assert_eq!(h.state().model.menu().unwrap().choices.len(), 3);
    assert_eq!(sent(&script, "bn.interaction.choices").len(), 1);
}

#[test]
fn typing_in_a_text_prompt_fills_the_field_and_enter_submits_it() {
    let (mut h, script) = harness(vec![]);
    let menu = menu_event(|i| {
        i["field"] = json!({"id":"field:value","label":"Filter","description":"","value":"ab",
            "type":"text","max_length":-1,"printable":true});
        i["choices"] = json!([]);
        i["choice_total"] = json!(0);
    });
    open(&mut h, &script, menu);
    let field = h.get_by_label("Filter");
    field.focus();
    h.run_steps(2);
    h.get_by_label("Filter").type_text("c");
    h.run_steps(3);
    let fills = sent(&script, "bn.command.submit");
    assert_eq!(
        fills.last().unwrap()["params"]["operation"],
        json!({"kind":"fill","field_id":"field:value","value":"abc","submit":false})
    );
    // The engine takes the next input once the first fill is done.
    script.push(fixture("command_completed"));
    h.run_steps(2);
    h.get_by_label("Filter").focus();
    h.key_press(egui::Key::Enter);
    h.run_steps(3);
    let fills = sent(&script, "bn.command.submit");
    assert_eq!(
        fills.last().unwrap()["params"]["operation"]["submit"],
        true,
        "{fills:?}"
    );
}

/// The engine takes the command asked for as request `id` and finishes it.
fn finish(script: &ScriptHandle, id: u64, command: &str) {
    script.push(json!({"jsonrpc":"2.0","id":id,"result":
        {"epoch":"e7f3","command_id":command,"stage":"received"}}));
    script.push(json!({"jsonrpc":"2.0","method":"bn.command","params":
        {"epoch":"e7f3","command_id":command,"stage":"completed"}}));
}

/// The prompt interaction with the engine's `value`, as revision `revision`.
fn prompt(value: &str, revision: u32) -> Value {
    let mut menu = menu_event(|i| {
        i["field"] = json!({"id":"field:value","label":"Filter","description":"","value":value,
            "type":"text","max_length":-1,"printable":true});
        i["choices"] = json!([]);
        i["choice_total"] = json!(0);
    });
    menu["params"]["events"][0]["sequence"] = (revision + 10).to_string().into();
    menu["params"]["events"][0]["revision"] = revision.to_string().into();
    menu
}

fn filled(script: &ScriptHandle) -> Vec<String> {
    sent(script, "bn.command.submit")
        .iter()
        .map(|m| m["params"]["operation"]["value"].as_str().unwrap().into())
        .collect()
}

#[test]
fn a_text_prompt_keeps_what_is_typed_while_the_engine_echoes_an_older_value() {
    let (mut h, script) = harness(vec![]);
    open(&mut h, &script, prompt("", 33));
    h.get_by_label("Filter").focus();
    h.run_steps(2);
    h.get_by_label("Filter").type_text("m");
    h.run_steps(2);
    h.get_by_label("Filter").type_text("od");
    h.run_steps(2);
    // The first fill is done and the engine shows the text it had then, while "mod" waits to go out.
    script.push(prompt("m", 34));
    finish(&script, 4, "c:5");
    h.run_steps(3);
    h.get_by_label("Filter").type_text("e");
    h.run_steps(2);
    script.push(prompt("mod", 35));
    finish(&script, 5, "c:6");
    h.run_steps(3);
    finish(&script, 6, "c:7");
    script.push(prompt("mode", 36));
    h.run_steps(3);
    assert_eq!(filled(&script), ["m", "mod", "mode"]);
}

#[test]
fn a_composition_is_committed_after_the_engine_echoes_an_older_value() {
    let (mut h, script) = harness(vec![]);
    open(&mut h, &script, prompt("", 33));
    h.get_by_label("Filter").focus();
    h.run_steps(2);
    h.get_by_label("Filter").type_text("a");
    h.run_steps(2);
    h.get_by_label("Filter").type_text("b");
    h.event(egui::Event::Ime(egui::ImeEvent::Preedit {
        text: "\u{314e}".into(),
        active_range_chars: None,
    }));
    h.run_steps(2);
    script.push(prompt("a", 34));
    finish(&script, 4, "c:5");
    h.run_steps(3);
    h.event(egui::Event::Ime(egui::ImeEvent::Preedit {
        text: "\u{d55c}".into(),
        active_range_chars: None,
    }));
    h.run_steps(2);
    h.event(egui::Event::Ime(egui::ImeEvent::Commit("\u{d55c}".into())));
    h.run_steps(3);
    finish(&script, 5, "c:6");
    h.run_steps(3);
    assert_eq!(filled(&script), ["a", "ab", "ab\u{d55c}"]);
}

#[test]
fn a_long_described_list_keeps_its_load_more_row_and_description_inside_the_window() {
    let (mut h, script) = harness(vec![]);
    let menu = menu_event(|i| {
        let rows = (0..60)
            .map(|n| {
                json!({"id":format!("c:{n}"),"label":format!("row {n}"),"enabled":true,
                    "description":"Volume: 0.025 L\nThis is a row.","denial":"","selectable":true,
                    "selected":n == 0,"columns":[]})
            })
            .collect::<Vec<_>>();
        i["choices"] = json!(rows);
        i["choice_total"] = json!(100);
    });
    open(&mut h, &script, menu);
    let more = h.get_by_label("Load more (60 of 100)").rect();
    // The description under the button has the room too: its 96 px height must fit under it.
    assert!(more.max.y + 96.0 <= 800.0, "{more:?}");
}
