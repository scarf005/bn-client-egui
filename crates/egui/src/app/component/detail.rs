//! A list whose rows are described, with the focused row's description in a pane of its own: beside the list
//! when the room holds both, else under it; as tall as the height left allows, up to the list's height or as many
//! lines as a list shows rows, so a long description scrolls instead of making its window cover the map.

use super::*;

/// The least room that holds a description beside its list.
const BESIDE: f32 = 900.0;

/// The most rows a list over its description shows before it scrolls, and the most lines of the description.
const ROWS_OVER: usize = 12;

impl<M: GameModel> App<M> {
    /// `list` (the component `c`), and the focused row's refusal and description.
    pub(super) fn described_list(
        &self,
        ui: &mut egui::Ui,
        screen: &Screen,
        c: &Component,
        list: &List,
        out: &mut Vec<Intent>,
    ) {
        let rows = self.known_rows(list);
        let focus = self.screens.interaction.focus();
        let focused = focus
            .row_id
            .as_deref()
            .filter(|_| focus.component_id.as_deref() == Some(list.head.id.as_str()))
            .and_then(|id| rows.iter().find(|r| r.id == id).copied());
        // Without descriptions, a refused row's reason is a line under the list.
        if rows.iter().all(|r| r.description.shown().is_empty()) {
            self.list(ui, screen, list, out);
            return self.row_detail(ui, &list.head.id, focused, 0.0);
        }
        let room = ui.available_width();
        if room < BESIDE {
            let rows = self.screens.list_rows.get().unwrap_or(ROWS_OVER);
            self.with_list_rows(Some(rows.min(ROWS_OVER)), || {
                self.list(ui, screen, list, out)
            });
            return self.row_detail(ui, &list.head.id, focused, ROWS_OVER as f32);
        }
        let gap = ui.spacing().item_spacing.x;
        let detail = Extent {
            min: 0.0,
            natural: 0.0,
            grows: true,
        };
        let widths = split::detail_widths(&[self.extent(ui, screen, c, room), detail], room, gap);
        let height = ui.available_height();
        ui.horizontal_top(|ui| {
            let pane = |ui: &mut egui::Ui, width: f32, draw: &mut dyn FnMut(&mut egui::Ui)| {
                let down = egui::Layout::top_down(egui::Align::Min);
                ui.allocate_ui_with_layout(egui::vec2(width, height), down, |ui| {
                    ui.set_width(width);
                    draw(ui)
                })
                .response
                .rect
                .height()
            };
            let rows = pane(ui, widths[0], &mut |ui| self.list(ui, screen, list, out));
            // As tall as the list, and no shorter than a list over it: the description makes its window no
            // taller than a few lines of it need.
            let line = ui.text_style_height(&egui::TextStyle::Body);
            let lines = (rows / line).floor().max(ROWS_OVER as f32);
            pane(ui, widths[1], &mut |ui| {
                self.row_detail(ui, &list.head.id, focused, lines)
            });
        });
    }

    /// The focused row's description, in the height left up to `lines` lines, and why the row is refused when
    /// it is.
    fn row_detail(&self, ui: &mut egui::Ui, list_id: &str, row: Option<&Row>, lines: f32) {
        let Some(row) = row else {
            return;
        };
        if !row.enabled && !row.denial.shown().is_empty() {
            ui.colored_label(BAD, markup::plain(&row.denial.shown()));
        }
        let description = row.description.shown();
        if !description.is_empty() {
            // A whole number of lines, so the last one shown is never cut through.
            let line = ui.text_style_height(&egui::TextStyle::Body);
            egui::ScrollArea::vertical()
                .id_salt(("description", list_id))
                .max_height((ui.available_height() / line).floor().min(lines) * line)
                .auto_shrink([false, true])
                .show(ui, |ui| ui.label(self.rich(ui, &description)));
        }
    }
}
