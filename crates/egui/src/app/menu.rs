//! The engine's current interaction as a window at the same place every time.

use super::App;
use crate::kit::{table, tokens};
use crate::windows::{self, WindowSpec};
use crate::{a11y, editor};
use bn_client_core::localise::{Value as Arg, tr, tr_format};
use bn_client_core::{GameModel, markup};
use eframe::egui::{self, accesskit};

/// Height of the entry description under the list.
const DESCRIPTION_HEIGHT: f32 = 96.0;

impl<M: GameModel> App<M> {
    /// The engine's current interaction, drawn generically: title, message, tabs, choices, cancel.
    pub(super) fn menu(&mut self, ctx: &egui::Context) {
        let Some(menu) = self.model.menu() else {
            self.forget_prompt();
            self.focus_menu = false;
            self.menu_ids.clear();
            self.action_ids.clear();
            self.refocus_map |= self.menu_title.take().is_some();
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
        a11y::trap_tab(
            ctx,
            self.trapped_by_menu(),
            super::menu_forwards_keys(Some(&menu)),
        );
        let mut ids = Vec::new();
        let mut action_ids = Vec::new();
        let mut focused = false;
        let mut chosen = None;
        let follows = self.menu_focus != menu.focus;
        let mut key_chosen = None;
        let mut more = false;
        let mut cancel = self.palette.is_none()
            && ctx.input(|i| i.key_pressed(egui::Key::Escape))
            && menu.can_cancel;
        let mut cursor_seen = None;
        let title = markup::plain(&menu.title);
        let frame = list_frame(ctx);
        let max_width = (ctx.content_rect().width() * 0.8).max(520.0);
        let kind = self.model.screens().last().and_then(super::screen_kind);
        let spec = WindowSpec {
            closable: menu.can_cancel && !menu.acknowledges(),
            width: Some(max_width + frame.total_margin().sum().x),
            frame: Some(frame),
            ..self.window_spec(ctx, kind.as_deref(), &title)
        };
        let window_id = spec.id;
        let shown = windows::show(ctx, spec, |ui| {
            // Cancel sits right under the title, so it is at the same place for every dialog.
            if menu.can_cancel && !menu.acknowledges() {
                let button = ui.add_enabled(self.idle(), egui::Button::new(tr("Cancel")));
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
            // What sits under the list takes room from it, so the window never outgrows the screen.
            let row = ui.spacing().interact_size.y + ui.spacing().item_spacing.y;
            let paged = menu.total > menu.choices.len();
            let info = menu.choices.iter().any(|c| !c.description.is_empty());
            let footer =
                row * usize::from(paged) as f32 + if info { DESCRIPTION_HEIGHT + row } else { 0.0 };
            let listed = area
                .max_height(super::dialog_scroll_height(ctx) - footer)
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
                        // A prompt takes the typing when it appears, even if the menu before it had
                        // the same (empty) title.
                        let new_prompt = self.prompt_focused.as_deref() != Some(&field.id);
                        if entered || new_prompt {
                            response.request_focus();
                            focused = response.has_focus();
                            self.prompt_focused = Some(field.id.clone());
                        }
                    } else {
                        self.forget_prompt();
                    }
                    // A run of rows refused for the same reason says it once.
                    let any_hotkey = menu.choices.iter().any(|c| !c.hotkey.is_empty());
                    let mut last_denial = "";
                    let entries = menu.entries();
                    for (i, choice) in entries.iter().copied().enumerate() {
                        // A list of named panes starts each under its heading, as the native screen separates them.
                        if let Some(heading) =
                            menu.heading(i.checked_sub(1).map(|p| entries[p]), choice)
                        {
                            ui.add(egui::Label::new(egui::RichText::new(heading).strong()));
                        }
                        let said =
                            std::mem::replace(&mut last_denial, &choice.denial) == choice.denial;
                        ui.horizontal(|ui| {
                            let whole_row = choice.id != "acknowledge" && choice.denial.is_empty();
                            if whole_row {
                                table::row_style(ui);
                            }
                            let text = if choice.id == "acknowledge" {
                                choice.label.clone().into()
                            } else if whole_row {
                                // Label, values and key share the row, so its highlight covers them.
                                let hint = choice
                                    .id
                                    .starts_with("action:")
                                    .then(|| {
                                        bn_client_core::action_hint(
                                            &self.model.actions(),
                                            &choice.id,
                                        )
                                    })
                                    .flatten()
                                    .filter(|_| choice.hotkey.is_empty())
                                    .map(|k| format!("  <color_dark_gray>[{k}]</color>"));
                                // The native key column: the row's key in green, blank where a row has none.
                                let key = match (choice.hotkey.as_str(), any_hotkey) {
                                    ("", true) => "  ".to_string(),
                                    ("", false) => String::new(),
                                    (k, _) => format!("<color_light_green>{k}</color> "),
                                };
                                self.rich(
                                    ui,
                                    &format!(
                                        "{key}{}{}{}",
                                        choice.label,
                                        choice.values_markup(),
                                        hint.unwrap_or_default()
                                    ),
                                )
                            } else {
                                self.rich(ui, &choice.label)
                            };
                            // Native list menus highlight the whole row; the title screen's entries,
                            // a popup's dismiss button and rows with values or a denial beside them
                            // keep their own width.
                            let layout = if whole_row {
                                egui::Layout::top_down_justified(egui::Align::Min)
                            } else {
                                *ui.layout()
                            };
                            // A refused row is plain dim text: the game's colours would hide that it is.
                            let text = if choice.enabled {
                                text
                            } else {
                                egui::RichText::new(text.text())
                                    .color(ui.visuals().weak_text_color())
                                    .into()
                            };
                            let button = ui
                                .with_layout(layout, |ui| {
                                    self.control(
                                        ui,
                                        text,
                                        &choice.id,
                                        choice.enabled,
                                        choice.selected || (whole_row && choice.highlighted),
                                    )
                                })
                                .inner;
                            let button = if choice.denial.is_empty() {
                                button
                            } else {
                                button.on_disabled_hover_text(markup::plain(&choice.denial))
                            };
                            ids.extend(button.enabled().then_some(button.id));
                            if choice.id.starts_with("action:") {
                                action_ids.push(button.id);
                                // The key the game binds to it, as the native screens print their keys.
                                if let (false, Some(key)) = (
                                    whole_row,
                                    bn_client_core::action_hint(&self.model.actions(), &choice.id),
                                ) {
                                    ui.weak(format!("[{key}]"));
                                }
                            }
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
                            let action_key =
                                bn_client_core::action_hint(&self.model.actions(), &choice.id);
                            ui.ctx().accesskit_node_builder(button.id, |b| {
                                b.set_role(accesskit::Role::MenuItem);
                                // The row's text also carries its key; its name is the label and values only.
                                if whole_row {
                                    let text = choice.values_markup();
                                    b.set_label(markup::plain(&format!("{}{text}", choice.label)));
                                }
                                // Said once on screen, but every refused row carries its reason.
                                if !choice.enabled && !choice.denial.is_empty() {
                                    b.set_description(markup::plain(&choice.denial));
                                } else if let Some(key) = &action_key {
                                    b.set_description(format!("[{key}]"));
                                }
                                if choice.id == "acknowledge" {
                                    // Screen readers keep the engine's own name for the row.
                                    b.set_label(markup::plain(&choice.label));
                                    b.set_description(tr("Space or Enter closes this popup"));
                                }
                            });
                            if choice.id != "acknowledge"
                                && (button.hovered() || (choice.selected && described.is_none()))
                            {
                                described = Some(&choice.description);
                            }
                            if !whole_row {
                                ui.label(self.rich(ui, &choice.values_markup()));
                            }
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
            // The end of a list the engine has more of fetches its next page, as the wheel or a drag gets there.
            windows::note_hidden(ui, &listed);
            let (shown, all) = (listed.inner_rect.height(), listed.content_size.y);
            more |= paged && all > shown && listed.state.offset.y + shown + 1.0 >= all;
            if menu.total > menu.choices.len() {
                let rest = ui.button(tr_format(
                    "Load more (%d of %d)",
                    &[
                        Arg::Int(menu.choices.len() as i64),
                        Arg::Int(menu.total as i64),
                    ],
                ));
                more |= rest.clicked();
            }
            // A menu that describes its entries keeps the room for the text, so nothing below the list
            // moves as the hovered entry changes.
            if menu.choices.iter().any(|c| !c.description.is_empty()) {
                ui.separator();
                egui::ScrollArea::vertical()
                    .id_salt("menu_description")
                    .max_height(DESCRIPTION_HEIGHT)
                    .show(ui, |ui| {
                        ui.set_min_height(DESCRIPTION_HEIGHT);
                        ui.set_min_width(ui.available_width());
                        if let Some(description) = described.filter(|d| !d.is_empty()) {
                            // As in the native text window, a line of two dashes is a rule.
                            let lines = description.lines().collect::<Vec<_>>();
                            for (i, part) in lines.split(|line| *line == "--").enumerate() {
                                if i > 0 {
                                    ui.separator();
                                }
                                if !part.is_empty() {
                                    ui.label(self.rich(ui, &part.join("\n")));
                                }
                            }
                        }
                    });
            }
        });
        if self.keep_window(ctx, kind.as_deref(), &shown) {
            self.model.cancel_menu();
        }
        self.menu_ids = ids;
        self.action_ids = action_ids;
        if more {
            self.model.more_choices();
            ctx.request_repaint();
        }
        // Every change is sent, and the newest text goes out again once an in-flight command is done.
        if let Some((id, text, submit)) = self.prompt_send.clone()
            && !self.model.busy()
        {
            self.prompt_send = None;
            self.prompt_sent.push(text.clone());
            self.model.fill(&id, &text, submit);
        }
        self.menu_focus = menu.focus.clone();
        self.menu_cursor = cursor_seen;
        self.focus_menu = entered && !focused;
        a11y::dialog(ctx, window_id, &title, &menu.message);
        if let (Some(field), Some(value)) = (&menu.field, key_chosen) {
            ctx.request_repaint();
            self.model.fill(&field.id, &value, true);
        } else if let Some(id) = chosen {
            ctx.request_repaint();
            self.model.choose(&id);
        } else if cancel {
            self.model.cancel_menu();
        }
    }

    /// Drops what the prompt that closed kept: the text not sent yet, its composition and its sends.
    fn forget_prompt(&mut self) {
        self.prompt = Default::default();
        self.prompt_send = None;
        self.prompt_sent.clear();
        self.prompt_composing = false;
        self.prompt_focused = None;
    }

    /// The engine's text prompt, such as a list filter: a box whose every change is sent and that Enter
    /// submits. The text is the engine's until the box is edited.
    fn text_prompt(
        &mut self,
        ui: &mut egui::Ui,
        field: &bn_client_core::model::MenuField,
    ) -> egui::Response {
        // The engine's value replaces the box only when it is news: a new prompt, or a value this box did not
        // send. The echo of an older send, or any change while input is still going out, must not undo the
        // newer text, whose cursor would then land in the wrong place.
        let value = (field.id.clone(), field.value.clone());
        if self.prompt.0 != value {
            let fresh = self.prompt.0.0 != field.id;
            let echo = self.prompt_sent.contains(&field.value);
            let sending = self.prompt_send.is_some() || self.model.busy();
            if fresh {
                self.forget_prompt();
            }
            if fresh || !(echo || sending) {
                self.prompt.1.clone_from(&field.value);
            }
            self.prompt.0 = value;
        }
        let text = &mut self.prompt.1;
        let label = markup::plain(&field.label);
        let edit = egui::TextEdit::singleline(text).desired_width(f32::INFINITY);
        let response = ui.add(edit.hint_text(label.as_str()));
        let name = label.clone();
        ui.ctx()
            .accesskit_node_builder(response.id, move |b| b.set_label(name));
        let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
        let submit = (response.lost_focus() || response.has_focus()) && enter;
        // Preedit text sits in the box while composing; only the committed text is typed.
        let composing = ui.input(|i| {
            i.events
                .iter()
                .fold(self.prompt_composing, |composing, event| match event {
                    egui::Event::Ime(egui::ImeEvent::Preedit { text, .. }) => !text.is_empty(),
                    egui::Event::Ime(egui::ImeEvent::Commit(_)) => false,
                    _ => composing,
                })
        });
        self.prompt_composing = composing;
        if (response.changed() && !composing) || submit {
            let submit = submit || self.prompt_send.as_ref().is_some_and(|s| s.2);
            self.prompt_send = Some((field.id.clone(), text.clone(), submit));
        }
        response
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
}

/// The native list's window, framed like every other.
pub(super) fn list_frame(ctx: &egui::Context) -> egui::Frame {
    egui::Frame::window(&ctx.global_style())
        .fill(tokens::DIALOG)
        .stroke(egui::Stroke::new(1.0, tokens::BORDER))
        .corner_radius(0)
}
