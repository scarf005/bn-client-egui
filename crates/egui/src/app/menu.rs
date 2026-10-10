//! The engine's current interaction as a centered window.

use super::App;
use crate::a11y;
use bn_client_core::GameModel;
use eframe::egui::{self, accesskit};

impl<M: GameModel> App<M> {
    /// The engine's current interaction, drawn generically: title, message, choices, cancel.
    pub(super) fn menu(&mut self, ctx: &egui::Context) {
        let Some(menu) = self.model.menu() else {
            self.focus_menu = false;
            self.menu_ids.clear();
            self.refocus_map |= self.menu_title.take().is_some();
            return;
        };
        let entered = self.focus_menu
            | (self.menu_title.replace(menu.title.clone()).as_ref() != Some(&menu.title));
        a11y::trap_tab(ctx, &self.menu_ids);
        let mut ids = Vec::new();
        let mut focused = false;
        let mut chosen = None;
        let mut cancel = ctx.input(|i| i.key_pressed(egui::Key::Escape)) && menu.can_cancel;
        egui::Window::new(&menu.title)
            .id(a11y::dialog_id(&menu.title))
            .anchor(egui::Align2::CENTER_TOP, super::dialog_offset(ctx))
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.set_min_width(520.0);
                // Cancel sits right under the title, so it is at the same place for every dialog.
                if menu.can_cancel {
                    let button = ui.add_enabled(!self.model.busy(), egui::Button::new("Cancel"));
                    ids.extend(button.enabled().then_some(button.id));
                    cancel |= button.clicked();
                }
                if !menu.message.is_empty() {
                    ui.label(&menu.message);
                }
                let tabs = menu.tabs();
                ui.horizontal_wrapped(|ui| {
                    for tab in &tabs {
                        let button =
                            self.control(ui, tab.label.clone(), &tab.id, tab.enabled, tab.selected);
                        a11y::tab(ui.ctx(), button.id, tab.selected);
                        ids.extend(button.enabled().then_some(button.id));
                        if entered && tab.selected {
                            button.request_focus();
                            focused = button.has_focus();
                        }
                        if button.clicked() {
                            chosen = Some(tab.id.clone());
                        }
                    }
                });
                egui::ScrollArea::vertical()
                    .max_height(ctx.content_rect().height() * 0.6)
                    .show(ui, |ui| {
                        ui.with_layout(egui::Layout::top_down_justified(egui::Align::Min), |ui| {
                            for (i, choice) in menu.entries().into_iter().enumerate() {
                                let button = self.control(
                                    ui,
                                    choice.label.clone(),
                                    &choice.id,
                                    choice.enabled,
                                    false,
                                );
                                ids.extend(button.enabled().then_some(button.id));
                                if entered && i == 0 && tabs.is_empty() {
                                    button.request_focus();
                                    focused = button.has_focus();
                                }
                                ui.ctx().accesskit_node_builder(button.id, |b| {
                                    b.set_role(accesskit::Role::MenuItem)
                                });
                                if !choice.enabled && !choice.denial.is_empty() {
                                    ui.label(&choice.denial);
                                }
                                if button.clicked() {
                                    chosen = Some(choice.id.clone());
                                }
                            }
                        });
                    });
            });
        self.menu_ids = ids;
        self.focus_menu = entered && !focused;
        a11y::dialog(ctx, &menu.title, &menu.message);
        if let Some(id) = chosen {
            ctx.request_repaint();
            self.model.choose(&id);
        } else if cancel {
            self.model.cancel_menu();
        }
    }
}
