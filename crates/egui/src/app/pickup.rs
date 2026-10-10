//! The native pickup list: one row per stack with its mark, the count box of a marked stack, the
//! predicted carry header, the cursor row's description, and the menu's own Confirm.
use super::{App, a11y};
use bn_client_core::{GameModel, Menu};
use eframe::egui::{self, accesskit};
use std::collections::HashMap;

#[derive(Default)]
pub struct Pickup {
    /// The count box of each marked stack: what the engine last chose, and the text being typed.
    counts: HashMap<String, (Option<u64>, String)>,
}

impl<M: GameModel> App<M> {
    /// Draws the menu; true when it took keyboard focus (on `entered`) this frame.
    pub(super) fn pickup(&mut self, ctx: &egui::Context, menu: &Menu, entered: bool) -> bool {
        a11y::trap_tab(ctx, &self.menu_ids);
        let mut ids = Vec::new();
        let mut focused = false;
        let mut chosen = None;
        let mut count = None;
        let mut confirm = false;
        let mut cancel = ctx.input(|i| i.key_pressed(egui::Key::Escape)) && menu.can_cancel;
        let rows = menu.choices.iter().filter(|c| !c.tab).collect::<Vec<_>>();
        let title = bn_client_core::markup::plain(&menu.title);
        egui::Window::new(&title)
            .id(a11y::dialog_id(&title))
            .default_width(560.0)
            .show(ctx, |ui| {
                if !menu.message.is_empty() {
                    ui.label(self.rich(ui, &menu.message));
                }
                let mut described = None;
                egui::ScrollArea::vertical()
                    .max_height(ctx.content_rect().height() * 0.45)
                    .show(ui, |ui| {
                        for (i, row) in rows.iter().enumerate() {
                            ui.horizontal(|ui| {
                                let mark = if row.selected { "[x]" } else { "[ ]" };
                                let label = markup_plain(&row.label);
                                let text = format!("{mark} {label}");
                                let button =
                                    self.control(ui, text, &row.id, row.enabled, row.highlighted);
                                ids.extend(button.enabled().then_some(button.id));
                                ui.ctx().accesskit_node_builder(button.id, |b| {
                                    b.set_role(accesskit::Role::MenuItem);
                                    b.set_toggled(if row.selected {
                                        accesskit::Toggled::True
                                    } else {
                                        accesskit::Toggled::False
                                    });
                                });
                                if entered && i == 0 {
                                    button.request_focus();
                                    focused = button.has_focus();
                                }
                                if button.hovered() || (row.highlighted && described.is_none()) {
                                    described = Some(&row.description);
                                }
                                if button.clicked() {
                                    chosen = Some(row.id.clone());
                                }
                                let available = row.available_count.filter(|n| *n > 1);
                                if let (true, Some(available)) = (row.selected, available) {
                                    let (seen, text) = self
                                        .pickup
                                        .counts
                                        .entry(row.id.clone())
                                        .or_insert((None, String::new()));
                                    if *seen != row.selected_count {
                                        *seen = row.selected_count;
                                        *text = row
                                            .selected_count
                                            .map_or(String::new(), |n| n.to_string());
                                    }
                                    let edit = egui::TextEdit::singleline(text).desired_width(40.0);
                                    let response = ui.add(edit);
                                    let name = format!("Count of {label}");
                                    ui.ctx()
                                        .accesskit_node_builder(response.id, |b| b.set_label(name));
                                    ids.push(response.id);
                                    ui.label(format!("/ {available}"));
                                    let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
                                    if response.lost_focus() && enter {
                                        // A count beyond the stack would only be refused by the engine.
                                        count = text
                                            .trim()
                                            .parse::<u64>()
                                            .ok()
                                            .filter(|n| (1..=available).contains(n))
                                            .map(|n| (row.id.clone(), n));
                                    }
                                }
                                if !row.enabled && !row.denial.is_empty() {
                                    ui.label(self.rich(ui, &row.denial));
                                }
                            });
                        }
                    });
                // The row under the pointer, else the native cursor row, else the first.
                let described = described.or(rows.first().map(|r| &r.description));
                if let Some(description) = described.filter(|d| !d.is_empty()) {
                    ui.separator();
                    egui::ScrollArea::vertical()
                        .id_salt("pickup_description")
                        .max_height(ctx.content_rect().height() * 0.3)
                        .show(ui, |ui| ui.label(self.rich(ui, description)));
                }
                ui.separator();
                ui.horizontal(|ui| {
                    let any = rows.iter().any(|r| r.selected);
                    let button = self.control(ui, "Confirm", "CONFIRM", any, false);
                    ids.extend(button.enabled().then_some(button.id));
                    confirm = button.clicked();
                    if menu.can_cancel {
                        let button =
                            ui.add_enabled(!self.model.busy(), egui::Button::new("Cancel"));
                        ids.extend(button.enabled().then_some(button.id));
                        cancel |= button.clicked();
                    }
                });
            });
        self.menu_ids = ids;
        a11y::dialog(ctx, &title, &menu.message);
        if let Some((id, n)) = count {
            ctx.request_repaint();
            self.model.set_count(&id, n);
        } else if confirm {
            ctx.request_repaint();
            self.model.invoke_action("CONFIRM");
        } else if let Some(id) = chosen {
            ctx.request_repaint();
            self.model.choose(&id);
        } else if cancel {
            self.model.cancel_menu();
        }
        focused
    }
}

fn markup_plain(text: &str) -> String {
    bn_client_core::markup::plain(text)
}
