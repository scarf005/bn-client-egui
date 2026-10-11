mod common;
use bn_client_core::transport::ScriptHandle;
use bn_client_core::{EngineModel, GameModel};
use bn_client_tui::TuiApp;
use common::{fixture, session};
use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::style::Color;
use serde_json::{Value, json};

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

fn open(boundary: Value) -> (TuiApp<EngineModel>, ScriptHandle) {
    let (t, h) = session();
    let t = (0..12).fold(t, |t, _| {
        t.on("bn.command.submit", vec![fixture("submit_result")])
    });
    let mut model = EngineModel::new(Box::new(t));
    model.poll();
    h.push(event(41, boundary));
    model.poll();
    (TuiApp::new(model), h)
}

fn render(app: &mut TuiApp<EngineModel>) -> Terminal<TestBackend> {
    let mut t = Terminal::new(TestBackend::new(120, 40)).unwrap();
    t.draw(|f| app.draw(f)).unwrap();
    t
}

fn lines(t: &Terminal<TestBackend>) -> Vec<String> {
    let b = t.backend().buffer();
    b.content()
        .chunks(b.area.width as usize)
        .map(|row| row.iter().map(|c| c.symbol()).collect::<String>())
        .collect()
}

fn find(t: &Terminal<TestBackend>, text: &str) -> (u16, u16) {
    lines(t)
        .iter()
        .enumerate()
        .find_map(|(y, l)| l.find(text).map(|x| (x as u16, y as u16)))
        .unwrap_or_else(|| panic!("{text} not drawn:\n{}", lines(t).join("\n")))
}

fn operations(h: &ScriptHandle) -> Vec<Value> {
    h.sent()
        .into_iter()
        .filter(|m| m["method"] == "bn.command.submit")
        .map(|m| m["params"]["operation"].clone())
        .collect()
}

fn mouse(kind: MouseEventKind, column: u16, row: u16) -> MouseEvent {
    MouseEvent {
        kind,
        column,
        row,
        modifiers: KeyModifiers::NONE,
    }
}

/// The engine finished the command in flight; the next queued input may go.
fn finished(app: &mut TuiApp<EngineModel>, h: &ScriptHandle) {
    h.push(json!({"jsonrpc":"2.0","method":"bn.command","params":{
        "epoch":"e7f3","command_id":"c:5","stage":"completed"}}));
    app.model.poll();
    app.tick();
}

fn action(id: &str) -> Value {
    json!({"kind":"action","action_id":id})
}

#[test]
fn two_panes_show_their_area_totals_columns_rows_filter_and_description() {
    let (mut app, _) = open(aim());
    let t = render(&mut app);
    let all = lines(&t).join("\n");
    for text in [
        "Left pane: Directly below you",
        "Right pane: Worn Items",
        "Name (charges)",
        "amt weight vol",
        "[f, F or /] Filter",
        "A shirt.",
    ] {
        assert!(all.contains(text), "{text}:\n{all}");
    }
    // The screen covers the map and the side panels instead of drawing over them.
    assert!(
        !all.contains("Message log") && !all.contains("Pan down"),
        "{all}"
    );
    // The left pane is drawn left of the right one, each with its own rows.
    let (shirt, row) = find(&t, "long-sleeved shirt");
    let (coat, _) = find(&t, "trench coat");
    assert!(shirt < 60 && coat >= 60);
    let line = &lines(&t)[row as usize];
    for value in ["0.15 kg", "0.75 L"] {
        assert!(line.contains(value), "{value} on the shirt's row: {line}");
    }
    // The pane total is the sum of its rows: the shirt and three cans.
    assert!(all.contains("0.45 kg"), "left total:\n{all}");
}

#[test]
fn color_tags_in_an_item_name_colour_its_cells() {
    let mut boundary = aim();
    boundary["interaction"]["choices"][0]["label"] =
        "<color_c_light_red>long-sleeved shirt</color>".into();
    let (mut app, _) = open(boundary);
    let t = render(&mut app);
    let (x, y) = find(&t, "long-sleeved shirt");
    assert_ne!(t.backend().buffer()[(x, y)].fg, Color::Reset);
}

#[test]
fn clicking_a_row_chooses_it_and_clicking_the_other_pane_points_the_engine_at_it() {
    let (mut app, h) = open(aim());
    let t = render(&mut app);
    let (x, y) = find(&t, "tin can");
    app.handle_mouse(mouse(MouseEventKind::Down(MouseButton::Left), x + 1, y));
    app.tick();
    assert_eq!(
        operations(&h),
        [json!({"kind":"choose","choice_id":"aim:cans"})]
    );
    finished(&mut app, &h);
    // An empty spot of the right pane: the right pane becomes the one the engine acts on.
    let (cx, cy) = find(&t, "trench coat");
    app.handle_mouse(mouse(
        MouseEventKind::Down(MouseButton::Left),
        cx + 30,
        cy + 8,
    ));
    app.tick();
    assert_eq!(operations(&h).last().unwrap(), &action("RIGHT"));
}

#[test]
fn the_wheel_moves_the_engine_cursor_three_rows_in_the_pane_under_it() {
    let (mut app, h) = open(aim());
    let t = render(&mut app);
    let (x, y) = find(&t, "tin can");
    app.handle_mouse(mouse(MouseEventKind::ScrollDown, x, y));
    app.tick();
    for _ in 0..3 {
        finished(&mut app, &h);
    }
    assert_eq!(
        operations(&h),
        [action("DOWN"), action("DOWN"), action("DOWN")]
    );
    // Over the pane the engine does not act on, it is pointed there first.
    let (cx, cy) = find(&t, "trench coat");
    app.handle_mouse(mouse(MouseEventKind::ScrollUp, cx, cy));
    for _ in 0..4 {
        finished(&mut app, &h);
    }
    let ops = operations(&h);
    assert_eq!(
        &ops[3..],
        [action("RIGHT"), action("UP"), action("UP"), action("UP")]
    );
}
