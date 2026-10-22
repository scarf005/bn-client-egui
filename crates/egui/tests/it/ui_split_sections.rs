//! Splits and descriptions as a player reads them over the map: rows of peer panes painted whole, the only
//! prose of a split as wide as the panes beside it, the active outline only around a pane that holds something, and the focused row's description in a pane of its own instead of a strip under the list.
use bn_client_egui::kit::tokens;
use bn_client_egui::{App, EngineModel};
use eframe::egui::{self, epaint};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use serde_json::Value;

type H<'a> = Harness<'a, App<EngineModel>>;

/// The screen named `name` in the engine's 0.2 example file `file`.
fn example(file: &str, name: &str) -> Value {
    let path = format!(
        "{}/../core/tests/fixtures/0.2.examples/{file}.json",
        env!("CARGO_MANIFEST_DIR")
    );
    let all = serde_json::from_str::<Value>(&std::fs::read_to_string(path).unwrap()).unwrap();
    all["valid"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["name"] == name)
        .unwrap()["value"]
        .clone()
}

/// `screen` over the map in a themed window of `size`, on its first settled frame.
fn shown<'a>(screen: Value, size: egui::Vec2) -> H<'a> {
    let (app, _) = crate::common::staged(crate::common::over_map(screen));
    let mut h = crate::common::themed_harness(size, app);
    h.run_steps(6);
    crate::common::settle_windows(&mut h);
    h
}

/// Every painted text with the clip rectangle it is painted in.
fn texts(shapes: &[epaint::ClippedShape]) -> Vec<(egui::Rect, &epaint::TextShape)> {
    fn walk<'a>(
        clip: egui::Rect,
        shape: &'a egui::Shape,
        out: &mut Vec<(egui::Rect, &'a epaint::TextShape)>,
    ) {
        match shape {
            egui::Shape::Text(text) => out.push((clip, text)),
            egui::Shape::Vec(shapes) => shapes.iter().for_each(|s| walk(clip, s, out)),
            _ => {}
        }
    }
    let mut out = Vec::new();
    shapes
        .iter()
        .for_each(|s| walk(s.clip_rect, &s.shape, &mut out));
    out
}

/// The rectangle of the text holding `part`, when every one of its lines is painted inside its clip.
fn whole(h: &H, part: &str) -> Option<egui::Rect> {
    let shapes = &h.output().shapes;
    texts(shapes).into_iter().find_map(|(clip, text)| {
        let rect = text.visual_bounding_rect();
        let inside = text.galley.rows.iter().all(|row| {
            clip.expand(1.0)
                .contains_rect(row.rect().translate(text.pos.to_vec2()))
        });
        (text.galley.text().contains(part) && inside).then_some(rect)
    })
}

/// The outlines drawn around panes: rectangles stroked in the selection's colour and not filled.
fn outlines(h: &H) -> Vec<egui::Rect> {
    let stroke = h.ctx.global_style().visuals.selection.stroke.color;
    fn walk(shape: &egui::Shape, stroke: egui::Color32, out: &mut Vec<egui::Rect>) {
        match shape {
            egui::Shape::Rect(r)
                if r.stroke.color == stroke && r.fill == egui::Color32::TRANSPARENT =>
            {
                out.push(r.rect)
            }
            egui::Shape::Vec(shapes) => shapes.iter().for_each(|s| walk(s, stroke, out)),
            _ => {}
        }
    }
    let mut out = Vec::new();
    h.output()
        .shapes
        .iter()
        .for_each(|s| walk(&s.shape, stroke, &mut out));
    out
}

#[test]
fn the_character_sheets_stat_rows_are_painted_whole_at_1280() {
    let h = shown(
        example("character", "character sheet"),
        egui::vec2(1280.0, 800.0),
    );
    for name in ["Strength:", "Dexterity:", "Intelligence:", "Perception:"] {
        assert!(whole(&h, name).is_some(), "{name} is not painted whole");
    }
}

/// The NPC talk screen, its dialogue `lines` longer than the example's greeting.
fn talk(lines: usize) -> Value {
    let mut screen = example("npctalk", "NPC talk screen");
    let text = &mut screen["root"]["children"][0]["child"]["markup"];
    let said = "You: \"What happened to the people who lived here before the refugees came?\"";
    *text = Value::from(
        std::iter::once(text.as_str().unwrap().to_string())
            .chain(std::iter::repeat_n(said.to_string(), lines))
            .collect::<Vec<_>>()
            .join("\n"),
    );
    screen
}

/// A dialogue longer than half the window keeps half; a short greeting wraps instead
/// (`ui_window_class::a_dialog_holding_panes_keeps_each_reply_on_one_line`).
#[test]
fn the_npc_dialogue_is_at_least_as_wide_as_the_responses_at_1280() {
    dialogue_beside_responses(6);
}

fn dialogue_beside_responses(lines: usize) {
    let h = shown(talk(lines), egui::vec2(1280.0, 800.0));
    let window = h
        .query_all_by_role(egui::accesskit::Role::Window)
        .find(|w| {
            w.accesskit_node()
                .label()
                .is_some_and(|l| l.contains("Daniel Arevalo"))
        })
        .unwrap()
        .rect();
    let replies = h.get_by_label("Your response:").rect();
    assert!(
        replies.left() >= window.center().x - 8.0,
        "the responses start at {} in {window:?}: the dialogue is narrower",
        replies.left()
    );
}

#[test]
fn the_active_outline_is_drawn_only_around_a_pane_that_holds_something() {
    let h = shown(
        example("aim", "advanced inventory screen"),
        egui::vec2(1280.0, 800.0),
    );
    let left = h.get_by_label("Left pane").rect();
    let drawn = outlines(&h);
    assert_eq!(drawn.len(), 1, "{drawn:?}");
    assert!(drawn[0].contains_rect(left), "{drawn:?} around {left:?}");
    // The active pane's outline frames its whole slot, not a strip of it.
    let rows = h
        .query_all_by_label("Inventory")
        .map(|n| n.rect())
        .min_by(|a, b| a.left().total_cmp(&b.left()))
        .unwrap();
    assert!(
        drawn[0].bottom() >= rows.bottom(),
        "{drawn:?} over {rows:?}"
    );
}

/// The rock's description in the drop selector at `size`: where it is painted, its lines cut by its clip and
/// how many it shows.
struct Description {
    rect: egui::Rect,
    cut: usize,
    shown: usize,
}

fn description(h: &H) -> Description {
    let shapes = &h.output().shapes;
    let (clip, text) = texts(shapes)
        .into_iter()
        .find(|(_, t)| t.galley.text().contains("A rock the size of a baseball"))
        .unwrap();
    let rows = text
        .galley
        .rows
        .iter()
        .map(|row| row.rect().translate(text.pos.to_vec2()))
        .collect::<Vec<_>>();
    Description {
        rect: text.visual_bounding_rect(),
        cut: rows
            .iter()
            .filter(|r| clip.intersect(**r).is_positive() && !clip.expand(1.0).contains_rect(**r))
            .count(),
        shown: rows.iter().filter(|r| clip.contains_rect(**r)).count(),
    }
}

#[test]
fn the_focused_items_description_is_beside_the_list_in_whole_lines_at_1920() {
    let h = shown(
        example("inventory", "drop selector screen"),
        egui::vec2(1920.0, 1080.0),
    );
    let text = description(&h);
    let row = h.get_by_label("rock (1)").rect();
    assert!(
        text.rect.left() >= row.right(),
        "{:?} is not beside the row {row:?}",
        text.rect
    );
    assert!(
        text.rect.top() < row.bottom() + 40.0,
        "{:?} starts under the row {row:?}",
        text.rect
    );
    assert_eq!(text.cut, 0, "a line cut");
    // The strip under the list held five lines.
    assert!(text.shown > 10, "{} lines shown", text.shown);
}

#[test]
fn the_focused_items_description_shows_whole_lines_past_the_old_strip_at_1280() {
    let h = shown(
        example("inventory", "drop selector screen"),
        egui::vec2(1280.0, 800.0),
    );
    let text = description(&h);
    assert_eq!(text.cut, 0, "a line cut");
    assert!(text.shown > 10, "{} lines shown", text.shown);
}

/// The left edge of the leftmost node labelled `label`.
fn left_of(h: &H, label: &str) -> egui::Rect {
    h.query_all_by_label(label)
        .map(|n| n.rect())
        .min_by(|a, b| a.left().total_cmp(&b.left()))
        .unwrap_or_else(|| panic!("no {label}"))
}

#[test]
fn a_panels_stats_are_facts_whose_values_share_one_column() {
    let h = shown(
        example("aim", "advanced inventory screen"),
        egui::vec2(1280.0, 800.0),
    );
    let values = ["0.0 kg", "0.00/1000.00 L", "name"].map(|v| left_of(&h, v));
    assert!(
        values
            .iter()
            .all(|v| (v.left() - values[0].left()).abs() < 0.5),
        "{values:?}"
    );
    let label = left_of(&h, "Weight");
    assert!(label.right() < values[0].left(), "{label:?} {values:?}");
}

#[test]
fn a_panels_title_sits_on_a_band_across_its_section() {
    let h = shown(
        example("aim", "advanced inventory screen"),
        egui::vec2(1280.0, 800.0),
    );
    let title = h.get_by_label("Left pane").rect();
    let rows = left_of(&h, "Inventory");
    let bands = h
        .output()
        .shapes
        .iter()
        .filter_map(|s| match &s.shape {
            egui::Shape::Rect(r) if r.fill == tokens::BAND => Some(r.rect),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert!(
        bands
            .iter()
            .any(|b| b.contains_rect(title) && b.right() >= rows.right()),
        "{title:?} on none of {bands:?}"
    );
}
