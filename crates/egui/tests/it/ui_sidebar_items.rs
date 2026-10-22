use crate::common::{fixture, session};
use bn_client_egui::{App, EngineModel};
use eframe::egui;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use serde_json::json;

type H<'a> = Harness<'a, App<EngineModel>>;

/// A window whose avatar carries 60 items, far more than the sidebar shows at once.
fn crowded<'a>() -> H<'a> {
    let (t, script) = session();
    let mut h = Harness::builder()
        .with_size(egui::vec2(1100.0, 800.0))
        .build_ui_state(
            |ui, app: &mut App<EngineModel>| app.show(ui),
            App::new(EngineModel::new(Box::new(t))),
        );
    h.run_steps(3);
    let mut event = fixture("events_41_walk");
    event["params"]["events"][0]["changes"]["avatar"]["inventory"] = (0..60)
        .map(|n| {
            json!({
                "look": {"kind": "item", "id": "rock", "glyph": "*", "color": "light_gray"},
                "name": format!("item{n}"),
                "label": format!("item{n}"),
                "slot": "carried"
            })
        })
        .collect();
    script.push(event);
    h.run_steps(5);
    h
}

fn drop_rect(h: &H, n: usize) -> egui::Rect {
    h.get_by_label(&format!("Drop item{n}")).rect()
}

fn wheel(h: &mut H, at: egui::Pos2, dy: f32) {
    h.hover_at(at);
    h.run_steps(2);
    h.event(egui::Event::MouseWheel {
        unit: egui::MouseWheelUnit::Point,
        delta: egui::vec2(0.0, dy),
        phase: egui::TouchPhase::Move,
        modifiers: egui::Modifiers::NONE,
    });
    h.run_steps(3);
}

#[test]
fn the_wheel_over_the_inventory_scrolls_to_items_below_the_fold() {
    let mut h = crowded();
    let log_top = h.get_by_label("Resize message log").rect().min.y;
    let ground = h.get_by_label("Ground").rect();
    let first = drop_rect(&h, 0);
    assert!(
        drop_rect(&h, 59).min.y > log_top,
        "the last item starts below the visible list"
    );
    wheel(&mut h, first.center(), -4000.0);
    let last = drop_rect(&h, 59);
    assert!(
        last.max.y <= log_top,
        "the last item scrolled above the log: {last:?} vs {log_top}"
    );
    assert_eq!(
        h.get_by_label("Ground").rect(),
        ground,
        "only the list moved, not the headings above it"
    );
}

#[test]
fn the_wheel_over_the_map_leaves_the_sidebar_where_it_is() {
    let mut h = crowded();
    let before = drop_rect(&h, 0);
    let map = h.get_by_label("Map").rect();
    wheel(&mut h, map.center(), -4000.0);
    assert_eq!(drop_rect(&h, 0), before);
}

#[test]
fn tabbing_to_an_item_below_the_fold_scrolls_it_into_view() {
    let mut h = crowded();
    let log_top = h.get_by_label("Resize message log").rect().min.y;
    h.get_by_label("Drop item59").focus();
    h.run_steps(4);
    let rect = drop_rect(&h, 59);
    assert!(rect.max.y <= log_top && rect.min.y >= 0.0, "{rect:?}");
}

#[test]
fn tab_on_the_map_is_the_engines_and_leaves_the_sidebar_scroll_alone() {
    let mut h = crowded();
    let before = drop_rect(&h, 0);
    for _ in 0..40 {
        h.key_press(egui::Key::Tab);
        h.run_steps(2);
    }
    assert_eq!(drop_rect(&h, 0), before);
}

fn colour(name: &str) -> egui::Color32 {
    let c = bn_client_core::colors::ColorMap::default().resolve(name).0;
    egui::Color32::from_rgb(c.0, c.1, c.2)
}

fn gray() -> egui::Color32 {
    colour("light_gray")
}

/// A window on the test tileset whose avatar carries one worn item that has a sprite (`mon_a`), one that
/// has none and a rock on the ground, the worn one in good repair.
fn dressed<'a>(tag: &str, tiles: bool) -> H<'a> {
    let root = crate::common::game_root(tag);
    let settings = root.join("settings.json");
    std::fs::write(&settings, json!({ "tiles": tiles }).to_string()).unwrap();
    let (t, script) = session();
    let app = App::new(EngineModel::new(Box::new(t)))
        .with_root(root)
        .with_settings(settings);
    let mut h = Harness::builder()
        .with_size(egui::vec2(1100.0, 800.0))
        .build_ui_state(|ui, app: &mut App<EngineModel>| app.show(ui), app);
    let mut event = fixture("events_41_walk");
    let avatar = &mut event["params"]["events"][0]["changes"]["avatar"];
    avatar["inventory"] = json!([
        {
            "look": {"kind": "item", "id": "bra", "tile": "mon_a", "glyph": "[", "color": "pink"},
            "name": "||\u{a0}bra", "slot": "worn",
            "label": "<color_light_green>||</color>\u{a0}bra (worn)",
            "durability": {"symbol": "||", "color": "c_light_green"}
        },
        {
            "look": {"kind": "item", "id": "rock", "glyph": "*", "color": "light_gray"},
            "name": "rock", "slot": "carried", "label": "rock"
        }
    ]);
    avatar["ground"] = json!([]);
    script.push(event);
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

/// The shapes painted left of the row at `row` and level with it: textured meshes (sprites) and text.
fn beside(h: &H, row: egui::Rect) -> (usize, Vec<(String, egui::Color32)>) {
    let near = |r: egui::Rect| {
        (row.min.x - 40.0..=row.min.x).contains(&r.max.x)
            && (r.center().y - row.center().y).abs() < 12.0
    };
    let (mut sprites, mut glyphs) = (0, vec![]);
    for clipped in &h.output().shapes {
        match &clipped.shape {
            egui::Shape::Mesh(mesh)
                if mesh.texture_id != egui::TextureId::Managed(0) && near(mesh.calc_bounds()) =>
            {
                sprites += 1;
            }
            egui::Shape::Text(text) if near(text.visual_bounding_rect()) => {
                glyphs.push((
                    text.galley.text().to_string(),
                    text.galley.job.sections[0].format.color,
                ));
            }
            _ => {}
        }
    }
    (sprites, glyphs)
}

#[test]
fn an_item_with_a_sprite_shows_it_beside_its_row_in_tiles_mode() {
    let h = dressed("sidebar-sprites", true);
    let (sprites, glyphs) = beside(&h, h.get_by_label("||\u{a0}bra (worn)").rect());
    assert_eq!(sprites, 1, "the bra's sprite");
    assert!(glyphs.is_empty(), "{glyphs:?}");
    // The rock has no sprite in the test tileset: its glyph stands in.
    let (sprites, glyphs) = beside(&h, h.get_by_label("rock").rect());
    assert_eq!(sprites, 0);
    assert_eq!(glyphs, [("*".to_string(), gray())]);
}

#[test]
fn ascii_mode_shows_the_item_symbol_in_its_colour() {
    let h = dressed("sidebar-ascii", false);
    let (sprites, glyphs) = beside(&h, h.get_by_label("||\u{a0}bra (worn)").rect());
    assert_eq!(sprites, 0);
    assert_eq!(glyphs.len(), 1, "{glyphs:?}");
    assert_eq!(glyphs[0].0, "[");
    assert_ne!(glyphs[0].1, gray(), "pink, not gray");
}

#[test]
fn the_condition_marker_is_drawn_in_its_native_colour_and_the_rest_of_the_name_is_not() {
    let h = dressed("sidebar-marker", false);
    let green = colour("light_green");
    let runs = h
        .output()
        .shapes
        .iter()
        .filter_map(|c| match &c.shape {
            egui::Shape::Text(t) if t.galley.text().contains("bra") => Some(t),
            _ => None,
        })
        .flat_map(|t| {
            t.galley.job.sections.iter().map(|s| {
                (
                    t.galley.text()[s.byte_range.start.0..s.byte_range.end.0].to_string(),
                    s.format.color,
                )
            })
        })
        .collect::<Vec<_>>();
    assert_eq!(runs[0], ("||".to_string(), green), "{runs:?}");
    assert_ne!(runs[1].1, green, "{runs:?}");
}
