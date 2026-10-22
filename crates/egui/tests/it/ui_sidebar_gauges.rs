//! A limb's health and the stamina are bars filled in proportion to the engine's value, in its colour, with
//! the text cells kept for their place and the screen reader but not shown as `|||..`.
use bn_client_core::Rgb;
use bn_client_core::sidebar::{Gauge, Line, Span};
use bn_client_egui::kit::tokens;
use eframe::egui;
use egui_kittest::Harness;

const GREEN: Rgb = Rgb(0, 200, 0);

fn row(current: i64, max: i64) -> Vec<Line> {
    let label = Span {
        text: "HEAD".into(),
        color: Rgb(255, 255, 255),
        gauge: None,
    };
    let bar = Span {
        text: "||...".into(),
        color: GREEN,
        gauge: Some(Gauge { current, max }),
    };
    vec![vec![label, bar]]
}

/// The rects the sidebar painted: the gauge's track and anything inside it.
fn painted(lines: Vec<Line>) -> Vec<egui::epaint::RectShape> {
    let mut h = Harness::builder()
        .with_size(egui::vec2(400.0, 100.0))
        .build_ui(move |ui| bn_client_egui::sidebar::show(ui, &lines));
    h.run_steps(2);
    h.output()
        .shapes
        .iter()
        .filter_map(|c| match &c.shape {
            egui::Shape::Rect(r) => Some(r.clone()),
            _ => None,
        })
        .collect()
}

/// The gauge's track: a thin, wide rect.
fn track(rects: &[egui::epaint::RectShape]) -> egui::epaint::RectShape {
    rects
        .iter()
        .find(|r| {
            r.rect.height() <= 8.0
                && r.rect.width() > 20.0
                && r.fill != tokens::readable(GREEN, tokens::PANEL)
        })
        .cloned()
        .expect("the gauge's track")
}

#[test]
fn a_gauge_fills_its_track_in_proportion_in_the_values_colour() {
    let rects = painted(row(30, 100));
    let track = track(&rects);
    let fill = rects
        .iter()
        .find(|r| r.fill == tokens::readable(GREEN, tokens::PANEL))
        .expect("the filled part");
    assert!(track.rect.contains_rect(fill.rect), "{fill:?} in {track:?}");
    let share = fill.rect.width() / track.rect.width();
    assert!((share - 0.3).abs() < 0.02, "{share}");
}

#[test]
fn an_empty_gauge_shows_its_track_outlined_in_the_values_colour() {
    let rects = painted(row(0, 100));
    let track = track(&rects);
    assert_eq!(track.stroke.color, tokens::readable(GREEN, tokens::PANEL));
    assert!(
        !rects
            .iter()
            .any(|r| r.fill == tokens::readable(GREEN, tokens::PANEL))
    );
}

#[test]
fn the_bar_text_is_not_shown() {
    let lines = row(30, 100);
    let mut h = Harness::builder()
        .with_size(egui::vec2(400.0, 100.0))
        .build_ui(move |ui| bn_client_egui::sidebar::show(ui, &lines));
    h.run_steps(2);
    let shown = h
        .output()
        .shapes
        .iter()
        .filter_map(|c| match &c.shape {
            egui::Shape::Text(t) => Some(t.galley.job.clone()),
            _ => None,
        })
        .flat_map(|job| {
            job.sections
                .iter()
                .filter(|s| s.format.color != egui::Color32::TRANSPARENT)
                .map(|s| job.text[s.byte_range.start.0..s.byte_range.end.0].to_string())
                .collect::<Vec<_>>()
        })
        .collect::<String>();
    assert!(!shown.contains(['|', '.']), "{shown:?}");
}
