//! The character sheet and the inventory in the client's own windows: the engine's keys for them open the windows
//! while the game keeps taking keys, the windows read the engine's panels and send nothing from their rows, and
//! each opens the engine's own screen.
mod common;
use bn_client_egui::transport::ScriptHandle;
use bn_client_egui::{App, Clock, EngineModel};
use common::{fixture, panel_view};
use eframe::egui;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use serde_json::{Value, json};

type H<'a> = Harness<'a, App<EngineModel>>;

/// A game whose actions include the character sheet (`@`), the inventory (`i`) and the map (`m`).
fn game() -> (App<EngineModel>, ScriptHandle) {
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
    (App::new(model).with_clock(clock), script)
}

fn open<'a>() -> (H<'a>, ScriptHandle) {
    let (app, script) = game();
    let mut h = Harness::builder()
        .with_size(egui::vec2(1600.0, 1000.0))
        .build_ui_state(|ui, app: &mut App<EngineModel>| app.show(ui), app);
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
    serve_with(h, script, answered, &panel_view);
}

/// `serve`, with `view` giving the screen of each panel.
fn serve_with(
    h: &mut H,
    script: &ScriptHandle,
    answered: &mut usize,
    view: &dyn Fn(&str) -> Value,
) {
    for _ in 0..4 {
        let reads = panel_reads(script);
        for read in &reads[*answered..] {
            let panel = read["params"]["panel"].as_str().unwrap();
            script.push(json!({"jsonrpc": "2.0", "id": read["id"], "result": view(panel)}));
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

/// The window titled `title` is shown.
fn window(h: &H, title: &str) {
    h.get_by_role_and_label(egui::accesskit::Role::Window, title);
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
    window(&h, "John Smith | Male");
    key(&mut h, "i");
    serve(&mut h, &script, &mut answered);
    window(&h, "Inventory");
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
    window(&h, "John Smith | Male");
    window(&h, "Inventory");
    h.get_by_label("Travel and notes");
    key(&mut h, "@");
    serve(&mut h, &script, &mut answered);
    assert!(
        h.query_by_role_and_label(egui::accesskit::Role::Window, "John Smith | Male")
            .is_none(),
        "the key closes the window again"
    );
    window(&h, "Inventory");
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
    h.get_by_role_and_label(egui::accesskit::Role::Tab, "SKILLS")
        .click();
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

/// Sixty items in the inventory's `list:worn`, labelled `{tag} row {n}`, with the focus on `item:{focus}`.
fn crowded(mut screen: Value, tag: &str, focus: usize) -> Value {
    let rows = (0..60)
        .map(|n| {
            json!({"id": format!("item:{n}"), "label": format!("{tag} row {n}"), "description": "",
                "denial": "", "enabled": true, "selectable": true, "selected": false, "columns": []})
        })
        .collect::<Vec<_>>();
    let list = &mut screen["root"]["child"]["children"][0]["child"];
    list["rows"] = json!(rows);
    list["total"] = json!(60);
    screen["focus"] = json!({"component_id": "list:worn", "row_id": format!("item:{focus}")});
    screen
}

/// The game moves on to offer `actions` under the screens of `stack`.
fn interaction_changed(h: &mut H, script: &ScriptHandle, actions: Value, stack: Value) {
    script.push(fixture("events_41_walk"));
    h.run_steps(4);
    script.push(json!({"jsonrpc": "2.0", "method": "bn.events", "params": {"epoch": "e7f3", "events": [{
        "sequence": "42", "revision": "33", "type": "interaction.changed", "changes": {"interaction": {
        "boundary_id": "b:60",
        "readiness": {"phase": "playing", "game_ready": true, "accepts_interaction_commands": true,
            "accepts_registered_actions": true},
        "actions": actions, "interaction": null, "stack": stack}}}]}}));
    h.run_steps(6);
}

/// The inventory window open on sixty items with its focus on the sixth, under the engine's drop screen on the
/// same items with its cursor on the fifty-first, which offers no action of the game.
fn inventory_under_the_drop_screen<'a>() -> (H<'a>, ScriptHandle) {
    let (mut h, script) = open();
    let mut answered = 0;
    let view = |panel: &str| crowded(panel_view(panel), "panel", 5);
    key(&mut h, "i");
    serve_with(&mut h, &script, &mut answered, &view);
    let path = format!(
        "{}/../core/tests/fixtures/0.2.examples/inventory.json",
        env!("CARGO_MANIFEST_DIR")
    );
    let all = serde_json::from_str::<Value>(&std::fs::read_to_string(path).unwrap()).unwrap();
    let drop = all["valid"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["name"] == "drop selector screen")
        .unwrap()["value"]
        .clone();
    let drop = crowded(drop, "screen", 50);
    let map = json!({"screen_id": "screen:map", "role": "map", "root": null, "affordances": []});
    interaction_changed(&mut h, &script, json!([]), json!([map, drop]));
    serve_with(&mut h, &script, &mut answered, &view);
    (h, script)
}

fn wheel(h: &mut H, at: egui::Pos2, dy: f32) {
    h.hover_at(at);
    h.run_steps(2);
    h.event(egui::Event::MouseWheel {
        unit: egui::MouseWheelUnit::Point,
        delta: egui::vec2(0.0, dy),
        phase: egui::TouchPhase::Move,
        modifiers: egui::Modifiers::NONE,
    });
    h.run_steps(6);
}

/// Clicks the title bar of the window titled `title`, which brings it in front of the windows it overlaps.
fn to_front(h: &mut H, title: &str) {
    let window = h
        .get_by_role_and_label(egui::accesskit::Role::Window, title)
        .rect();
    let title = egui::pos2(window.center().x as f32, window.min.y as f32 + 4.0);
    h.hover_at(title);
    h.run_steps(2);
    for pressed in [true, false] {
        h.event(egui::Event::PointerButton {
            pos: title,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        });
    }
    h.run_steps(2);
}

fn shown(h: &H, label: &str) -> bool {
    h.query_by_label(label).is_some()
}

#[test]
fn the_inventory_window_and_the_engines_inventory_screen_each_keep_their_own_scroll() {
    let (mut h, _script) = inventory_under_the_drop_screen();
    assert!(shown(&h, "panel row 5") && shown(&h, "screen row 50"));
    to_front(&mut h, "Inventory");
    let at = h.get_by_label("panel row 5").rect().center();
    wheel(&mut h, egui::pos2(at.x as f32, at.y as f32), -4000.0);
    assert!(
        !shown(&h, "panel row 5"),
        "the window scrolled off its focus"
    );
    assert!(shown(&h, "screen row 50"), "the screen kept its scroll");
    let at = h.get_by_label("screen row 50").rect().center();
    wheel(&mut h, egui::pos2(200.0, at.y as f32), 4000.0);
    assert!(
        !shown(&h, "screen row 50"),
        "the screen scrolled off its cursor"
    );
    assert!(!shown(&h, "panel row 5"), "the window kept its scroll");
}

#[test]
fn a_double_click_on_an_inventory_row_opens_nothing_while_the_game_offers_no_inventory() {
    let (mut h, script) = open();
    let mut answered = 0;
    key(&mut h, "i");
    serve(&mut h, &script, &mut answered);
    // An action bound to no key is not offered off the map's affordances.
    let inventory = json!({"id": "inventory", "name": "Open Inventory", "keys": []});
    let map = json!({"screen_id": "screen:map", "role": "map", "root": null, "affordances": []});
    interaction_changed(&mut h, &script, json!([inventory]), json!([map]));
    serve(&mut h, &script, &mut answered);
    assert!(h.query_by_label("Open Inventory").is_none());
    let at = h.get_by_label("rock (1)").rect().center();
    common::double_click(&mut h, egui::pos2(at.x as f32, at.y as f32));
    h.run_steps(2);
    assert_eq!(operations(&script), Vec::<Value>::new());
}

#[test]
fn a_double_click_on_an_inventory_row_opens_nothing_under_another_screen() {
    let (mut h, script) = inventory_under_the_drop_screen();
    to_front(&mut h, "Inventory");
    let at = h.get_by_label("panel row 5").rect().center();
    common::double_click(&mut h, egui::pos2(at.x as f32, at.y as f32));
    h.run_steps(2);
    assert_eq!(operations(&script), Vec::<Value>::new());
}

/// Writes a PNG of the sheet, the inventory and the map open together to `$SHOT_DIR`. Needs a Vulkan adapter:
/// `cargo test --test ui_panels -- --ignored`.
#[test]
#[ignore = "needs a Vulkan adapter"]
fn writes_the_sheet_inventory_and_map_windows_open_together() {
    let (app, script) = game();
    let mut h = common::shot_harness(egui::vec2(1600.0, 1000.0), app);
    h.run_steps(3);
    let mut answered = 0;
    for window in ["@", "i", "m"] {
        key(&mut h, window);
        serve(&mut h, &script, &mut answered);
    }
    let image = h.render().expect("renders");
    if let Some(dir) = std::env::var_os("SHOT_DIR") {
        std::fs::create_dir_all(&dir).unwrap();
        image
            .save(std::path::Path::new(&dir).join("panels.png"))
            .unwrap();
    }
}
