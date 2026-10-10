//! The engine's current interaction as a window at the same place every time.

use super::App;
use crate::{a11y, editor};
use bn_client_core::{GameModel, markup};
use eframe::egui::{self, accesskit};

impl<M: GameModel> App<M> {
    /// The engine's current interaction, drawn generically: title, message, tabs, choices, cancel.
    pub(super) fn menu(&mut self, ctx: &egui::Context) {
        let Some(menu) = self.model.menu() else {
            self.focus_menu = false;
            self.menu_ids.clear();
            self.refocus_map |= self.menu_title.take().is_some();
            return;
        };
        let entered = self.focus_menu
            | (self.menu_title.replace(menu.title.clone()).as_ref() != Some(&menu.title));
        if menu.context == "CRAFTING" {
            let focused = self.crafting(ctx, &menu, entered);
            self.focus_menu = entered && !focused;
            return;
        }
        if menu.context == "PICKUP" {
            let focused = self.pickup(ctx, &menu, entered);
            self.focus_menu = entered && !focused;
            return;
        }
        a11y::trap_tab(ctx, &self.menu_ids);
        let mut ids = Vec::new();
        let mut focused = false;
        let mut chosen = None;
        let follows = self.menu_focus != menu.focus;
        let mut edited = None;
        let mut key_chosen = None;
        let mut cancel = ctx.input(|i| i.key_pressed(egui::Key::Escape)) && menu.can_cancel;
        // The wheel moves the engine's cursor by its own steps, as in a native list; the list only
        // follows the cursor, so one wheel does not scroll twice.
        let engine_scroll = self.model.can_scroll_menu();
        if engine_scroll {
            // One engine step per wheel event, not per smoothed frame.
            let steps = ctx.input(|i| {
                i.raw
                    .events
                    .iter()
                    .filter_map(|e| match e {
                        egui::Event::MouseWheel { delta, .. } if delta.y != 0.0 => {
                            Some(delta.y > 0.0)
                        }
                        _ => None,
                    })
                    .collect::<Vec<_>>()
            });
            for up in steps {
                if self.model.scroll_menu(up) {
                    ctx.request_repaint();
                }
            }
        }
        let mut cursor_seen = None;
        let title = markup::plain(&menu.title);
        egui::Window::new(&title)
            .id(a11y::dialog_id(&title))
            .title_bar(!title.is_empty())
            .anchor(egui::Align2::CENTER_TOP, super::dialog_offset(ctx))
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.set_min_width(520.0);
                // Cancel sits right under the title, so it is at the same place for every dialog.
                if menu.can_cancel && !menu.acknowledges() {
                    let button = ui.add_enabled(!self.model.busy(), egui::Button::new("Cancel"));
                    ids.extend(button.enabled().then_some(button.id));
                    cancel |= button.clicked();
                }
                let tabs = menu.tabs();
                ui.horizontal_wrapped(|ui| {
                    for tab in &tabs {
                        let text = self.rich(ui, &tab.label);
                        let button = self.control(ui, text, &tab.id, tab.enabled, tab.selected);
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
                // The entry under the pointer explains itself, else the engine's selected one.
                let mut described = None;
                // A composed screen is as wide as the engine's terminal: it scrolls sideways too.
                let area = if menu.screen {
                    egui::ScrollArea::both()
                } else {
                    egui::ScrollArea::vertical()
                };
                area.max_height(ctx.content_rect().height() * 0.6)
                    .scroll_source(egui::scroll_area::ScrollSource {
                        mouse_wheel: !engine_scroll,
                        ..Default::default()
                    })
                    .show(ui, |ui| {
                        if menu.screen {
                            // A composed screen: columns line up only in a fixed-width font.
                            ui.add(
                                egui::Label::new(egui::RichText::new(&menu.message).monospace())
                                    .wrap_mode(egui::TextWrapMode::Extend),
                            );
                        } else if !menu.message.is_empty() {
                            ui.label(self.rich(ui, &menu.message));
                        }
                        if menu.field.as_ref().is_some_and(|f| f.kind == "key") {
                            key_chosen = editor::key_prompt(ui);
                        }
                        for (i, choice) in menu.entries().into_iter().enumerate() {
                            if let Some(row) = &choice.editor {
                                let usable = choice.enabled && !self.model.busy();
                                let label = markup::plain(&choice.label);
                                if let Some(value) =
                                    editor::value_editor(ui, &choice.id, &label, row, usable)
                                {
                                    edited = Some((choice.id.clone(), value));
                                }
                                if !choice.enabled && !choice.denial.is_empty() {
                                    ui.label(self.rich(ui, &choice.denial));
                                }
                                continue;
                            }
                            ui.horizontal(|ui| {
                                let text = self.rich(ui, &choice.label);
                                let button = self.control(
                                    ui,
                                    text,
                                    &choice.id,
                                    choice.enabled,
                                    choice.selected,
                                );
                                ids.extend(button.enabled().then_some(button.id));
                                if choice.selected {
                                    cursor_seen = Some(choice.id.clone());
                                    if self.menu_cursor.as_deref() != Some(&choice.id) {
                                        button.scroll_to_me(None);
                                    }
                                }
                                // The keyboard focus follows the engine's own cursor.
                                let at_cursor = menu.focus.as_deref() == Some(choice.id.as_str());
                                if (entered && i == 0 && tabs.is_empty() && menu.focus.is_none())
                                    || (at_cursor && (entered || follows))
                                {
                                    button.request_focus();
                                    focused = button.has_focus();
                                }
                                ui.ctx().accesskit_node_builder(button.id, |b| {
                                    b.set_role(accesskit::Role::MenuItem)
                                });
                                if choice.id != "acknowledge"
                                    && (button.hovered()
                                        || (choice.selected && described.is_none()))
                                {
                                    described = Some(&choice.description);
                                }
                                self.columns(ui, &choice.label, &choice.columns);
                                if !choice.enabled && !choice.denial.is_empty() {
                                    ui.label(self.rich(ui, &choice.denial));
                                }
                                if button.clicked() {
                                    chosen = Some(choice.id.clone());
                                }
                            });
                        }
                    });
                if let Some(description) = described.filter(|d| !d.is_empty()) {
                    ui.separator();
                    ui.label(self.rich(ui, description));
                }
            });
        self.menu_ids = ids;
        self.menu_focus = menu.focus.clone();
        self.menu_cursor = cursor_seen;
        self.focus_menu = entered && !focused;
        a11y::dialog(ctx, &title, &menu.message);
        if let (Some(field), Some(value)) = (&menu.field, key_chosen) {
            ctx.request_repaint();
            self.model.fill(&field.id, &value, true);
        } else if let Some((id, value)) = edited {
            ctx.request_repaint();
            self.model.fill(&id, &value, true);
        } else if let Some(id) = chosen {
            ctx.request_repaint();
            self.model.choose(&id);
        } else if cancel {
            self.model.cancel_menu();
        }
    }

    /// An entry's labelled values; a `<name> color` column colours the value of `<name>`, and a column
    /// named like the entry itself is just its value.
    fn columns(&self, ui: &mut egui::Ui, entry: &str, columns: &[(String, String)]) {
        let tint = |name: &str| {
            columns
                .iter()
                .find(|(label, _)| label.strip_suffix(" color") == Some(name))
                .map(|(_, color)| color.as_str())
        };
        let shown = columns.iter().filter(|(label, value)| {
            // `source` names the selector column an item comes from; the engine keeps it for clients.
            !label.ends_with(" color") && label != "source" && !value.is_empty()
        });
        for (label, value) in shown {
            let value = match tint(label) {
                Some(color) => format!("<color_{color}>{value}</color>"),
                None => value.clone(),
            };
            let text = if label.is_empty() || label == entry {
                value
            } else {
                format!("{}: {value}", label.trim_end_matches(':'))
            };
            ui.label(self.rich(ui, &text));
        }
    }
}
