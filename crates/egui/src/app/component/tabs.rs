//! A tabs component's strip: the engine's tabs on one row attached to the pane under them, unchosen ones on the
//! panel ground and the chosen one on the pane's ground under a bar; the tabs the row has no room for are in a
//! menu at its end.

use super::*;
use bn_client_core::protocol::{Tab, Tabs};

/// The height of the bar over the chosen tab.
const BAR: f32 = 2.0;

/// The tabs, by index, that a row `room` wide shows: all when they fit, else the first ones that fit beside the
/// chosen one and the menu button `more` wide.
fn shown_tabs(widths: &[f32], chosen: Option<usize>, more: f32, room: f32) -> Vec<usize> {
    if widths.iter().sum::<f32>() <= room + 0.5 {
        return (0..widths.len()).collect();
    }
    let budget = room - more - chosen.map_or(0.0, |i| widths[i]);
    let mut used = 0.0;
    let mut shown = (0..widths.len())
        .filter(|i| Some(*i) != chosen)
        .take_while(|i| {
            used += widths[*i];
            used <= budget + 0.5
        })
        .chain(chosen)
        .collect::<Vec<_>>();
    shown.sort_unstable();
    shown
}

impl<M: GameModel> App<M> {
    /// The strip of `tabs` on one row as wide as the room left to it.
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
        let widths = labels
            .iter()
            .map(|l| width(&markup::plain(l), egui::TextStyle::Body))
            .collect::<Vec<_>>();
        let chosen = tabs.tabs.iter().position(|t| t.selected);
        let more = width(MORE, egui::TextStyle::Button);
        let shown = shown_tabs(&widths, chosen, more, ui.available_width());
        egui::Frame::NONE
            .fill(crate::kit::tokens::PANEL)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 0.0;
                    let visuals = ui.visuals_mut();
                    visuals.selection.bg_fill = crate::kit::tokens::DIALOG;
                    visuals.widgets.inactive.weak_bg_fill = crate::kit::tokens::PANEL;
                    for i in &shown {
                        self.tab(ui, tabs, select, &tabs.tabs[*i], &labels[*i], out);
                    }
                    if shown.len() < tabs.tabs.len() {
                        ui.menu_button(MORE, |ui| {
                            let hidden = (0..tabs.tabs.len()).filter(|i| !shown.contains(i));
                            for i in hidden {
                                if self.tab(ui, tabs, select, &tabs.tabs[i], &labels[i], out) {
                                    ui.close();
                                }
                            }
                        });
                    }
                });
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
        let button = self.control(ui, text, &tab.id, on, tab.selected);
        let button = match (&tab.denial, on) {
            (Some(denial), false) => button.on_disabled_hover_text(markup::plain(&denial.shown())),
            _ => button,
        };
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
    use super::shown_tabs;

    #[test]
    fn a_row_with_room_for_every_tab_shows_them_all() {
        assert_eq!(
            shown_tabs(&[10.0, 20.0, 30.0], Some(2), 5.0, 60.0),
            [0, 1, 2]
        );
    }

    #[test]
    fn a_narrow_row_keeps_the_chosen_tab_and_the_first_that_fit_beside_the_menu() {
        assert_eq!(
            shown_tabs(&[10.0, 20.0, 30.0, 40.0], Some(3), 5.0, 70.0),
            [0, 3]
        );
        assert_eq!(
            shown_tabs(&[10.0, 20.0, 30.0, 40.0], None, 5.0, 40.0),
            [0, 1]
        );
    }

    #[test]
    fn a_row_narrower_than_the_chosen_tab_still_shows_it() {
        assert_eq!(shown_tabs(&[10.0, 50.0], Some(1), 5.0, 20.0), [1]);
    }
}
