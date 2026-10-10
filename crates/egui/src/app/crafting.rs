//! The native crafting menu: category tabs, sub-tabs, a search field, the recipe list and the
//! selected recipe's detail text. The engine owns the lists; the client only keeps which recipe is selected.
use super::{App, a11y};
use bn_client_core::{GameModel, Menu, MenuChoice};
use eframe::egui::{self, accesskit};

#[derive(Default)]
pub struct Crafting {
    selected: Option<String>,
    /// The search box text, resynced whenever the engine reports a different filter.
    search: String,
    engine_filter: String,
    count: String,
    /// A text box had focus last frame; arrows and Enter then belong to it.
    editing: bool,
    scroll_to_selected: bool,
}

fn recipes(menu: &Menu) -> Vec<&MenuChoice> {
    let all = menu.choices.iter();
    all.filter(|c| c.id.starts_with("recipe:")).collect()
}

/// Names a text box for screen readers.
fn labelled(ui: &egui::Ui, response: &egui::Response, label: &str) {
    let label = label.to_owned();
    ui.ctx()
        .accesskit_node_builder(response.id, move |b| b.set_label(label));
}

impl<M: GameModel> App<M> {
    /// Draws the menu; true when it took keyboard focus (on `entered`) this frame.
    pub(super) fn crafting(&mut self, ctx: &egui::Context, menu: &Menu, entered: bool) -> bool {
        a11y::trap_tab(ctx, self.trapped_by_menu());
        let mut ids = Vec::new();
        let rows = recipes(menu);
        if self.crafting.engine_filter != menu.field.as_ref().map_or("", |f| &f.value) {
            self.crafting.engine_filter = menu.field.as_ref().map_or("", |f| &f.value).into();
            self.crafting.search = self.crafting.engine_filter.clone();
        }
        let current = rows
            .iter()
            .find(|c| self.crafting.selected.as_deref() == Some(&c.id))
            .or(rows.first())
            .copied();
        let tabs = menu
            .choices
            .iter()
            .filter(|c| c.id.starts_with("tab:"))
            .collect::<Vec<_>>();
        let mut chosen = None;
        let mut craft = false;
        let mut cancel = self.palette.is_none()
            && ctx.input(|i| i.key_pressed(egui::Key::Escape))
            && menu.can_cancel;
        let mut selected = current.map(|c| c.id.clone());

        if !self.crafting.editing && !self.model.busy() {
            let pressed = |k| ctx.input(|i| i.key_pressed(k));
            let at = rows.iter().position(|c| Some(&c.id) == selected.as_ref());
            let step = |from: Option<usize>, len: usize, back: bool| match (from, back) {
                (Some(i), true) => (i + len - 1) % len,
                (Some(i), false) => (i + 1) % len,
                (None, back) => {
                    if back {
                        len - 1
                    } else {
                        0
                    }
                }
            };
            if pressed(egui::Key::ArrowDown) && !rows.is_empty() {
                let next = at.map_or(0, |i| (i + 1).min(rows.len() - 1));
                selected = Some(rows[next].id.clone());
                self.crafting.scroll_to_selected = true;
            }
            if pressed(egui::Key::ArrowUp) && !rows.is_empty() {
                let prev = at.map_or(0, |i| i.saturating_sub(1));
                selected = Some(rows[prev].id.clone());
                self.crafting.scroll_to_selected = true;
            }
            for (key, back) in [(egui::Key::ArrowLeft, true), (egui::Key::ArrowRight, false)] {
                if pressed(key) && !tabs.is_empty() {
                    let from = tabs.iter().position(|t| t.selected);
                    chosen = Some(tabs[step(from, tabs.len(), back)].id.clone());
                }
            }
            craft |= pressed(egui::Key::Enter) && at.is_some();
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
        let mut batch = None;
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
                ui.horizontal_wrapped(|ui| {
                    for sub in menu.choices.iter().filter(|c| c.id.starts_with("subtab:")) {
                        let button =
                            self.control(ui, sub.label.clone(), &sub.id, sub.enabled, sub.selected);
                        a11y::tab(ui.ctx(), button.id, sub.selected);
                        ids.extend(button.enabled().then_some(button.id));
                        if button.clicked() {
                            chosen = Some(sub.id.clone());
                        }
                    }
                });
                if let Some(field) = &menu.field {
                    let hint = field.label.trim_end_matches(':');
                    let edit = egui::TextEdit::singleline(&mut self.crafting.search)
                        .hint_text(hint)
                        .desired_width(f32::INFINITY);
                    let response = ui.add(edit);
                    labelled(ui, &response, hint);
                    a11y::text_field(ui.ctx(), response.id);
                    ids.push(response.id);
                    editing |= response.has_focus();
                    if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        fill = Some((field.id.clone(), self.crafting.search.clone()));
                    }
                }
                ui.separator();
                ui.columns(2, |cols| {
                    egui::ScrollArea::vertical()
                        .id_salt("recipes")
                        .max_height(420.0)
                        .show(&mut cols[0], |ui| {
                            for (i, row) in rows.iter().enumerate() {
                                let on = selected.as_ref() == Some(&row.id);
                                let text = egui::RichText::new(&row.label);
                                let text = if row.enabled { text } else { text.weak() };
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
                                if on && std::mem::take(&mut self.crafting.scroll_to_selected) {
                                    response.scroll_to_me(None);
                                    response.request_focus();
                                }
                                if response.clicked() {
                                    selected = Some(row.id.clone());
                                }
                                if response.double_clicked() {
                                    craft = true;
                                }
                            }
                        });
                    let ui = &mut cols[1];
                    let Some(row) = rows.iter().find(|c| selected.as_ref() == Some(&c.id)) else {
                        return;
                    };
                    egui::ScrollArea::vertical()
                        .id_salt("detail")
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
                            ui.label(&row.description);
                        });
                    ui.separator();
                    ui.horizontal(|ui| {
                        let name = format!("Craft {}", row.label);
                        let button = self.control(ui, name, &row.id, row.enabled, false);
                        ids.extend(button.enabled().then_some(button.id));
                        craft |= button.clicked();
                        if menu.can_set_count && row.enabled {
                            let edit = egui::TextEdit::singleline(&mut self.crafting.count)
                                .hint_text("Count")
                                .desired_width(60.0);
                            let response = ui.add(edit);
                            labelled(ui, &response, "Batch count");
                            ids.push(response.id);
                            editing |= response.has_focus();
                            let count = self.crafting.count.trim().parse::<u64>().ok();
                            let button = ui.add_enabled(
                                count.is_some_and(|n| n > 0),
                                egui::Button::new("Craft batch"),
                            );
                            a11y::button(ui.ctx(), button.id, false);
                            ids.extend(button.enabled().then_some(button.id));
                            if let (true, Some(n)) = (button.clicked(), count) {
                                batch = Some((row.id.clone(), n));
                            }
                        }
                    });
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
        self.crafting.editing = editing;
        self.crafting.selected = selected.clone();
        ctx.request_repaint_after(std::time::Duration::from_millis(100));
        if let Some((field, value)) = fill {
            self.model.fill(&field, &value, true);
        } else if let Some((id, count)) = batch {
            self.model.set_count(&id, count);
        } else if let Some(id) = chosen {
            self.model.choose(&id);
        } else if let (true, Some(id)) = (craft, selected) {
            self.model.choose(&id);
        } else if cancel {
            self.model.cancel_menu();
        }
        focused
    }
}
