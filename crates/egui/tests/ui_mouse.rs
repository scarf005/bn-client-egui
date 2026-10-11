#![recursion_limit = "512"]
//! Every native mouse behavior of the game, checked in the egui client against the wire it uses.
mod common;
use bn_client_egui::app::CELL;
use bn_client_egui::transport::ScriptHandle;
use bn_client_egui::{App, Clock, EngineModel, GameModel};
use common::{fixture, session};
use eframe::egui;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use serde_json::{Value, json};

type H<'a> = Harness<'a, App<EngineModel>>;

fn harness<'a>() -> (H<'a>, ScriptHandle) {
    let (t, h) = session();
    let t = t
        .on("bn.command.submit", vec![fixture("submit_result")])
        .on("bn.command.submit", vec![fixture("submit_result")])
        .on("bn.command.submit", vec![fixture("submit_result")])
        .on(
            "bn.world.describe",
            vec![json!({"jsonrpc":"2.0","id":0,"result":{"lines":["Floor"]}})],
        )
        .on(
            "bn.world.describe",
            vec![json!({"jsonrpc":"2.0","id":0,"result":{"lines":["Wall","A zombie is here."]}})],
        );
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

fn operations(h: &ScriptHandle) -> Vec<Value> {
    h.sent()
        .into_iter()
        .filter(|m| m["method"] == "bn.command.submit")
        .map(|m| m["params"]["operation"].clone())
        .collect()
}

fn square(h: &mut H, view: (f32, f32)) -> egui::Pos2 {
    h.get_by_label("Map").rect().min + egui::vec2(view.0 * CELL, view.1 * CELL)
}

fn click(h: &mut H, pos: egui::Pos2, button: egui::PointerButton) {
    h.hover_at(pos);
    h.run_steps(2);
    for pressed in [true, false] {
        h.event(egui::Event::PointerButton {
            pos,
            button,
            pressed,
            modifiers: egui::Modifiers::NONE,
        });
        h.run_steps(1);
    }
    h.run_steps(2);
}

/// The pushed menu: rows and the registered actions the boundary offers.
fn open_menu(
    h: &mut H,
    script: &ScriptHandle,
    choices: Value,
    actions: Value,
    target: Option<Value>,
) {
    h.run_steps(3);
    for name in ["events_41_walk", "events_42_message"] {
        script.push(fixture(name));
    }
    let mut menu = fixture("events_43_menu");
    let change = &mut menu["params"]["events"][0]["changes"]["interaction"];
    change["actions"] = actions;
    change["interaction"]["choices"] = choices;
    if let Some(target) = target {
        change["interaction"]["kind"] = "target".into();
        change["interaction"]["target"] = target;
    }
    script.push(menu);
    h.run_steps(3);
}

fn row(id: &str, label: &str, selected: bool, pane: Option<&str>) -> Value {
    let mut c = json!({"id":id,"label":label,"description":"","denial":"","enabled":true,
        "selectable":true,"selected":selected,"columns":[]});
    if let Some(pane) = pane {
        c["pane_id"] = pane.into();
    }
    c
}

#[test]
fn hovering_a_map_square_asks_the_engine_for_its_native_description_and_shows_it() {
    let (mut h, script) = harness();
    h.run_steps(3);
    let at = square(&mut h, (22.5, 12.5));
    h.hover_at(at);
    h.run_steps(4);
    let asked: Vec<_> = script
        .sent()
        .into_iter()
        .filter(|m| m["method"] == "bn.world.describe")
        .collect();
    // The focused map's keyboard cursor sits on the avatar's square and is described once, then the hover.
    assert_eq!(asked.len(), 2, "one request per described square");
    assert_eq!(
        asked[1]["params"]["pos"],
        json!({"dim":"","x":5,"y":4,"z":0})
    );
    assert_eq!(asked[1]["params"]["boundary_id"], "b:12");
    h.run_steps(3);
    h.get_by_label("A zombie is here.");
    // Staying on the square asks nothing more; moving to another asks again.
    h.run_steps(3);
    assert_eq!(
        script
            .sent()
            .iter()
            .filter(|m| m["method"] == "bn.world.describe")
            .count(),
        2
    );
}

#[test]
fn left_click_travels_to_the_square_even_next_to_the_avatar() {
    let (mut h, script) = harness();
    h.run_steps(3);
    // The avatar is at view (20, 12): an adjacent square plans a route like any other.
    let at = square(&mut h, (21.5, 12.5));
    click(&mut h, at, egui::PointerButton::Primary);
    assert_eq!(
        operations(&script),
        [json!({"kind":"travel","pos":{"dim":"","x":4,"y":4,"z":0}})]
    );
}

#[test]
fn right_click_is_the_engines_context_click_and_opens_no_menu_of_its_own() {
    let (mut h, script) = harness();
    h.run_steps(3);
    let at = square(&mut h, (22.5, 12.5));
    click(&mut h, at, egui::PointerButton::Secondary);
    assert_eq!(
        operations(&script),
        [json!({"kind":"context","pos":{"dim":"","x":5,"y":4,"z":0}})]
    );
    assert!(h.query_by_label("Move here").is_none());
}

#[test]
fn the_wheel_in_a_scrollable_menu_is_one_engine_scroll_step_per_event() {
    let (mut h, script) = harness();
    let actions =
        json!([{"id":"SCROLL_UP","name":"Scroll up"},{"id":"SCROLL_DOWN","name":"Scroll down"}]);
    let rows = (0..30)
        .map(|i| row(&format!("c:{i}"), &format!("entry {i}"), i == 0, None))
        .collect::<Vec<_>>();
    open_menu(&mut h, &script, json!(rows), actions, None);
    h.hover_at(egui::pos2(400.0, 300.0));
    h.event(egui::Event::MouseWheel {
        unit: egui::MouseWheelUnit::Line,
        delta: egui::vec2(0.0, -1.0),
        modifiers: egui::Modifiers::NONE,
        phase: egui::TouchPhase::Move,
    });
    h.run_steps(3);
    assert_eq!(
        operations(&script),
        [json!({"kind":"action","action_id":"SCROLL_DOWN"})]
    );
}

#[test]
fn a_menu_without_engine_scrolling_scrolls_its_own_list_and_sends_nothing() {
    let (mut h, script) = harness();
    let rows = (0..60)
        .map(|i| row(&format!("c:{i}"), &format!("entry {i}"), false, None))
        .collect::<Vec<_>>();
    open_menu(&mut h, &script, json!(rows), json!([]), None);
    h.hover_at(egui::pos2(300.0, 200.0));
    h.event(egui::Event::MouseWheel {
        unit: egui::MouseWheelUnit::Line,
        delta: egui::vec2(0.0, -3.0),
        modifiers: egui::Modifiers::NONE,
        phase: egui::TouchPhase::Move,
    });
    h.run_steps(4);
    assert!(operations(&script).is_empty());
}

#[test]
fn clicking_tabs_rows_and_column_rows_chooses_them_by_id() {
    let (mut h, script) = harness();
    let rows = json!([
        row("tab:a", "General", true, None),
        row("tab:b", "Other", false, None),
        row("column:1", "Rules", false, Some("columns")),
        row("rule:0", "*knife*", true, Some("rules")),
    ]);
    open_menu(&mut h, &script, rows, json!([]), None);
    for (label, id) in [
        ("Other", "tab:b"),
        ("Rules", "column:1"),
        ("*knife*", "rule:0"),
    ] {
        h.get_by_label(label).click();
        h.run_steps(2);
        script.push(
            json!({"jsonrpc":"2.0","method":"bn.command","params":{"epoch":"e7f3",
            "command_id":"c:5","stage":"completed"}}),
        );
        h.run_steps(3);
        assert_eq!(
            operations(&script).last().unwrap(),
            &json!({"kind":"choose","choice_id":id})
        );
    }
}

#[test]
fn while_looking_the_mouse_moves_the_engines_cursor_and_a_second_click_confirms_it() {
    let (mut h, script) = harness();
    let target = json!({"current":{"dim":"","x":3,"y":4,"z":0}});
    open_menu(
        &mut h,
        &script,
        json!([]),
        json!([{"id":"CONFIRM","name":"Confirm"}]),
        Some(target),
    );
    // Hovering another square sends the engine's own set_target, as MOUSE_MOVE does natively.
    let away = square(&mut h, (22.5, 12.5));
    h.hover_at(away);
    h.run_steps(3);
    assert_eq!(
        operations(&script),
        [json!({"kind":"set_target","pos":{"dim":"","x":5,"y":4,"z":0}})]
    );
    // On the cursor's own square a click confirms (SELECT) instead of travelling.
    script.push(
        json!({"jsonrpc":"2.0","method":"bn.command","params":{"epoch":"e7f3",
        "command_id":"c:5","stage":"completed"}}),
    );
    h.run_steps(3);
    let here = {
        let origin = h.get_by_label("Map").rect().min;
        let view = h.state().model.focus().expect("the engine cursor");
        origin + egui::vec2(view.x as f32 + 0.5, view.y as f32 + 0.5) * CELL
    };
    click(&mut h, here, egui::PointerButton::Primary);
    let ops = operations(&script);
    assert_eq!(
        ops.last().unwrap(),
        &json!({"kind":"action","action_id":"CONFIRM"}),
        "{ops:?}"
    );
}

#[test]
fn a_click_goes_to_the_square_it_landed_on_even_when_the_pointer_just_arrived() {
    let (mut h, script) = harness();
    h.run_steps(3);
    // The pointer was last seen two squares east; the press arrives at another square in one frame.
    let seen = square(&mut h, (22.5, 12.5));
    h.hover_at(seen);
    h.run_steps(2);
    let landed = square(&mut h, (23.5, 12.5));
    h.event(egui::Event::PointerMoved(landed));
    for pressed in [true, false] {
        h.event(egui::Event::PointerButton {
            pos: landed,
            button: egui::PointerButton::Secondary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        });
    }
    h.run_steps(3);
    assert_eq!(
        operations(&script),
        [json!({"kind":"context","pos":{"dim":"","x":6,"y":4,"z":0}})]
    );
}

#[test]
fn a_double_click_on_a_row_chooses_it_once_as_native_has_no_double_click() {
    let (mut h, script) = harness();
    let rows = json!([
        row("c:0", "first", true, None),
        row("c:1", "second", false, None),
    ]);
    open_menu(&mut h, &script, rows, json!([]), None);
    let at = h.get_by_label("second").rect().center();
    h.hover_at(at);
    h.run_steps(2);
    // Two presses in quick succession: the second finds a command in flight and is not sent.
    for _ in 0..2 {
        for pressed in [true, false] {
            h.event(egui::Event::PointerButton {
                pos: at,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            });
            h.run_steps(1);
        }
    }
    h.run_steps(2);
    assert_eq!(
        operations(&script),
        [json!({"kind":"choose","choice_id":"c:1"})]
    );
}

#[test]
fn clicking_the_text_of_a_composed_screen_sends_nothing_as_native_has_no_tab_click_there() {
    let (mut h, script) = harness();
    h.run_steps(3);
    for name in ["events_41_walk", "events_42_message"] {
        script.push(fixture(name));
    }
    let mut menu = fixture("events_43_menu");
    let interaction = &mut menu["params"]["events"][0]["changes"]["interaction"]["interaction"];
    interaction["context"] = "NEW_CHAR_POINTS".into();
    interaction["title"] = "Character creation".into();
    interaction["message"] = "POINTS  SCENARIO  PROFESSION".into();
    interaction["choices"] = json!([]);
    script.push(menu);
    h.run_steps(3);
    let at = h.get_by_label_contains("SCENARIO").rect().center();
    h.hover_at(at);
    h.run_steps(2);
    for pressed in [true, false] {
        h.event(egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        });
        h.run_steps(1);
    }
    h.run_steps(2);
    assert!(operations(&script).is_empty());
}

#[test]
fn the_map_does_not_move_with_the_length_of_the_hovered_description() {
    let long = "very dark interlocking wooden tiles that are more than likely treated against fire, with wooden posts and beams supporting a roof. ".repeat(4);
    let (t, _script) = session();
    let t = t
        .on("bn.command.submit", vec![fixture("submit_result")])
        .on(
            "bn.world.describe",
            vec![json!({"jsonrpc":"2.0","id":0,"result":{"lines":[long, "Move cost: 150"]}})],
        )
        .on(
            "bn.world.describe",
            vec![json!({"jsonrpc":"2.0","id":0,"result":{"lines":["floor"]}})],
        );
    let clock = Clock::manual();
    let model = EngineModel::new(Box::new(t)).with_clock(clock.clone());
    let mut h = Harness::builder()
        .with_size(egui::vec2(1100.0, 800.0))
        .build_ui_state(
            |ui, app: &mut App<EngineModel>| app.show(ui),
            App::new(model).with_clock(clock),
        );
    h.run_steps(3);
    let map = h.get_by_label("Map").rect();
    let at = |h: &mut H, x: f32| {
        let p = square(h, (x, 12.5));
        h.hover_at(p);
        h.run_steps(4);
    };
    at(&mut h, 22.5);
    assert_eq!(h.get_by_label("Map").rect(), map, "long text moved the map");
    at(&mut h, 23.5);
    assert_eq!(
        h.get_by_label("Map").rect(),
        map,
        "short text moved the map"
    );
}
