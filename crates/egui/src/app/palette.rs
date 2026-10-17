//! Searchable list of everything the engine accepts, opened from the top bar or with Ctrl+P.

use super::App;
use crate::a11y;
use crate::windows::{self, WindowSpec};
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
        a11y::trap_tab(ctx, &self.palette_ids, false);
        let entered = self.palette_ids.is_empty();
        let mut ids = Vec::new();
        let spec = WindowSpec {
            closable: true,
            width: Some(420.0),
            ..self.window_spec(ctx, Some("palette"), "Actions")
        };
        let window_id = spec.id;
        let shown = windows::show(ctx, spec, |ui| {
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
                                false => {
                                    format!("{} ({})", markup::plain(&a.name), a.keys.join(", "))
                                }
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
        if self.keep_window(ctx, Some("palette"), &shown) {
            self.palette = None;
        }
        a11y::dialog(ctx, window_id, "Actions", "");
        self.palette_ids = ids;
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
