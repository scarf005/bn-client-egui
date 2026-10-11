//! The native NPC trade window: your items and theirs side by side with the unit price of each row,
//! a count box for a marked stack, the balance line, and the menu's own Confirm, Autobalance and Cancel.
use super::{App, a11y};
use bn_client_core::{GameModel, Menu, MenuPane};
use eframe::egui::{self, accesskit};
use std::collections::HashMap;

#[derive(Default)]
pub struct Trade {
    /// The count box of each marked stack: what the engine last chose, and the text being typed.
    counts: HashMap<String, (Option<u64>, String)>,
}

/// What a frame of the window asks the engine for.
#[derive(Default)]
struct Out {
    ids: Vec<egui::Id>,
    chosen: Option<String>,
    count: Option<(String, u64)>,
    described: Option<String>,
}

impl<M: GameModel> App<M> {
    /// Draws the menu; true when it took keyboard focus (on `entered`) this frame.
    pub(super) fn trade(&mut self, ctx: &egui::Context, menu: &Menu, entered: bool) -> bool {
        a11y::trap_tab(
            ctx,
            self.trapped_by_menu(),
            super::menu_forwards_keys(Some(menu)),
        );
        let mut out = Out::default();
        let mut focused = false;
        let mut action = None;
        let mut cancel = self.palette.is_none()
            && ctx.input(|i| i.key_pressed(egui::Key::Escape))
            && menu.can_cancel;
        let title = bn_client_core::markup::plain(&menu.title);
        egui::Window::new(&title)
            .id(a11y::dialog_id(&title))
            .anchor(egui::Align2::CENTER_TOP, super::dialog_offset(ctx))
            .collapsible(false)
            .default_width(1000.0)
            .show(ctx, |ui| {
                ui.label(self.rich(ui, &menu.message));
                ui.columns(2, |cols| {
                    for (index, ui) in cols.iter_mut().enumerate() {
                        let pane = &menu.panes[index];
                        let first = entered && index == 0;
                        focused |= self.trade_pane(ui, menu, (pane, first), &mut out);
                    }
                });
                ui.separator();
                let height = ctx.content_rect().height() * 0.2;
                egui::ScrollArea::vertical()
                    .id_salt("trade_description")
                    .max_height(height)
                    .show(ui, |ui| {
                        ui.set_min_height(height);
                        ui.set_min_width(ui.available_width());
                        ui.label(self.rich(ui, &out.described.take().unwrap_or_default()));
                    });
                ui.separator();
                ui.horizontal(|ui| {
                    for (label, id) in [
                        ("Confirm trade", "CONFIRM"),
                        ("Autobalance", "AUTOBALANCE"),
                        ("Examine", "EXAMINE"),
                    ] {
                        let offered = self.model.actions().iter().any(|a| a.id == id);
                        let button = self.control(ui, label, id, offered, false);
                        out.ids.extend(button.enabled().then_some(button.id));
                        if button.clicked() {
                            action = Some(id);
                        }
                    }
                    if menu.can_cancel {
                        let button =
                            ui.add_enabled(!self.model.busy(), egui::Button::new("Cancel"));
                        out.ids.extend(button.enabled().then_some(button.id));
                        cancel |= button.clicked();
                    }
                });
            });
        self.menu_ids = out.ids;
        a11y::dialog(ctx, &title, &menu.message);
        if let Some((id, n)) = out.count {
            ctx.request_repaint();
            self.model.set_count(&id, n);
        } else if let Some(id) = action {
            ctx.request_repaint();
            self.model.invoke_action(id);
        } else if let Some(id) = out.chosen {
            ctx.request_repaint();
            self.model.choose(&id);
        } else if cancel {
            self.model.cancel_menu();
        }
        focused
    }

    fn trade_pane(
        &mut self,
        ui: &mut egui::Ui,
        menu: &Menu,
        (pane, first): (&MenuPane, bool),
        out: &mut Out,
    ) -> bool {
        let mut focused = false;
        ui.group(|ui| {
            ui.set_min_width(ui.available_width());
            ui.heading(format!("{}: {}", pane.label, pane.area));
            ui.label(&pane.description);
            if !pane.filter.is_empty() {
                ui.weak(format!("Filter: {}", pane.filter));
            }
            egui::ScrollArea::vertical()
                .id_salt(("trade_rows", &pane.id))
                .max_height(ui.ctx().content_rect().height() * 0.45)
                .show(ui, |ui| {
                    ui.set_min_height(120.0);
                    let rows = menu
                        .choices
                        .iter()
                        .filter(|c| c.group.as_deref() == Some(pane.id.as_str()));
                    for (i, row) in rows.enumerate() {
                        ui.horizontal(|ui| {
                            let label = bn_client_core::markup::plain(&row.label);
                            let mark = if row.selected { "[x]" } else { "[ ]" };
                            let button = self.control(
                                ui,
                                format!("{mark} {label}"),
                                &row.id,
                                row.enabled,
                                row.highlighted,
                            );
                            out.ids.extend(button.enabled().then_some(button.id));
                            ui.ctx().accesskit_node_builder(button.id, |b| {
                                b.set_role(accesskit::Role::MenuItem);
                                b.set_toggled(if row.selected {
                                    accesskit::Toggled::True
                                } else {
                                    accesskit::Toggled::False
                                });
                            });
                            if first && i == 0 {
                                button.request_focus();
                                focused = button.has_focus();
                            }
                            if button.clicked() {
                                out.chosen = Some(row.id.clone());
                            }
                            if button.hovered() || (row.highlighted && out.described.is_none()) {
                                out.described = Some(row.description.clone());
                            }
                            for key in ["Unit price", "Weight", "Volume"] {
                                let found =
                                    row.columns.iter().find(|(l, v)| l == key && !v.is_empty());
                                if let Some((_, v)) = found {
                                    ui.weak(v);
                                }
                            }
                            let available = row.available_count.filter(|n| *n > 1);
                            if let (true, Some(available)) = (row.selected, available) {
                                let (seen, text) = self
                                    .trade
                                    .counts
                                    .entry(row.id.clone())
                                    .or_insert((None, String::new()));
                                if *seen != row.selected_count {
                                    *seen = row.selected_count;
                                    *text =
                                        row.selected_count.map_or(String::new(), |n| n.to_string());
                                }
                                let response =
                                    ui.add(egui::TextEdit::singleline(text).desired_width(40.0));
                                let name = format!("Count of {label}");
                                ui.ctx()
                                    .accesskit_node_builder(response.id, |b| b.set_label(name));
                                out.ids.push(response.id);
                                ui.label(format!("/ {available}"));
                                let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
                                if response.lost_focus() && enter {
                                    out.count = text
                                        .trim()
                                        .parse::<u64>()
                                        .ok()
                                        .filter(|n| (1..=available).contains(n))
                                        .map(|n| (row.id.clone(), n));
                                }
                            }
                            if !row.enabled && !row.denial.is_empty() {
                                ui.label(self.rich(ui, &row.denial));
                            }
                        });
                    }
                });
        });
        focused
    }
}
