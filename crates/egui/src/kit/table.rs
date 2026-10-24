//! A list's rows as a table, the same for every list:
//!
//! `| pick | item | key | label | value columns | count | spare |`
//!
//! The pick column holds a drawn checkbox, the engine's mark and the favourite; the key column the row's key,
//! right-aligned so the labels line up. Columns no row uses take no room. A label takes its natural width when the
//! table has it, so the values sit right after it and the spare width goes to the empty last column; with less
//! room it is cut, and the whole text shows on hover.

use super::tokens;
use eframe::egui;
use egui_extras::{Column, TableBuilder, TableRow};

/// Side of a drawn checkbox.
pub const CHECK: f32 = 3.0 * tokens::U;

/// What each of a list's columns needs, measured from its rows; a column 0 wide is left out.
#[derive(Default)]
pub struct Widths {
    /// The checkbox, the engine's mark and the favourite.
    pub pick: f32,
    /// The item's square or glyph.
    pub item: f32,
    /// The rows' keys.
    pub key: f32,
    /// The label's longest word: the least it is given.
    pub label_min: f32,
    /// The longest label on one line.
    pub label: f32,
    /// Each value column: the widest of its caption and its values.
    pub values: Vec<f32>,
    /// The count editor of a list that picks amounts.
    pub count: f32,
}

impl Widths {
    /// The widths of the columns drawn, in order, with the label `label` wide.
    fn drawn(&self, label: f32) -> Vec<f32> {
        let lead = [self.pick, self.item, self.key]
            .into_iter()
            .filter(|w| *w > 0.0);
        let values = self.values.iter().copied().filter(|w| *w > 0.0);
        let count = Some(self.count).filter(|w| *w > 0.0);
        lead.chain([label]).chain(values).chain(count).collect()
    }

    /// The width the rows need with every label on one line, the gaps between the columns included.
    pub fn natural(&self, gap: f32) -> f32 {
        let drawn = self.drawn(self.label);
        drawn.iter().sum::<f32>() + gap * drawn.len().saturating_sub(1) as f32
    }

    /// The label's width in `room`: its natural width when that fits beside the other columns, else what they
    /// leave it, but never less than its longest word.
    pub fn label_in(&self, room: f32, gap: f32) -> f32 {
        let others = self.natural(gap) - self.label;
        (room - others).min(self.label).max(self.label_min)
    }

    /// The label's width in `room` for a list whose labels wrap: all the other columns leave it.
    pub fn wrapped_label_in(&self, room: f32, gap: f32) -> f32 {
        (room - (self.natural(gap) - self.label)).max(self.label_min)
    }
}

/// A table of the columns of `widths`, the label `label` wide and the spare room in an empty last column.
pub fn table<'a>(ui: &'a mut egui::Ui, widths: &Widths, label: f32) -> TableBuilder<'a> {
    let columns = widths.drawn(label);
    columns
        .into_iter()
        .fold(TableBuilder::new(ui), |table, w| {
            table.column(Column::exact(w).clip(true))
        })
        .column(Column::remainder())
        .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
        // A click anywhere on a row is a click on the row, not only on its label.
        .sense(egui::Sense::click())
        .animate_scrolling(false)
}

/// The cell of a column `width` wide, when the list draws it.
pub fn cell(row: &mut TableRow, width: f32, add: impl FnOnce(&mut egui::Ui)) {
    if width > 0.0 {
        row.col(add);
    }
}

/// The empty last column, so the row's highlight reaches the table's right edge.
pub fn spare(row: &mut TableRow) {
    row.col(|_| {});
}

/// The header row: each value column's caption over its values, right-aligned.
pub fn header(mut row: TableRow, widths: &Widths, captions: &[String]) {
    for w in [widths.pick, widths.item, widths.key] {
        cell(&mut row, w, |_| {});
    }
    row.col(|_| {});
    for (caption, w) in captions.iter().zip(&widths.values) {
        cell(&mut row, *w, |ui| {
            right_aligned(
                ui,
                egui::RichText::new(caption).size(tokens::CAPTION).weak(),
            );
        });
    }
    cell(&mut row, widths.count, |_| {});
    spare(&mut row);
}

/// The height of the header row.
pub fn header_height(ui: &egui::Ui) -> f32 {
    ui.fonts_mut(|f| f.row_height(&egui::FontId::proportional(tokens::CAPTION)))
}

/// A category row: its title bright on a band across the whole table (`across`, in the body's coordinates).
pub fn section(mut row: TableRow, widths: &Widths, across: egui::Rect, title: String) {
    for w in [widths.pick, widths.item, widths.key] {
        cell(&mut row, w, |_| {});
    }
    row.col(|ui| {
        let rect = egui::Rect::from_x_y_ranges(across.x_range(), ui.max_rect().y_range());
        let shown = egui::Rect::from_x_y_ranges(across.x_range(), ui.clip_rect().y_range());
        // Past the cell's own clip, across the row.
        ui.ctx()
            .layer_painter(ui.layer_id())
            .with_clip_rect(shown)
            .rect_filled(rect, 0.0, tokens::BAND);
        ui.label(
            egui::RichText::new(title)
                .size(tokens::SECTION)
                .color(tokens::TEXT),
        );
    });
}

/// A value at the right of its column, so the digits of a column line up.
pub fn right_aligned(ui: &mut egui::Ui, text: impl Into<egui::WidgetText>) {
    let layout = egui::Layout::right_to_left(egui::Align::Center);
    // Not selectable: a click on the text is a click on its row.
    ui.with_layout(layout, |ui| {
        ui.add(egui::Label::new(text).truncate().selectable(false))
    });
}

/// A row's key, right-aligned in its column so the labels after it line up.
pub fn key(ui: &mut egui::Ui, text: egui::RichText) {
    right_aligned(ui, text.font(key_font()));
}

/// The font keys are shown in.
pub fn key_font() -> egui::FontId {
    egui::FontId::monospace(tokens::CAPTION)
}

/// The least the key column takes: two characters.
pub fn key_min(ui: &egui::Ui) -> f32 {
    ui.fonts_mut(|f| f.glyph_width(&key_font(), '0')) * 2.0
}

/// A checkbox drawn as the row's state: a square, filled with a tick when `on`. The row's click changes it.
pub fn checkbox(ui: &mut egui::Ui, on: bool) {
    let (rect, _) = ui.allocate_exact_size(egui::Vec2::splat(CHECK), egui::Sense::hover());
    let painter = ui.painter();
    let ink = ui.visuals().text_color();
    if on {
        painter.rect_filled(rect, 0.0, tokens::SELECTION);
        let r = rect.shrink(CHECK / 4.0);
        let tick = [
            egui::pos2(r.left(), r.center().y),
            egui::pos2(r.left() + r.width() / 3.0, r.bottom()),
            r.right_top(),
        ];
        painter.add(egui::Shape::line(
            tick.to_vec(),
            egui::Stroke::new(2.0, ink),
        ));
    }
    painter.rect_stroke(
        rect,
        0.0,
        egui::Stroke::new(1.0, ink),
        egui::StrokeKind::Inside,
    );
}

/// Rows of a list have no button frame; the cursor row is the selection bar.
pub fn row_style(ui: &mut egui::Ui) {
    let visuals = ui.visuals_mut();
    visuals.widgets.inactive.weak_bg_fill = egui::Color32::TRANSPARENT;
    visuals.widgets.inactive.bg_stroke = egui::Stroke::NONE;
    visuals.selection.bg_fill = tokens::SELECTION;
    visuals.selection.stroke = egui::Stroke::new(1.0, egui::Color32::WHITE);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn widths() -> Widths {
        Widths {
            pick: 12.0,
            key: 16.0,
            label_min: 60.0,
            label: 200.0,
            values: vec![40.0, 0.0, 50.0],
            ..Widths::default()
        }
    }

    #[test]
    fn columns_no_row_uses_take_no_room() {
        assert_eq!(widths().drawn(200.0), vec![12.0, 16.0, 200.0, 40.0, 50.0]);
        assert_eq!(widths().natural(8.0), 318.0 + 4.0 * 8.0);
    }

    #[test]
    fn a_label_keeps_its_natural_width_when_there_is_room_and_its_longest_word_when_there_is_none()
    {
        let w = widths();
        assert_eq!(w.label_in(1000.0, 8.0), 200.0);
        assert_eq!(w.label_in(250.0, 8.0), 250.0 - 150.0);
        assert_eq!(w.label_in(100.0, 8.0), 60.0);
        assert_eq!(w.wrapped_label_in(1000.0, 8.0), 1000.0 - 150.0);
    }
}
