//! The engine's current interaction as a window at the same place every time.

use super::App;
use crate::{a11y, editor, text};
use bn_client_core::{GameModel, markup};
use eframe::egui::{self, accesskit};

/// Height of the entry description under the list.
const DESCRIPTION_HEIGHT: f32 = 96.0;

impl<M: GameModel> App<M> {
    /// The engine's current interaction, drawn generically: title, message, tabs, choices, cancel.
    pub(super) fn menu(&mut self, ctx: &egui::Context) {
        let Some(menu) = self.model.menu() else {
            self.focus_menu = false;
            self.menu_ids.clear();
            self.action_ids.clear();
            self.refocus_map |= self.menu_title.take().is_some();
            if std::mem::take(&mut self.in_popup) {
                self.ack_closed = Some(self.clock.now());
            }
            return;
        };
        self.follow_options(&menu);
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
        if menu.context == "CONSTRUCTION" {
            let focused = self.construction(ctx, &menu, entered);
            self.focus_menu = entered && !focused;
            return;
        }
        if menu.context == "ADVANCED_INVENTORY" {
            let focused = self.aim(ctx, &menu, entered);
            self.focus_menu = entered && !focused;
            return;
        }
        if menu.context == "NPC_TRADE" && menu.panes.len() == 2 {
            let focused = self.trade(ctx, &menu, entered);
            self.focus_menu = entered && !focused;
            return;
        }
        if super::character::CONTEXTS.contains(&menu.context.as_str()) {
            let focused = self.character(ctx, &menu, entered);
            self.focus_menu = entered && !focused;
            return;
        }
        if menu.context == "INVENTORY" {
            let focused = self.inventory(ctx, &menu, entered);
            self.focus_menu = entered && !focused;
            return;
        }
        if menu.context == "PICKUP" {
            let focused = self.pickup(ctx, &menu, entered);
            self.focus_menu = entered && !focused;
            return;
        }
        self.in_popup = menu.acknowledges();
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
        let frame = list_frame(ctx);
        let max_width = (ctx.content_rect().width() * 0.8).max(520.0);
        egui::Window::new(&title)
            .id(a11y::dialog_id(&title))
            .title_bar(!title.is_empty() && menu.banner.is_empty())
            .anchor(egui::Align2::CENTER_TOP, super::dialog_offset(ctx))
            .collapsible(false)
            .resizable(false)
            // Rows fill the list, so the window is as wide as it may get from its first frame; growing
            // into it later would move its centred position.
            .default_width(max_width + frame.total_margin().sum().x)
            .frame(frame)
            .show(ctx, |ui| {
                ui.set_min_width(520.0);
                ui.set_max_width(max_width);
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
                            // A prompt takes the typing when it appears, even if the menu before it had
                            // the same (empty) title.
                            let new_prompt = self.prompt_focused.as_deref() != Some(&field.id);
                            if entered || new_prompt {
                                response.request_focus();
                                focused = response.has_focus();
                                self.prompt_focused = Some(field.id.clone());
                            }
                        } else {
                            self.prompt_focused = None;
                        }
                        // A run of rows refused for the same reason says it once.
                        let any_hotkey = menu.choices.iter().any(|c| !c.hotkey.is_empty());
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
                                let whole_row = menu.context != "MAIN_MENU"
                                    && choice.id != "acknowledge"
                                    && choice.denial.is_empty();
                                if whole_row {
                                    list_row_style(ui);
                                }
                                let text = if choice.id == "acknowledge" {
                                    dismiss_text(&choice.label).into()
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
                                            column_text(&choice.label, &choice.columns),
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
                                ids.extend(button.enabled().then_some(button.id));
                                if choice.id.starts_with("action:") {
                                    action_ids.push(button.id);
                                    // The key the game binds to it, as the native screens print their keys.
                                    if let (false, Some(key)) = (
                                        whole_row,
                                        bn_client_core::action_hint(
                                            &self.model.actions(),
                                            &choice.id,
                                        ),
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
                                        let text = column_text(&choice.label, &choice.columns);
                                        b.set_label(markup::plain(&format!(
                                            "{}{text}",
                                            choice.label
                                        )));
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
                                        b.set_description("Space or Enter closes this popup");
                                    }
                                });
                                if choice.id != "acknowledge"
                                    && (button.hovered()
                                        || (choice.selected && described.is_none()))
                                {
                                    described = Some(&choice.description);
                                }
                                if !whole_row {
                                    ui.label(
                                        self.rich(ui, &column_text(&choice.label, &choice.columns)),
                                    );
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
                if self.model.actions().iter().any(|a| a.id == "FILTER")
                    && menu.field.is_none()
                    && menu.total > FILTER_MIN_ROWS
                {
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
                // A menu that describes its entries keeps the room for the text, so nothing below the list
                // moves as the hovered entry changes.
                if menu.choices.iter().any(|c| is_prose(&c.description)) {
                    ui.separator();
                    egui::ScrollArea::vertical()
                        .id_salt("menu_description")
                        .max_height(DESCRIPTION_HEIGHT)
                        .show(ui, |ui| {
                            ui.set_min_height(DESCRIPTION_HEIGHT);
                            ui.set_min_width(ui.available_width());
                            if let Some(description) = described.filter(|d| is_prose(d)) {
                                ui.label(self.rich(ui, description));
                            }
                        });
                }
            });
        self.menu_ids = ids;
        self.action_ids = action_ids;
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
}

/// The native list's window: black, with its thin pink border.
pub(super) fn list_frame(ctx: &egui::Context) -> egui::Frame {
    egui::Frame::window(&ctx.global_style())
        .fill(egui::Color32::BLACK)
        .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(139, 58, 98)))
        .corner_radius(0)
}

/// Rows of a native list have no button frame; the cursor row is a blue bar.
pub(super) fn list_row_style(ui: &mut egui::Ui) {
    let visuals = ui.visuals_mut();
    visuals.widgets.inactive.weak_bg_fill = egui::Color32::TRANSPARENT;
    visuals.widgets.inactive.bg_stroke = egui::Stroke::NONE;
    visuals.selection.bg_fill = super::inventory::BAR;
    visuals.selection.stroke = egui::Stroke::new(1.0, egui::Color32::WHITE);
}

/// An entry's labelled values as markup after its label; a `<name> color` column colours the value of
/// `<name>`, and a column named like the entry itself is just its value.
fn column_text(entry: &str, columns: &[(String, String)]) -> String {
    let tint = |name: &str| {
        columns
            .iter()
            .find(|(label, _)| label.strip_suffix(" color") == Some(name))
            .map(|(_, color)| color.as_str())
    };
    columns
        .iter()
        // `source` names the selector column an item comes from; the engine keeps it for clients.
        .filter(|(label, value)| {
            !label.ends_with(" color") && label != "source" && !value.is_empty()
        })
        .map(|(label, value)| {
            let value = match tint(label) {
                Some(color) => format!("<color_{color}>{value}</color>"),
                None => value.clone(),
            };
            if label.is_empty() || label == entry {
                // A value that only repeats the entry's own text adds nothing.
                if markup::plain(value.as_str()) == markup::plain(entry) {
                    return String::new();
                }
                format!("  {value}")
            } else {
                format!("  {}: {value}", label.trim_end_matches(':'))
            }
        })
        .collect()
}

/// A description the player can read; the engine fills it with the bound action's name (`NO`,
/// `QUIT`, `ANY_INPUT`) when an entry has no text of its own.
fn is_prose(description: &str) -> bool {
    description
        .chars()
        .any(|c| !(c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_'))
}

/// What a popup's dismiss button shows: the any-key prompt as the engine words it, otherwise "OK" in
/// place of the engine's action name ("Cancel popup"), which the native popup never shows.
fn dismiss_text(engine_label: &str) -> &str {
    if engine_label == "Press any key" {
        engine_label
    } else {
        "OK"
    }
}

/// A list this short is read at a glance; the engine's filter is offered from here on.
const FILTER_MIN_ROWS: usize = 10;

#[cfg(test)]
mod tests {
    use super::dismiss_text;

    #[test]
    fn a_popup_button_says_ok_and_the_any_key_prompt_keeps_its_words() {
        assert_eq!(dismiss_text("Cancel popup"), "OK");
        assert_eq!(dismiss_text("Exit screen"), "OK");
        assert_eq!(dismiss_text("Press any key"), "Press any key");
    }
}
