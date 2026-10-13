//! The native spell window: the spell list on the left with the engine's hotkeys and favorite marks,
//! the highlighted spell's detail text on the right, and the menu's own keys (favorite, distractions,
//! hotkey) below. The engine owns the cursor, the categories and every number shown.
use super::{App, a11y};
use crate::windows::{self, WindowSpec};
use bn_client_core::{GameModel, Menu, markup};
use eframe::egui::{self, accesskit};

impl<M: GameModel> App<M> {
    /// Draws the menu; true when it took keyboard focus (on `entered`) this frame.
    pub(super) fn spells(&mut self, ctx: &egui::Context, menu: &Menu, entered: bool) -> bool {
        a11y::trap_tab(
            ctx,
            self.trapped_by_menu(),
            super::menu_forwards_keys(Some(menu)),
        );
        let mut ids = Vec::new();
        let mut focused = false;
        let mut chosen = None;
        let mut cancel = self.palette.is_none()
            && ctx.input(|i| i.key_pressed(egui::Key::Escape))
            && menu.can_cancel;
        let rows = menu
            .choices
            .iter()
            .filter(|c| c.id.starts_with("entry:"))
            .collect::<Vec<_>>();
        let keys = menu
            .choices
            .iter()
            .filter(|c| c.id.starts_with("action:key:"))
            .collect::<Vec<_>>();
        let title = markup::plain(&menu.title);
        let height = ctx.content_rect().height() * 0.6;
        let spec = WindowSpec {
            closable: menu.can_cancel,
            width: Some(900.0),
            ..self.window_spec(ctx, "spells", &title)
        };
        let shown = windows::show(ctx, spec, |ui| {
            let mut described = None;
            ui.columns(2, |cols| {
                egui::ScrollArea::vertical()
                    .id_salt("spells")
                    .max_height(height)
                    .show(&mut cols[0], |ui| {
                        for (i, row) in rows.iter().enumerate() {
                            let favorite = row.favorite;
                            let label = markup::plain(&row.label);
                            let text = match (row.hotkey.is_empty(), favorite) {
                                (true, false) => label,
                                (true, true) => format!("{label} *"),
                                (false, false) => format!("{} {label}", row.hotkey),
                                (false, true) => format!("{} {label} *", row.hotkey),
                            };
                            let button =
                                self.control(ui, text, &row.id, row.enabled, row.highlighted);
                            ids.extend(button.enabled().then_some(button.id));
                            ui.ctx().accesskit_node_builder(button.id, |b| {
                                b.set_role(accesskit::Role::MenuItem)
                            });
                            if entered && i == 0 {
                                button.request_focus();
                                focused = button.has_focus();
                            }
                            if row.highlighted {
                                button.scroll_to_me(None);
                                described = Some(*row);
                            }
                            if button.clicked() {
                                chosen = Some(row.id.clone());
                            }
                        }
                    });
                egui::ScrollArea::vertical()
                    .id_salt("spell_detail")
                    .max_height(height)
                    .show(&mut cols[1], |ui| {
                        ui.set_min_width(ui.available_width());
                        if let Some(row) = described.or(rows.first().copied()) {
                            ui.label(self.rich(ui, &row.description));
                        }
                    });
            });
            ui.separator();
            ui.horizontal_wrapped(|ui| {
                for key in &keys {
                    let button = self.control(ui, &key.label, &key.id, key.enabled, false);
                    ids.extend(button.enabled().then_some(button.id));
                    if button.clicked() {
                        chosen = Some(key.id.clone());
                    }
                }
                if menu.can_cancel {
                    let button = ui.add_enabled(self.idle(), egui::Button::new("Cancel"));
                    ids.extend(button.enabled().then_some(button.id));
                    cancel |= button.clicked();
                }
            });
        });
        if self.keep_window(ctx, "spells", &shown) {
            self.model.cancel_menu();
        }
        self.menu_ids = ids;
        a11y::dialog(ctx, &title, &menu.message);
        if let Some(id) = chosen {
            ctx.request_repaint();
            self.model.choose(&id);
        } else if cancel {
            self.model.cancel_menu();
        }
        focused
    }
}
