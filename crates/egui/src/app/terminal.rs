//! The text windows the game draws whole: a computer terminal (green text on black, its answers
//! under it) and the extended description of a square (light gray text, the targets it can describe
//! as buttons). The engine owns the text and the answers; the keys still go to the engine.
use super::menu::{list_frame, list_row_style};
use super::{App, a11y};
use bn_client_core::{GameModel, Menu, markup};
use eframe::egui;

/// The native `c_green` of a terminal's own text.
const TERMINAL: egui::Color32 = egui::Color32::from_rgb(0, 170, 0);

impl<M: GameModel> App<M> {
    /// Draws the menu; true when it took keyboard focus (on `entered`) this frame.
    pub(super) fn terminal(&mut self, ctx: &egui::Context, menu: &Menu, entered: bool) -> bool {
        a11y::trap_tab(
            ctx,
            self.trapped_by_menu(),
            super::menu_forwards_keys(Some(menu)),
        );
        let mut ids = Vec::new();
        let mut focused = false;
        let mut chosen = None;
        let cancel = self.palette.is_none()
            && ctx.input(|i| i.key_pressed(egui::Key::Escape))
            && menu.can_cancel;
        let title = markup::plain(&menu.title);
        let color = match menu.terminal() {
            true => TERMINAL,
            false => egui::Color32::LIGHT_GRAY,
        };
        let font = egui::FontId::monospace(14.0);
        egui::Window::new(&title)
            .id(a11y::dialog_id(&title))
            .anchor(egui::Align2::CENTER_TOP, super::dialog_offset(ctx))
            .collapsible(false)
            .resizable(false)
            .frame(list_frame(ctx))
            .default_width((ctx.content_rect().width() * 0.7).clamp(480.0, 960.0))
            .show(ctx, |ui| {
                ui.visuals_mut().override_text_color = Some(color);
                egui::ScrollArea::vertical()
                    .max_height(super::dialog_scroll_height(ctx))
                    .show(ui, |ui| {
                        ui.set_min_width(ui.available_width());
                        for line in menu.message.lines() {
                            // The native text window draws a line of two dashes as a rule.
                            if line == "--" {
                                ui.separator();
                                continue;
                            }
                            let runs = self.model.styled(line);
                            let job = crate::text::job(&runs, &font);
                            ui.add(egui::Label::new(job).wrap_mode(egui::TextWrapMode::Wrap));
                        }
                    });
                ui.separator();
                ui.horizontal_wrapped(|ui| {
                    list_row_style(ui);
                    for (i, row) in menu.choices.iter().enumerate() {
                        let button = self.control(
                            ui,
                            markup::plain(&row.label),
                            &row.id,
                            row.enabled,
                            row.selected,
                        );
                        ids.extend(button.enabled().then_some(button.id));
                        if entered && i == 0 {
                            button.request_focus();
                            focused = button.has_focus();
                        }
                        if button.clicked() {
                            chosen = Some(row.id.clone());
                        }
                    }
                });
            });
        self.menu_ids = ids;
        a11y::dialog(ctx, &title, &markup::plain(&menu.message));
        if let Some(id) = chosen {
            ctx.request_repaint();
            self.model.choose(&id);
        } else if cancel {
            self.model.cancel_menu();
        }
        focused
    }
}
