//! The native pickup list: `PICK Wgt/Vol` header, one row per stack as its letter, nesting slash, mark
//! and name in the item's color, the cursor bar, the count box of a marked stack, the key hints under
//! the list, and the cursor row's item description. Letters mark rows; Enter confirms, Escape cancels.
use super::inventory::{key_hints, tinted, typed_hotkey};
use super::menu::{list_frame, list_row_style};
use super::{App, a11y};
use crate::windows::{self, Origin, WindowSpec};
use bn_client_core::{GameModel, Menu};
use eframe::egui::{self, accesskit};

#[derive(Default)]
pub struct Pickup {
    /// The row the cursor was last on, so the list scrolls only when it moves.
    cursor: Option<String>,
}

impl<M: GameModel> App<M> {
    /// Draws the menu; true when it took keyboard focus (on `entered`) this frame.
    pub(super) fn pickup(&mut self, ctx: &egui::Context, menu: &Menu, entered: bool) -> bool {
        a11y::trap_tab(
            ctx,
            self.trapped_by_menu(),
            super::menu_forwards_keys(Some(menu)),
        );
        let mut ids = Vec::new();
        let mut focused = false;
        let mut chosen = typed_hotkey(ctx, &self.model, &menu.choices);
        let mut count = None;
        let mut confirm = false;
        let any = menu.choices.iter().any(|c| c.selected);
        let cancel = self.palette.is_none()
            && ctx.input(|i| i.key_pressed(egui::Key::Escape))
            && menu.can_cancel;
        let rows = menu.choices.iter().filter(|c| !c.tab).collect::<Vec<_>>();
        let title = bn_client_core::markup::plain(&menu.title);
        let mut hovered = None;
        let placement = menu.panes.first().map(|p| p.placement.as_str());
        let spec = WindowSpec {
            closable: menu.can_cancel,
            origin: Origin {
                pivot: match placement {
                    Some("right") => egui::Align2::RIGHT_TOP,
                    _ => egui::Align2::LEFT_TOP,
                },
                pos: egui::pos2(
                    match placement {
                        Some("right") => self.sidebar_left.unwrap_or(ctx.content_rect().right()),
                        _ => ctx.content_rect().left(),
                    },
                    ctx.content_rect().top(),
                ),
            },
            width: Some(560.0),
            frame: Some(list_frame(ctx)),
            ..self.window_spec(ctx, "pickup", &title)
        };
        let shown = windows::show(ctx, spec, |ui| {
            // The native header the engine words: predicted against capacity, red where over, then the filter.
            ui.label(self.rich(ui, &tinted("c_white", &menu.message)));
            egui::ScrollArea::vertical()
                .max_height(ctx.content_rect().height() * 0.45)
                .show(ui, |ui| {
                    for (i, row) in rows.iter().enumerate() {
                        ui.horizontal(|ui| {
                            list_row_style(ui);
                            ui.spacing_mut().item_spacing.x = 0.0;
                            let color = row.color.as_str();
                            let key = match row.hotkey.as_str() {
                                "" => " ".to_string(),
                                k => tinted(color, k),
                            };
                            let slash = match row.nest_color.as_str() {
                                "" => " ".to_string(),
                                c => tinted(c, "\\"),
                            };
                            let mark = row.mark.as_ref().map_or(String::new(), |m| {
                                tinted(m.color.as_deref().unwrap_or(""), &format!("{} ", m.glyph))
                            });
                            let name = match row.markup.as_str() {
                                "" => tinted(color, &row.label),
                                markup => tinted(color, markup),
                            };
                            let text = self.rich(ui, &format!("{key}{slash}{mark}{name}"));
                            let button =
                                self.control(ui, text, &row.id, row.enabled, row.highlighted);
                            ids.extend(button.enabled().then_some(button.id));
                            ui.ctx().accesskit_node_builder(button.id, |b| {
                                b.set_role(accesskit::Role::MenuItem);
                                b.set_label(bn_client_core::markup::plain(&row.label));
                                b.set_toggled(if row.selected {
                                    accesskit::Toggled::True
                                } else {
                                    accesskit::Toggled::False
                                });
                            });
                            if entered && i == 0 {
                                button.request_focus();
                                focused = button.has_focus();
                            }
                            if row.highlighted && self.pickup.cursor.as_deref() != Some(&row.id) {
                                button.scroll_to_me(None);
                                self.pickup.cursor = Some(row.id.clone());
                            }
                            if button.hovered() {
                                hovered = Some(*row);
                            }
                            // Enter on a focused row is the native confirm key; a click marks.
                            if button.clicked() {
                                if ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                                    confirm = any;
                                } else {
                                    chosen = Some(row.id.clone());
                                }
                            }
                            if let Some(c) = self.count_box(ui, row, &mut ids) {
                                count = Some(c);
                            }
                            if !row.enabled && !row.denial.is_empty() {
                                ui.label(self.rich(ui, &tinted("c_red", &row.denial)));
                            }
                        });
                    }
                });
            let actions = self.model.actions();
            for hints in [
                &[
                    ("LEFT", "Unmark"),
                    ("HELP_KEYBINDINGS", "Help"),
                    ("RIGHT", "Mark"),
                ],
                &[
                    ("PREV_TAB", "Prev"),
                    ("SELECT_ALL", "All"),
                    ("NEXT_TAB", "Next"),
                ],
            ] {
                ui.label(self.rich(ui, &tinted("c_light_gray", &key_hints(&actions, hints))));
            }
            let shown = hovered.or_else(|| rows.iter().copied().find(|r| r.highlighted));
            self.item_info(
                ui,
                &shown
                    .map(|r| tinted(&r.color, &r.label))
                    .unwrap_or_default(),
                shown.map(|r| r.description.as_str()),
            );
        });
        if self.keep_window(ctx, "pickup", &shown) {
            self.model.cancel_menu();
        }
        self.menu_ids = ids;
        a11y::dialog(ctx, &title, &menu.message);
        if let Some((id, n)) = count {
            ctx.request_repaint();
            self.model.set_count(&id, n);
        } else if confirm {
            ctx.request_repaint();
            self.model.invoke_action("CONFIRM");
        } else if let Some(id) = chosen {
            ctx.request_repaint();
            self.model.choose(&id);
        } else if cancel {
            self.model.cancel_menu();
        }
        focused
    }
}
