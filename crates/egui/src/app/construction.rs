//! The native construction menu: category tabs, a search field, the construction list colored by
//! buildability and the selected construction's requirements. The engine owns the lists and the
//! placement that follows a choice; the client only keeps which construction is selected.
use super::{App, a11y};
use bn_client_core::{GameModel, Menu, MenuChoice};
use eframe::egui::{self, accesskit};

#[derive(Default)]
pub struct Construction {
    selected: Option<String>,
    /// The search box text, resynced whenever the engine reports a different filter.
    search: String,
    engine_filter: String,
    /// A text box had focus last frame; arrows and Enter then belong to it.
    editing: bool,
    scroll_to_selected: bool,
}

fn rows(menu: &Menu) -> Vec<&MenuChoice> {
    let all = menu.choices.iter();
    all.filter(|c| c.id.starts_with("construction:")).collect()
}

impl<M: GameModel> App<M> {
    /// Draws the menu; true when it took keyboard focus (on `entered`) this frame.
    pub(super) fn construction(&mut self, ctx: &egui::Context, menu: &Menu, entered: bool) -> bool {
        a11y::trap_tab(ctx, self.trapped_by_menu(), false);
        let mut ids = Vec::new();
        let rows = rows(menu);
        let filter = menu.field.as_ref().map_or("", |f| &f.value);
        if self.construction.engine_filter != filter {
            self.construction.engine_filter = filter.into();
            self.construction.search = filter.into();
        }
        let current = rows
            .iter()
            .find(|c| self.construction.selected.as_deref() == Some(&c.id))
            .or(rows.first())
            .copied();
        let tabs = menu
            .choices
            .iter()
            .filter(|c| c.id.starts_with("tab:"))
            .collect::<Vec<_>>();
        let mut chosen = None;
        let mut build = false;
        let mut cancel = self.palette.is_none()
            && ctx.input(|i| i.key_pressed(egui::Key::Escape))
            && menu.can_cancel;
        let mut selected = current.map(|c| c.id.clone());

        if !self.construction.editing && !self.model.busy() {
            let pressed = |k| ctx.input(|i| i.key_pressed(k));
            let at = rows.iter().position(|c| Some(&c.id) == selected.as_ref());
            if pressed(egui::Key::ArrowDown) && !rows.is_empty() {
                let next = at.map_or(0, |i| (i + 1).min(rows.len() - 1));
                selected = Some(rows[next].id.clone());
                self.construction.scroll_to_selected = true;
            }
            if pressed(egui::Key::ArrowUp) && !rows.is_empty() {
                let prev = at.map_or(0, |i| i.saturating_sub(1));
                selected = Some(rows[prev].id.clone());
                self.construction.scroll_to_selected = true;
            }
            for (key, back) in [(egui::Key::ArrowLeft, true), (egui::Key::ArrowRight, false)] {
                if pressed(key) && !tabs.is_empty() {
                    let from = tabs.iter().position(|t| t.selected).unwrap_or(0);
                    let to = if back {
                        from + tabs.len() - 1
                    } else {
                        from + 1
                    };
                    chosen = Some(tabs[to % tabs.len()].id.clone());
                }
            }
            build |= pressed(egui::Key::Enter) && at.is_some();
            if let Some(id) = ctx.memory(|m| m.focused()) {
                ctx.memory_mut(|m| {
                    m.set_focus_lock_filter(
                        id,
                        egui::EventFilter {
                            horizontal_arrows: true,
                            vertical_arrows: true,
                            tab: true,
                            ..Default::default()
                        },
                    )
                });
            }
        }

        let mut editing = false;
        let mut focused = false;
        let mut fill = None;
        egui::Window::new(&menu.title)
            .id(a11y::dialog_id(&menu.title))
            .anchor(egui::Align2::CENTER_TOP, super::dialog_offset(ctx))
            .collapsible(false)
            .default_size(egui::vec2(900.0, 560.0))
            .show(ctx, |ui| {
                ui.horizontal_wrapped(|ui| {
                    for tab in &tabs {
                        let button =
                            self.control(ui, tab.label.clone(), &tab.id, tab.enabled, tab.selected);
                        a11y::tab(ui.ctx(), button.id, tab.selected);
                        ids.extend(button.enabled().then_some(button.id));
                        if button.clicked() {
                            chosen = Some(tab.id.clone());
                        }
                    }
                });
                if let Some(field) = &menu.field {
                    let hint = field.label.trim_end_matches(':');
                    let edit = egui::TextEdit::singleline(&mut self.construction.search)
                        .hint_text(hint)
                        .desired_width(f32::INFINITY);
                    let response = ui.add(edit);
                    let label = hint.to_owned();
                    ui.ctx()
                        .accesskit_node_builder(response.id, move |b| b.set_label(label));
                    a11y::text_field(ui.ctx(), response.id);
                    ids.push(response.id);
                    editing |= response.has_focus();
                    if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        fill = Some((field.id.clone(), self.construction.search.clone()));
                    }
                }
                ui.separator();
                ui.columns(2, |cols| {
                    egui::ScrollArea::vertical()
                        .id_salt("constructions")
                        .max_height(420.0)
                        .show(&mut cols[0], |ui| {
                            for (i, row) in rows.iter().enumerate() {
                                let on = selected.as_ref() == Some(&row.id);
                                let text = egui::RichText::new(&row.label);
                                let text = if row.enabled {
                                    text
                                } else {
                                    text.color(ui.visuals().error_fg_color)
                                };
                                let mut response = ui.selectable_label(on, text);
                                ids.extend(on.then_some(response.id));
                                if !row.denial.is_empty() {
                                    response = response.on_hover_text(&row.denial);
                                }
                                ui.ctx().accesskit_node_builder(response.id, |b| {
                                    b.set_role(accesskit::Role::MenuItem)
                                });
                                if entered && i == 0 {
                                    response.request_focus();
                                    focused = response.has_focus();
                                }
                                if on && std::mem::take(&mut self.construction.scroll_to_selected) {
                                    response.scroll_to_me(None);
                                    response.request_focus();
                                }
                                if response.clicked() {
                                    selected = Some(row.id.clone());
                                }
                                if response.double_clicked() {
                                    build = true;
                                }
                            }
                        });
                    let ui = &mut cols[1];
                    let Some(row) = rows.iter().find(|c| selected.as_ref() == Some(&c.id)) else {
                        return;
                    };
                    egui::ScrollArea::vertical()
                        .id_salt("requirements")
                        .max_height(420.0)
                        .show(ui, |ui| {
                            if !row.denial.is_empty() {
                                ui.colored_label(ui.visuals().warn_fg_color, &row.denial);
                            }
                            for (label, value) in &row.columns {
                                ui.horizontal(|ui| {
                                    ui.label(label);
                                    ui.label(value);
                                });
                            }
                            ui.separator();
                            ui.label(&row.description);
                        });
                    ui.separator();
                    let name = format!("Construct {}", row.label);
                    let button = self.control(ui, name, &row.id, row.enabled, false);
                    ids.extend(button.enabled().then_some(button.id));
                    build |= button.clicked();
                });
                if menu.can_cancel {
                    let close = ui.button("Close");
                    a11y::button(ui.ctx(), close.id, false);
                    ids.push(close.id);
                    cancel |= close.clicked();
                }
            });

        self.menu_ids = ids;
        a11y::dialog(ctx, &menu.title, &menu.message);
        self.construction.editing = editing;
        self.construction.selected = selected.clone();
        ctx.request_repaint_after(std::time::Duration::from_millis(100));
        if let Some((field, value)) = fill {
            self.model.fill(&field, &value, true);
        } else if let Some(id) = chosen {
            self.model.choose(&id);
        } else if let (true, Some(id)) = (build, selected) {
            self.model.choose(&id);
        } else if cancel {
            self.model.cancel_menu();
        }
        focused
    }
}
