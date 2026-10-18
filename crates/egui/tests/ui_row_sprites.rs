//! A list row that carries an item look draws it in a square before its text: the sprite when the tileset has
//! one, else the glyph in its colour; ASCII keeps the glyph in the text. Rows are flat text in one column beside
//! the squares, their values right-aligned.
mod common;
use bn_client_egui::transport::ScriptedTransport;
use bn_client_egui::{App, Clock, ConnectOptions, EngineModel};
use common::fixture;
use eframe::egui;
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use serde_json::{Value, json};

type H<'a> = Harness<'a, App<EngineModel>>;

fn row(id: &str, look: Value, weight: &str) -> Value {
    json!({"id": id, "label": id, "description": "", "denial": "", "enabled": true,
        "selectable": true, "selected": false, "look": look,
        "columns": [{"key": "weight", "label": "Weight", "value": weight}]})
}

/// An inventory-like dialog on the test tileset: a row with a sprite (`mon_a`), a rock the tileset lacks and a note
/// with no item, each with its weight.
fn listed<'a>(tag: &str, tiles: bool, width: f32) -> H<'a> {
    let root = common::game_root(tag);
    let settings = root.join("settings.json");
    std::fs::write(&settings, json!({ "tiles": tiles }).to_string()).unwrap();
    let list = json!({"id": "list", "kind": "list", "total": 3, "selection": "single",
    "columns": [{"key": "weight", "label": "Weight"}],
    "rows": [
        row("bra", json!({"kind": "item", "id": "bra", "tile": "mon_a", "glyph": "[", "color": "pink"}), "0.25"),
        row("rock", json!({"kind": "item", "id": "rock", "glyph": "*", "color": "light_gray"}), "12.50"),
        row("note", Value::Null, "1.00"),
    ]});
    let screen = json!({"screen_id": "screen:9", "role": "dialog", "title": "Test", "root": list,
        "affordances": []});
    let mut subscribe = fixture("subscribe_result");
    subscribe["result"]["interaction"]["stack"] = json!([
        {"screen_id": "screen:map:e1", "role": "map", "root": null, "affordances": []},
        screen
    ]);
    let reply = vec![
        subscribe,
        fixture("snapshot_part_0"),
        fixture("snapshot_part_1"),
    ];
    let (t, _script) = ScriptedTransport::new();
    let t = t
        .on("bn.hello", vec![fixture("hello_result")])
        .on("bn.subscribe", reply.clone())
        .on("bn.subscribe", reply);
    let clock = Clock::manual();
    let model = EngineModel::open(Box::new(t), ConnectOptions::default()).with_clock(clock.clone());
    let app = App::new(model)
        .with_clock(clock)
        .with_root(root)
        .with_settings(settings);
    let mut h = Harness::builder()
        .with_size(egui::vec2(width, 800.0))
        .build_ui_state(|ui, app: &mut App<EngineModel>| app.show(ui), app);
    for _ in 0..200 {
        h.run_steps(2);
        if h.state().display().ready() || !tiles {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    h.run_steps(4);
    h
}

/// The sprites (textured meshes) and the text painted in the row's own square, left of the row at `row`.
fn beside(h: &H, row: egui::Rect) -> (usize, Vec<String>) {
    let near = |r: egui::Rect| {
        (row.min.x - 8.0..=row.min.x).contains(&r.max.x)
            && (r.center().y - row.center().y).abs() < 12.0
            && r.width() < 30.0
    };
    let (mut sprites, mut glyphs) = (0, vec![]);
    for clipped in h.output().shapes.iter().filter(|c| near(c.clip_rect)) {
        match &clipped.shape {
            egui::Shape::Mesh(mesh) if mesh.texture_id != egui::TextureId::Managed(0) => {
                sprites += 1;
            }
            egui::Shape::Text(text) => {
                glyphs.push(text.galley.text().to_string());
            }
            _ => {}
        }
    }
    (sprites, glyphs)
}

fn text_of(h: &H, label: &str) -> String {
    h.get_by_label_contains(label)
        .accesskit_node()
        .label()
        .unwrap_or_default()
        .to_string()
}

#[test]
fn a_row_with_a_sprite_draws_it_before_its_text_in_tiles_mode() {
    let h = listed("row-sprite", true, 1100.0);
    let (sprites, glyphs) = beside(&h, h.get_by_label_contains("bra").rect());
    assert_eq!(sprites, 1, "the bra's sprite");
    assert!(glyphs.is_empty(), "{glyphs:?}");
    assert_eq!(
        text_of(&h, "bra"),
        "bra",
        "the glyph is not also in the text"
    );
}

#[test]
fn a_row_the_tileset_has_no_sprite_for_draws_its_glyph_in_the_square() {
    let h = listed("row-glyph", true, 1100.0);
    let (sprites, glyphs) = beside(&h, h.get_by_label_contains("rock").rect());
    assert_eq!(sprites, 0);
    assert_eq!(glyphs, ["*"]);
}

#[test]
fn ascii_mode_keeps_the_glyph_in_the_row_text() {
    let h = listed("row-ascii", false, 1100.0);
    let (sprites, _) = beside(&h, h.get_by_label_contains("bra").rect());
    assert_eq!(sprites, 0);
    let text = h.output().shapes.iter().filter_map(|c| match &c.shape {
        egui::Shape::Text(t) => Some(t.galley.text().to_string()),
        _ => None,
    });
    assert!(
        text.into_iter().any(|t| t == "[ bra"),
        "the glyph leads the row text"
    );
}

#[test]
fn a_narrow_window_still_draws_the_sprite_beside_the_row() {
    let h = listed("row-narrow", true, 420.0);
    let (sprites, _) = beside(&h, h.get_by_label_contains("bra").rect());
    assert_eq!(sprites, 1);
}

#[test]
fn a_row_without_an_item_starts_its_text_where_the_rows_beside_squares_do() {
    let h = listed("row-column", true, 1100.0);
    let x = |label: &str| h.get_by_label_contains(label).rect().min.x;
    assert_eq!(x("note"), x("bra"));
    assert_eq!(x("note"), x("rock"));
}

#[test]
fn a_row_off_the_cursor_paints_no_box_behind_its_text() {
    let h = listed("row-flat", true, 1100.0);
    let row = h.get_by_label_contains("note").rect();
    let boxes = h
        .output()
        .shapes
        .iter()
        .filter_map(|c| match &c.shape {
            egui::Shape::Rect(r)
                if r.rect.contains(row.center()) && r.rect.height() < 2.0 * row.height() =>
            {
                Some(r.fill)
            }
            _ => None,
        })
        .filter(|fill| *fill != egui::Color32::TRANSPARENT)
        .collect::<Vec<_>>();
    assert!(boxes.is_empty(), "{boxes:?}");
}

#[test]
fn a_columns_values_end_at_one_right_edge() {
    let h = listed("row-values", true, 1100.0);
    let right = |value: &str| {
        h.output()
            .shapes
            .iter()
            .find_map(|c| match &c.shape {
                egui::Shape::Text(t) if t.galley.text() == value => {
                    Some(t.pos.x + t.galley.rect.max.x)
                }
                _ => None,
            })
            .unwrap_or_else(|| panic!("{value} painted"))
    };
    assert!((right("0.25") - right("12.50")).abs() < 1.0);
    assert!((right("1.00") - right("12.50")).abs() < 1.0);
}
