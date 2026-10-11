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
fn a_character_creation_list_fetches_its_missing_pages_without_a_click() {
    let reply = json!({"jsonrpc":"2.0","id":4,"result":{"boundary_id":"b:13","total":3,"choices":[
        {"id":"profession:last","label":"Used Car Salesman","enabled":true,"description":"","denial":"",
         "selectable":true,"selected":false,"columns":[]}]}});
    let (mut h, script) = harness(vec![reply]);
    let menu = menu_event(|i| {
        i["choice_total"] = json!(3);
        i["context"] = json!("NEW_CHAR_PROFESSION");
    });
    open(&mut h, &script, menu);
    h.run_steps(4);
    let asked = sent(&script, "bn.interaction.choices");
    assert_eq!(asked.len(), 1, "the missing page is asked for once");
    assert_eq!(asked[0]["params"]["offset"], 2);
    h.get_by_label("Used Car Salesman");
    assert_eq!(h.state().model.menu().unwrap().choices.len(), 3);
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
