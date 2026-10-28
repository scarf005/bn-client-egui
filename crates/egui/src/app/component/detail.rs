//! A list whose rows are described, with the focused row's description in a pane of its own: beside the list
//! when the room holds both, else under it; as tall as the height left allows, up to the list's height or as many
//! lines as a list shows rows, so a long description scrolls instead of making its window cover the map.

use super::*;

/// The least room that holds a description beside its list.
const BESIDE: f32 = 900.0;

/// The most rows a list over its description shows before it scrolls, and the most lines of the description.
const ROWS_OVER: usize = 12;

/// The share of a window's height a list over its description takes at most, and the fewest rows it keeps.
const LIST_SHARE: f32 = 0.55;
const LEAST_ROWS: usize = 4;

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
        let focused = self.described(list);
        // Without descriptions, a refused row's reason is a line under the list.
        if rows.iter().all(|r| r.description.shown().is_empty()) {
            self.list(ui, screen, list, out);
            return self.row_detail(ui, &list.head.id, focused, 0.0);
        }
        let room = ui.available_width();
        if room < BESIDE {
            // The list keeps to its share of the window's height, so the description under it has the rest.
            let spacing = ui.spacing();
            let step = spacing.interact_size.y + spacing.item_spacing.y;
            let share = (LIST_SHARE * ui.available_height() / step).floor() as usize;
            let rows = self.screens.list_rows.get().unwrap_or(ROWS_OVER);
            self.with_list_rows(Some(rows.min(ROWS_OVER).min(share.max(LEAST_ROWS))), || {
                self.list(ui, screen, list, out)
            });
            // A rule across the pane, so the description reads as no row of the list over it.
            if focused.is_some_and(|r| !r.description.shown().is_empty()) {
                ui.separator();
            }
            return self.row_detail(ui, &list.head.id, focused, ROWS_OVER as f32);
        }
        let gap = ui.spacing().item_spacing.x;
        let detail = Extent {
            min: 0.0,
            natural: 0.0,
            grows: true,
            alone: false,
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

    /// The row of `list` described under or beside it: the focused one; while no row of it has the focus, the row a
    /// list of one choice has selected, such as the entry open in an editor.
    fn described<'a>(&'a self, list: &'a List) -> Option<&'a Row> {
        let rows = self.known_rows(list);
        let focus = self.screens.interaction.focus();
        let here = focus.component_id.as_deref() == Some(list.head.id.as_str());
        focus
            .row_id
            .as_deref()
            .filter(|_| here)
            .and_then(|id| rows.iter().find(|r| r.id == id).copied())
            .or_else(|| {
                rows.iter()
                    .find(|r| r.selected && list.selection == Selection::Single)
                    .copied()
            })
    }

    /// What the described rows of `screen`'s lists say: their refusals and descriptions.
    pub(super) fn said_by_rows(&self, screen: &Screen) -> Vec<String> {
        screen
            .components()
            .into_iter()
            .filter_map(|c| match c {
                Component::List(list) => self.described(list),
                _ => None,
            })
            .flat_map(|row| {
                [
                    (!row.enabled).then(|| row.denial.shown()),
                    Some(row.description.shown()),
                ]
            })
            .flatten()
            .filter(|t| !t.is_empty())
            .collect()
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
