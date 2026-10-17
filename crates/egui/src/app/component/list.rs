//! A list component: its rows, columns, count editors and the description of the focused row.

use super::*;

/// Rows left below the last one shown before the next page is asked for.
const PREFETCH: usize = 20;

/// Height the description under a list may take.
const DESCRIPTION_HEIGHT: f32 = 96.0;

/// A list row's item and the square it is drawn in.
pub(super) struct RowIcon {
    pub(super) painter: egui::Painter,
    pub(super) rect: egui::Rect,
    pub(super) cell: Cell,
}

/// How the rows of a list are laid out this frame: the width of the label and of each column.
struct RowLayout<'a> {
    label: f32,
    height: f32,
    columns: &'a [f32],
}

/// The columns' captions over their values, past the label and the item's square (`lead` wide).
fn list_header(ui: &mut egui::Ui, list: &List, lead: f32, layout: &RowLayout) {
    ui.horizontal(|ui| {
        ui.allocate_space(egui::vec2(layout.label + lead, layout.height));
        for (column, w) in list.columns.iter().zip(layout.columns) {
            let caption = markup::plain(&column.label.shown());
            ui.add_sized(
                [*w, layout.height],
                egui::Label::new(egui::RichText::new(caption).weak()).truncate(),
            );
        }
    });
}

impl<M: GameModel> App<M> {
    /// The room a list has here: what the window has left, less its description and the buttons under it.
    fn list_height(ui: &egui::Ui) -> f32 {
        let spacing = ui.spacing();
        let buttons = 2.0 * (spacing.interact_size.y + spacing.item_spacing.y);
        (ui.available_height() - DESCRIPTION_HEIGHT - buttons).max(DESCRIPTION_HEIGHT)
    }

    /// The row's text with its marks: the picking mark, the engine's mark, the key, the name in its colour. The
    /// item's glyph is left out when its square beside the row draws it.
    pub(super) fn row_markup(list: &List, row: &Row, picked: bool, square: bool) -> String {
        let look = row.look.as_ref();
        let parts = [
            row.parent_id.is_some().then(|| "  ".to_string()),
            matches!(list.selection, Selection::Multi | Selection::Count)
                .then(|| if picked { "[x]" } else { "[ ]" }.to_string()),
            row.toggled
                .map(|on| if on { "[x]" } else { "[ ]" }.to_string()),
            row.mark
                .as_ref()
                .map(|m| tagged(m.color.as_deref(), &m.glyph)),
            look.filter(|_| !square)
                .map(|l| tagged(Some(&l.color), &l.glyph)),
            row.favorite.filter(|&f| f).map(|_| "*".to_string()),
            row.hotkey
                .as_ref()
                .map(|k| tagged(row.key_color.as_deref(), k)),
            Some(tagged(
                row.color.as_deref(),
                &row.markup.clone().unwrap_or_else(|| row.label.shown()),
            )),
        ];
        parts.into_iter().flatten().collect::<Vec<_>>().join(" ")
    }

    pub(super) fn list(
        &self,
        ui: &mut egui::Ui,
        screen: &Screen,
        list: &List,
        out: &mut Vec<Intent>,
    ) {
        let id = &list.head.id;
        let interaction = &self.screens.interaction;
        let rows = self.model.rows(list);
        let focus = interaction.focus();
        let cursor = focus
            .row_id
            .as_deref()
            .filter(|_| focus.component_id.as_deref() == Some(id));
        let spacing = ui.spacing().item_spacing.x;
        // The rows sit in a scroll area, which keeps room for its bar.
        let width = ui.available_width() - ui.spacing().scroll.allocated_width();
        let height = ui.spacing().interact_size.y;
        // The columns take what their content needs; the label keeps the rest, less the item's square.
        let widths = self.list_widths(ui, list, &rows);
        let squares = rows
            .iter()
            .any(|r| r.look.is_some() && self.display.tiles());
        let icon = if squares { height + spacing } else { 0.0 };
        let columns_w = widths.columns.iter().map(|w| w + spacing).sum::<f32>();
        let layout = RowLayout {
            label: (width - columns_w - icon).max(0.0),
            height,
            columns: &widths.columns,
        };
        if !list.columns.is_empty() {
            list_header(ui, list, icon, &layout);
        }
        let list_height = Self::list_height(ui);
        self.screens
            .list_heights
            .borrow_mut()
            .insert(id.clone(), list_height);
        let mut area = egui::ScrollArea::vertical()
            .id_salt(("list", id))
            .auto_shrink([false, true])
            .max_height(list_height);
        if self.screens.reveal.as_deref() == Some(id.as_str()) {
            let step = height + ui.spacing().item_spacing.y;
            area = area.vertical_scroll_offset(interaction.scroll(id) as f32 * step);
        }
        area.show_rows(ui, height, rows.len(), |ui, range| {
            for row in &rows[range.clone()] {
                ui.horizontal(|ui| self.list_row(ui, list, row, cursor, &layout, out));
            }
            if range.end + PREFETCH >= rows.len() {
                out.push(Intent::More(id.clone()));
            }
        });
        let shown = cursor.and_then(|c| rows.iter().find(|r| r.id == c));
        if let Some(row) = shown {
            self.row_description(ui, id, row);
        }
        let own = list
            .head
            .affordances
            .iter()
            .filter_map(|a| find(screen, a))
            .collect::<Vec<_>>();
        self.affordance_buttons(ui, &own, shown.copied(), out);
    }

    /// One row: its item's square, its text as a button, its count editor and its column values. A click
    /// focuses the row and marks it (a picking list) or runs the list's `activate`.
    fn list_row(
        &self,
        ui: &mut egui::Ui,
        list: &List,
        row: &Row,
        cursor: Option<&str>,
        layout: &RowLayout,
        out: &mut Vec<Intent>,
    ) {
        let id = &list.head.id;
        if row.header == Some(true) {
            let heading = markup::plain(&row.label.shown());
            ui.label(egui::RichText::new(heading).strong());
            return;
        }
        let pick = self
            .screens
            .interaction
            .picks(id)
            .iter()
            .find(|p| p.id == row.id);
        let on = row.enabled && row.selectable;
        let cell = row
            .look
            .as_ref()
            .filter(|_| self.display.tiles())
            .and_then(|look| self.model.look_cell(look));
        if let Some(cell) = &cell {
            let size = egui::Vec2::splat(layout.height);
            let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
            self.screens.icons.borrow_mut().push(RowIcon {
                painter: ui.painter_at(rect),
                rect,
                cell: cell.clone(),
            });
        }
        let marked = Self::row_markup(list, row, pick.is_some(), cell.is_some());
        // A refused row is plain dim text: the game's colours would hide that it is.
        let text = if row.enabled {
            self.line(ui, &marked)
        } else {
            egui::RichText::new(markup::plain(&marked))
                .color(ui.visuals().weak_text_color())
                .into()
        };
        let highlighted = cursor == Some(row.id.as_str()) || (cursor.is_none() && row.selected);
        let justified = egui::Layout::top_down_justified(egui::Align::Min);
        let size = egui::vec2(layout.label, layout.height);
        let button = ui
            .allocate_ui_with_layout(size, justified, |ui| {
                self.control(ui, text, &row.id, on, highlighted)
            })
            .inner;
        button.widget_info(|| {
            WidgetInfo::selected(
                WidgetType::SelectableLabel,
                button.enabled(),
                highlighted,
                markup::plain(&row.label.shown()),
            )
        });
        let denial = row.denial.shown();
        let button = match (on, denial.is_empty()) {
            (false, false) => button.on_disabled_hover_text(denial),
            _ => button,
        };
        if button.clicked() {
            out.push(Intent::Focus {
                component: id.clone(),
                row: row.id.clone(),
            });
            match (list.selection, &list.activate) {
                (Selection::Multi | Selection::Count, _) => out.push(Intent::Mark {
                    component: id.clone(),
                    row: row.id.clone(),
                }),
                (_, Some(activate)) => out.push(Intent::Run(activate.clone())),
                _ => {}
            }
        }
        if let (Selection::Count, Some(pick), Some(max)) = (list.selection, pick, row.max) {
            let mut count = pick.count.unwrap_or(max);
            let drag = ui.add(egui::DragValue::new(&mut count).range(0..=max));
            if drag.changed() {
                out.push(Intent::MarkCount {
                    component: id.clone(),
                    row: row.id.clone(),
                    count,
                });
            }
        }
        for (column, column_w) in list.columns.iter().zip(layout.columns) {
            let value = row
                .columns
                .iter()
                .find(|c| c.key.as_deref() == Some(column.key.as_str()))
                .map(|c| tagged(c.color.as_deref(), &c.value.shown()))
                .unwrap_or_default();
            ui.add_sized(
                [*column_w, layout.height],
                egui::Label::new(self.line(ui, &value)).truncate(),
            );
        }
    }

    /// The focused row's description under the list, and why it is refused when it is.
    fn row_description(&self, ui: &mut egui::Ui, list_id: &str, row: &Row) {
        let description = row.description.shown();
        if !description.is_empty() {
            egui::ScrollArea::vertical()
                .id_salt(("description", list_id))
                .max_height(DESCRIPTION_HEIGHT)
                .show(ui, |ui| ui.label(self.rich(ui, &description)));
        }
        if !row.enabled && !row.denial.shown().is_empty() {
            ui.colored_label(BAD, markup::plain(&row.denial.shown()));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(toggled: Option<bool>) -> Row {
        serde_json::from_value(serde_json::json!({
            "id": "r", "label": "Lighting", "description": "", "denial": "", "enabled": true,
            "selectable": true, "selected": false, "columns": [], "toggled": toggled
        }))
        .unwrap()
    }

    fn list() -> List {
        serde_json::from_value(serde_json::json!({
            "id": "l", "kind": "list", "columns": [], "rows": [], "total": 0, "selection": "single"
        }))
        .unwrap()
    }

    #[test]
    fn a_switch_row_shows_whether_it_is_on_and_a_plain_row_shows_nothing() {
        let text = |toggled| {
            App::<bn_client_core::EngineModel>::row_markup(&list(), &row(toggled), false, false)
        };
        assert_eq!(text(Some(true)), "[x] Lighting");
        assert_eq!(text(Some(false)), "[ ] Lighting");
        assert_eq!(text(None), "Lighting");
    }
}
