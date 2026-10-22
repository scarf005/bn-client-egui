use crate::common::{fixture, session};
use bn_client_egui::transport::ScriptHandle;
use bn_client_egui::{App, EngineModel};
use eframe::egui;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use serde_json::{Value, json};

type H<'a> = Harness<'a, App<EngineModel>>;

fn harness<'a>() -> (H<'a>, ScriptHandle) {
    let (t, h) = session();
    let t = (0..6).fold(t, |t, _| {
        t.on("bn.command.submit", vec![fixture("submit_result")])
    });
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

fn refused(id: &str, label: &str, denial: &str) -> Value {
    let mut c = choice(id, label);
    c["enabled"] = false.into();
    c["denial"] = denial.into();
    c
}

fn menu_of(choices: Vec<Value>) -> Value {
    json!({"schema_id":"s:list","kind":"choices","context":"UILIST","title":"Options",
        "message":"","allow_cancel":true,"choices":choices,"choice_total":0,
        "allow_set_count":false})
}

#[test]
fn hovering_a_refused_row_gives_the_engines_reason_and_clicking_it_sends_nothing() {
    let (mut h, script) = harness();
    h.run_steps(3);
    script.push(show(
        41,
        json!([]),
        menu_of(vec![
            refused("b", "Adjacent", "Needs auto pickup"),
            refused("c", "Weight limit", "Needs auto pickup"),
        ]),
    ));
    h.run_steps(4);
    // The reason is printed once under the run of refused rows.
    assert_eq!(h.get_all_by_label("Needs auto pickup").count(), 1);
    h.get_by_label("Weight limit").hover();
    h.run_steps(6);
    assert_eq!(h.get_all_by_label("Needs auto pickup").count(), 2);
    h.get_by_label("Weight limit").click();
    h.run_steps(3);
    assert!(submits(&script).is_empty());
}
