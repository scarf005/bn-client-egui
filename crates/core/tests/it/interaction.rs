//! The interaction core: the invoke path, the keys, the local state of the top screen, paging and the
//! override list.

use bn_client_core::interaction::{
    ArgError, ArgKind, Basis, Interaction, InvokeError, OVERRIDES, Pages, Window,
    affordance_for_key, binds_key, check_args, submit, with_role,
};
use bn_client_core::protocol::{
    Affordance, AffordanceRole, ArgShape, At, Boundary, Component, Expect, InvokeArgs, List,
    Operation, Row, RowsResult, Screen,
};
use serde_json::{Value, json};

fn at() -> At {
    At {
        epoch: "epoch:e7f3".into(),
        sequence: "40".into(),
        revision: "31".into(),
    }
}

fn boundary(stack: Value) -> Boundary {
    serde_json::from_value(json!({
        "boundary_id": "boundary:7",
        "readiness": {"phase": "waiting_for_input", "game_ready": true,
            "accepts_interaction_commands": true, "accepts_registered_actions": true},
        "actions": [], "interaction": null, "stack": stack}))
    .unwrap()
}

fn screen(id: &str, affordances: Value) -> Value {
    json!({"screen_id": id, "role": "dialog", "root": null, "affordances": affordances})
}

fn affordance(v: Value) -> Affordance {
    serde_json::from_value(v).unwrap()
}

fn plain(id: &str, expect: &str, keys: &[&str]) -> Value {
    json!({"id": id, "label": id, "keys": keys, "enabled": true, "expect": expect})
}

fn invoke(id: &str, args: Option<InvokeArgs>) -> Operation {
    Operation::Invoke {
        affordance_id: id.into(),
        args,
    }
}

fn command(b: &Boundary, busy: bool, operation: Operation) -> Result<Value, InvokeError> {
    queued(b, usize::from(busy), 8, operation)
}

fn queued(
    b: &Boundary,
    outstanding: usize,
    queue: usize,
    operation: Operation,
) -> Result<Value, InvokeError> {
    let at = at();
    let basis = Basis {
        at: &at,
        boundary: b,
        outstanding,
        queue,
    };
    submit(&basis, operation).map(|s| serde_json::to_value(s).unwrap())
}

fn aim_stack() -> Boundary {
    boundary(json!([screen(
        "screen:3",
        json!([
            plain("UP", "screen", &["UP", "k"]),
            {"id": "pick", "label": "Choose", "keys": [], "enabled": true, "expect": "boundary",
                "args": {"row": {"component_id": "list"}}},
            {"id": "move", "label": "Move", "keys": [], "enabled": true, "expect": "boundary",
                "args": {"rows": {"component_id": "list", "counts": true}, "pane": {"component_id": "root"}}},
            {"id": "off", "label": "Off", "keys": ["o"], "enabled": false, "denial": "Not now",
                "expect": "screen"},
            {"id": "batch", "label": "Batch", "keys": [], "enabled": true, "expect": "boundary",
                "args": {"row": {"component_id": "list"}, "count": {"min": 1, "max": 50, "default": 1}}}
        ])
    )]))
}

#[test]
fn a_screen_scoped_invoke_expects_the_top_screen() {
    let sent = command(&aim_stack(), false, invoke("UP", None)).unwrap();
    assert_eq!(
        sent,
        json!({"epoch": "epoch:e7f3", "expect": {"screen_id": "screen:3"},
            "operation": {"kind": "invoke", "affordance_id": "UP"}})
    );
}

#[test]
fn a_boundary_scoped_invoke_expects_the_boundary_and_revision() {
    let args = InvokeArgs {
        row: Some("item:1".into()),
        ..InvokeArgs::default()
    };
    let sent = command(&aim_stack(), false, invoke("pick", Some(args))).unwrap();
    assert_eq!(
        sent["expect"],
        json!({"boundary_id": "boundary:7", "revision": "31"})
    );
    assert_eq!(sent["operation"]["args"], json!({"row": "item:1"}));
}

#[test]
fn an_operation_that_is_not_an_invoke_keeps_the_0_1_expectation() {
    let sent = command(&aim_stack(), false, Operation::Cancel).unwrap();
    assert_eq!(
        sent["expect"],
        json!({"revision": "31", "boundary_id": "boundary:7", "schema_id": null})
    );
}

#[test]
fn only_the_top_screen_takes_input() {
    let below = boundary(json!([
        screen("screen:1", json!([plain("DEEP", "screen", &[])])),
        screen("screen:2", json!([plain("UP", "screen", &[])])),
    ]));
    assert_eq!(
        command(&below, false, invoke("DEEP", None)),
        Err(InvokeError::Unknown("DEEP".into()))
    );
    let empty = boundary(json!([]));
    assert_eq!(
        command(&empty, false, invoke("UP", None)),
        Err(InvokeError::NoScreen)
    );
}

#[test]
fn an_affordance_that_is_off_says_why_and_is_not_sent() {
    let Err(InvokeError::Disabled(Some(why))) = command(&aim_stack(), false, invoke("off", None))
    else {
        panic!("a disabled affordance must not be sent")
    };
    assert_eq!(why.shown(), "Not now");
}

#[test]
fn arguments_must_match_the_shape_before_anything_is_sent() {
    let b = aim_stack();
    let args = |f: fn(&mut InvokeArgs)| {
        let mut a = InvokeArgs::default();
        f(&mut a);
        Some(a)
    };
    let err = |id, args| command(&b, false, invoke(id, args)).unwrap_err();
    assert_eq!(
        err("pick", None),
        InvokeError::Args(ArgError::Missing(ArgKind::Row))
    );
    assert_eq!(
        err("UP", args(|a| a.text = Some("x".into()))),
        InvokeError::Args(ArgError::Unexpected(ArgKind::Text))
    );
    assert_eq!(
        err(
            "batch",
            args(|a| (a.row, a.count) = (Some("r".into()), Some(51)))
        ),
        InvokeError::Args(ArgError::OutOfRange {
            min: Some(1),
            max: Some(50)
        })
    );
    let fitting = args(|a| a.row = Some("r".into()));
    assert!(command(&b, false, invoke("pick", fitting)).is_ok());
}

#[test]
fn only_a_shape_that_counts_takes_row_counts() {
    let counted = ArgShape {
        rows: Some(serde_json::from_value(json!({"component_id": "l", "counts": true})).unwrap()),
        ..ArgShape::default()
    };
    let plain = ArgShape {
        rows: Some(serde_json::from_value(json!({"component_id": "l"})).unwrap()),
        ..ArgShape::default()
    };
    let args =
        serde_json::from_value::<InvokeArgs>(json!({"rows": [{"id": "a", "count": 2}]})).unwrap();
    assert_eq!(check_args(&counted, &args), Ok(()));
    assert_eq!(check_args(&plain, &args), Err(ArgError::RowCount));
    let none = InvokeArgs {
        rows: Some(vec![]),
        ..InvokeArgs::default()
    };
    assert_eq!(check_args(&counted, &none), Err(ArgError::EmptyRows));
}

#[test]
fn a_running_command_blocks_boundary_scoped_invokes_only() {
    let b = aim_stack();
    let args = InvokeArgs {
        row: Some("item:1".into()),
        ..InvokeArgs::default()
    };
    assert_eq!(
        command(&b, true, invoke("pick", Some(args))),
        Err(InvokeError::Busy)
    );
    assert!(
        command(&b, true, invoke("UP", None)).is_ok(),
        "a held arrow queues in the engine"
    );
}

#[test]
fn a_key_runs_the_enabled_affordance_that_binds_it() {
    let b = aim_stack();
    let top = b.stack.last().unwrap();
    assert_eq!(
        affordance_for_key(top, "k").map(|a| a.id.as_str()),
        Some("UP")
    );
    assert_eq!(
        affordance_for_key(top, "UP").map(|a| a.id.as_str()),
        Some("UP")
    );
    assert!(
        affordance_for_key(top, "o").is_none(),
        "off, and not any_key"
    );
    assert!(affordance_for_key(top, "x").is_none());
}

#[test]
fn a_navigation_key_belongs_to_the_engine_when_the_screen_binds_it_even_disabled() {
    let b = aim_stack();
    let top = b.stack.last().unwrap();
    assert!(binds_key(top, "UP"), "the engine moves the cursor");
    assert!(binds_key(top, "o"), "bound to a disabled affordance");
    assert!(!binds_key(top, "x"), "unbound keys are the client's");
}

#[test]
fn any_key_does_not_bind_a_key_for_navigation() {
    let b = boundary(json!([screen(
        "screen:4",
        json!([{"id": "dismiss", "label": "Continue", "keys": [], "enabled": true,
            "role": "any_key", "expect": "screen"}])
    )]));
    assert!(!binds_key(b.stack.last().unwrap(), "DOWN"));
}

#[test]
fn any_key_runs_only_on_a_key_nothing_binds() {
    let b = boundary(json!([screen(
        "screen:4",
        json!([
            {"id": "go", "label": "Go", "keys": ["g"], "enabled": true, "expect": "screen"},
            {"id": "dismiss", "label": "Continue", "keys": [], "enabled": true,
                "role": "any_key", "expect": "screen"}
        ])
    )]));
    let top = b.stack.last().unwrap();
    assert_eq!(
        affordance_for_key(top, "g").map(|a| a.id.as_str()),
        Some("go")
    );
    assert_eq!(
        affordance_for_key(top, "SPACE").map(|a| a.id.as_str()),
        Some("dismiss")
    );
    assert_eq!(
        with_role(top, AffordanceRole::AnyKey).map(|a| a.id.as_str()),
        Some("dismiss")
    );
    assert!(with_role(top, AffordanceRole::Cancel).is_none());
}

fn stack_of(id: &str, focus: Option<Value>) -> Vec<Screen> {
    let mut s = screen(id, json!([plain("UP", "screen", &[])]));
    if let Some(focus) = focus {
        s["focus"] = focus;
    }
    vec![serde_json::from_value(s).unwrap()]
}

fn rows(spec: &[(&str, bool, bool)]) -> Vec<Row> {
    spec.iter()
        .map(|&(id, selectable, header)| {
            let mut row = json!({"id": id, "label": id, "description": "", "denial": "",
                "enabled": true, "selectable": selectable, "selected": false, "columns": []});
            if header {
                row["header"] = json!(true);
            }
            serde_json::from_value(row).unwrap()
        })
        .collect()
}

#[test]
fn a_new_screen_drops_the_local_state_and_the_same_one_keeps_it() {
    let mut ui = Interaction::default();
    ui.sync(&stack_of(
        "screen:1",
        Some(json!({"component_id": "list", "row_id": "a"})),
    ));
    assert_eq!(ui.focus().row_id.as_deref(), Some("a"));
    ui.focus_row("list", "c");
    ui.toggle_pick("list", "c");
    ui.scroll_by(
        "list",
        5,
        Window {
            total: 50,
            visible: 10,
        },
    );
    ui.sync(&stack_of(
        "screen:1",
        Some(json!({"component_id": "list", "row_id": "a"})),
    ));
    assert_eq!(
        ui.focus().row_id.as_deref(),
        Some("c"),
        "the engine cursor did not move"
    );
    assert_eq!((ui.picks("list").len(), ui.scroll("list")), (1, 5));
    ui.sync(&stack_of(
        "screen:1",
        Some(json!({"component_id": "list", "row_id": "b"})),
    ));
    assert_eq!(
        ui.focus().row_id.as_deref(),
        Some("b"),
        "a moved engine cursor takes the focus"
    );
    ui.sync(&stack_of("screen:2", None));
    assert_eq!(*ui.focus(), Default::default());
    assert_eq!((ui.picks("list").len(), ui.scroll("list")), (0, 0));
}

#[test]
fn focus_moves_among_the_rows_to_choose_and_stops_at_the_ends() {
    let all = rows(&[
        ("h", false, true),
        ("a", true, false),
        ("x", false, false),
        ("b", true, false),
    ]);
    let all = all.iter().collect::<Vec<_>>();
    let mut ui = Interaction::default();
    assert!(ui.move_focus("list", &all, 1));
    assert_eq!(
        ui.focus().row_id.as_deref(),
        Some("a"),
        "a heading is not a stop"
    );
    ui.move_focus("list", &all, 1);
    assert_eq!(
        ui.focus().row_id.as_deref(),
        Some("b"),
        "a row that cannot be selected is skipped"
    );
    ui.move_focus("list", &all, 9);
    assert_eq!(ui.focus().row_id.as_deref(), Some("b"));
    ui.move_focus("list", &all, -9);
    assert_eq!(ui.focus().row_id.as_deref(), Some("a"));
    let mut elsewhere = Interaction::default();
    elsewhere.move_focus("list", &all, -1);
    assert_eq!(
        elsewhere.focus().row_id.as_deref(),
        Some("b"),
        "going up from nowhere starts at the end"
    );
    assert!(!elsewhere.move_focus("list", &[], 1));
}

#[test]
fn scrolling_stays_within_the_list_and_reveals_the_focus() {
    let mut ui = Interaction::default();
    let window = Window {
        total: 30,
        visible: 10,
    };
    ui.scroll_by("l", 100, window);
    assert_eq!(ui.scroll("l"), 20, "the last row is the last one shown");
    ui.scroll_by("l", -100, window);
    assert_eq!(ui.scroll("l"), 0);
    ui.reveal("l", 12, window);
    assert_eq!(ui.scroll("l"), 3, "the least that shows row 12");
    ui.reveal("l", 5, window);
    assert_eq!(ui.scroll("l"), 3, "already shown");
    ui.reveal("l", 1, window);
    assert_eq!(ui.scroll("l"), 1);
    ui.reveal(
        "l",
        4,
        Window {
            total: 30,
            visible: 0,
        },
    );
    assert_eq!(ui.scroll("l"), 4, "a view of no rows still shows one");
}

#[test]
fn marked_rows_become_the_rows_argument_with_counts_only_where_counted() {
    let b = aim_stack();
    let move_ = b.stack[0]
        .affordances
        .iter()
        .find(|a| a.id == "move")
        .unwrap();
    let mut ui = Interaction::default();
    ui.sync(&b.stack);
    assert_eq!(
        ui.begin(move_),
        [ArgKind::Rows, ArgKind::Pane],
        "no row is marked or focused"
    );
    ui.focus_row("list", "f");
    assert_eq!(
        ui.begin(move_),
        [ArgKind::Pane],
        "the focused row stands for one"
    );
    ui.toggle_pick("list", "a");
    ui.toggle_pick("list", "b");
    ui.toggle_pick("list", "a");
    ui.toggle_pick("list", "c");
    ui.set_pick_count("list", "c", Some(2));
    ui.set_pick_count("list", "zz", Some(9));
    ui.set_pane("root");
    let args = ui.finish(move_).unwrap();
    assert_eq!(
        serde_json::to_value(&args).unwrap(),
        json!({"rows": [{"id": "b"}, {"id": "c", "count": 2}], "pane": "root"})
    );
    assert!(ui.picks("list").is_empty(), "the rows were used");
    assert_eq!(
        ui.args(move_),
        Err(vec![ArgKind::Pane]),
        "the draft ended; the focused row remains"
    );
}

#[test]
fn a_count_is_typed_held_to_the_bounds_and_defaults() {
    let b = aim_stack();
    let batch = b.stack[0]
        .affordances
        .iter()
        .find(|a| a.id == "batch")
        .unwrap();
    let mut ui = Interaction::default();
    ui.focus_row("list", "r");
    assert_eq!(
        ui.begin(batch),
        [ArgKind::Count],
        "a default is still confirmed"
    );
    assert_eq!(
        ui.args(batch).unwrap().count,
        Some(1),
        "the shape's default"
    );
    "250".chars().for_each(|c| assert!(ui.push_count(c)));
    assert_eq!(
        ui.args(batch).unwrap().count,
        Some(50),
        "held to the shape's maximum"
    );
    ui.pop_count();
    ui.pop_count();
    assert_eq!(ui.args(batch).unwrap().count, Some(2));
    ui.pop_count();
    assert_eq!(
        ui.args(batch).unwrap().count,
        Some(1),
        "an empty count falls back to the default"
    );
    assert!(!ui.push_count('x'), "only digits");
    assert!(ui.push_count('-'), "a minus opens the number");
    assert!(!ui.push_count('-'), "and only there");
    assert_eq!(
        ui.args(batch).unwrap().count,
        Some(1),
        "-, not yet a number, is the default"
    );
    ui.cancel();
    assert!(!ui.push_count('3'), "no collection, nothing typed");
}

#[test]
fn text_a_position_and_a_pane_are_gathered_by_the_affordance_that_asks() {
    let ask = affordance(
        json!({"id": "find", "label": "Find", "keys": [], "enabled": true,
        "expect": "boundary", "args": {"text": {}, "pos": {"unit": "square"}, "pane": {"component_id": "r"}}}),
    );
    let mut ui = Interaction::default();
    assert_eq!(ui.begin(&ask), [ArgKind::Text, ArgKind::Pos, ArgKind::Pane]);
    ui.push_text("sa");
    ui.push_text("w");
    ui.pop_text();
    ui.set_pos(bn_client_core::protocol::WorldPos {
        dim: String::new(),
        x: 3,
        y: 4,
        z: 0,
    });
    ui.set_pane("pane:b");
    let args = ui.finish(&ask).unwrap();
    assert_eq!(
        serde_json::to_value(args).unwrap(),
        json!({"text": "sa", "pos": {"dim": "", "x": 3, "y": 4, "z": 0}, "pane": "pane:b"})
    );
    let other = affordance(plain("other", "boundary", &[]));
    ui.begin(&ask);
    ui.push_text("kept");
    assert_eq!(
        ui.args(&ask).unwrap_err(),
        [ArgKind::Pos, ArgKind::Pane],
        "text is held while its affordance is collected"
    );
    assert!(
        ui.args(&other).is_ok(),
        "a draft belongs to its own affordance"
    );
}

#[test]
fn a_text_starts_as_the_engines_suggestion_which_typing_replaces() {
    let ask = affordance(
        json!({"id": "new", "label": "New vehicle", "keys": [], "enabled": true,
        "expect": "boundary", "args": {"text": {"default": "my_vehicle"}}}),
    );
    let mut ui = Interaction::default();
    ui.begin(&ask);
    assert_eq!(ui.ask().unwrap().text.as_deref(), Some("my_vehicle"));
    assert_eq!(
        serde_json::to_value(ui.args(&ask).unwrap()).unwrap(),
        json!({"text": "my_vehicle"}),
        "taken as it is"
    );
    ui.push_text("b");
    ui.push_text("us");
    assert_eq!(
        ui.ask().unwrap().text.as_deref(),
        Some("bus"),
        "typing replaces it"
    );
    ui.begin(&ask);
    ui.pop_text();
    assert_eq!(
        ui.ask().unwrap().text.as_deref(),
        Some(""),
        "a backspace clears it"
    );
}

#[test]
fn a_draft_ends_when_its_affordance_leaves_the_screen() {
    let mut ui = Interaction::default();
    let stack = stack_of("screen:1", None);
    ui.sync(&stack);
    ui.begin(&stack[0].affordances[0]);
    assert!(ui.push_count('4'));
    let mut gone = stack.clone();
    gone[0].affordances.clear();
    ui.sync(&gone);
    assert!(!ui.push_count('4'));
}

fn list(total: u64, first: usize, offset: Option<u64>) -> List {
    let held = rows(&vec![("r", true, false); first]);
    let mut value = json!({"id": "list", "kind": "list", "columns": [], "total": total,
        "selection": "single", "rows": held});
    if let Some(offset) = offset {
        value["offset"] = json!(offset);
    }
    match serde_json::from_value::<Component>(value).unwrap() {
        Component::List(list) => list,
        other => panic!("{other:?}"),
    }
}

fn page(boundary: &str, n: usize, total: u64) -> RowsResult {
    RowsResult {
        boundary_id: boundary.into(),
        component_id: "list".into(),
        total,
        rows: rows(&vec![("p", true, false); n]),
    }
}

#[test]
fn pages_are_asked_once_in_turn_until_every_row_is_held() {
    let list = list(450, 200, None);
    let mut pages = Pages::default();
    let first = pages.next("b:1", &list).unwrap();
    assert_eq!(
        (first.component_id.as_str(), first.offset, first.limit),
        ("list", 200, 200)
    );
    assert!(pages.next("b:1", &list).is_none(), "the page is on its way");
    pages.accept(page("b:1", 200, 450));
    assert_eq!(pages.next("b:1", &list).unwrap().offset, 400);
    pages.accept(page("b:1", 50, 450));
    assert!(pages.next("b:1", &list).is_none(), "all 450 rows are held");
    assert_eq!(pages.rows("b:1", &list).len(), 450);
    assert_eq!(
        pages.rows("b:3", &list).len(),
        200,
        "pages belong to their boundary"
    );
}

#[test]
fn a_list_with_all_its_rows_asks_for_nothing() {
    let mut pages = Pages::default();
    assert!(pages.next("b:1", &list(3, 3, None)).is_none());
    assert!(pages.next("b:1", &list(0, 0, None)).is_none());
}

#[test]
fn a_page_counts_from_the_offset_of_the_first_rows() {
    let mut pages = Pages::default();
    let ask = pages.next("b:1", &list(500, 200, Some(100))).unwrap();
    assert_eq!(ask.offset, 300);
}

#[test]
fn a_refused_or_empty_page_closes_the_list_until_the_next_boundary_and_a_stray_page_is_dropped() {
    let list = list(450, 200, None);
    let mut pages = Pages::default();
    pages.next("b:1", &list).unwrap();
    pages.accept(page("b:0", 200, 450));
    assert!(
        pages.next("b:1", &list).is_none(),
        "an answer for another boundary was not taken"
    );
    pages.failed("list");
    assert!(
        pages.next("b:1", &list).is_none(),
        "a refusal is not retried every frame"
    );
    assert_eq!(pages.next("b:2", &list).unwrap().offset, 200);
    pages.accept(page("b:2", 0, 450));
    assert!(
        pages.next("b:2", &list).is_none(),
        "an empty page ends the asking"
    );
    pages.next("b:3", &list).unwrap();
    pages.accept(page("b:1", 200, 450));
    pages.accept(page("b:1", 200, 450));
    assert_eq!(
        pages.rows("b:2", &list).len(),
        200,
        "a new boundary starts over"
    );
}

/// The Rust files under `dir` and its subdirectories.
fn files_under(dir: &str) -> Vec<String> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut found = std::fs::read_dir(root.join(dir))
        .unwrap()
        .map(|e| e.unwrap().path())
        .flat_map(|p| {
            let name = format!("{dir}/{}", p.file_name().unwrap().to_string_lossy());
            match (p.is_dir(), p.extension().is_some_and(|e| e == "rs")) {
                (true, _) => files_under(&name),
                (false, true) => vec![name],
                (false, false) => vec![],
            }
        })
        .collect::<Vec<_>>();
    found.sort();
    found
}

/// Files that are client chrome, not a screen: input routes keys and the pointer, the map paints the view, the
/// palette lists the engine's actions, the sidebar lays out values the engine labels, the component renderers
/// draw what the core decides, and the binary only parses arguments.
const CLIENT_OWNED: &[&str] = &[
    "crates/egui/src/app/component.rs",
    "crates/egui/src/app/component/canvas.rs",
    "crates/egui/src/app/component/detail.rs",
    "crates/egui/src/app/component/drag.rs",
    "crates/egui/src/app/component/footer.rs",
    "crates/egui/src/app/component/form.rs",
    "crates/egui/src/app/component/list.rs",
    "crates/egui/src/app/component/measure.rs",
    "crates/egui/src/app/component/overmap.rs",
    "crates/egui/src/app/component/panel.rs",
    "crates/egui/src/app/component/placement.rs",
    "crates/egui/src/app/component/region.rs",
    "crates/egui/src/app/component/tabs.rs",
    "crates/egui/src/app/component/target.rs",
    "crates/egui/src/app/component/workspace.rs",
    "crates/egui/src/app/input.rs",
    "crates/egui/src/app/map.rs",
    "crates/egui/src/app/palette.rs",
    "crates/egui/src/app/sidebar.rs",
    "crates/egui/src/app/sounds.rs",
    "crates/tui/src/component.rs",
    "crates/tui/src/main.rs",
];

#[test]
fn every_bespoke_screen_module_is_in_the_override_list() {
    let listed = OVERRIDES.iter().map(|o| o.module).collect::<Vec<_>>();
    let modules = ["crates/egui/src/app", "crates/tui/src"]
        .into_iter()
        .flat_map(files_under)
        .collect::<Vec<_>>();
    for module in &modules {
        assert!(
            listed.contains(&module.as_str()) || CLIENT_OWNED.contains(&module.as_str()),
            "{module} draws or decides a screen: list it in OVERRIDES, or in CLIENT_OWNED if it is chrome"
        );
    }
}

#[test]
fn the_override_list_names_only_modules_that_still_exist() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    for o in OVERRIDES {
        assert!(
            root.join(o.module).is_file(),
            "{} was deleted: drop its line",
            o.module
        );
        assert!(o.slice.starts_with('S'), "{}: {}", o.module, o.slice);
    }
    let mut modules = OVERRIDES.iter().map(|o| o.module).collect::<Vec<_>>();
    modules.sort();
    modules.dedup();
    assert_eq!(modules.len(), OVERRIDES.len(), "a module is listed once");
}

#[test]
fn expectations_are_the_forms_the_wire_defines() {
    let expect = serde_json::from_value::<Expect>(json!({"screen_id": "screen:3"})).unwrap();
    assert!(matches!(expect, Expect::Screen(_)));
    let expect =
        serde_json::from_value::<Expect>(json!({"boundary_id": "b", "revision": "1"})).unwrap();
    assert!(matches!(expect, Expect::Boundary(_)));
}

#[test]
fn a_screen_scoped_invoke_queues_up_to_the_engines_depth() {
    let b = aim_stack();
    let up = || invoke("UP", None);
    assert!(
        queued(&b, 8, 8, up()).is_ok(),
        "the running one and eight waiting"
    );
    assert_eq!(queued(&b, 9, 8, up()), Err(InvokeError::Busy));
    assert!(queued(&b, 1, 1, up()).is_ok());
    assert_eq!(queued(&b, 2, 1, up()), Err(InvokeError::Busy));
}

fn selecting(id: &str, focus: Value, select_list: &str, enabled: bool) -> Vec<Screen> {
    let mut s = screen(
        id,
        json!([
            {"id": "pick:select", "label": "Select", "keys": [], "enabled": enabled,
                "expect": "boundary", "role": "select",
                "args": {"row": {"component_id": select_list}}},
        ]),
    );
    s["focus"] = focus;
    vec![serde_json::from_value(s).unwrap()]
}

#[test]
fn moving_the_focus_in_a_list_a_select_affordance_names_invokes_it_once_with_the_row() {
    let focus = json!({"component_id": "list", "row_id": "a"});
    let mut ui = Interaction::default();
    ui.sync(&selecting("screen:1", focus.clone(), "list", true));
    assert!(
        ui.take_select().is_none(),
        "the engine's own cursor is not sent back"
    );
    ui.focus_row("list", "c");
    ui.sync(&selecting("screen:1", focus.clone(), "list", true));
    let (id, args) = ui.take_select().expect("the focus moved");
    assert_eq!(
        (id.as_str(), args.row.as_deref()),
        ("pick:select", Some("c"))
    );
    ui.sync(&selecting("screen:1", focus, "list", true));
    assert!(ui.take_select().is_none(), "once per move");
}

#[test]
fn the_focus_moving_elsewhere_or_with_no_enabled_select_affordance_invokes_nothing() {
    let focus = json!({"component_id": "list", "row_id": "a"});
    let mut ui = Interaction::default();
    ui.sync(&selecting("screen:1", focus.clone(), "list", true));
    ui.focus_row("tabs", "x");
    ui.sync(&selecting("screen:1", focus.clone(), "list", true));
    assert!(
        ui.take_select().is_none(),
        "the affordance names another list"
    );
    ui.focus_row("list", "b");
    ui.sync(&selecting("screen:1", focus, "list", false));
    assert!(ui.take_select().is_none(), "a disabled affordance");
}

#[test]
fn an_engine_cursor_that_moves_is_followed_and_not_sent_back() {
    let mut ui = Interaction::default();
    ui.sync(&selecting(
        "screen:1",
        json!({"component_id": "list", "row_id": "a"}),
        "list",
        true,
    ));
    ui.sync(&selecting(
        "screen:1",
        json!({"component_id": "list", "row_id": "b"}),
        "list",
        true,
    ));
    assert_eq!(ui.focus().row_id.as_deref(), Some("b"));
    assert!(ui.take_select().is_none());
}
