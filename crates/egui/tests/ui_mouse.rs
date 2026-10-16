#![recursion_limit = "512"]
//! Every native mouse behavior of the game, checked in the egui client against the wire it uses.
mod common;
use bn_client_egui::app::CELL;
use bn_client_egui::transport::ScriptHandle;
use bn_client_egui::{App, Clock, EngineModel, GameModel};
use common::{fixture, session};
use eframe::egui;
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
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
            vec![json!({"jsonrpc":"2.0","id":0,"result":{"parts":[{"kind":"terrain","text":"Floor"}],"actions":[]}})],
        )
        .on(
            "bn.world.describe",
            vec![json!({"jsonrpc":"2.0","id":0,"result":{"parts":[{"kind":"terrain","text":"Wall"}, {"kind":"creature","text":"A zombie is here.","detail":"Slow and hungry."}],
                "actions":[{"id":"open","label":"Open","keys":["o"]},{"id":"look","label":"Look around","keys":[";"]}]}})],
        )
        .on(
            "bn.world.describe",
            vec![json!({"jsonrpc":"2.0","id":0,"result":{"parts":[{"kind":"terrain","text":"Door"}],
                "actions":[{"id":"open","label":"Open","keys":["o"]},{"id":"look","label":"Look around","keys":[";"]}]}})],
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

/// The view square at `view`, given as if the avatar stood at (20, 12); it is really at the view's centre.
fn square(h: &mut H, view: (f32, f32)) -> egui::Pos2 {
    let me = h.state().model.player();
    let (dx, dy) = (me.x as f32 - 20.0, me.y as f32 - 12.0);
    h.get_by_label("Map").rect().min + egui::vec2((view.0 + dx) * CELL, (view.1 + dy) * CELL)
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
    // What more the engine says of a fact is shown with it.
    h.get_by_label("Slow and hungry.");
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

fn described(script: &ScriptHandle) -> usize {
    (script.sent().iter())
        .filter(|m| m["method"] == "bn.world.describe")
        .count()
}

#[test]
fn arrow_keys_on_the_map_only_walk_and_neither_move_the_cursor_nor_describe_a_square() {
    let (mut h, script) = harness();
    h.run_steps(3);
    h.get_by_label("Map").focus();
    h.run_steps(4);
    let (asked, before) = (described(&script), h.get_by_label("Map").rect());
    for _ in 0..3 {
        h.key_press(egui::Key::ArrowRight);
        h.run_steps(2);
    }
    assert!(!operations(&script).is_empty(), "the arrows walk");
    assert_eq!(described(&script), asked, "an arrow sends no hover");
    h.get_by_label_contains("Cursor (3, 4)");
    assert_eq!(h.get_by_label("Map").rect(), before);
}

#[test]
fn arrow_keys_leave_a_hovered_square_as_the_cursor() {
    let (mut h, script) = harness();
    h.run_steps(3);
    let at = square(&mut h, (22.5, 12.5));
    h.hover_at(at);
    h.run_steps(4);
    let asked = described(&script);
    h.get_by_label("Map").focus();
    h.run_steps(2);
    for _ in 0..2 {
        h.key_press(egui::Key::ArrowDown);
        h.run_steps(2);
    }
    assert_eq!(described(&script), asked, "an arrow sends no hover");
    h.get_by_label_contains("Cursor (5, 4)");
}

/// The square's own actions, as the engine described it, in the menu a right click opens.
#[test]
fn right_click_lists_the_engines_actions_for_the_square_and_choosing_one_sends_it() {
    let (mut h, script) = harness();
    h.run_steps(3);
    let at = square(&mut h, (22.5, 12.5));
    click(&mut h, at, egui::PointerButton::Secondary);
    h.run_steps(3);
    h.get_by_label("Look around [;]");
    h.get_by_label("Open [o]").click();
    h.run_steps(3);
    assert_eq!(
        operations(&script),
        [json!({"kind":"tile_action","action_id":"open","pos":{"dim":"","x":5,"y":4,"z":0}})]
    );
    assert!(h.query_by_label("Open [o]").is_none(), "the menu closes");
}

#[test]
fn the_menu_acts_on_the_square_the_press_landed_on_not_where_the_pointer_was_last_seen() {
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
    h.run_steps(4);
    h.get_by_label("Look around [;]").click();
    h.run_steps(3);
    assert_eq!(
        operations(&script),
        [json!({"kind":"tile_action","action_id":"look","pos":{"dim":"","x":6,"y":4,"z":0}})]
    );
}

#[test]
fn the_menu_acts_on_the_square_it_opened_on_after_the_view_scrolled() {
    let (mut h, script) = harness();
    h.run_steps(3);
    let at = square(&mut h, (22.5, 12.5));
    click(&mut h, at, egui::PointerButton::Secondary);
    h.run_steps(3);
    h.get_by_label("Open [o]");
    let origin = h.state().model.origin();
    // The avatar walks while the menu is open: the view follows it.
    script.push(fixture("events_41_walk"));
    h.run_steps(4);
    assert_ne!(h.state().model.origin(), origin, "the view scrolled");
    h.get_by_label("Open [o]").click();
    h.run_steps(3);
    assert_eq!(
        operations(&script),
        [json!({"kind":"tile_action","action_id":"open","pos":{"dim":"","x":5,"y":4,"z":0}})]
    );
}

#[test]
fn the_wheel_in_a_menu_the_engine_can_scroll_scrolls_the_clients_list_and_sends_no_scroll_action() {
    let (mut h, script) = harness();
    let actions =
        json!([{"id":"SCROLL_UP","name":"Scroll up"},{"id":"SCROLL_DOWN","name":"Scroll down"}]);
    let rows = (0..30)
        .map(|i| row(&format!("c:{i}"), &format!("entry {i}"), i == 0, None))
        .collect::<Vec<_>>();
    open_menu(&mut h, &script, json!(rows), actions, None);
    let before = h.get_by_label("entry 10").rect();
    h.hover_at(egui::pos2(400.0, 300.0));
    for _ in 0..3 {
        h.event(egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Line,
            delta: egui::vec2(0.0, -1.0),
            modifiers: egui::Modifiers::NONE,
            phase: egui::TouchPhase::Move,
        });
        h.run_steps(3);
    }
    assert!(operations(&script).is_empty());
    assert!(h.get_by_label("entry 10").rect().min.y < before.min.y - 20.0);
}

#[test]
fn the_wheel_in_a_scrollable_menu_never_blinks_the_cancel_button() {
    let (mut h, script) = harness();
    let actions =
        json!([{"id":"SCROLL_UP","name":"Scroll up"},{"id":"SCROLL_DOWN","name":"Scroll down"}]);
    let rows = (0..30)
        .map(|i| row(&format!("c:{i}"), &format!("entry {i}"), i == 0, None))
        .collect::<Vec<_>>();
    open_menu(&mut h, &script, json!(rows), actions, None);
    h.hover_at(egui::pos2(400.0, 300.0));
    for _ in 0..6 {
        h.event(egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Line,
            delta: egui::vec2(0.0, -1.0),
            modifiers: egui::Modifiers::NONE,
            phase: egui::TouchPhase::Move,
        });
        h.run_steps(1);
        assert!(
            !h.get_by_label("Cancel").accesskit_node().is_disabled(),
            "Cancel went dim while a wheel step was in flight"
        );
    }
}

#[test]
fn the_wheel_to_the_end_of_a_list_the_engine_has_more_of_fetches_the_next_page() {
    let (mut h, script) = harness();
    h.run_steps(3);
    for name in ["events_41_walk", "events_42_message"] {
        script.push(fixture(name));
    }
    let mut menu = fixture("events_43_menu");
    let change = &mut menu["params"]["events"][0]["changes"]["interaction"]["interaction"];
    change["choices"] = (0..30)
        .map(|i| row(&format!("c:{i}"), &format!("entry {i}"), false, None))
        .collect();
    change["choice_total"] = json!(90);
    script.push(menu);
    h.run_steps(3);
    let pages = |script: &ScriptHandle| {
        script
            .sent_methods()
            .iter()
            .filter(|m| *m == "bn.interaction.choices")
            .count()
    };
    assert_eq!(
        pages(&script),
        0,
        "the first page is all it asks for at first"
    );
    h.hover_at(egui::pos2(300.0, 200.0));
    for _ in 0..10 {
        h.event(egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Line,
            delta: egui::vec2(0.0, -10.0),
            modifiers: egui::Modifiers::NONE,
            phase: egui::TouchPhase::Move,
        });
        h.run_steps(3);
    }
    assert_eq!(pages(&script), 1);
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
    // Hovering another square sends the engine's own set_target, as MOUSE_MOVE does natively. The avatar
    // walked to x 4 and the view followed, so two squares east of the centre is x 6.
    let away = square(&mut h, (22.5, 12.5));
    h.hover_at(away);
    h.run_steps(3);
    assert_eq!(
        operations(&script),
        [json!({"kind":"set_target","pos":{"dim":"","x":6,"y":4,"z":0}})]
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
fn hovering_the_engines_cursor_square_does_not_confirm_it() {
    let (mut h, script) = harness();
    let target = json!({"current":{"dim":"","x":3,"y":4,"z":0}});
    open_menu(
        &mut h,
        &script,
        json!([]),
        json!([{"id":"CONFIRM","name":"Confirm"}]),
        Some(target),
    );
    let here = {
        let origin = h.get_by_label("Map").rect().min;
        let view = h.state().model.focus().expect("the engine cursor");
        origin + egui::vec2(view.x as f32 + 0.5, view.y as f32 + 0.5) * CELL
    };
    h.hover_at(here);
    h.run_steps(3);
    assert_eq!(operations(&script), Vec::<Value>::new());
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
fn the_map_does_not_move_with_the_length_of_the_hovered_description() {
    let long = "very dark interlocking wooden tiles that are more than likely treated against fire, with wooden posts and beams supporting a roof. ".repeat(4);
    let (t, _script) = session();
    let t = t
        .on("bn.command.submit", vec![fixture("submit_result")])
        .on(
            "bn.world.describe",
            vec![json!({"jsonrpc":"2.0","id":0,"result":{"parts":[{"kind":"terrain","text":long}, {"kind":"terrain","text":"Move cost: 150"}],"actions":[]}})],
        )
        .on(
            "bn.world.describe",
            vec![json!({"jsonrpc":"2.0","id":0,"result":{"parts":[{"kind":"terrain","text":"floor"}],"actions":[]}})],
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

#[test]
fn a_square_under_an_open_screen_has_no_actions_and_asks_nothing() {
    let (mut h, script) = harness();
    open_menu(
        &mut h,
        &script,
        json!([row("c:0", "entry", false, None)]),
        json!([]),
        None,
    );
    let asked = |script: &ScriptHandle| {
        script
            .sent()
            .into_iter()
            .filter(|m| m["method"] == "bn.world.describe")
            .count()
    };
    let before = asked(&script);
    let me = h.state().model.player();
    assert_eq!(h.state_mut().model.tile_actions(me), Some(vec![]));
    h.run_steps(3);
    assert_eq!(asked(&script), before);
}

#[test]
fn keys_go_to_the_square_menu_and_not_to_the_game_under_it() {
    let (mut h, script) = harness();
    h.run_steps(3);
    let at = square(&mut h, (22.5, 12.5));
    click(&mut h, at, egui::PointerButton::Secondary);
    h.run_steps(3);
    h.get_by_label("Open [o]");
    h.key_press(egui::Key::ArrowDown);
    h.run_steps(3);
    h.key_press(egui::Key::Escape);
    h.run_steps(3);
    assert!(h.query_by_label("Open [o]").is_none(), "the menu closes");
    assert_eq!(operations(&script), Vec::<Value>::new());
}
