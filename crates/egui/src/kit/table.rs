//! A list's rows as a table, the same for every list:
//!
//! `| pick | item | key | label | value columns | count | spare |`
//!
//! The pick column holds a drawn checkbox, the engine's mark and the favourite; the key column the row's key,
//! right-aligned so the labels line up. Columns no row uses take no room. A label takes its natural width when the
//! table has it, so the values sit right after it and the spare width goes to the empty last column; with less
//! room the widest of the label and the value columns gives way first: the label wraps between words, never below
//! its longest word, and a value column wider than the label (an id) is cut before it wraps. Beside a label of one
//! word the value columns take the room in order, each its values before the captions.

use super::tokens;
use eframe::egui;
use egui_extras::{Column, TableBuilder, TableRow};

/// Side of a drawn checkbox.
pub const CHECK: f32 = 3.0 * tokens::U;

/// What each of a list's columns needs, measured from its rows; a column 0 wide is left out.
#[derive(Clone, Default)]
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
    /// Each value column's widest value alone, which a column keeps before another column's caption is made whole;
    /// a column it names none of is as wide as `values` says.
    pub cells: Vec<f32>,
    /// The count editor of a list that picks amounts.
    pub count: f32,
}

impl Widths {
    /// The widths of the columns drawn, in order, with the label `label` wide.
    fn drawn(&self, label: f32) -> Vec<f32> {
        let lead = [self.pick, self.item, self.key]
            .into_iter()
            .filter(|w| *w > 0.0);
        // A value is right-aligned, so the room a column gap adds lands before it.
        let apart = |w: f32| w + tokens::COLUMN_GAP - tokens::ITEM_GAP;
        let values = self.values.iter().copied().filter(|w| *w > 0.0).map(apart);
        let count = Some(self.count).filter(|w| *w > 0.0).map(apart);
        lead.chain([label]).chain(values).chain(count).collect()
    }

    /// The width the rows need with every label on one line, the gaps between the columns included.
    pub fn natural(&self, gap: f32) -> f32 {
        let drawn = self.drawn(self.label);
        drawn.iter().sum::<f32>() + gap * drawn.len().saturating_sub(1) as f32
    }

    /// The width the label and the value columns are cut down to in `room` when they do not all fit, the widest
    /// first: each of them is as wide as the level or narrower, and the label no narrower than its longest word.
    /// None when they fit or the label is one word (a legend's symbol), which no value column makes room for: the
    /// value columns then take the room in order (`fitted`).
    fn level(&self, room: f32, gap: f32) -> Option<f32> {
        let values = self.values.iter().copied().filter(|w| *w > 0.0);
        // The room the label and the values share beside the other columns, the gaps and the spare last column.
        let shared = room - (self.natural(gap) - self.label - values.clone().sum::<f32>()) - gap;
        let filled = |level: f32| {
            level.max(self.label_min).min(self.label)
                + values.clone().map(|v| v.min(level)).sum::<f32>()
        };
        if self.label_min >= self.label || self.label + values.clone().sum::<f32>() <= shared {
            return None;
        }
        // Past the widest of their bounds that fits, the columns not at their bound yet widen with the level.
        let mut bounds = values
            .clone()
            .chain([self.label_min, self.label])
            .collect::<Vec<_>>();
        bounds.sort_by(f32::total_cmp);
        let below = (bounds.into_iter().rev())
            .find(|b| filled(*b) <= shared)
            .unwrap_or(0.0);
        let label = usize::from(self.label_min <= below && below < self.label);
        let widening = values.clone().filter(|v| *v > below).count() + label;
        Some(below + (shared - filled(below)).max(0.0) / widening.max(1) as f32)
    }

    /// The label's width in `room`: its natural width when that fits beside the other columns, else the level the
    /// value columns are cut down to beside it (`level`), but no less than its longest word while the lead columns
    /// leave that.
    pub fn label_in(&self, room: f32, gap: f32) -> f32 {
        let lead = [self.pick, self.item, self.key]
            .into_iter()
            .filter(|w| *w > 0.0)
            .map(|w| w + gap)
            .sum::<f32>();
        (self.level(room, gap))
            .map_or(self.label, |level| {
                level.max(self.label_min).min(self.label)
            })
            .min(room - lead - gap)
            .max(0.0)
    }

    /// These widths in `room` beside a label `label` wide: each value column in order takes the room its values
    /// want, then in order the room its caption wants, neither past the level the label and the values are cut down
    /// to (`level`); a value column past the room's edge is cut at it, and one wholly past it is left out, so the
    /// scroll bar stays at the room's edge.
    pub fn fitted(mut self, room: f32, gap: f32, label: f32) -> Self {
        let level = self.level(room, gap).unwrap_or(f32::INFINITY);
        let lead = [self.pick, self.item, self.key]
            .into_iter()
            .filter(|w| *w > 0.0);
        // The spare last column stands a gap after the others.
        let mut left = room - gap - label - lead.map(|w| w + gap).sum::<f32>();
        let apart = gap + tokens::COLUMN_GAP - tokens::ITEM_GAP;
        // Each column's values and how wide it gets at most; the count editor keeps its width.
        let cells = (self.values.iter().enumerate())
            .map(|(at, w)| {
                (
                    self.cells.get(at).map_or(*w, |c| c.min(*w)).min(level),
                    level,
                )
            })
            .chain([(self.count, f32::INFINITY)])
            .collect::<Vec<_>>();
        let mut columns = (self.values.iter_mut().chain([&mut self.count]))
            .zip(cells)
            .filter(|(w, _)| **w > 0.0)
            .map(|(w, (cell, most))| {
                let wanted = w.min(most);
                *w = cell.min(left - apart).max(0.0);
                left -= *w + apart;
                (w, wanted)
            })
            .collect::<Vec<_>>();
        for (w, wanted) in &mut columns {
            let more = (*wanted - **w).min(left).max(0.0);
            **w += more;
            left -= more;
        }
        self
    }

    /// The label's width in `room` for a list without columns: all the other columns leave it.
    pub fn wrapped_label_in(&self, room: f32, gap: f32) -> f32 {
        (room - (self.natural(gap) - self.label)).max(0.0)
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
        // A click anywhere on a row is a click on the row, not only on its label; the keyboard stops at the
        // row's label only, not at each of its cells.
        .sense(egui::Sense::CLICK)
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
        // The title spans the row as its band does, not only the label's column.
        let mut over = ui.new_child(egui::UiBuilder::new().max_rect(rect));
        over.set_clip_rect(shown);
        over.add(
            egui::Label::new(
                egui::RichText::new(title)
                    .size(tokens::SECTION)
                    .color(tokens::TEXT),
            )
            .extend(),
        );
    });
}

/// A value at the right of its column, so the digits of a column line up.
pub fn right_aligned(ui: &mut egui::Ui, text: impl Into<egui::WidgetText>) {
    let layout = egui::Layout::right_to_left(egui::Align::Center);
    // Not selectable: a click on the text is a click on its row.
    ui.with_layout(layout, |ui| {
        // A value column is measured to its widest value, so a value never wraps; one cut at the room's edge
        // (`Widths::fitted`) is shortened to what fits, its whole text on hover.
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
    fn columns_no_row_uses_take_no_room_and_value_columns_take_a_column_gap() {
        assert_eq!(widths().drawn(200.0), vec![12.0, 16.0, 200.0, 48.0, 58.0]);
        assert_eq!(widths().natural(8.0), 334.0 + 4.0 * 8.0);
    }

    #[test]
    fn a_label_keeps_its_natural_width_when_there_is_room_and_else_what_narrower_columns_leave_but_its_longest_word()
     {
        let w = widths();
        assert_eq!(w.label_in(1000.0, 8.0), 200.0);
        assert_eq!(w.label_in(250.0, 8.0), 250.0 - 174.0);
        assert_eq!(w.label_in(200.0, 8.0), 60.0);
        assert_eq!(w.label_in(100.0, 8.0), 100.0 - 44.0 - 8.0);
        assert_eq!(w.wrapped_label_in(1000.0, 8.0), 1000.0 - 166.0);
    }

    #[test]
    fn value_columns_past_the_room_are_cut_at_its_edge_and_those_wholly_past_it_left_out() {
        assert_eq!(
            widths().fitted(374.0, 8.0, 200.0).values,
            vec![40.0, 0.0, 50.0]
        );
        assert_eq!(
            widths().fitted(338.0, 8.0, 200.0).values,
            vec![40.0, 0.0, 14.0]
        );
        assert_eq!(
            widths().fitted(308.0, 8.0, 200.0).values,
            vec![40.0, 0.0, 0.0]
        );
        assert_eq!(
            widths().fitted(278.0, 8.0, 200.0).values,
            vec![10.0, 0.0, 0.0]
        );
    }

    #[test]
    fn a_value_wider_than_the_label_is_cut_before_the_label_wraps_and_both_down_to_one_width() {
        let w = || Widths {
            item: 24.0,
            label_min: 60.0,
            label: 200.0,
            values: vec![300.0],
            ..Widths::default()
        };
        let fit = |room: f32| {
            let label = w().label_in(room, 8.0);
            (label, w().fitted(room, 8.0, label).values[0])
        };
        // The label and the value fit whole; the value alone gives way; both give way alike; the label keeps its
        // longest word.
        assert_eq!(fit(564.0), (200.0, 300.0));
        assert_eq!(fit(500.0), (200.0, 244.0));
        assert_eq!(fit(400.0), (172.0, 172.0));
        assert_eq!(fit(150.0), (60.0, 34.0));
    }

    #[test]
    fn a_value_narrower_than_the_label_is_whole_while_the_label_wraps() {
        let w = Widths {
            item: 24.0,
            label_min: 60.0,
            label: 200.0,
            values: vec![40.0],
            ..Widths::default()
        };
        let label = w.label_in(200.0, 8.0);
        assert_eq!(
            (label, w.fitted(200.0, 8.0, label).values[0]),
            (104.0, 40.0)
        );
    }

    #[test]
    fn a_column_keeps_its_values_before_another_columns_caption_is_made_whole() {
        let w = Widths {
            label_min: 20.0,
            label: 20.0,
            values: vec![100.0, 60.0],
            cells: vec![100.0, 20.0],
            ..Widths::default()
        };
        let room = |n: f32| 20.0 + 8.0 + n + 2.0 * (8.0 + tokens::COLUMN_GAP - tokens::ITEM_GAP);
        assert_eq!(
            w.clone().fitted(room(160.0), 8.0, 20.0).values,
            vec![100.0, 60.0]
        );
        assert_eq!(
            w.clone().fitted(room(130.0), 8.0, 20.0).values,
            vec![100.0, 30.0]
        );
        assert_eq!(w.fitted(room(120.0), 8.0, 20.0).values, vec![100.0, 20.0]);
    }

    #[test]
    fn a_value_column_wider_than_the_label_gives_another_columns_caption_room_before_the_label_wraps()
     {
        let w = Widths {
            label_min: 60.0,
            label: 120.0,
            values: vec![200.0, 60.0],
            cells: vec![200.0, 20.0],
            ..Widths::default()
        };
        let room = 120.0 + 8.0 + 230.0 + 2.0 * (8.0 + tokens::COLUMN_GAP - tokens::ITEM_GAP);
        let label = w.label_in(room, 8.0);
        assert_eq!(
            (label, w.fitted(room, 8.0, label).values),
            (120.0, vec![170.0, 60.0])
        );
    }
}
