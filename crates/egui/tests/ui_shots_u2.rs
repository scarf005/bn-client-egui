//! Writes PNGs of the list filter, refused rows and sound lines to `$SHOT_DIR` for a person to look at.
//! Needs a Vulkan adapter, so it only runs when asked: `cargo test --test ui_shots_u2 -- --ignored`.
mod common;
use bn_client_egui::transport::ScriptHandle;
use bn_client_egui::{App, EngineModel};
use common::{fixture, session};
use eframe::egui;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use serde_json::{Value, json};
use std::time::Duration;

type H<'a> = Harness<'a, App<EngineModel>>;

fn harness<'a>() -> (H<'a>, ScriptHandle) {
    let (t, h) = session();
    let clock = bn_client_core::Clock::manual();
    let model = EngineModel::new(Box::new(t)).with_clock(clock.clone());
    let harness = Harness::builder()
        .with_size(egui::vec2(900.0, 560.0))
        .wgpu()
        .build_ui_state(
            |ui, app: &mut App<EngineModel>| app.show(ui),
            App::new(model).with_clock(clock),
        );
    (harness, h)
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

fn row(id: &str, label: &str, denial: &str) -> Value {
    json!({"id":id,"label":label,"enabled":denial.is_empty(),"description":"","denial":denial,
        "selectable":denial.is_empty(),"selected":false,"columns":[]})
}

fn list(rows: Vec<Value>) -> Value {
    let total = rows.len();
    json!({"schema_id":"s:list","kind":"choices","context":"UILIST","title":"Debug",
        "message":"","allow_cancel":true,"choices":rows,"choice_total":total,
        "allow_set_count":false})
}

fn save(h: &mut H, name: &str) {
    let dir = std::path::PathBuf::from(std::env::var("SHOT_DIR").expect("SHOT_DIR"));
    std::fs::create_dir_all(&dir).unwrap();
    h.render().expect("renders").save(dir.join(name)).unwrap();
}

#[test]
#[ignore = "needs a Vulkan adapter and SHOT_DIR"]
fn shots() {
    let filter = json!([{"id":"FILTER","name":"Filter","keys":["/"]}]);
    let (mut h, script) = harness();
    h.run_steps(3);
    script.push(show(
        41,
        filter,
        list(vec![
            row("e:0", "Spawning…", ""),
            row("e:1", "Player…", ""),
            row("e:2", "Vehicle…", "Needs a vehicle nearby"),
            row("e:3", "Teleport…", ""),
            row("e:4", "Map…", ""),
            row("e:5", "Lua console", "Needs a vehicle nearby"),
            row("e:6", "Info…", ""),
            row("e:7", "Quit to main menu", ""),
        ]),
    ));
    h.run_steps(5);
    save(&mut h, "menu-refused-and-filter.png");
    h.get_by_label("Lua console").hover();
    h.run_steps(8);
    save(&mut h, "menu-refused-hover.png");

    let (mut h, script) = harness();
    h.run_steps(3);
    script.push(fixture("events_41_walk"));
    for (n, text) in ["gargling grunting", "gargling grunting", "a door slamming"]
        .into_iter()
        .enumerate()
    {
        script.push(
            json!({"jsonrpc": "2.0", "method": "bn.events", "params": {"epoch": "e7f3", "events": [{
            "sequence": (42 + n).to_string(), "revision": "32", "type": "sound.heard",
            "data": {"text": text, "volume": 52, "logged": false}}]}}),
        );
    }
    for (secs, name) in [
        (0, "sounds-0s.png"),
        (6, "sounds-6s.png"),
        (5, "sounds-11s.png"),
    ] {
        h.state().clock().advance(Duration::from_secs(secs));
        h.run_steps(4);
        save(&mut h, name);
    }
}
