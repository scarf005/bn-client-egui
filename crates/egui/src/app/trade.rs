//! The native NPC trade window: your items and theirs side by side as category headers over an
//! amount, weight, volume and unit price table, a filter box and carried totals per pane, a count box
//! for a marked stack, the key hints and the colored balance line, and the menu's own Confirm,
//! Autobalance and Cancel.
use super::aim::Step;
use super::{App, a11y};
use bn_client_core::{ActionEntry, GameModel, Menu, MenuChoice, MenuPane};
use eframe::egui::{self, accesskit};
use std::collections::HashMap;

#[derive(Default)]
pub struct Trade {
    /// The count box of each marked stack: what the engine last chose, and the text being typed.
    counts: HashMap<String, (Option<u64>, String)>,
    /// The filter box text of each pane, and the filter the engine last reported for it.
    filters: [(String, String); 2],
}

const GREEN: egui::Color32 = egui::Color32::from_rgb(90, 255, 90);
const RED: egui::Color32 = egui::Color32::from_rgb(255, 110, 110);
const MAGENTA: egui::Color32 = egui::Color32::from_rgb(190, 70, 160);

/// The colour the native screen gives a `good` or `bad` tone; `None` for any other.
fn tone_color(tone: &str) -> Option<egui::Color32> {
    match tone {
        "good" => Some(GREEN),
        "bad" => Some(RED),
        _ => None,
    }
}

fn column<'a>(row: &'a MenuChoice, label: &str) -> &'a str {
    let found = row.columns.iter().find(|(l, _)| l == label);
    found.map_or("", |(_, v)| v.as_str())
}

/// The key hints of the native header: `[key] label` for each action the engine offers.
fn key_hints(actions: &[ActionEntry]) -> String {
    let hint = |id: &str, label: &str| {
        let key = actions.iter().find(|a| a.id == id)?.keys.first()?;
        Some(format!("[{key}] {label}"))
    };
    [
        ("EXAMINE", "examine item"),
        ("SWITCH_LISTS", "switch panes"),
        ("CONFIRM", "confirm trade"),
        ("AUTOBALANCE", "autobalance"),
        ("CATEGORY_SELECTION", "category select"),
    ]
    .iter()
    .filter_map(|(id, label)| hint(id, label))
    .collect::<Vec<_>>()
    .join("  ")
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
        self.aim.editing = false;
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
            .max_width(ctx.content_rect().width() - 16.0)
            .show(ctx, |ui| {
                ui.weak(key_hints(&self.model.actions()));
                let message = bn_client_core::markup::plain(&menu.message);
                match tone_color(&menu.message_tone) {
                    Some(color) => ui.colored_label(color, message),
                    None => ui.label(message),
                };
                ui.columns(2, |cols| {
                    for (index, ui) in cols.iter_mut().enumerate() {
                        let pane = &menu.panes[index];
                        let first = entered && index == 0;
                        focused |= self.trade_pane(ui, menu, (index, pane, first), &mut out);
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
                        ("Category select", "CATEGORY_SELECTION"),
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
        (index, pane, first): (usize, &MenuPane, bool),
        out: &mut Out,
    ) -> bool {
        let mut focused = false;
        ui.group(|ui| {
            ui.set_min_width(ui.available_width());
            ui.heading(format!("Inventory: {}", pane.label));
            if let Some(totals) = &pane.totals {
                let over = |over| if over { RED } else { GREEN };
                ui.horizontal(|ui| {
                    ui.colored_label(over(totals.over_weight), &totals.weight);
                    ui.label(format!("/{} {}", totals.weight_max, totals.weight_unit));
                    ui.colored_label(over(totals.over_volume), &totals.volume);
                    ui.label(format!("/{} {}", totals.volume_max, totals.volume_unit));
                });
            }
            self.trade_filter(ui, menu, (index, pane), out);
            egui::ScrollArea::vertical()
                .id_salt(("trade_rows", &pane.id))
                .max_height(ui.ctx().content_rect().height() * 0.45)
                .show(ui, |ui| {
                    ui.set_min_height(120.0);
                    let rows = menu
                        .choices
                        .iter()
                        .filter(|c| c.group.as_deref() == Some(pane.id.as_str()));
                    // What the number columns and the spacing leave for a name; longer ones are cut.
                    let name_width = (ui.available_width() - 270.0).max(120.0);
                    egui::Grid::new(("trade_table", &pane.id))
                        .spacing([10.0, 2.0])
                        .show(ui, |ui| {
                            for label in ["Name (charges)", "amt", "weight", "vol", "unit price"] {
                                ui.weak(label);
                            }
                            ui.end_row();
                            let mut category = "";
                            for (i, row) in rows.enumerate() {
                                let shown = column(row, "Category");
                                if shown != category && !shown.is_empty() {
                                    category = shown;
                                    ui.colored_label(MAGENTA, shown);
                                    ui.end_row();
                                }
                                ui.horizontal(|ui| {
                                    let label = bn_client_core::markup::plain(&row.label);
                                    let mark = if row.selected { "[x]" } else { "[ ]" };
                                    let button = ui
                                        .scope(|ui| {
                                            ui.set_max_width(name_width);
                                            ui.style_mut().wrap_mode =
                                                Some(egui::TextWrapMode::Truncate);
                                            let text = format!("{mark} {label}");
                                            self.control(
                                                ui,
                                                text,
                                                &row.id,
                                                row.enabled,
                                                row.highlighted,
                                            )
                                        })
                                        .inner;
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
                                    if button.hovered()
                                        || (row.highlighted && out.described.is_none())
                                    {
                                        out.described = Some(row.description.clone());
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
                                            *text = row
                                                .selected_count
                                                .map_or(String::new(), |n| n.to_string());
                                        }
                                        let response = ui.add(
                                            egui::TextEdit::singleline(text).desired_width(40.0),
                                        );
                                        let name = format!("Count of {label}");
                                        ui.ctx().accesskit_node_builder(response.id, |b| {
                                            b.set_label(name)
                                        });
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
                                let stack = row.available_count.unwrap_or(1);
                                ui.label(if stack > 1 {
                                    stack.to_string()
                                } else {
                                    String::new()
                                });
                                ui.label(column(row, "Stack weight"));
                                ui.label(column(row, "Stack volume"));
                                let price = egui::RichText::new(column(row, "Unit price"));
                                ui.label(match column(row, "Price tone") {
                                    "good" => price.color(GREEN),
                                    "bad" => price.color(RED),
                                    _ => price.weak(),
                                });
                                ui.end_row();
                            }
                        });
                });
        });
        focused
    }

    /// The `/` filter box of a pane. The engine filters the pane the cursor is in, so another pane is
    /// switched to first.
    fn trade_filter(
        &mut self,
        ui: &mut egui::Ui,
        menu: &Menu,
        (index, pane): (usize, &MenuPane),
        out: &mut Out,
    ) {
        let (typed, seen) = &mut self.trade.filters[index];
        if *seen != pane.filter {
            seen.clone_from(&pane.filter);
            typed.clone_from(&pane.filter);
        }
        let response = ui.add(
            egui::TextEdit::singleline(typed)
                .hint_text("/ filter")
                .desired_width(f32::INFINITY),
        );
        ui.ctx().accesskit_node_builder(response.id, |b| {
            b.set_label(format!("{} filter", pane.label));
        });
        out.ids.push(response.id);
        self.aim.editing |= response.has_focus();
        if !(response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter))) {
            return;
        }
        let text = typed.trim().to_string();
        let focused = menu.choices.iter().find(|c| c.highlighted);
        let here = focused.is_none_or(|c| c.group.as_deref() == Some(pane.id.as_str()));
        let mut steps = Vec::new();
        if !here {
            steps.push(Step::Action("SWITCH_LISTS".into()));
        }
        if text.is_empty() {
            steps.push(Step::Action("RESET_FILTER".into()));
        } else {
            steps.push(Step::Action("FILTER".into()));
            steps.push(Step::Fill(text));
        }
        self.aim_do(steps);
    }
}
