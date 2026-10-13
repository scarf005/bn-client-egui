//! The 0.2 components in the terminal client, on the engine's own examples (`core/tests/fixtures/0.2.examples/`):
//! each component kind drawn, the screen's affordances, and the keys, which only the core turns into commands.
mod common;
use bn_client_core::colors::ColorMap;
use bn_client_core::transport::{ScriptHandle, ScriptedTransport};
use bn_client_core::{ConnectOptions, EngineModel, GameModel, Rgb};
use bn_client_tui::TuiApp;
use common::fixture;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::style::{Color, Modifier};
use serde_json::{Value, json};

const WIDTH: u16 = 80;
const HEIGHT: u16 = 14;

/// The value of the example called `name` in an examples file.
fn example(file: &str, name: &str) -> Value {
    let path = format!(
        "{}/../core/tests/fixtures/0.2.examples/{file}",
        env!("CARGO_MANIFEST_DIR")
    );
    let all: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    all["valid"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["name"] == name)
        .unwrap_or_else(|| panic!("no example {name}"))["value"]
        .clone()
}

/// A screen of its own around a component.
fn screen(root: Value, affordances: Value) -> Value {
    json!({"screen_id": "screen:1", "role": "screen", "title": "Test", "root": root,
        "affordances": affordances})
}

fn choose(id: &str, component: &str) -> Value {
    json!({"id": id, "label": "Choose", "keys": [], "enabled": true, "expect": "boundary",
        "args": {"row": {"component_id": component}}})
}

type Script = fn(ScriptedTransport) -> ScriptedTransport;

/// A 0.2 session whose boundary holds `screen` as its only screen.
fn open_with(screen: Value, extra: Script) -> (TuiApp<EngineModel>, ScriptHandle) {
    let mut subscribe = fixture("subscribe_result");
    subscribe["result"]["interaction"]["stack"] = json!([screen]);
    let (t, h) = ScriptedTransport::new();
    let t = t
        .on("bn.hello", vec![fixture("hello_result")])
        .on(
            "bn.subscribe",
            vec![
                subscribe,
                fixture("snapshot_part_0"),
                fixture("snapshot_part_1"),
            ],
        )
        .on("bn.command.submit", vec![fixture("submit_result")]);
    let options = ConnectOptions::default();
    let mut model = EngineModel::open(Box::new(extra(t)), options);
    model.poll();
    (TuiApp::new(model), h)
}

fn open(screen: Value) -> (TuiApp<EngineModel>, ScriptHandle) {
    open_with(screen, |t| t)
}

fn render(app: &mut TuiApp<EngineModel>) -> Terminal<TestBackend> {
    let mut t = Terminal::new(TestBackend::new(WIDTH, HEIGHT)).unwrap();
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

/// Where `needle` first appears, as column and row.
fn at(t: &Terminal<TestBackend>, needle: &str) -> (u16, u16) {
    let found = lines(t).iter().enumerate().find_map(|(y, line)| {
        line.find(needle)
            .map(|i| (line[..i].chars().count() as u16, y as u16))
    });
    found.unwrap_or_else(|| panic!("no {needle:?} in {:#?}", lines(t)))
}

fn fg(t: &Terminal<TestBackend>, needle: &str) -> Color {
    let (x, y) = at(t, needle);
    t.backend().buffer()[(x, y)].fg
}

fn modifier(t: &Terminal<TestBackend>, needle: &str) -> Modifier {
    let (x, y) = at(t, needle);
    t.backend().buffer()[(x, y)].modifier
}

fn press(app: &mut TuiApp<EngineModel>, code: KeyCode) {
    app.handle_key(KeyEvent::new(code, KeyModifiers::NONE));
}

fn submits(h: &ScriptHandle) -> Vec<Value> {
    h.sent()
        .into_iter()
        .filter(|m| m["method"] == "bn.command.submit")
        .map(|m| m["params"].clone())
        .collect()
}

fn operations(h: &ScriptHandle) -> Vec<Value> {
    submits(h)
        .into_iter()
        .map(|p| p["operation"].clone())
        .collect()
}

fn rgb(name: &str) -> Color {
    let Rgb(r, g, b) = ColorMap::default().resolve(name).0;
    Color::Rgb(r, g, b)
}

#[test]
fn a_list_draws_headings_columns_and_the_state_of_its_pages() {
    let list = example(
        "interaction.json",
        "list with category header rows and a row that allows only some affordances",
    );
    let set_count = json!({"id": "bn:set_count:list:npc", "label": "Set count", "keys": [],
        "enabled": true, "expect": "boundary",
        "args": {"row": {"component_id": "list:npc"}, "count": {"min": 1, "max": 5, "default": 1}}});
    let (mut app, h) = open_with(
        screen(
            list,
            json!([choose("bn:choose:list:npc", "list:npc"), set_count]),
        ),
        |t| {
            t.on(
                "bn.interaction.choices",
                vec![json!({"jsonrpc": "2.0", "result": {"boundary_id": "b:12",
                "component_id": "list:npc", "total": 40, "rows": [{"id": "item:saw", "label": "saw",
                "description": "", "denial": "", "enabled": true, "selectable": true,
                "selected": false, "columns": [{"key": "unit_price", "label": "Price",
                "value": "$8.00"}]}]}})],
            )
        },
    );
    let t = render(&mut app);
    let text = lines(&t).join("\n");
    assert!(text.contains("hammer") && text.contains("$12.00") && text.contains("Price"));
    assert!(text.contains("1-2/40+"), "{text}");
    assert!(modifier(&t, "TOOLS").contains(Modifier::BOLD), "a heading");
    assert!(
        h.sent_methods()
            .contains(&"bn.interaction.choices".to_string()),
        "the rest of the list is asked for"
    );
    app.model.poll();
    let text = lines(&render(&mut app)).join("\n");
    assert!(text.contains("saw") && text.contains("$8.00") && text.contains("1-3/40+"));
    // The focus skips the heading, and moving it sends nothing.
    press(&mut app, KeyCode::Down);
    let t = render(&mut app);
    assert!(modifier(&t, "hammer").contains(Modifier::REVERSED));
    assert!(!modifier(&t, "TOOLS").contains(Modifier::REVERSED));
    assert!(
        lines(&t)
            .join("\n")
            .contains("Set count: Only one is offered."),
        "the row's own denial"
    );
    assert!(submits(&h).is_empty());
    // The row allows only set_count, which is off: the core sends nothing and the engine's reason shows.
    press(&mut app, KeyCode::Enter);
    assert!(submits(&h).is_empty());
    // The next row came in with the second page: the keyboard reaches it and chooses it.
    press(&mut app, KeyCode::Down);
    assert!(modifier(&render(&mut app), "saw").contains(Modifier::REVERSED));
    press(&mut app, KeyCode::Enter);
    assert_eq!(
        operations(&h),
        [
            json!({"kind": "invoke", "affordance_id": "bn:choose:list:npc", "args": {"row": "item:saw"}})
        ]
    );
}

#[test]
fn tabs_mark_the_selected_one_and_a_side_key_selects_the_neighbour() {
    let tabs = example(
        "construction.json",
        "construction menu: tabs over the buildable rows",
    );
    let (mut app, h) = open(screen(tabs, json!([choose("bn:choose:tabs", "tabs")])));
    let t = render(&mut app);
    let text = lines(&t).join("\n");
    assert!(text.contains("Build Wooden Wall") && text.contains("Buildable"));
    assert!(
        modifier(&t, "Walls").contains(Modifier::REVERSED),
        "the selected tab"
    );
    assert!(!modifier(&t, "All").contains(Modifier::REVERSED));
    press(&mut app, KeyCode::Left);
    assert_eq!(
        operations(&h),
        [json!({"kind": "invoke", "affordance_id": "bn:choose:tabs", "args": {"row": "tab:ALL"}})]
    );
}

#[test]
fn a_split_lays_its_children_side_by_side_and_the_text_keeps_the_game_colour() {
    let mut lab = example(
        "terminal.json",
        "terminal output with an answer row at a security query",
    );
    lab["role"] = json!("screen");
    let (mut app, h) = open(lab);
    let t = render(&mut app);
    let (text_x, text_y) = at(&t, "ERROR: Please place a security card.");
    let (yes_x, yes_y) = at(&t, "Yes");
    assert_eq!(text_y, yes_y, "peers share the row");
    assert!(yes_x > text_x + 30);
    assert_eq!(fg(&t, "ERROR"), Color::Green, "terminal output");
    press(&mut app, KeyCode::Down);
    press(&mut app, KeyCode::Down);
    assert!(modifier(&render(&mut app), "No").contains(Modifier::REVERSED));
    press(&mut app, KeyCode::Enter);
    assert_eq!(
        operations(&h),
        [
            json!({"kind": "invoke", "affordance_id": "bn:choose:list:terminal", "args": {"row": "no"}})
        ]
    );
}

#[test]
fn escape_runs_the_cancel_affordance_of_the_screen_scope() {
    let mut lab = example(
        "terminal.json",
        "terminal output with an answer row at a security query",
    );
    lab["role"] = json!("screen");
    let (mut app, h) = open(lab);
    render(&mut app);
    press(&mut app, KeyCode::Esc);
    assert_eq!(
        operations(&h),
        [json!({"kind": "invoke", "affordance_id": "bn:cancel", "args": {}})]
    );
    assert_eq!(submits(&h)[0]["expect"], json!({"screen_id": "screen:14"}));
}

#[test]
fn a_panel_draws_its_title_subtitle_and_stats() {
    let panel = example("aim.json", "advanced inventory pane with totals");
    let (mut app, _) = open(screen(panel, json!([])));
    let text = lines(&render(&mut app)).join("\n");
    for shown in [
        "Destination",
        "Directly under you",
        "Weight: 0.0 kg",
        "Volume: 0.00/1000.00 L",
        "Sort: name",
    ] {
        assert!(text.contains(shown), "{shown}: {text}");
    }
}

#[test]
fn a_panel_nests_a_split_of_text_and_a_list_with_the_engines_column_colour() {
    let panel = example(
        "menu.json",
        "character sheet speed pane with its text and rows",
    );
    let (mut app, _) = open(screen(panel, json!([])));
    let t = render(&mut app);
    let text = lines(&t).join("\n");
    assert!(text.contains("SPEED") && text.contains("Base Move Cost: 100"));
    assert!(text.contains("Current Speed:") && text.contains("Value"));
    assert_eq!(fg(&t, "98"), rgb("c_red"));
}

#[test]
fn a_form_draws_its_fields_and_a_number_is_set_through_the_core() {
    let mut options = example("menu.json", "options page with each editor type");
    options["role"] = json!("screen");
    let (mut app, h) = open(options);
    let text = lines(&render(&mut app)).join("\n");
    assert!(text.contains("Safe mode proximity distance: 7"), "{text}");
    assert!(
        text.contains("Drop empty containers: No"),
        "an option by its label: {text}"
    );
    // Tab takes the focus from the list to the form.
    press(&mut app, KeyCode::Tab);
    press(&mut app, KeyCode::Tab);
    assert!(modifier(&render(&mut app), "> Safe mode").contains(Modifier::REVERSED));
    press(&mut app, KeyCode::Enter);
    assert!(
        lines(&render(&mut app))
            .iter()
            .any(|l| l.contains("Set: 7_")),
        "the prompt"
    );
    press(&mut app, KeyCode::Backspace);
    press(&mut app, KeyCode::Char('8'));
    assert!(submits(&h).is_empty(), "typing sends nothing");
    press(&mut app, KeyCode::Enter);
    assert_eq!(
        operations(&h),
        [
            json!({"kind": "invoke", "affordance_id": "bn:submit:editors",
            "args": {"row": "option:SAFEMODEPROXIMITY", "text": "8"}})
        ]
    );
}

#[test]
fn a_select_field_steps_through_its_options_by_id() {
    let mut options = example("menu.json", "options page with each editor type");
    options["role"] = json!("screen");
    let (mut app, h) = open(options);
    render(&mut app);
    press(&mut app, KeyCode::Tab);
    press(&mut app, KeyCode::Tab);
    press(&mut app, KeyCode::Down);
    press(&mut app, KeyCode::Enter);
    press(&mut app, KeyCode::Down);
    assert!(
        lines(&render(&mut app))
            .iter()
            .any(|l| l.contains("Set: Watertight_"))
    );
    press(&mut app, KeyCode::Enter);
    assert_eq!(
        operations(&h),
        [
            json!({"kind": "invoke", "affordance_id": "bn:submit:editors",
            "args": {"row": "option:DROP_EMPTY", "text": "watertight"}})
        ]
    );
}

#[test]
fn a_key_field_takes_the_next_key_as_its_value() {
    let field = example("menu.json", "key field of a keybinding prompt");
    let affordances = json!([
        {"id": "bn:submit:field", "label": "Set", "keys": [], "enabled": true, "expect": "boundary",
            "args": {"text": {"field_id": "field:key"}}}]);
    let (mut app, h) = open(screen(field, affordances));
    assert!(
        lines(&render(&mut app))
            .join("\n")
            .contains("New key for Move East")
    );
    press(&mut app, KeyCode::Tab);
    press(&mut app, KeyCode::Enter);
    press(&mut app, KeyCode::Char('x'));
    assert_eq!(
        operations(&h),
        [json!({"kind": "invoke", "affordance_id": "bn:submit:field", "args": {"text": "x"}})]
    );
}

#[test]
fn a_text_takes_the_tone_the_markup_colours_and_wraps_at_the_width() {
    let mut owed = example("trade.json", "trade balance in the trader's favour");
    let (mut app, _) = open(screen(owed.clone(), json!([])));
    assert_eq!(fg(&render(&mut app), "You'll owe"), Color::Red, "bad tone");
    owed["tone"] = json!(null);
    owed["markup"] = json!(format!(
        "<color_c_light_green>ok</color> {}",
        "word ".repeat(30)
    ));
    let (mut app, _) = open(screen(owed, json!([])));
    let t = render(&mut app);
    assert_eq!(fg(&t, "ok"), rgb("c_light_green"));
    let rows = lines(&t).iter().filter(|l| l.contains("word")).count();
    assert_eq!(rows, 2, "150 columns of words over a 78-column text");
}

#[test]
fn a_text_taller_than_its_area_scrolls_and_stops_at_its_end() {
    let mut body = example("trade.json", "trade balance in the trader's favour");
    body["markup"] = json!(
        (1..=30)
            .map(|n| format!("line {n}"))
            .collect::<Vec<_>>()
            .join("\n")
    );
    let (mut app, h) = open(screen(body, json!([])));
    assert!(at(&render(&mut app), "line 1").1 < 3);
    for _ in 0..5 {
        press(&mut app, KeyCode::PageDown);
    }
    let text = lines(&render(&mut app));
    assert!(text[HEIGHT as usize - 2].contains("line 30"), "{text:#?}");
    assert!(!text.iter().any(|l| l.contains("line 1 ")));
    assert!(submits(&h).is_empty(), "scrolling is the client's");
}

#[test]
fn a_grid_draws_its_cells_and_the_focus_moves_between_them_before_one_is_chosen() {
    let panel = example(
        "vehicle.json",
        "vehicle install list with the highlighted part's requirements",
    );
    let mut grid = panel["child"]["children"][0].clone();
    grid["cells"][0]["id"] = json!("cell:o");
    grid["cells"][1]["id"] = json!("cell:h");
    grid["cells"][1]["affordance"] = json!("pick");
    grid["cells"][1]["label"] = json!("Hood");
    assert_eq!(grid["kind"], "grid");
    let (mut app, h) = open(screen(grid, json!([choose("pick", "vehicle:grid")])));
    let t = render(&mut app);
    let (hx, hy) = at(&t, "H");
    let (ox, oy) = at(&t, "o");
    assert_eq!((hx, oy), (ox, hy + 1), "the cell at y 1 is one row down");
    assert!(
        modifier(&t, "o").contains(Modifier::REVERSED),
        "the engine's cursor"
    );
    press(&mut app, KeyCode::Up);
    press(&mut app, KeyCode::Up);
    let t = render(&mut app);
    assert!(modifier(&t, "H").contains(Modifier::REVERSED));
    assert!(lines(&t).join("\n").contains("Hood"), "the cell's label");
    assert!(submits(&h).is_empty(), "moving the focus sends nothing");
    press(&mut app, KeyCode::Enter);
    assert_eq!(
        operations(&h),
        [json!({"kind": "invoke", "affordance_id": "pick", "args": {"row": "cell:h"}})]
    );
}

#[test]
fn a_map_component_draws_the_overmap_cells_and_what_the_engine_says_of_the_cursor() {
    let map = example("target.json", "overmap map with the cursor described");
    let (mut app, _) = open(screen(map, json!([])));
    let t = render(&mut app);
    let text = lines(&t).join("\n");
    assert!(text.contains("│F forest") && text.contains("forest"));
    assert_eq!(fg(&t, "F"), Color::Rgb(40, 160, 60), "curses colour 2");
}

#[test]
fn a_canvas_is_drawn_in_the_colours_the_engine_drew_it_with_and_its_keys_are_the_screens() {
    let mut game = example("canvas.json", "timed mini-game window with color runs");
    game["role"] = json!("screen");
    let (mut app, h) = open(game);
    let t = render(&mut app);
    assert!(lines(&t).join("\n").contains("S N A K E"));
    assert_eq!(fg(&t, "#####"), Color::Rgb(180, 180, 180));
    assert_eq!(fg(&t, "*"), Color::Rgb(240, 90, 90));
    assert!(lines(&t).join("\n").contains("Turn left [h]"));
    press(&mut app, KeyCode::Char('h'));
    assert_eq!(
        operations(&h),
        [json!({"kind": "invoke", "affordance_id": "LEFT", "args": {}})]
    );
    assert_eq!(submits(&h)[0]["expect"], json!({"screen_id": "screen:2"}));
}

#[test]
fn the_bar_shows_keys_and_a_denial_and_a_denied_key_sends_nothing_while_any_key_runs_the_rest() {
    let body = example("trade.json", "trade balance in the trader's favour");
    let affordances = json!([
        example(
            "interaction.json",
            "affordance moving counted rows to a pane"
        ),
        example("interaction.json", "disabled affordance with its reason"),
        example("interaction.json", "press-any-key affordance"),
    ]);
    let (mut app, h) = open(screen(body, affordances));
    let t = render(&mut app);
    let text = lines(&t).join("\n");
    assert!(
        text.contains("Move [m]") && text.contains("Continue"),
        "{text}"
    );
    assert!(
        text.contains("Pick up [g] (There is nothing here.)"),
        "{text}"
    );
    assert_eq!(fg(&t, "Pick up"), Color::DarkGray);
    press(&mut app, KeyCode::Char('g'));
    let shown = lines(&render(&mut app));
    let said = shown
        .iter()
        .filter(|l| l.contains("There is nothing here."))
        .count();
    assert_eq!(said, 2, "the bar and the refusal");
    press(&mut app, KeyCode::Char('m'));
    assert!(
        submits(&h).is_empty(),
        "an affordance whose rows are not chosen is not sent"
    );
    press(&mut app, KeyCode::Char('z'));
    assert_eq!(
        operations(&h),
        [json!({"kind": "invoke", "affordance_id": "bn:any_key", "args": {}})]
    );
    assert_eq!(submits(&h)[0]["expect"], json!({"screen_id": "screen:1"}));
}

#[test]
fn a_dialog_is_drawn_over_the_screen_beneath_it() {
    let options = example("menu.json", "options page with each editor type");
    let (mut app, _) = open(options);
    let t = render(&mut app);
    let rows = lines(&t);
    let top = rows[HEIGHT as usize / 10].chars().collect::<Vec<_>>();
    assert_eq!(top[WIDTH as usize / 20], '┌');
    assert!(rows[1].contains("Options"));
    assert!(
        rows[HEIGHT as usize - 1].starts_with('└'),
        "the message log below is still there: {rows:#?}"
    );
}

#[test]
fn a_screen_without_components_stays_with_the_0_1_drawing() {
    let bare = example(
        "interaction.json",
        "screen the engine has not described by components yet",
    );
    let (mut app, h) = open(bare);
    assert!(lines(&render(&mut app)).join("\n").contains("Message log"));
    press(&mut app, KeyCode::Char('z'));
    assert!(operations(&h).iter().all(|op| op["kind"] != "invoke"));
}
