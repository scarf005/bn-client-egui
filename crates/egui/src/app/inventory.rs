//! The native inventory selector (`i` and the item pickers): the carried, nearby and worn or wielded
//! lists side by side under magenta category headers, the key, mark, name and cells of each row in the
//! row's own color, the hint and weight and volume totals in the header, the cursor row's item
//! description under the lists, and the filter and category mode in the footer. Also the pieces the
//! pickup list shares: the count box of a marked stack, the key hints and the letter keys.
use super::menu::{list_frame, list_row_style};
use super::{App, a11y};
use crate::windows::{self, WindowSpec};
use bn_client_core::protocol::Column;
use bn_client_core::{ActionEntry, GameModel, Menu, MenuChoice, PaneTotals};
use eframe::egui::{self, accesskit};
use std::collections::HashMap;

#[derive(Default)]
pub struct Inventory {
    /// The count box of each marked stack: what the engine last chose, and the text being typed.
    pub(super) counts: HashMap<String, (Option<u64>, String)>,
    /// The row the cursor was last on, so the list scrolls only when it moves.
    cursor: Option<String>,
}

/// The blue of the cursor row.
pub(super) const BAR: egui::Color32 = egui::Color32::from_rgb(10, 10, 220);

/// `text` in the game color `name`, as markup.
pub(super) fn tinted(name: &str, text: &str) -> String {
    match name {
        "" => text.to_string(),
        name => format!("<color_{name}>{text}</color>"),
    }
}

/// `[key] label` for each of the actions the engine offers, in the order given.
pub(super) fn key_hints(actions: &[ActionEntry], wanted: &[(&str, &str)]) -> String {
    wanted
        .iter()
        .filter_map(|(id, label)| {
            let key = actions.iter().find(|a| a.id == *id)?.keys.first()?;
            Some(format!("[{key}] {label}"))
        })
        .collect::<Vec<_>>()
        .join("  ")
}

/// The letter typed this frame that is a row's key, which the game marks or chooses natively. A key
/// bound to an action stays that action's, and nothing is taken while a box has the keyboard.
pub(super) fn typed_hotkey<'a, M: GameModel>(
    ctx: &egui::Context,
    model: &M,
    rows: impl IntoIterator<Item = &'a MenuChoice>,
) -> Option<String> {
    if ctx.egui_wants_keyboard_input() {
        return None;
    }
    let typed = ctx.input(|i| {
        i.events.iter().find_map(|e| match e {
            egui::Event::Text(t) if t.chars().count() == 1 && !model.key_bound(t) => {
                Some(t.clone())
            }
            _ => None,
        })
    })?;
    rows.into_iter()
        .find(|r| r.enabled && r.hotkey == typed)
        .map(|r| r.id.clone())
}

/// The cells of a row: the first is the name, the others have titles the first category header prints.
fn cells(row: &MenuChoice) -> Vec<&Column> {
    row.columns
        .iter()
        .filter(|c| c.key.starts_with("cell"))
        .collect()
}

/// A row's weight and volume header as the native window prints them: `Weight (kg): 3.8 / 45.0`.
pub(super) fn totals_lines(totals: &PaneTotals) -> [(String, bool); 2] {
    let line = |name: &str, unit: &str, value: &str, max: &str, over: bool| {
        let max = if max.is_empty() {
            String::new()
        } else {
            format!(" / {max}")
        };
        (format!("{name} ({unit}): {value}{max}"), over)
    };
    [
        line(
            "Weight",
            &totals.weight_unit,
            &totals.weight,
            &totals.weight_max,
            totals.over_weight,
        ),
        line(
            "Volume",
            &totals.volume_unit,
            &totals.volume,
            &totals.volume_max,
            totals.over_volume,
        ),
    ]
}

impl<M: GameModel> App<M> {
    /// The count box of a marked stack; the count typed and submitted with Enter, when it fits the stack.
    pub(super) fn count_box(
        &mut self,
        ui: &mut egui::Ui,
        row: &MenuChoice,
        ids: &mut Vec<egui::Id>,
    ) -> Option<(String, u64)> {
        let available = row.available_count.filter(|n| *n > 1)?;
        if !row.selected {
            return None;
        }
        let (seen, text) = self
            .inventory
            .counts
            .entry(row.id.clone())
            .or_insert((None, String::new()));
        if *seen != row.selected_count {
            *seen = row.selected_count;
            *text = row.selected_count.map_or(String::new(), |n| n.to_string());
        }
        let response = ui.add(egui::TextEdit::singleline(text).desired_width(40.0));
        let name = format!("Count of {}", bn_client_core::markup::plain(&row.label));
        ui.ctx()
            .accesskit_node_builder(response.id, |b| b.set_label(name));
        ids.push(response.id);
        ui.label(format!("/ {available}"));
        let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
        // A count beyond the stack would only be refused by the engine.
        (response.lost_focus() && enter)
            .then(|| text.trim().parse::<u64>().ok())
            .flatten()
            .filter(|n| (1..=available).contains(n))
            .map(|n| (row.id.clone(), n))
    }

    /// The item description under a list: always the same height, so nothing above it moves.
    pub(super) fn item_info(&self, ui: &mut egui::Ui, title: &str, description: Option<&str>) {
        ui.separator();
        ui.label(self.rich(ui, &format!("< {title} >")));
        let height = ui.ctx().content_rect().height() * 0.25;
        egui::ScrollArea::vertical()
            .id_salt("inventory_description")
            .max_height(height)
            .show(ui, |ui| {
                ui.set_min_height(height);
                ui.set_min_width(ui.available_width());
                for line in description.iter().flat_map(|d| d.lines()) {
                    // As in the native item window, a line of two dashes is a rule.
                    match line {
                        "--" => ui.separator(),
                        _ => ui.label(self.rich(ui, line)),
                    };
                }
            });
    }

    /// Draws the menu; true when it took keyboard focus (on `entered`) this frame.
    pub(super) fn inventory(&mut self, ctx: &egui::Context, menu: &Menu, entered: bool) -> bool {
        a11y::trap_tab(
            ctx,
            self.trapped_by_menu(),
            super::menu_forwards_keys(Some(menu)),
        );
        let mut ids = Vec::new();
        let mut focused = false;
        let mut chosen = typed_hotkey(ctx, &self.model, &menu.choices);
        let mut count = None;
        let cancel = self.palette.is_none()
            && ctx.input(|i| i.key_pressed(egui::Key::Escape))
            && menu.can_cancel;
        let title = bn_client_core::markup::plain(&menu.title);
        let pane = menu.panes.first();
        // The lists in the native order, each as long as the selector has rows from it.
        let sources = ["inventory", "map_or_vehicle", "worn_or_wielded"];
        let lists = sources
            .iter()
            .map(|s| {
                menu.choices
                    .iter()
                    .filter(|c| c.column("source") == *s)
                    .collect::<Vec<_>>()
            })
            .filter(|rows| !rows.is_empty())
            .collect::<Vec<_>>();
        let frame = list_frame(ctx);
        let mut described = None;
        let spec = WindowSpec {
            closable: menu.can_cancel,
            width: Some((lists.len().max(1) as f32 * 420.0).min(ctx.content_rect().width() * 0.9)),
            frame: Some(frame),
            ..self.window_spec(ctx, "inventory", &title)
        };
        let hint = pane.map(|p| &p.description).filter(|h| !h.is_empty());
        let totals = pane.and_then(|p| p.totals.as_ref());
        let shown = windows::show(ctx, spec, |ui| {
            // A header with nothing to say takes no line.
            if hint.is_some() || totals.is_some() {
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        if let Some(hint) = hint {
                            ui.label(self.rich(ui, &tinted("c_dark_gray", hint)));
                        }
                    });
                    if let Some(totals) = totals {
                        ui.with_layout(egui::Layout::top_down(egui::Align::Max), |ui| {
                            for (line, over) in totals_lines(totals) {
                                let tone = if over { "c_red" } else { "c_dark_gray" };
                                ui.label(self.rich(ui, &tinted(tone, &line)));
                            }
                        });
                    }
                });
                ui.separator();
            }
            if lists.is_empty() {
                ui.label(self.rich(ui, &tinted("c_dark_gray", &menu.message)));
            }
            let height = ctx.content_rect().height() * 0.45;
            ui.columns(lists.len().max(1), |cols| {
                for (index, (ui, rows)) in cols.iter_mut().zip(&lists).enumerate() {
                    let first = entered && index == 0;
                    focused |= self.inventory_list(
                        ui,
                        (index, rows, first, height),
                        (&mut ids, &mut chosen, &mut count, &mut described),
                    );
                }
            });
            let hovered = described.take();
            let cursor = menu.choices.iter().find(|c| c.highlighted);
            let shown = hovered.or(cursor);
            self.item_info(
                ui,
                &shown
                    .map(|c| tinted(&c.color, &c.label))
                    .unwrap_or_default(),
                shown.map(|c| c.description.as_str()),
            );
            ui.separator();
            ui.horizontal(|ui| {
                let actions = self.model.actions();
                let filter = pane.map_or("", |p| p.filter.as_str());
                let text = match filter {
                    "" => key_hints(&actions, &[("INVENTORY_FILTER", "Filter")]),
                    f => key_hints(&actions, &[("INVENTORY_FILTER", "Filter:")]) + " " + f,
                };
                ui.label(self.rich(ui, &tinted("c_light_gray", &text)));
                if menu.category_mode == Some(true) {
                    ui.label(self.rich(ui, &tinted("h_white", "Category selection mode")));
                }
            });
        });
        if self.keep_window(ctx, "inventory", &shown) {
            self.model.cancel_menu();
        }
        self.menu_ids = ids;
        a11y::dialog(ctx, &title, &menu.message);
        if let Some((id, n)) = count {
            ctx.request_repaint();
            self.model.set_count(&id, n);
        } else if let Some(id) = chosen {
            ctx.request_repaint();
            self.model.choose(&id);
        } else if cancel {
            self.model.cancel_menu();
        }
        focused
    }

    /// One of the lists: category headers over rows of key, mark, name and right-aligned cells.
    fn inventory_list<'a>(
        &mut self,
        ui: &mut egui::Ui,
        (index, rows, first, height): (usize, &[&'a MenuChoice], bool, f32),
        (ids, chosen, count, described): (
            &mut Vec<egui::Id>,
            &mut Option<String>,
            &mut Option<(String, u64)>,
            &mut Option<&'a MenuChoice>,
        ),
    ) -> bool {
        let mut focused = false;
        egui::ScrollArea::vertical()
            .id_salt(("inventory_rows", index))
            .max_height(height)
            .show(ui, |ui| {
                egui::Grid::new(("inventory_table", index))
                    .spacing([10.0, 0.0])
                    .show(ui, |ui| {
                        // A cell no row of the list fills is not shown, as in the native column.
                        let shown_cell = |at: usize| {
                            rows.iter()
                                .any(|r| cells(r).get(at).is_some_and(|c| !c.value.is_empty()))
                        };
                        let mut category = None;
                        for (i, row) in rows.iter().enumerate() {
                            let shown = row.column("category");
                            if category != Some(shown) && !shown.is_empty() {
                                if category.is_none() {
                                    ui.label(self.rich(ui, &tinted("c_magenta", shown)));
                                    for (at, cell) in cells(row).iter().enumerate().skip(1) {
                                        if !shown_cell(at) {
                                            continue;
                                        }
                                        let title = tinted("c_magenta", &cell.label);
                                        ui.label(self.rich(ui, &title));
                                    }
                                } else {
                                    ui.label(self.rich(ui, &tinted("c_magenta", shown)));
                                }
                                category = Some(shown);
                                ui.end_row();
                            }
                            let own = cells(row);
                            let name = own.first().map_or(row.label.as_str(), |c| c.value.as_str());
                            let key = match row.hotkey.as_str() {
                                "" => " ".to_string(),
                                k => tinted(&row.key_color, k),
                            };
                            let mark = row.mark.as_ref().map_or(String::new(), |m| {
                                format!("{} ", tinted(m.color.as_deref().unwrap_or(""), &m.glyph))
                            });
                            let tone = if row.enabled {
                                row.color.as_str()
                            } else {
                                "c_dark_gray"
                            };
                            let button = ui
                                .horizontal(|ui| {
                                    list_row_style(ui);
                                    // The native row keeps the name's own colors, such as green charges, unless
                                    // it is refused.
                                    let name = match (row.enabled, row.markup.as_str()) {
                                        (true, m) if !m.is_empty() => m,
                                        _ => name,
                                    };
                                    let text = self
                                        .rich(ui, &format!("{key} {mark}{}", tinted(tone, name)));
                                    let button = self.control(
                                        ui,
                                        text,
                                        &row.id,
                                        row.enabled,
                                        row.highlighted,
                                    );
                                    if let Some(c) = self.count_box(ui, row, ids) {
                                        *count = Some(c);
                                    }
                                    if !row.enabled && !row.denial.is_empty() {
                                        ui.label(self.rich(ui, &tinted("c_red", &row.denial)));
                                    }
                                    button
                                })
                                .inner;
                            ids.extend(button.enabled().then_some(button.id));
                            ui.ctx().accesskit_node_builder(button.id, |b| {
                                b.set_role(accesskit::Role::MenuItem);
                                b.set_label(bn_client_core::markup::plain(name));
                                if !row.denial.is_empty() {
                                    b.set_description(bn_client_core::markup::plain(&row.denial));
                                }
                            });
                            if row.highlighted && self.inventory.cursor.as_deref() != Some(&row.id)
                            {
                                button.scroll_to_me(None);
                                self.inventory.cursor = Some(row.id.clone());
                            }
                            if first && i == 0 {
                                button.request_focus();
                                focused = button.has_focus();
                            }
                            if button.clicked() {
                                *chosen = Some(row.id.clone());
                            }
                            if button.hovered() {
                                *described = Some(*row);
                            }
                            for (at, cell) in own.iter().enumerate().skip(1) {
                                let value = &cell.value;
                                if !shown_cell(at) {
                                    continue;
                                }
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        if row.highlighted {
                                            // The cursor bar runs across the cells too.
                                            let bar = egui::RichText::new(value)
                                                .color(egui::Color32::WHITE)
                                                .background_color(BAR);
                                            ui.label(bar)
                                        } else {
                                            ui.label(self.rich(ui, &tinted(tone, value)))
                                        }
                                    },
                                );
                            }
                            ui.end_row();
                        }
                    });
            });
        focused
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn totals(over_weight: bool, weight_max: &str) -> PaneTotals {
        PaneTotals {
            weight: "3.8".into(),
            weight_max: weight_max.into(),
            weight_unit: "kg".into(),
            volume: "1.20".into(),
            volume_max: "15.00".into(),
            volume_unit: "L".into(),
            over_weight,
            over_volume: false,
        }
    }

    #[test]
    fn the_header_prints_carried_against_capacity_like_the_native_window() {
        let [weight, volume] = totals_lines(&totals(true, "45.0"));
        assert_eq!(weight, ("Weight (kg): 3.8 / 45.0".to_string(), true));
        assert_eq!(volume, ("Volume (L): 1.20 / 15.00".to_string(), false));
    }

    #[test]
    fn a_header_without_capacity_prints_only_the_carried_value() {
        let [weight, _] = totals_lines(&totals(false, ""));
        assert_eq!(weight.0, "Weight (kg): 3.8");
    }

    #[test]
    fn the_hints_list_only_the_actions_the_engine_offers() {
        let action = |id: &str, key: &str| ActionEntry {
            id: id.into(),
            name: id.into(),
            keys: vec![key.into()],
        };
        let actions = [action("LEFT", "<"), action("RIGHT", ">")];
        let hints = key_hints(
            &actions,
            &[("LEFT", "Unmark"), ("SELECT_ALL", "All"), ("RIGHT", "Mark")],
        );
        assert_eq!(hints, "[<] Unmark  [>] Mark");
    }
}
