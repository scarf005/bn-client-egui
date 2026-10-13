//! The native advanced inventory manager: two panes, each pointed at an area with buttons, rows that
//! select on click and move on Enter or by dragging to the other pane, a move button with an amount,
//! and a filter per pane. Every control is a native action; the engine owns the lists.
use super::{App, a11y};
use bn_client_core::{GameModel, Menu, MenuChoice, MenuPane};
use eframe::egui::{self, accesskit};
use std::collections::VecDeque;

type Cell = Option<(&'static str, &'static str, &'static str)>;

/// The native area picker as the game lays it out: Container and Grabbed Vehicle over Surrounding area
/// and Worn Items beside the compass, Inventory level with the middle row, Above and Below at the right edge.
const GRID: [[Cell; 7]; 3] = [
    [
        Some(("ITEMS_CONTAINER", "Container", "C")),
        Some(("ITEMS_DRAGGED_CONTAINER", "Grabbed Vehicle", "D")),
        None,
        Some(("ITEMS_NW", "North West", "7")),
        Some(("ITEMS_N", "North", "8")),
        Some(("ITEMS_NE", "North East", "9")),
        Some(("ITEMS_UP", "Above", "<")),
    ],
    [
        None,
        Some(("ITEMS_INVENTORY", "Inventory", "I")),
        None,
        Some(("ITEMS_W", "West", "4")),
        Some(("ITEMS_CE", "Directly below you", "5")),
        Some(("ITEMS_E", "East", "6")),
        Some(("ITEMS_DOWN", "Below", ">")),
    ],
    [
        Some(("ITEMS_AROUND", "Surrounding area", "A")),
        Some(("ITEMS_WORN", "Worn Items", "W")),
        None,
        Some(("ITEMS_SW", "South West", "1")),
        Some(("ITEMS_S", "South", "2")),
        Some(("ITEMS_SE", "South East", "3")),
        None,
    ],
];

/// Height of the item description under the panes.
const DESCRIPTION_HEIGHT: f32 = 96.0;

/// How long a typed answer waits for the prompt it is meant for.
const PROMPT_WAIT: std::time::Duration = std::time::Duration::from_secs(5);

/// One input for the engine, sent once the previous one has finished.
pub enum Step {
    Choose(String),
    Action(String),
    /// Answers the text prompt a previous action opened.
    Fill(String),
}

#[derive(Default)]
pub struct Aim {
    steps: VecDeque<Step>,
    /// The filter box text of each pane, and the filter the engine last reported for it.
    filters: [(String, String); 2],
    amount: String,
    /// When the front step began waiting for a prompt.
    waiting: Option<std::time::Duration>,
    /// A text box of the screen has the keyboard, so typed keys are text and not game keys.
    pub(super) editing: bool,
}

/// What a frame of the screen collects: the controls in Tab order, inputs to send, the text to describe.
#[derive(Default)]
struct Out {
    ids: Vec<egui::Id>,
    todo: Vec<Step>,
    described: Option<String>,
}

#[derive(Clone)]
struct Dragged {
    pane: usize,
    row: String,
}

/// The action that makes pane `index` the one the engine acts on.
fn side(index: usize) -> &'static str {
    if index == 0 { "LEFT" } else { "RIGHT" }
}

impl<M: GameModel> App<M> {
    /// Sends the next queued input once the engine is free. A step that cannot apply (an action the
    /// engine does not offer, a prompt that never opened) drops the rest, which belonged to it.
    pub(super) fn run_aim_steps(&mut self) {
        if self.model.busy() {
            return;
        }
        let Some(step) = self.aim.steps.front() else {
            self.aim.waiting = None;
            return;
        };
        let sent = match step {
            Step::Choose(id) => {
                let id = id.clone();
                self.model.choose(&id);
                true
            }
            Step::Action(id) => {
                let id = id.clone();
                self.model.invoke_action(&id)
            }
            Step::Fill(text) => match self.model.menu().and_then(|m| m.field) {
                Some(field) => {
                    let text = text.clone();
                    self.model.fill(&field.id, &text, true);
                    true
                }
                None => {
                    // The prompt an earlier action opens can arrive after that action completed.
                    let since = *self.aim.waiting.get_or_insert_with(|| self.clock.now());
                    if self.clock.now().saturating_sub(since) < PROMPT_WAIT {
                        return;
                    }
                    false
                }
            },
        };
        self.aim.waiting = None;
        if sent {
            self.aim.steps.pop_front();
        } else {
            self.aim.steps.clear();
        }
    }

    pub(super) fn aim_do(&mut self, steps: impl IntoIterator<Item = Step>) {
        self.aim.steps.extend(steps);
    }

    /// Draws the menu; true when it took keyboard focus (on `entered`) this frame.
    pub(super) fn aim(&mut self, ctx: &egui::Context, menu: &Menu, _entered: bool) -> bool {
        a11y::trap_tab(
            ctx,
            self.trapped_by_menu(),
            super::menu_forwards_keys(Some(menu)),
        );
        let offered = self
            .model
            .actions()
            .into_iter()
            .map(|a| a.id)
            .collect::<Vec<_>>();
        let has = |id: &str| offered.iter().any(|a| a == id);
        let mut out = Out::default();
        self.aim.editing = false;
        let mut cancel = self.palette.is_none()
            && ctx.input(|i| i.key_pressed(egui::Key::Escape))
            && menu.can_cancel;
        let title = bn_client_core::markup::plain(&menu.title);
        egui::Window::new(&title)
            .id(a11y::dialog_id(&title))
            .anchor(egui::Align2::CENTER_TOP, super::dialog_offset(ctx))
            .collapsible(false)
            .default_width(1100.0)
            .show(ctx, |ui| {
                ui.columns(2, |cols| {
                    for (index, ui) in cols.iter_mut().enumerate() {
                        let Some(pane) = menu.panes.get(index) else {
                            continue;
                        };
                        let rows = menu
                            .choices
                            .iter()
                            .filter(|c| c.group.as_deref() == Some(pane.id.as_str()))
                            .collect::<Vec<_>>();
                        self.aim_pane(ui, (index, pane, &rows), &has, &mut out);
                    }
                });
                // Always the same height, so the buttons below do not move as the cursor row changes.
                ui.separator();
                let text = out.described.take().unwrap_or_default();
                egui::ScrollArea::vertical()
                    .id_salt("aim_description")
                    .max_height(DESCRIPTION_HEIGHT)
                    .show(ui, |ui| {
                        ui.set_min_height(DESCRIPTION_HEIGHT);
                        ui.set_min_width(ui.available_width());
                        ui.label(self.rich(ui, &text));
                    });
                ui.separator();
                ui.horizontal(|ui| {
                    for (label, action, tip) in [
                        ("Move", "MOVE_SINGLE_ITEM", "Move one of the selected item"),
                        (
                            "Move stack",
                            "MOVE_ITEM_STACK",
                            "Move the whole selected stack",
                        ),
                        ("Examine", "EXAMINE", "Read the selected item's description"),
                        ("Sort", "SORT", "Change the sorting of the source pane"),
                    ] {
                        self.aim_button(ui, &mut out, (label, action, tip), has(action));
                    }
                    let edit = egui::TextEdit::singleline(&mut self.aim.amount)
                        .desired_width(50.0)
                        .hint_text("Amount");
                    let response = ui.add(edit);
                    ui.ctx().accesskit_node_builder(response.id, |b| {
                        b.set_label("Amount to move");
                    });
                    out.ids.push(response.id);
                    self.aim.editing |= response.has_focus();
                    let amount = self
                        .aim
                        .amount
                        .trim()
                        .parse::<u64>()
                        .ok()
                        .filter(|n| *n > 0);
                    let go = ui.add_enabled(
                        amount.is_some() && has("MOVE_VARIABLE_ITEM") && !self.model.busy(),
                        egui::Button::new("Move amount"),
                    );
                    a11y::button(ui.ctx(), go.id, false);
                    out.ids.extend(go.enabled().then_some(go.id));
                    if let (true, Some(n)) = (go.clicked(), amount) {
                        out.todo.push(Step::Action("MOVE_VARIABLE_ITEM".into()));
                        out.todo.push(Step::Fill(n.to_string()));
                    }
                    let all = (
                        "Move all",
                        "MOVE_ALL_ITEMS",
                        "Move every item of the source pane",
                    );
                    self.aim_button(ui, &mut out, all, has(all.1));
                    if menu.can_cancel {
                        let close = ui.button("Close");
                        a11y::button(ui.ctx(), close.id, false);
                        out.ids.push(close.id);
                        cancel |= close.clicked();
                    }
                });
            });
        self.menu_ids = out.ids;
        a11y::dialog(ctx, &title, &menu.message);
        self.aim_do(out.todo);
        if cancel {
            self.model.cancel_menu();
        }
        false
    }

    fn aim_button(
        &self,
        ui: &mut egui::Ui,
        out: &mut Out,
        (label, action, tip): (&str, &str, &str),
        offered: bool,
    ) {
        let b = ui
            .add_enabled(offered && !self.model.busy(), egui::Button::new(label))
            .on_hover_text(tip);
        a11y::button(ui.ctx(), b.id, false);
        out.ids.extend(b.enabled().then_some(b.id));
        if b.clicked() {
            out.todo.push(Step::Action(action.into()));
        }
    }

    fn aim_pane(
        &mut self,
        ui: &mut egui::Ui,
        (index, pane, rows): (usize, &MenuPane, &[&MenuChoice]),
        has: &dyn Fn(&str) -> bool,
        out: &mut Out,
    ) {
        let name = if index == 0 {
            "Left pane"
        } else {
            "Right pane"
        };
        let zone = ui.dnd_drop_zone::<Dragged, _>(egui::Frame::group(ui.style()), |ui| {
            ui.set_min_width(ui.available_width());
            // The title on the left, the area picker at the top right, as the native header draws them.
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| {
                    ui.heading(format!("{name}: {}", pane.area));
                    if let Some(totals) = &pane.totals {
                        ui.label(format!(
                            "{}  {}  Sort: {}",
                            totals.weight_text(),
                            totals.volume_text(),
                            pane.sort
                        ));
                    }
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                    ui.vertical(|ui| {
                        // The native grid: marker columns, the compass with Above and Below, as drawn by the game.
                        egui::Grid::new(("aim_areas", index))
                            .spacing([4.0, 2.0])
                            .show(ui, |ui| {
                                for row in GRID {
                                    for cell in row {
                                        let Some((action, area, mark)) = cell else {
                                            ui.label("");
                                            continue;
                                        };
                                        let on = pane.area == area;
                                        let blocked = pane.blocked.iter().any(|b| b == action);
                                        let b = ui
                                            .add_enabled(
                                                has(action) && !self.model.busy(),
                                                egui::Button::new(
                                                    egui::RichText::new(mark)
                                                        .color(area_color(on, blocked)),
                                                )
                                                .selected(on)
                                                .min_size(egui::vec2(24.0, 0.0)),
                                            )
                                            .on_hover_text(area);
                                        a11y::tab(ui.ctx(), b.id, on);
                                        ui.ctx().accesskit_node_builder(b.id, |n| {
                                            n.set_label(format!("{name} area {area}"));
                                        });
                                        out.ids.extend(b.enabled().then_some(b.id));
                                        if b.clicked() {
                                            out.todo.push(Step::Action(side(index).into()));
                                            out.todo.push(Step::Action(action.into()));
                                        }
                                    }
                                    ui.end_row();
                                }
                            });
                        if has("TOGGLE_VEH") {
                            let b = ui.add_enabled(!self.model.busy(), egui::Button::new("V"));
                            b.clone().on_hover_text("Vehicle storage");
                            ui.ctx().accesskit_node_builder(b.id, |n| {
                                n.set_label(format!("{name} area vehicle storage"));
                            });
                            out.ids.extend(b.enabled().then_some(b.id));
                            if b.clicked() {
                                out.todo.push(Step::Action(side(index).into()));
                                out.todo.push(Step::Action("TOGGLE_VEH".into()));
                            }
                        }
                    });
                });
            });
            // The filter box follows what the engine reports until the player types in it.
            let (typed, seen) = &mut self.aim.filters[index];
            if *seen != pane.filter {
                seen.clone_from(&pane.filter);
                typed.clone_from(&pane.filter);
            }
            let edit = egui::TextEdit::singleline(typed)
                .hint_text("Filter")
                .desired_width(f32::INFINITY);
            let response = ui.add(edit);
            ui.ctx().accesskit_node_builder(response.id, |n| {
                n.set_label(format!("{name} filter"));
            });
            out.ids.push(response.id);
            self.aim.editing |= response.has_focus();
            if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                out.todo.push(Step::Action(side(index).into()));
                if typed.trim().is_empty() {
                    out.todo.push(Step::Action("RESET_FILTER".into()));
                } else {
                    out.todo.push(Step::Action("FILTER".into()));
                    out.todo.push(Step::Fill(typed.clone()));
                }
            }
            egui::ScrollArea::vertical()
                .id_salt(("aim_rows", index))
                .max_height(ui.ctx().content_rect().height() * 0.45)
                .show(ui, |ui| {
                    ui.set_min_height(120.0);
                    for row in rows {
                        ui.horizontal(|ui| {
                            let text = self.rich(ui, &row.label);
                            let shown = ui.selectable_label(row.highlighted, text);
                            let label =
                                ui.interact(shown.rect, shown.id, egui::Sense::click_and_drag());
                            label.dnd_set_drag_payload(Dragged {
                                pane: index,
                                row: row.id.clone(),
                            });
                            ui.ctx().accesskit_node_builder(label.id, |b| {
                                b.set_role(accesskit::Role::ListBoxOption);
                                b.set_label(bn_client_core::markup::plain(&row.label));
                            });
                            if label.clicked() {
                                out.todo.push(Step::Choose(row.id.clone()));
                                label.surrender_focus();
                            }
                            if label.hovered() || (row.highlighted && out.described.is_none()) {
                                out.described = Some(row.description.clone());
                            }
                            for key in ["count", "weight", "volume"] {
                                let value = row.column(key);
                                if !value.is_empty() {
                                    ui.weak(value);
                                }
                            }
                        });
                    }
                });
        });
        if let Some(d) = zone.1.filter(|d| d.pane != index) {
            out.todo.push(Step::Choose(d.row.clone()));
            out.todo.push(Step::Action("MOVE_SINGLE_ITEM".into()));
        }
    }
}

/// The colour the native picker draws an area's mark in: red when the pane cannot put items there,
/// white for the pane's own area, dark gray for the rest.
fn area_color(on: bool, blocked: bool) -> egui::Color32 {
    match (blocked, on) {
        (true, _) => egui::Color32::from_rgb(255, 60, 60),
        (false, true) => egui::Color32::WHITE,
        (false, false) => egui::Color32::from_gray(110),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocked_areas_are_red_the_panes_own_white_and_the_rest_dark_gray() {
        let red = area_color(false, true);
        assert!(red.r() > 200 && red.g() < 100 && red.b() < 100);
        assert_eq!(area_color(true, true), red, "blocked wins over selected");
        assert_eq!(area_color(true, false), egui::Color32::WHITE);
        assert!(area_color(false, false).r() < 140);
    }
}
