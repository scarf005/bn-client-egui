//! The panes of a target screen, such as the map editor's tile and actions, in a window beside the map
//! the cursor moves on. The keys still go to the engine; an action row is a button for it.
use super::App;
use super::menu::{column_text, list_frame, list_row_style};
use bn_client_core::{GameModel, markup};
use eframe::egui;

impl<M: GameModel> App<M> {
    pub(super) fn target_panel(&mut self, ctx: &egui::Context) {
        let Some(panel) = self.model.target_panel() else {
            return;
        };
        let title = markup::plain(&panel.title);
        let mut chosen = None;
        egui::Window::new(&title)
            .id(egui::Id::new("target_panel"))
            .anchor(egui::Align2::RIGHT_TOP, [-8.0, 48.0])
            .collapsible(false)
            .resizable(false)
            .frame(list_frame(ctx))
            .show(ctx, |ui| {
                list_row_style(ui);
                let rows = panel.entries();
                for (i, row) in rows.iter().copied().enumerate() {
                    if let Some(heading) = panel.heading(i.checked_sub(1).map(|p| rows[p]), row) {
                        ui.add(egui::Label::new(egui::RichText::new(heading).strong()));
                    }
                    if row.id.starts_with("action:") {
                        let key = match row.hotkey.is_empty() {
                            true => String::new(),
                            false => format!("<color_light_green>{}</color> ", row.hotkey),
                        };
                        let text = self.rich(ui, &format!("{key}{}", row.label));
                        let button = self.control(ui, text, &row.id, row.enabled, row.selected);
                        if button.clicked() {
                            chosen = Some(row.id.clone());
                        }
                    } else {
                        let values = column_text(&row.label, &row.columns);
                        let text = format!("{}: {}{values}", row.label, row.description);
                        ui.label(self.rich(ui, &text));
                    }
                }
            });
        if let Some(id) = chosen {
            ctx.request_repaint();
            self.model.choose(&id);
        }
    }
}
