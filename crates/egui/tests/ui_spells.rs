#![recursion_limit = "512"]
mod common;
use bn_client_egui::transport::ScriptHandle;
use bn_client_egui::{App, Clock, EngineModel};
use common::{fixture, session};
use eframe::egui;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use serde_json::{Value, json};

type H<'a> = Harness<'a, App<EngineModel>>;

fn spells() -> Value {
    let text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../core/tests/fixtures/spells.json"
    ))
    .unwrap();
    serde_json::from_str::<Value>(&text).unwrap()["spells"].clone()
}

fn harness<'a>() -> (H<'a>, ScriptHandle) {
    let (t, script) = session();
    let t = t.on("bn.command.submit", vec![fixture("submit_result")]);
    let clock = Clock::manual();
    let model = EngineModel::new(Box::new(t)).with_clock(clock.clone());
    let mut h = Harness::builder()
        .with_size(egui::vec2(1100.0, 800.0))
        .build_ui_state(
            |ui, app: &mut App<EngineModel>| app.show(ui),
            App::new(model).with_clock(clock),
        );
    h.run_steps(3);
    script.push(
        json!({"jsonrpc":"2.0","method":"bn.events","params":{"epoch":"e7f3","events":[{
        "sequence": "41", "revision": "32", "type": "interaction.changed",
        "changes": {"interaction": spells()}}]}}),
    );
    h.run_steps(4);
    (h, script)
}

fn operations(script: &ScriptHandle) -> Vec<Value> {
    script
        .sent()
        .into_iter()
        .filter(|m| m["method"] == "bn.command.submit")
        .map(|m| m["params"]["operation"].clone())
        .collect()
}

#[test]
fn the_list_shows_hotkeys_favorites_and_the_cursor_spells_detail_text() {
    let (h, _) = harness();
    h.get_by_label("a Magic Missile *");
    h.get_by_label("b Stone Skin");
    // The native cursor is on Stone Skin, so its text is shown and Magic Missile's is not.
    h.get_by_label_contains("Hardens the skin.");
    h.get_by_label_contains("Not Enough Energy");
    assert!(h.query_by_label_contains("A missile of force.").is_none());
}

#[test]
fn clicking_a_spell_or_a_key_row_chooses_it_through_the_engine() {
    let (mut h, script) = harness();
    h.get_by_label("a Magic Missile *").click();
    h.run_steps(3);
    assert_eq!(
        operations(&script),
        [json!({"kind":"choose","choice_id":"entry:0"})]
    );
}

#[test]
fn a_key_row_presses_the_menus_own_key() {
    let (mut h, script) = harness();
    h.get_by_label("Toggle favorite").click();
    h.run_steps(3);
    assert_eq!(
        operations(&script),
        [json!({"kind":"choose","choice_id":"action:key:42"})]
    );
}
