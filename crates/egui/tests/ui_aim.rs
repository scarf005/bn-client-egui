mod common;
use bn_client_egui::transport::ScriptHandle;
use bn_client_egui::{App, Clock, EngineModel};
use common::{fixture, session};
use eframe::egui;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use serde_json::{Value, json};

type H<'a> = Harness<'a, App<EngineModel>>;

fn aim() -> Value {
    let text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../core/tests/fixtures/aim.json"
    ))
    .unwrap();
    serde_json::from_str::<Value>(&text).unwrap()["aim"].clone()
}

fn event(sequence: u32, boundary: Value) -> Value {
    json!({"jsonrpc":"2.0","method":"bn.events","params":{"epoch":"e7f3","events":[{
        "sequence": sequence.to_string(), "revision": (sequence - 9).to_string(),
        "type": "interaction.changed", "changes": {"interaction": boundary}}]}})
}

fn harness<'a>() -> (H<'a>, ScriptHandle) {
    let (t, script) = session();
    let t = (0..8).fold(t, |t, _| {
        t.on("bn.command.submit", vec![fixture("submit_result")])
    });
    let clock = Clock::manual();
    let model = EngineModel::new(Box::new(t)).with_clock(clock.clone());
    let mut h = Harness::builder()
        .with_size(egui::vec2(1300.0, 800.0))
        .build_ui_state(
            |ui, app: &mut App<EngineModel>| app.show(ui),
            App::new(model).with_clock(clock),
        );
    h.run_steps(3);
    script.push(event(41, aim()));
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

/// The engine finished the command in flight, so the next queued step may go.
fn finished(h: &mut H, script: &ScriptHandle) {
    script.push(json!({"jsonrpc":"2.0","method":"bn.command","params":{
        "epoch":"e7f3","command_id":"c:5","stage":"completed"}}));
    h.run_steps(4);
}

fn action(id: &str) -> Value {
    json!({"kind":"action","action_id":id})
}

#[test]
fn both_panes_show_their_area_and_their_items_with_count_weight_and_volume() {
    let (h, _) = harness();
    h.get_by_label("Left pane: Directly below you");
    h.get_by_label("Right pane: Worn Items");
    h.get_by_label("long-sleeved shirt");
    h.get_by_label("tin can");
    h.get_by_label("trench coat");
    for value in ["0.15 kg", "0.75 L", "1.00 kg", "3.00 L"] {
        h.get_all_by_label(value).next().expect(value);
    }
    h.get_by_label("3");
}

#[test]
fn an_area_button_points_the_pane_at_that_area_through_the_native_actions() {
    let (mut h, script) = harness();
    h.get_by_label("Right pane area North").click();
    h.run_steps(3);
    assert_eq!(operations(&script), [action("RIGHT")]);
    finished(&mut h, &script);
    assert_eq!(operations(&script), [action("RIGHT"), action("ITEMS_N")]);
}

#[test]
fn clicking_a_row_selects_it_and_enter_moves_it() {
    let (mut h, script) = harness();
    h.get_by_label("tin can").click();
    h.run_steps(3);
    assert_eq!(
        operations(&script),
        [json!({"kind":"choose","choice_id":"aim:cans"})]
    );
    finished(&mut h, &script);
    h.key_press(egui::Key::Enter);
    h.run_steps(3);
    assert_eq!(
        operations(&script).last(),
        Some(&action("MOVE_SINGLE_ITEM"))
    );
}

#[test]
fn the_move_button_moves_the_engines_selected_row() {
    let (mut h, script) = harness();
    h.get_by_label("Move").click();
    h.run_steps(3);
    assert_eq!(operations(&script), [action("MOVE_SINGLE_ITEM")]);
}

#[test]
fn dragging_a_row_onto_the_other_pane_selects_then_moves_it() {
    let (mut h, script) = harness();
    let from = h.get_by_label("tin can").rect().center();
    let to = h.get_by_label("Right pane: Worn Items").rect().center();
    h.hover_at(from);
    h.run_steps(2);
    h.drag_at(from);
    h.run_steps(2);
    for t in [0.25, 0.5, 0.75, 1.0] {
        h.hover_at(from.lerp(to, t));
        h.run_steps(2);
    }
    h.drop_at(to);
    h.run_steps(3);
    assert_eq!(
        operations(&script),
        [json!({"kind":"choose","choice_id":"aim:cans"})]
    );
    finished(&mut h, &script);
    assert_eq!(
        operations(&script).last(),
        Some(&action("MOVE_SINGLE_ITEM"))
    );
}

#[test]
fn a_count_moves_that_many_through_the_native_amount_prompt() {
    let (mut h, script) = harness();
    h.get_by_label("tin can").click();
    h.run_steps(2);
    finished(&mut h, &script);
    h.get_by_label("Amount to move").focus();
    h.run_steps(2);
    h.get_by_label("Amount to move").type_text("2");
    h.run_steps(2);
    h.get_by_label("Move amount").click();
    h.run_steps(3);
    assert_eq!(
        operations(&script).last(),
        Some(&action("MOVE_VARIABLE_ITEM"))
    );
    // The engine asks how many, as the native prompt does, and then finishes the command.
    let mut prompt = aim();
    prompt["interaction"]["context"] = json!("STRING_INPUT");
    prompt["interaction"]["kind"] = json!("field");
    prompt["interaction"]["choices"] = json!([]);
    prompt["interaction"]["choice_total"] = json!(0);
    prompt["interaction"]["compat"] = json!({"panes": []});
    prompt["interaction"]["field"] = json!({"id":"field:value","label":"How many?","description":"",
        "value":"3","type":"text","max_length":10,"printable":false});
    script.push(event(42, prompt));
    h.run_steps(2);
    finished(&mut h, &script);
    assert_eq!(
        operations(&script).last(),
        Some(&json!({"kind":"fill","field_id":"field:value","value":"2","submit":true}))
    );
}

#[test]
fn a_pane_filter_is_typed_in_its_own_box_and_sent_through_the_native_filter() {
    let (mut h, script) = harness();
    h.get_by_label("Left pane filter").focus();
    h.run_steps(2);
    h.get_by_label("Left pane filter").type_text("can");
    h.run_steps(2);
    h.key_press(egui::Key::Enter);
    h.run_steps(3);
    assert_eq!(operations(&script), [action("LEFT")]);
    finished(&mut h, &script);
    assert_eq!(operations(&script).last(), Some(&action("FILTER")));
}

#[test]
fn colour_tags_in_an_item_name_are_drawn_as_colour_and_not_as_text() {
    let (t, script) = session();
    let t = t.on("bn.command.submit", vec![fixture("submit_result")]);
    let clock = Clock::manual();
    let model = EngineModel::new(Box::new(t)).with_clock(clock.clone());
    let mut h = Harness::builder()
        .with_size(egui::vec2(1300.0, 800.0))
        .build_ui_state(
            |ui, app: &mut App<EngineModel>| app.show(ui),
            App::new(model).with_clock(clock),
        );
    h.run_steps(3);
    let mut boundary = aim();
    boundary["interaction"]["choices"][0]["label"] =
        json!("<color_c_light_green>||</color> long-sleeved shirt");
    script.push(event(41, boundary));
    h.run_steps(4);
    h.get_by_label("|| long-sleeved shirt");
    assert!(h.query_by_label_contains("<color").is_none());
}

fn area(h: &H, pane: &str, name: &str) -> egui::Rect {
    h.get_by_label(&format!("{pane} area {name}")).rect()
}

#[test]
fn the_areas_sit_in_the_native_compass_grid_with_the_marker_columns_beside_it() {
    let (h, _) = harness();
    let at = |n: &str| area(&h, "Left pane", n);
    let (here, north, south) = (at("Directly below you"), at("North"), at("South"));
    let (west, east) = (at("West"), at("East"));
    assert!(north.center().y < here.center().y && here.center().y < south.center().y);
    assert!((north.center().x - here.center().x).abs() < 1.0);
    assert!((south.center().x - here.center().x).abs() < 1.0);
    assert!(west.center().x < here.center().x && here.center().x < east.center().x);
    assert!((west.center().y - here.center().y).abs() < 1.0);
    // The corners fill the rows, and Above and Below close the right edge of the first two rows.
    assert!(at("North West").center().x < north.center().x);
    assert!(at("North East").center().x > north.center().x);
    assert!(at("Above").center().x > at("North East").center().x);
    assert!(at("Below").center().x > east.center().x);
    assert!((at("Above").center().y - north.center().y).abs() < 1.0);
    assert!((at("Below").center().y - here.center().y).abs() < 1.0);
    // Container and Grabbed Vehicle above Surrounding area and Worn Items, Inventory between them.
    let (container, grabbed) = (at("Container"), at("Grabbed Vehicle"));
    let (around, worn, inventory) = (at("Surrounding area"), at("Worn Items"), at("Inventory"));
    assert!(container.center().y < around.center().y && grabbed.center().y < worn.center().y);
    assert!(container.center().x < grabbed.center().x);
    assert!((container.center().x - around.center().x).abs() < 1.0);
    assert!((grabbed.center().x - inventory.center().x).abs() < 1.0);
    assert!(grabbed.center().x < west.center().x);
    assert!((inventory.center().y - here.center().y).abs() < 1.0);
}

#[test]
fn an_area_the_engine_does_not_offer_stays_in_its_place_dimmed() {
    let (t, script) = session();
    let clock = Clock::manual();
    let model = EngineModel::new(Box::new(t)).with_clock(clock.clone());
    let mut h = Harness::builder()
        .with_size(egui::vec2(1300.0, 800.0))
        .build_ui_state(
            |ui, app: &mut App<EngineModel>| app.show(ui),
            App::new(model).with_clock(clock),
        );
    h.run_steps(3);
    let mut boundary = aim();
    boundary["actions"]
        .as_array_mut()
        .unwrap()
        .retain(|a| a["id"] != "ITEMS_NW");
    script.push(event(41, boundary));
    h.run_steps(4);
    let node = h.get_by_label("Left pane area North West");
    assert!(egui_kittest::kittest::NodeT::accesskit_node(&node).is_disabled());
    assert!(
        area(&h, "Left pane", "North West").center().x < area(&h, "Left pane", "North").center().x
    );
}
