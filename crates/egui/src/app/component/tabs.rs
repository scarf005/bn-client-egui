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
    /// The strip of `tabs` over the page of the chosen tab.
    pub(super) fn tabs(
        &self,
        ui: &mut egui::Ui,
        screen: &Screen,
        tabs: &Tabs,
        out: &mut Vec<Intent>,
    ) {
        self.tab_strip(ui, screen, tabs, out);
        if let Some(child) = &tabs.child {
            self.component(ui, screen, child, out);
        }
    }

    /// The strip of `tabs` in rows as wide as the room left to it.
    fn tab_strip(&self, ui: &mut egui::Ui, screen: &Screen, tabs: &Tabs, out: &mut Vec<Intent>) {
        let select = tabs.select.as_deref().and_then(|id| find(screen, id));
        let labels = tabs
            .tabs
            .iter()
            .map(|t| format!("{}{}", t.label.shown(), if t.unread { " •" } else { "" }))
            .collect::<Vec<_>>();
        // A tab's key stands after its name as a dim caption.
        let width = |(l, tab): (&String, &Tab)| {
            captioned_width(
                ui,
                &markup::plain(l),
                egui::TextStyle::Body,
                tab.hotkey.as_ref(),
            )
        };
        let widths = labels.iter().zip(&tabs.tabs).map(width).collect::<Vec<_>>();
        let rows = tab_rows(&widths, ui.available_width());
        // The strip is one stop of the keyboard's walk: its chosen tab when it is on, else its first on one; Left
        // and Right switch. A strip with no tab on is not a stop, as a disabled button is not.
        let on = |t: &Tab| t.enabled && select.map_or(self.screens.passive, |a| a.enabled);
        let chosen = tabs.tabs.iter().position(|t| t.selected);
        let stop = chosen
            .filter(|i| on(&tabs.tabs[*i]))
            .or_else(|| tabs.tabs.iter().position(on));
        let keys = egui::Id::new(("tabs keyboard", &screen.screen_id, &tabs.head.id));
        // The chosen tab the strip had the keyboard on last frame: a newly chosen one takes it over.
        let keyed = ui.data(|d| d.get_temp::<Option<Keyed>>(keys)).flatten();
        let moved =
            stop.is_some_and(|i| takes_keyboard(ui.ctx(), keyed.as_ref(), &tabs.tabs[i].id));
        let mut focused = None;
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
                            let strip = Strip {
                                tabs,
                                select,
                                stop: stop == Some(i),
                                moved,
                            };
                            focused =
                                focused.or(self.tab(ui, &strip, &tabs.tabs[i], &labels[i], out));
                        }
                    });
                }
            });
        let held = focused.zip(stop).map(|(w, i)| (tabs.tabs[i].id.clone(), w));
        ui.data_mut(|d| d.insert_temp(keys, held));
    }

    /// One tab labelled `label`; a click chooses it with the strip's `select`. Its button, when that has the keyboard.
    fn tab(
        &self,
        ui: &mut egui::Ui,
        strip: &Strip,
        tab: &Tab,
        label: &str,
        out: &mut Vec<Intent>,
    ) -> Option<egui::Id> {
        let Strip { tabs, select, .. } = *strip;
        let on = tab.enabled && select.map_or(self.screens.passive, |a| a.enabled);
        let sense = match strip.stop && on {
            true => egui::Sense::click(),
            false => egui::Sense::CLICK,
        };
        let text = self.line(ui, label);
        let button = match &tab.hotkey {
            Some(key) => {
                let key = egui::RichText::new(key).small().weak();
                self.control_sensed(ui, (text, key), &tab.id, (on, tab.selected), sense)
            }
            None => self.control_sensed(ui, text, &tab.id, (on, tab.selected), sense),
        };
        let button = match (&tab.hotkey, on) {
            (Some(key), true) => button.on_hover_text(key),
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
        // A click on a tab that is off asks all the same, for the core to tell the engine's reason; its hover says
        // that reason too.
        let refused = !on && {
            let off = ui.interact(button.rect, button.id.with("off"), egui::Sense::CLICK);
            let off = match &tab.denial {
                Some(denial) => off.on_hover_text(markup::plain(&denial.shown())),
                None => off,
            };
            off.clicked()
        };
        if strip.stop && strip.moved && !button.has_focus() {
            button.request_focus();
        }
        let focused = button.has_focus().then_some(button.id);
        if focused.is_some() {
            let arrows = egui::EventFilter {
                horizontal_arrows: true,
                ..Default::default()
            };
            ui.memory_mut(|m| m.set_focus_lock_filter(button.id, arrows));
            if button.gained_focus() {
                out.push(Intent::Focus {
                    component: tabs.head.id.clone(),
                    row: tab.id.clone(),
                });
            }
        }
        if button.clicked() || refused {
            out.push(Intent::Focus {
                component: tabs.head.id.clone(),
                row: tab.id.clone(),
            });
            out.extend(select.map(|a| Intent::Run(a.id.clone())));
        }
        focused
    }
}

/// What every tab of a strip shares.
struct Strip<'a> {
    tabs: &'a Tabs,
    /// What a click on a tab runs.
    select: Option<&'a Affordance>,
    /// Whether this tab is the strip's keyboard stop.
    stop: bool,
    /// Whether the strip had the keyboard last frame on another chosen tab, so its stop takes it over.
    moved: bool,
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
