//! A section of a window: its title on a band across its width, its body under it (Factorio's subheader frame),
//! and facts as a grid of weak labels beside their values.

use super::tokens::{BAND, WEAK};
use eframe::egui::{self, RichText};

/// The room around a section's body (Factorio's frame padding), as wide at the sides as the title's on its band,
/// so the two line up.
const BODY: egui::Margin = egui::Margin {
    left: 8,
    right: 8,
    top: 4,
    bottom: 8,
};

/// The room around a section's title on its band.
const TITLE: egui::Margin = egui::Margin::symmetric(8, 4);

/// What a section adds to its body's width.
pub fn margin_width() -> f32 {
    BODY.sum().x
}

/// A section titled `title` around `body`; an empty title has no band.
pub fn show<R>(ui: &mut egui::Ui, title: &str, body: impl FnOnce(&mut egui::Ui) -> R) -> R {
    ui.vertical(|ui| {
        let spacing = ui.spacing().item_spacing;
        ui.spacing_mut().item_spacing.y = 0.0;
        // The band is painted once the body has set the section's width.
        let band = ui.painter().add(egui::Shape::Noop);
        let title = (!title.is_empty()).then(|| {
            egui::Frame::NONE
                .inner_margin(TITLE)
                .show(ui, |ui| ui.label(RichText::new(title).strong()))
                .response
                .rect
        });
        let inner = egui::Frame::NONE
            .inner_margin(BODY)
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing = spacing;
                body(ui)
            })
            .inner;
        if let Some(title) = title {
            let rect = title.with_max_x(ui.min_rect().right());
            ui.painter()
                .set(band, egui::Shape::rect_filled(rect, 0.0, BAND));
        }
        inner
    })
    .inner
}

/// A fact: its label, and its value when it has one; a fact without a value is its label alone.
pub struct Fact {
    pub label: RichText,
    pub value: Option<RichText>,
}

/// The facts as a grid: each label weak, its value beside it, the values in one column.
pub fn facts(ui: &mut egui::Ui, id: egui::Id, facts: &[Fact]) {
    egui::Grid::new(id)
        .num_columns(2)
        .min_col_width(0.0)
        .max_col_width(ui.available_width())
        .show(ui, |ui| {
            for fact in facts {
                match &fact.value {
                    Some(value) => {
                        ui.label(fact.label.clone().color(WEAK));
                        ui.label(value.clone());
                    }
                    None => {
                        ui.label(fact.label.clone());
                    }
                }
                ui.end_row();
            }
        });
}

/// The width of the facts' grid, from the width of each fact's label and of its value (0 without one).
pub fn facts_width(ui: &egui::Ui, widths: impl Iterator<Item = (f32, f32)>) -> f32 {
    let (label, value) = widths.fold((0.0_f32, 0.0_f32), |(l, v), (a, b)| (l.max(a), v.max(b)));
    match value > 0.0 {
        true => label + ui.spacing().item_spacing.x + value,
        false => label,
    }
}
