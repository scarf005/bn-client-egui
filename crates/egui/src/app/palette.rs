//! Searchable list of everything the engine accepts, opened from the top bar or with Ctrl+P.

use super::App;
use bn_client_core::GameModel;
use eframe::egui::{self, accesskit};

impl<M: GameModel> App<M> {
    /// The opened palette's search text is typed text, not game keys.
    pub(super) fn palette_keys(&mut self, ctx: &egui::Context) {
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::P)) {
            self.palette = self.palette.is_none().then(String::new);
        }
    }

    pub(super) fn palette(&mut self, ctx: &egui::Context) {
        let Some(mut query) = self.palette.take() else {
            return;
        };
        let mut invoke = None;
        let close = ctx.input(|i| i.key_pressed(egui::Key::Escape));
        let needle = query.to_lowercase();
        let mut actions = self
            .model
            .actions()
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
        egui::Window::new("Actions")
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
                if !search.has_focus() && !search.lost_focus() {
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
                                    true => a.name.clone(),
                                    false => format!("{} ({})", a.name, a.keys.join(", ")),
                                };
                                let button = self.control(ui, label, &a.id, true, false);
                                ui.ctx().accesskit_node_builder(button.id, |b| {
                                    b.set_role(accesskit::Role::MenuItem)
                                });
                                if button.clicked() {
                                    invoke = Some(a.id.clone());
                                }
                            }
                        });
                    });
            });
        match invoke {
            Some(id) => {
                ctx.request_repaint();
                self.model.invoke_action(&id);
            }
            None if !close => self.palette = Some(query),
            None => {}
        }
    }
}
