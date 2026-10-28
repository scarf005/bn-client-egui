//! A tabs component's strip: the engine's tabs attached to the pane under them, unchosen ones on the panel ground
//! and the chosen one on the pane's ground under a bar; the tabs a row has no room for wrap to further rows.

use super::*;
use bn_client_core::protocol::{Tab, Tabs};

/// The height of the bar over the chosen tab.
const BAR: f32 = 2.0;

/// The tabs, by index, on each row of a strip `room` wide: in order, a new row when the next tab does not fit.
/// A tab wider than the room has a row of its own.
fn tab_rows(widths: &[f32], room: f32) -> Vec<Vec<usize>> {
    let mut rows = Vec::<Vec<usize>>::new();
    let mut used = 0.0;
    for (i, width) in widths.iter().enumerate() {
        match rows.last_mut() {
            Some(row) if used + width <= room + 0.5 => {
                row.push(i);
                used += width;
            }
            _ => {
                rows.push(vec![i]);
                used = *width;
            }
        }
    }
    rows
}

impl<M: GameModel> App<M> {
    /// The strip of `tabs` in rows as wide as the room left to it.
    pub(super) fn tab_strip(
        &self,
        ui: &mut egui::Ui,
        screen: &Screen,
        tabs: &Tabs,
        out: &mut Vec<Intent>,
    ) {
        let select = tabs.select.as_deref().and_then(|id| find(screen, id));
        let labels = tabs
            .tabs
            .iter()
            .map(|t| format!("{}{}", t.label.shown(), if t.unread { " •" } else { "" }))
            .collect::<Vec<_>>();
        let padding = 2.0 * ui.spacing().button_padding.x;
        let width = |text: &str, style: egui::TextStyle| {
            text_width(ui, text, &style.resolve(ui.style())) + padding
        };
        // A tab's key stands after its name as a dim caption.
        let key = |tab: &Tab| {
            tab.hotkey.as_ref().map_or(0.0, |k| {
                ui.spacing().icon_spacing
                    + text_width(ui, k, &egui::TextStyle::Small.resolve(ui.style()))
            })
        };
        let widths = labels
            .iter()
            .zip(&tabs.tabs)
            .map(|(l, tab)| width(&markup::plain(l), egui::TextStyle::Body) + key(tab))
            .collect::<Vec<_>>();
        let rows = tab_rows(&widths, ui.available_width());
        egui::Frame::NONE
            .fill(crate::kit::tokens::PANEL)
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                let visuals = ui.visuals_mut();
                visuals.selection.bg_fill = crate::kit::tokens::DIALOG;
                visuals.widgets.inactive.weak_bg_fill = crate::kit::tokens::PANEL;
                for row in rows {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 0.0;
                        for i in row {
                            self.tab(ui, tabs, select, &tabs.tabs[i], &labels[i], out);
                        }
                    });
                }
            });
    }

    /// One tab labelled `label`; a click chooses it with `select`. Whether it was clicked.
    fn tab(
        &self,
        ui: &mut egui::Ui,
        tabs: &Tabs,
        select: Option<&Affordance>,
        tab: &Tab,
        label: &str,
        out: &mut Vec<Intent>,
    ) -> bool {
        let on = tab.enabled && select.map_or(self.screens.passive, |a| a.enabled);
        let text = self.line(ui, label);
        let button = match &tab.hotkey {
            Some(key) => {
                let key = egui::RichText::new(key).small().weak();
                self.control(ui, (text, key), &tab.id, on, tab.selected)
            }
            None => self.control(ui, text, &tab.id, on, tab.selected),
        };
        let button = match (&tab.denial, on) {
            (Some(denial), false) => button.on_disabled_hover_text(markup::plain(&denial.shown())),
            (None, true) | (Some(_), true) => match &tab.hotkey {
                Some(key) => button.on_hover_text(key),
                None => button,
            },
            _ => button,
        };
        // Named by its name alone, its key as the shortcut.
        a11y::keyed_button(
            ui.ctx(),
            button.id,
            &markup::plain(label),
            tab.hotkey.as_deref(),
        );
        a11y::tab(ui.ctx(), button.id, tab.selected);
        if tab.selected {
            let bar =
                egui::Rect::from_min_size(button.rect.min, egui::vec2(button.rect.width(), BAR));
            ui.painter()
                .rect_filled(bar, 0.0, crate::kit::tokens::SELECTION);
        }
        if button.clicked() {
            out.push(Intent::Focus {
                component: tabs.head.id.clone(),
                row: tab.id.clone(),
            });
            out.extend(select.map(|a| Intent::Run(a.id.clone())));
        }
        button.clicked()
    }
}

#[cfg(test)]
mod tests {
    use super::tab_rows;

    #[test]
    fn a_row_with_room_for_every_tab_holds_them_all() {
        assert_eq!(tab_rows(&[10.0, 20.0, 30.0], 60.0), [[0, 1, 2]]);
    }

    #[test]
    fn a_narrow_strip_puts_the_tabs_that_do_not_fit_on_further_rows_in_order() {
        assert_eq!(
            tab_rows(&[10.0, 20.0, 30.0, 40.0, 5.0], 55.0),
            vec![vec![0, 1], vec![2], vec![3, 4]]
        );
    }

    #[test]
    fn a_tab_wider_than_the_room_has_a_row_of_its_own() {
        assert_eq!(
            tab_rows(&[10.0, 50.0, 10.0], 20.0),
            [vec![0], vec![1], vec![2]]
        );
    }
}
