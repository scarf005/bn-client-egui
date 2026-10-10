//! Searchable list of everything the engine accepts, opened from the top bar or with Ctrl+P.

use super::{App, QUEUE, Queued};
use crate::a11y;
use bn_client_core::{GameModel, markup};
use eframe::egui::{self, accesskit};

impl<M: GameModel> App<M> {
    /// The opened palette's search text is typed text, not game keys.
    pub(super) fn palette_keys(&mut self, ctx: &egui::Context) {
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::P)) {
            self.palette = self.palette.is_none().then(String::new);
        }
    }

    /// The menu's controls, unless the palette sits on top of it and owns Tab.
    pub(super) fn trapped_by_menu(&self) -> &[egui::Id] {
        match self.palette {
            Some(_) => &[],
            None => &self.menu_ids,
        }
    }

    pub(super) fn palette(&mut self, ctx: &egui::Context) {
        let Some(mut query) = self.palette.take() else {
            // Closed since the last frame, by any route: the map takes focus back.
            self.refocus_map |= !std::mem::take(&mut self.palette_ids).is_empty();
            return;
        };
        let mut invoke = None;
        let close = ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
        let needle = query.to_lowercase();
        let mut actions = self.model.actions();
        // Options is a row of the main menu, not an action of its own; the palette offers it anyway.
        if actions.iter().any(|a| a.id == "main_menu") {
            actions.push(bn_client_core::ActionEntry {
                id: OPTIONS.into(),
                name: "Options".into(),
                keys: vec![],
            });
        }
        let mut actions = actions
            .into_iter()
            .filter(|a| {
                a.name.to_lowercase().contains(&needle)
                    || a.keys.iter().any(|k| k.to_lowercase().contains(&needle))
            })
            .collect::<Vec<_>>();
        // Whole-word matches first ("west" before "Southwest"); the first row is what Enter runs.
        actions.sort_by_key(|a| {
            !a.name
                .to_lowercase()
                .split_whitespace()
                .any(|w| w.starts_with(&needle))
        });
        a11y::trap_tab(ctx, &self.palette_ids);
        let entered = self.palette_ids.is_empty();
        let mut ids = Vec::new();
        egui::Window::new("Actions")
            .id(a11y::dialog_id("Actions"))
            .anchor(egui::Align2::CENTER_TOP, super::dialog_offset(ctx))
            .collapsible(false)
            .resizable(false)
            .default_width(420.0)
            .show(ctx, |ui| {
                let search = ui.add(
                    egui::TextEdit::singleline(&mut query)
                        .hint_text("Search")
                        .desired_width(f32::INFINITY),
                );
                search.widget_info(|| {
                    egui::WidgetInfo::labeled(egui::WidgetType::TextEdit, true, "Search actions")
                });
                a11y::text_field(ui.ctx(), search.id);
                ids.push(search.id);
                if entered {
                    search.request_focus();
                }
                if search.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    invoke = actions.first().map(|a| a.id.clone());
                }
                egui::ScrollArea::vertical()
                    .max_height(ui.ctx().content_rect().height() * 0.6)
                    .show(ui, |ui| {
                        ui.with_layout(egui::Layout::top_down_justified(egui::Align::Min), |ui| {
                            for a in &actions {
                                let label = match a.keys.is_empty() {
                                    true => markup::plain(&a.name),
                                    false => format!(
                                        "{} ({})",
                                        markup::plain(&a.name),
                                        a.keys.join(", ")
                                    ),
                                };
                                let button = self.control(ui, label, &a.id, true, false);
                                ui.ctx().accesskit_node_builder(button.id, |b| {
                                    b.set_role(accesskit::Role::MenuItem)
                                });
                                ids.push(button.id);
                                if button.clicked() {
                                    invoke = Some(a.id.clone());
                                }
                            }
                        });
                    });
            });
        a11y::dialog(ctx, "Actions", "");
        self.palette_ids = ids;
        match invoke {
            Some(id) => {
                ctx.request_repaint();
                let id = if id == OPTIONS {
                    self.open_options = Some(self.clock.now());
                    "main_menu".to_string()
                } else {
                    id
                };
                match self.model.busy() {
                    true => self.enqueue(Queued::Action(id)),
                    false => {
                        self.model.invoke_action(&id);
                    }
                }
            }
            None if !close => self.palette = Some(query),
            None => {}
        }
    }

    pub(super) fn enqueue(&mut self, input: Queued) {
        if self.queued.len() < QUEUE {
            self.queued.push_back(input);
        }
    }

    /// Sends the waiting input once the command in flight is done; a menu that has opened meanwhile
    /// ends the wait, since the input was meant for the map.
    pub(super) fn run_queued(&mut self) {
        if self.model.busy() || self.queued.is_empty() {
            return;
        }
        if self.model.menu().is_some() {
            self.queued.clear();
        }
        while !self.model.busy()
            && let Some(input) = self.queued.pop_front()
        {
            match input {
                Queued::Action(id) => self.model.invoke_action(&id),
                Queued::Key(key) => self.model.press(&key),
            };
        }
    }
}

/// The palette's own entry for the native Options screen.
const OPTIONS: &str = "palette:options";

/// How long the palette waits for the main menu it asked for.
const OPTIONS_WAIT: std::time::Duration = std::time::Duration::from_secs(5);

impl<M: GameModel> App<M> {
    /// Chooses the Options row of the main menu the palette opened for it.
    pub(super) fn follow_options(&mut self, menu: &bn_client_core::Menu) {
        let Some(since) = self.open_options else {
            return;
        };
        if self.clock.now().saturating_sub(since) > OPTIONS_WAIT {
            self.open_options = None;
            return;
        }
        let row = menu
            .choices
            .iter()
            .find(|c| c.label == "Options" && c.enabled);
        if let (true, "MAIN MENU", Some(row), false) = (
            menu.context != "MAIN_MENU",
            bn_client_core::markup::plain(&menu.title).as_str(),
            row,
            self.model.busy(),
        ) {
            self.open_options = None;
            self.model.choose(&row.id.clone());
        }
    }
}
