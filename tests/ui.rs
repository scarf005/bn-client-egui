use bn_client_egui::app::CELL;
use bn_client_egui::{App, FakeModel, GameModel, Pos};
use eframe::egui;
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};

type H<'a> = Harness<'a, App<FakeModel>>;

fn harness<'a>() -> H<'a> {
    Harness::builder()
        .with_size(egui::vec2(1100.0, 800.0))
        .build_ui_state(
            |ui, app: &mut App<FakeModel>| app.show(ui),
            App::new(FakeModel::default()),
        )
}

fn cell_center(h: &H, at: Pos) -> egui::Pos2 {
    let origin = h.get_by_label("Map").rect().min;
    origin + egui::vec2((at.x as f32 + 0.5) * CELL, (at.y as f32 + 0.5) * CELL)
}

fn click_cell(h: &mut H, at: Pos, button: egui::PointerButton) {
    let pos = cell_center(h, at);
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
}

#[test]
fn clicking_a_map_cell_walks_the_player_there() {
    let mut h = harness();
    let target = Pos { x: 12, y: 9 };
    click_cell(&mut h, target, egui::PointerButton::Primary);
    for _ in 0..30 {
        let before = h.state().model.player();
        h.run_steps(1);
        let after = h.state().model.player();
        let expected = if before == target {
            before
        } else {
            Pos {
                x: before.x + (target.x - before.x).signum(),
                y: before.y + (target.y - before.y).signum(),
            }
        };
        assert_eq!(
            after, expected,
            "exactly one step toward the target per tick"
        );
    }
    h.run_steps(60);
    assert_eq!(h.state().model.player(), target);
}

#[test]
fn pick_up_button_moves_item_to_inventory() {
    let mut h = harness();
    h.get_by_label("Pick up Tin can").click();
    h.run_steps(3);
    assert_eq!(h.state().model.ground(), ["Rusty knife"]);
    assert_eq!(h.state().model.inventory(), ["Flashlight", "Tin can"]);
    h.get_by_label("Drop Flashlight").click();
    h.run_steps(3);
    assert_eq!(h.state().model.inventory(), ["Tin can"]);
    assert_eq!(h.state().model.ground(), ["Rusty knife", "Flashlight"]);
}

fn drag_item(h: &mut H, item: &str, to_column: &str) {
    let from = h.get_by_label(item).rect().center();
    let to = h.get_by_label(to_column).rect().center() + egui::vec2(0.0, 60.0);
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
}

#[test]
fn dragging_ground_item_to_inventory_moves_it() {
    let mut h = harness();
    drag_item(&mut h, "Rusty knife", "Inventory");
    assert_eq!(h.state().model.inventory(), ["Flashlight", "Rusty knife"]);
    assert_eq!(h.state().model.ground(), ["Tin can"]);
}

#[test]
fn dragging_inventory_item_to_ground_moves_it() {
    let mut h = harness();
    drag_item(&mut h, "Flashlight", "Ground");
    assert_eq!(h.state().model.inventory(), [] as [String; 0]);
    assert_eq!(
        h.state().model.ground(),
        ["Rusty knife", "Tin can", "Flashlight"]
    );
}

#[test]
fn context_menu_move_here_walks_to_clicked_cell() {
    let mut h = harness();
    let target = Pos { x: 8, y: 5 };
    click_cell(&mut h, target, egui::PointerButton::Secondary);
    h.get_by_label("Look");
    h.get_by_label("Move here").click();
    h.run_steps(60);
    assert_eq!(h.state().model.player(), target);
}

#[test]
fn context_menu_look_logs_cell_description() {
    let mut h = harness();
    click_cell(&mut h, Pos { x: 3, y: 3 }, egui::PointerButton::Secondary);
    h.get_by_label("Look").click();
    h.run_steps(3);
    let desc = h.state().model.cell(Pos { x: 3, y: 3 }).desc;
    let last = h.state().model.messages().last().cloned().unwrap();
    assert_eq!(last, format!("You see {desc} at (3, 3)."));
}

#[test]
fn map_status_text_reports_player_and_hovered_cell() {
    let mut h = harness();
    h.get_by_label_contains("Player at (5, 5)");
    h.hover_at(cell_center(&h, Pos { x: 2, y: 4 }));
    h.run_steps(3);
    h.get_by_label_contains("Cursor (2, 4)");
}

#[test]
fn message_log_is_a_live_region_with_messages() {
    let mut h = harness();
    h.run_steps(2);
    let log = h.get_by_label_contains("Welcome to the prototype.");
    assert_eq!(log.accesskit_node().live(), egui::accesskit::Live::Polite);
}

#[test]
fn keyboard_moves_cursor_and_enter_travels() {
    let mut h = harness();
    h.get_by_label("Map").focus();
    h.run_steps(2);
    for _ in 0..2 {
        h.key_press(egui::Key::ArrowRight);
        h.run_steps(2);
    }
    h.key_press(egui::Key::Enter);
    h.run_steps(30);
    assert_eq!(h.state().model.player(), Pos { x: 7, y: 5 });
}

#[test]
fn no_interactive_node_lacks_a_label() {
    use egui::accesskit::Role;
    let mut h = harness();
    click_cell(&mut h, Pos { x: 8, y: 5 }, egui::PointerButton::Secondary);
    h.run_steps(2);
    let interactive = [
        Role::Button,
        Role::CheckBox,
        Role::TextInput,
        Role::Slider,
        Role::ComboBox,
        Role::Link,
        Role::MenuItem,
        Role::Tab,
    ];
    let unlabeled: Vec<_> = h
        .root()
        .children_recursive()
        .filter(|n| {
            let k = n.accesskit_node();
            let supports_action = k.data().supports_action(egui::accesskit::Action::Click)
                || k.data().supports_action(egui::accesskit::Action::Focus);
            (interactive.contains(&k.role()) || supports_action)
                && k.label().is_none_or(|l| l.trim().is_empty())
                // Text labels are named by their own text, which AccessKit stores as value.
                && !(k.role() == Role::Label && k.value().is_some_and(|v| !v.trim().is_empty()))
        })
        .map(|n| format!("{:?}", n.accesskit_node().data()))
        .collect();
    assert!(
        unlabeled.is_empty(),
        "unlabeled interactive nodes: {unlabeled:?}"
    );
}

#[test]
fn hovering_a_cell_shows_tooltip_with_its_description() {
    let mut h = harness();
    let at = Pos { x: 2, y: 4 };
    let desc = h.state().model.cell(at).desc;
    h.hover_at(cell_center(&h, at));
    h.run_steps(10);
    h.get_by_label(&desc);
}
