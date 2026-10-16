mod common;
use bn_client_core::transport::ScriptHandle;
use bn_client_core::{EngineModel, GameModel};
use bn_client_tui::TuiApp;
use common::{connected, fixture, session};
use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use serde_json::{Value, json};

fn engine() -> (TuiApp<EngineModel>, ScriptHandle) {
    let (t, h) = session();
    let t = t
        .on("bn.command.submit", vec![fixture("submit_result")])
        .on("bn.command.submit", vec![fixture("submit_result")])
        .on(
            "bn.world.describe",
            vec![json!({"jsonrpc":"2.0","id":0,"result":{"parts":[{"kind":"terrain","text":"Wall"}, {"kind":"terrain","text":"A zombie is here."}],"actions":[]}})],
        );
    (TuiApp::new(connected(t)), h)
}

fn render(app: &mut TuiApp<EngineModel>) -> Terminal<TestBackend> {
    let mut t = Terminal::new(TestBackend::new(100, 30)).unwrap();
    t.draw(|f| app.draw(f)).unwrap();
    t
}

fn text(t: &Terminal<TestBackend>) -> String {
    let b = t.backend().buffer();
    b.content()
        .chunks(b.area.width as usize)
        .map(|row| row.iter().map(|c| c.symbol()).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n")
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

/// The terminal cell of the avatar, which the map draws as `@`.
fn avatar(t: &Terminal<TestBackend>) -> (u16, u16) {
    let buf = t.backend().buffer();
    (0..buf.area.height)
        .flat_map(|y| (0..buf.area.width).map(move |x| (x, y)))
        .find(|&(x, y)| buf[(x, y)].symbol() == "@")
        .unwrap()
}

fn open_menu(app: &mut TuiApp<EngineModel>, h: &ScriptHandle, actions: Value) {
    for name in ["events_41_walk", "events_42_message"] {
        h.push(fixture(name));
    }
    let mut menu = fixture("events_43_menu");
    let change = &mut menu["params"]["events"][0]["changes"]["interaction"];
    change["actions"] = actions;
    let rows = (0..12)
        .map(|i| {
            json!({"id":format!("c:{i}"),"label":format!("entry {i}"),"description":"","denial":"",
            "enabled":true,"selectable":true,"selected":i == 0,"columns":[]})
        })
        .collect::<Vec<_>>();
    change["interaction"]["choices"] = json!(rows);
    h.push(menu);
    app.model.poll();
}

#[test]
fn hovering_a_map_square_asks_for_its_native_description_and_the_status_line_shows_it() {
    let (mut app, h) = engine();
    let t = render(&mut app);
    let (x, y) = avatar(&t);
    app.handle_mouse(mouse(MouseEventKind::Moved, x + 2, y));
    let asked: Vec<_> = h
        .sent()
        .into_iter()
        .filter(|m| m["method"] == "bn.world.describe")
        .collect();
    assert_eq!(asked.len(), 1, "one request per hovered square");
    let want = app.model.origin().x + app.model.player().x + 2;
    assert_eq!(asked[0]["params"]["pos"]["x"], want);
    app.model.poll();
    let t = render(&mut app);
    assert!(text(&t).contains("Wall / A zombie is here."));
    // Staying on the square asks nothing more. The view follows the avatar, so it sits where it now is.
    let (x, y) = avatar(&t);
    app.handle_mouse(mouse(MouseEventKind::Moved, x + 2, y));
    assert_eq!(
        h.sent()
            .iter()
            .filter(|m| m["method"] == "bn.world.describe")
            .count(),
        1
    );
}

#[test]
fn leaving_the_map_clears_the_hover_text() {
    let (mut app, _h) = engine();
    let t = render(&mut app);
    let (x, y) = avatar(&t);
    app.handle_mouse(mouse(MouseEventKind::Moved, x + 2, y));
    app.model.poll();
    assert!(text(&render(&mut app)).contains("A zombie is here."));
    let right = t.backend().buffer().area.width - 1;
    app.handle_mouse(mouse(MouseEventKind::Moved, right, 0));
    let shown = text(&render(&mut app));
    assert!(!shown.contains("A zombie is here."), "{shown}");
}

#[test]
fn right_click_on_a_map_square_is_the_engines_context_click() {
    let (mut app, h) = engine();
    let t = render(&mut app);
    let (x, y) = avatar(&t);
    app.handle_mouse(mouse(MouseEventKind::Down(MouseButton::Right), x + 1, y));
    let want = app.model.origin().x + app.model.player().x + 1;
    let ops = operations(&h);
    assert_eq!(ops[0]["kind"], "context");
    assert_eq!(ops[0]["pos"]["x"], want);
}

#[test]
fn the_wheel_steps_an_engine_menu_by_scroll_actions() {
    let (mut app, h) = engine();
    render(&mut app);
    let actions =
        json!([{"id":"SCROLL_UP","name":"Scroll up"},{"id":"SCROLL_DOWN","name":"Scroll down"}]);
    open_menu(&mut app, &h, actions);
    app.handle_mouse(mouse(MouseEventKind::ScrollDown, 50, 12));
    assert_eq!(
        operations(&h),
        [json!({"kind":"action","action_id":"SCROLL_DOWN"})]
    );
}

#[test]
fn the_wheel_in_a_menu_the_engine_does_not_scroll_sends_nothing_and_keeps_the_highlight() {
    let (mut app, h) = engine();
    render(&mut app);
    open_menu(&mut app, &h, json!([]));
    render(&mut app);
    app.handle_mouse(mouse(MouseEventKind::ScrollDown, 50, 12));
    app.handle_mouse(mouse(MouseEventKind::ScrollUp, 50, 12));
    assert!(operations(&h).is_empty());
    assert_eq!(app.selected_label().as_deref(), Some("entry 0"));
}

#[test]
fn the_wheel_on_the_map_with_no_menu_sends_nothing() {
    let (mut app, h) = engine();
    let t = render(&mut app);
    let (x, y) = avatar(&t);
    app.handle_mouse(mouse(MouseEventKind::ScrollDown, x, y));
    assert!(operations(&h).is_empty());
}
