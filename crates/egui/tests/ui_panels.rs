//! The character sheet and the inventory in the client's own windows: the engine's keys for them open the windows
//! while the game keeps taking keys, the windows read the engine's panels and send nothing from their rows, and
//! each opens the engine's own screen.
mod common;
use bn_client_egui::transport::ScriptHandle;
use bn_client_egui::{App, Clock, EngineModel};
use common::{fixture, panel_view};
use eframe::egui;
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use serde_json::{Value, json};

type H<'a> = Harness<'a, App<EngineModel>>;

/// A game whose actions include the character sheet (`@`), the inventory (`i`) and the map (`m`).
fn open<'a>() -> (H<'a>, ScriptHandle) {
    let mut subscribed = fixture("subscribe_result");
    subscribed["result"]["interaction"]["actions"]
        .as_array_mut()
        .unwrap()
        .extend([
            json!({"id": "player_data", "name": "View Player Info", "keys": ["@"]}),
            json!({"id": "inventory", "name": "Open Inventory", "keys": ["i"]}),
            json!({"id": "map", "name": "View Map", "keys": ["m"]}),
        ]);
    let reply = vec![
        subscribed,
        fixture("snapshot_part_0"),
        fixture("snapshot_part_1"),
    ];
    let (t, script) = bn_client_egui::transport::ScriptedTransport::new();
    let t = t
        .on("bn.hello", vec![fixture("hello_result")])
        .on("bn.subscribe", reply.clone())
        .on("bn.subscribe", reply)
        .on("bn.command.submit", vec![fixture("submit_result")]);
    let clock = Clock::manual();
    let model = EngineModel::new(Box::new(t)).with_clock(clock.clone());
    let mut h = Harness::builder()
        .with_size(egui::vec2(1600.0, 1000.0))
        .build_ui_state(
            |ui, app: &mut App<EngineModel>| app.show(ui),
            App::new(model).with_clock(clock),
        );
    h.run_steps(3);
    (h, script)
}

fn panel_reads(script: &ScriptHandle) -> Vec<Value> {
    script
        .sent()
        .into_iter()
        .filter(|m| m["method"] == "bn.panel.view")
        .collect()
}

/// Answers every panel read from the `answered`th on with the panel it asked for.
fn serve(h: &mut H, script: &ScriptHandle, answered: &mut usize) {
    for _ in 0..4 {
        let reads = panel_reads(script);
        for read in &reads[*answered..] {
            let panel = read["params"]["panel"].as_str().unwrap();
            script.push(json!({"jsonrpc": "2.0", "id": read["id"], "result": panel_view(panel)}));
        }
        *answered = reads.len();
        h.run_steps(2);
    }
    common::settle_windows(h);
}

fn key(h: &mut H, text: &str) {
    h.event(egui::Event::Text(text.into()));
    h.run_steps(2);
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
fn the_sheet_inventory_and_map_keys_open_their_windows_together_and_the_player_still_moves() {
    let (mut h, script) = open();
    let mut answered = 0;
    key(&mut h, "@");
    serve(&mut h, &script, &mut answered);
    h.get_by_label("John Smith | Male");
    key(&mut h, "i");
    serve(&mut h, &script, &mut answered);
    h.get_by_label("Multidrop");
    key(&mut h, "m");
    serve(&mut h, &script, &mut answered);
    let panels = panel_reads(&script)
        .iter()
        .map(|r| r["params"]["panel"].clone())
        .collect::<Vec<_>>();
    assert!(panels.contains(&json!("character")) && panels.contains(&json!("inventory")));
    assert!(
        operations(&script).is_empty(),
        "the keys run no engine command"
    );
    key(&mut h, "k");
    h.run_steps(2);
    assert_eq!(
        operations(&script),
        [json!({"kind": "action", "action_id": "UP"})],
        "the player moves with the windows open"
    );
    for open in ["John Smith | Male", "Multidrop", "Travel and notes"] {
        h.get_by_label(open);
    }
    key(&mut h, "@");
    serve(&mut h, &script, &mut answered);
    assert!(
        h.query_by_label("John Smith | Male").is_none(),
        "the key closes the window again"
    );
    h.get_by_label("Multidrop");
}

#[test]
fn a_click_on_a_sheet_row_sends_no_command_and_reads_the_sheet_again_with_the_row() {
    let (mut h, script) = open();
    let mut answered = 0;
    key(&mut h, "@");
    serve(&mut h, &script, &mut answered);
    h.get_by_label("Dexterity:").click();
    h.run_steps(2);
    serve(&mut h, &script, &mut answered);
    let last = panel_reads(&script).pop().unwrap();
    assert_eq!(
        last["params"],
        json!({"epoch": "e7f3", "panel": "character", "row": "stat:dex"})
    );
    assert!(operations(&script).is_empty(), "the row runs nothing");
    key(&mut h, "k");
    assert_eq!(
        operations(&script),
        [json!({"kind": "action", "action_id": "UP"})],
        "the click left the keys to the game"
    );
}

#[test]
fn a_click_on_a_sheet_tab_reads_the_sheet_again_with_the_tab() {
    let (mut h, script) = open();
    let mut answered = 0;
    key(&mut h, "@");
    serve(&mut h, &script, &mut answered);
    h.get_by_label("SKILLS").click();
    h.run_steps(2);
    serve(&mut h, &script, &mut answered);
    let last = panel_reads(&script).pop().unwrap();
    assert_eq!(last["params"]["row"], "skills");
    assert!(operations(&script).is_empty(), "the tab runs nothing");
}

#[test]
fn the_sheets_button_opens_the_engines_own_screen() {
    let (mut h, script) = open();
    let mut answered = 0;
    key(&mut h, "@");
    serve(&mut h, &script, &mut answered);
    h.get_by_label("View Player Info").click();
    h.run_steps(2);
    assert_eq!(
        operations(&script),
        [json!({"kind": "action", "action_id": "player_data"})]
    );
}

#[test]
fn a_double_click_on_an_inventory_row_opens_the_engines_own_inventory() {
    let (mut h, script) = open();
    let mut answered = 0;
    key(&mut h, "i");
    serve(&mut h, &script, &mut answered);
    let at = h.get_by_label("rock (1)").rect().center();
    common::double_click(&mut h, egui::pos2(at.x as f32, at.y as f32));
    h.run_steps(2);
    assert_eq!(
        operations(&script),
        [json!({"kind": "action", "action_id": "inventory"})]
    );
}
