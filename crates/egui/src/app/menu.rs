//! The engine's current interaction as a window at the same place every time.

use super::App;
use crate::{a11y, editor, text};
use bn_client_core::{GameModel, markup};
use eframe::egui::{self, accesskit};

impl<M: GameModel> App<M> {
    /// The engine's current interaction, drawn generically: title, message, tabs, choices, cancel.
    pub(super) fn menu(&mut self, ctx: &egui::Context) {
        let Some(menu) = self.model.menu() else {
            self.focus_menu = false;
            self.menu_ids.clear();
            self.refocus_map |= self.menu_title.take().is_some();
            if std::mem::take(&mut self.in_popup) {
                self.ack_closed = Some(self.clock.now());
            }
            return;
        };
        // A popup follows another with the same empty title, so its text tells them apart.
        let shown = format!(
            "{}\n{}",
            menu.title,
            if menu.acknowledges() {
                &menu.message
            } else {
                ""
            }
        );
        let entered =
            self.focus_menu | (self.menu_title.replace(shown.clone()).as_ref() != Some(&shown));
        if menu.context == "CRAFTING" {
            let focused = self.crafting(ctx, &menu, entered);
            self.focus_menu = entered && !focused;
            return;
        }
        if menu.context == "ADVANCED_INVENTORY" {
            let focused = self.aim(ctx, &menu, entered);
            self.focus_menu = entered && !focused;
            return;
        }
        if menu.context == "PICKUP" {
            let focused = self.pickup(ctx, &menu, entered);
            self.focus_menu = entered && !focused;
            return;
        }
        self.in_popup = menu.acknowledges();
        a11y::trap_tab(ctx, self.trapped_by_menu());
        let mut ids = Vec::new();
        let mut focused = false;
        let mut chosen = None;
        let follows = self.menu_focus != menu.focus;
        let mut edited = None;
        let mut key_chosen = None;
        let mut more = false;
        let mut filter_clicked = false;
        let mut cancel = self.palette.is_none()
            && ctx.input(|i| i.key_pressed(egui::Key::Escape))
            && menu.can_cancel;
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
            .title_bar(!title.is_empty() && menu.banner.is_empty())
            .anchor(egui::Align2::CENTER_TOP, super::dialog_offset(ctx))
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.set_min_width(520.0);
                self.banner(ui, &menu.banner);
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
                area.max_height(super::dialog_scroll_height(ctx))
                    .scroll_source(egui::scroll_area::ScrollSource {
                        mouse_wheel: !engine_scroll,
                        ..Default::default()
                    })
                    .show(ui, |ui| {
                        // A popup's one button comes first, so it is where the last one was whatever the text.
                        if !menu.acknowledges() {
                            self.menu_message(ui, &menu);
                        }
                        if menu.field.as_ref().is_some_and(|f| f.kind == "key") {
                            key_chosen = editor::key_prompt(ui);
                        } else if let Some(field) = &menu.field {
                            let response = self.text_prompt(ui, field);
                            ids.push(response.id);
                            if entered {
                                response.request_focus();
                                focused = response.has_focus();
                            }
                        }
                        // A run of rows refused for the same reason says it once.
                        let mut last_denial = "";
                        for (i, choice) in menu.entries().into_iter().enumerate() {
                            let said = std::mem::replace(&mut last_denial, &choice.denial)
                                == choice.denial;
                            if let Some(row) = &choice.editor {
                                let usable = choice.enabled && !self.model.busy();
                                let label = markup::plain(&choice.label);
                                if let Some(value) =
                                    editor::value_editor(ui, &choice.id, &label, row, usable)
                                {
                                    edited = Some((choice.id.clone(), value));
                                }
                                if !choice.enabled && !choice.denial.is_empty() && !said {
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
                                    b.set_role(accesskit::Role::MenuItem);
                                    // Said once on screen, but every refused row carries its reason.
                                    if !choice.enabled && !choice.denial.is_empty() {
                                        b.set_description(markup::plain(&choice.denial));
                                    }
                                    if choice.id == "acknowledge" {
                                        b.set_description("Space or Enter closes this popup");
                                    }
                                });
                                if choice.id != "acknowledge"
                                    && (button.hovered()
                                        || (choice.selected && described.is_none()))
                                {
                                    described = Some(&choice.description);
                                }
                                self.columns(ui, &choice.label, &choice.columns);
                                if !choice.enabled && !choice.denial.is_empty() && !said {
                                    ui.label(self.rich(ui, &choice.denial));
                                }
                                if button.clicked() {
                                    chosen = Some(choice.id.clone());
                                }
                            });
                        }
                        if menu.acknowledges() {
                            self.menu_message(ui, &menu);
                        }
                    });
                if self.model.actions().iter().any(|a| a.id == "FILTER") && menu.field.is_none() {
                    let filter = ui.add_enabled(!self.model.busy(), egui::Button::new("Filter"));
                    ids.extend(filter.enabled().then_some(filter.id));
                    filter_clicked = filter.clicked();
                }
                if menu.total > menu.choices.len() {
                    // A character creation list is all there is to pick from, so its pages are fetched
                    // as they are missing; any other long list comes one page per click.
                    more = menu.context.starts_with("NEW_CHAR_");
                    let rest = ui.button(format!(
                        "Load more ({} of {})",
                        menu.choices.len(),
                        menu.total
                    ));
                    more |= rest.clicked();
                }
                if let Some(description) = described.filter(|d| is_prose(d)) {
                    ui.separator();
                    ui.label(self.rich(ui, description));
                }
            });
        self.menu_ids = ids;
        if filter_clicked {
            self.model.invoke_action("FILTER");
        }
        if more {
            self.model.more_choices();
            ctx.request_repaint();
        }
        // Every change is sent, and the newest text goes out again once an in-flight command is done.
        if let Some((id, text, submit)) = self.prompt_send.clone()
            && !self.model.busy()
        {
            self.prompt_send = None;
            self.model.fill(&id, &text, submit);
        }
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

    /// The engine's text prompt, such as a list filter: a box whose every change is sent and that Enter
    /// submits. The text is the engine's until the box is edited.
    fn text_prompt(
        &mut self,
        ui: &mut egui::Ui,
        field: &bn_client_core::model::MenuField,
    ) -> egui::Response {
        let (seen, text) = &mut self.prompt;
        if *seen != (field.id.clone(), field.value.clone()) {
            *seen = (field.id.clone(), field.value.clone());
            text.clone_from(&field.value);
        }
        let label = markup::plain(&field.label);
        let edit = egui::TextEdit::singleline(text).desired_width(f32::INFINITY);
        let response = ui.add(edit.hint_text(label.as_str()));
        let name = label.clone();
        ui.ctx()
            .accesskit_node_builder(response.id, move |b| b.set_label(name));
        let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
        let submit = (response.lost_focus() || response.has_focus()) && enter;
        if response.changed() || submit {
            let submit = submit || self.prompt_send.as_ref().is_some_and(|s| s.2);
            self.prompt_send = Some((field.id.clone(), text.clone(), submit));
        }
        response
    }

    /// The title art and version line: fixed-width text, so the art keeps its shape.
    fn banner(&self, ui: &mut egui::Ui, lines: &[String]) {
        let font = egui::FontId::monospace(12.0);
        ui.spacing_mut().item_spacing.y = 0.0;
        for line in lines {
            let runs = self.model.styled(line);
            ui.add(egui::Label::new(text::job(&runs, &font)).wrap_mode(egui::TextWrapMode::Extend));
        }
        ui.spacing_mut().item_spacing.y = 4.0;
    }

    fn menu_message(&self, ui: &mut egui::Ui, menu: &bn_client_core::Menu) {
        if menu.screen {
            // A composed screen: columns line up only in a fixed-width font.
            ui.add(
                egui::Label::new(egui::RichText::new(&menu.message).monospace())
                    .wrap_mode(egui::TextWrapMode::Extend),
            );
        } else if !menu.message.is_empty() {
            ui.label(self.rich(ui, &menu.message));
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

/// A description the player can read; the engine fills it with the bound action's name (`NO`,
/// `QUIT`, `ANY_INPUT`) when an entry has no text of its own.
fn is_prose(description: &str) -> bool {
    description
        .chars()
        .any(|c| !(c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_'))
}
