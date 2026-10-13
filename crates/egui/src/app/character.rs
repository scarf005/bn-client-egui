//! The native character screens as tabbed windows: the character sheet (`@`) with its stats,
//! encumbrance, skills, traits, bionics and effects tabs and the speed breakdown, the bionics list
//! (`p`) with its active and passive tabs, and the mutation list (`[`) with its passive and active
//! columns. The engine owns every list and every action; a click only chooses a row or runs an action.
use super::menu::list_row_style;
use super::{App, a11y};
use crate::text;
use crate::windows::{self, WindowSpec};
use bn_client_core::{GameModel, Menu, MenuChoice, markup};
use eframe::egui::{self, accesskit};

/// The contexts this module draws.
pub(super) const CONTEXTS: [&str; 3] = ["PLAYER_INFO", "BIONICS", "MUTATIONS"];

/// What a window collects while it is drawn, applied once it is done.
#[derive(Default)]
struct Out {
    ids: Vec<egui::Id>,
    chosen: Option<String>,
    action: Option<String>,
    cancel: bool,
    focused: bool,
    /// The text of the row under the pointer, else of the engine's cursor row.
    described: Option<String>,
}

/// How one list prints its rows: the keys of the value columns.
struct Spec<'a> {
    columns: &'a [&'a str],
}

fn colored(text: &str, color: &str) -> String {
    match color {
        "" => text.to_string(),
        color => format!("<color_{color}>{text}</color>"),
    }
}

/// `text` followed by spaces up to `width` characters of its visible text.
fn pad(text: &str, width: usize) -> String {
    let shown = markup::plain(text).chars().count();
    format!("{text}{}", " ".repeat(width.saturating_sub(shown)))
}

/// A row the engine lists only to name the group under it, such as a skill category.
fn is_header(row: &MenuChoice) -> bool {
    !row.enabled && row.denial.is_empty() && row.columns.is_empty()
}

fn row_markup(row: &MenuChoice, spec: &Spec, widths: &[usize]) -> String {
    let key = match row.hotkey.as_str() {
        "" => String::new(),
        key => format!("{} ", colored(key, "light_green")),
    };
    let mut line = format!("{key}{}", pad(&colored(&name(row), &row.color), widths[0]));
    for (index, key) in spec.columns.iter().enumerate() {
        let value = colored(row.column(key), column_color(row, key));
        line += &format!("  {}", pad(&value, widths[index + 1]));
    }
    line.trim_end().to_string()
}

/// The game color of the value of the column `key`; empty when the engine sent none.
fn column_color<'a>(row: &'a MenuChoice, key: &str) -> &'a str {
    row.columns
        .iter()
        .find(|c| c.key == key)
        .and_then(|c| c.color.as_deref())
        .unwrap_or("")
}

/// A row's name; a bionic's label ends with the power the `power` column already holds.
fn name(row: &MenuChoice) -> String {
    let power = row.column("power");
    match row.label.strip_suffix(power) {
        Some(name) if !power.is_empty() => name.trim_end_matches(", ").to_string(),
        _ => row.label.clone(),
    }
}

/// The engine's caption of the column `key`, from the first row that has it.
fn caption(rows: &[&MenuChoice], key: &str) -> String {
    let mut labels = rows.iter().map(|r| r.column_label(key));
    labels
        .find(|l| !l.is_empty())
        .unwrap_or_default()
        .to_string()
}

/// The widths of the label and of each value column over `rows`, the header names included.
fn widths(rows: &[&MenuChoice], spec: &Spec) -> Vec<usize> {
    let width = |text: &str| markup::plain(text).chars().count();
    let label = rows
        .iter()
        .filter(|r| !is_header(r))
        .map(|r| width(&name(r)))
        .max()
        .unwrap_or(0);
    std::iter::once(label)
        .chain(spec.columns.iter().map(|key| {
            let longest = rows.iter().map(|r| width(r.column(key))).max();
            longest.unwrap_or(0).max(width(&caption(rows, key)))
        }))
        .collect()
}

/// A body part's used and total bionic slots, such as `Head: 0/18`.
fn is_slots(line: &str) -> bool {
    let count = |n: &str| n.parse::<u32>().is_ok();
    line.rsplit_once(": ")
        .filter(|(name, _)| !name.contains(": "))
        .and_then(|(_, n)| n.split_once('/'))
        .is_some_and(|(used, total)| count(used) && count(total))
}

fn tab_rows(menu: &Menu) -> Vec<&MenuChoice> {
    menu.choices.iter().filter(|c| c.tab).collect()
}

fn group<'a>(menu: &'a Menu, id: &str) -> Vec<&'a MenuChoice> {
    let all = menu.choices.iter();
    all.filter(|c| !c.tab && c.group.as_deref() == Some(id))
        .collect()
}

impl<M: GameModel> App<M> {
    /// Draws the character screen of `menu.context`; true when it took keyboard focus (on `entered`).
    pub(super) fn character(&mut self, ctx: &egui::Context, menu: &Menu, entered: bool) -> bool {
        a11y::trap_tab(ctx, self.trapped_by_menu(), true);
        let mut out = Out {
            cancel: self.palette.is_none()
                && ctx.input(|i| i.key_pressed(egui::Key::Escape))
                && menu.can_cancel,
            ..Default::default()
        };
        let title = markup::plain(&menu.title);
        let kind = format!("character:{}", menu.context);
        let width = (ctx.content_rect().width() * 0.8).clamp(560.0, 900.0);
        let margin = egui::Frame::window(&ctx.global_style()).total_margin();
        let spec = WindowSpec {
            closable: menu.can_cancel,
            width: Some(width + margin.sum().x),
            ..self.window_spec(ctx, &kind, &title)
        };
        let shown = windows::show(ctx, spec, |ui| {
            ui.set_width(width);
            match menu.context.as_str() {
                "PLAYER_INFO" => self.sheet(ui, menu, entered, &mut out),
                "BIONICS" => self.bionics(ui, menu, &mut out),
                _ => self.mutations(ui, menu, &mut out),
            }
        });
        if self.keep_window(ctx, &kind, &shown) {
            self.model.cancel_menu();
        }
        self.menu_ids = std::mem::take(&mut out.ids);
        a11y::dialog(ctx, &title, &markup::plain(&menu.message));
        if let Some(id) = out.action {
            ctx.request_repaint();
            self.model.invoke_action(&id);
        } else if let Some(id) = out.chosen {
            ctx.request_repaint();
            self.model.choose(&id);
        } else if out.cancel {
            self.model.cancel_menu();
        }
        out.focused
    }

    /// The tab headers of the menu as buttons; clicking one chooses it.
    fn tab_strip(&self, ui: &mut egui::Ui, menu: &Menu, entered: bool, out: &mut Out) {
        ui.horizontal_wrapped(|ui| {
            for tab in tab_rows(menu) {
                let text = self.rich(ui, &tab.label);
                let button = self.control(ui, text, &tab.id, tab.enabled, tab.selected);
                a11y::tab(ui.ctx(), button.id, tab.selected);
                out.ids.extend(button.enabled().then_some(button.id));
                if entered && tab.selected {
                    button.request_focus();
                    out.focused = button.has_focus();
                }
                if button.clicked() {
                    out.chosen = Some(tab.id.clone());
                }
            }
        });
    }

    /// The panes the engine lists one at a time (bionics: active and passive) as tabs; any other
    /// than the current one is the engine's `NEXT_TAB`.
    fn pane_strip(&self, ui: &mut egui::Ui, menu: &Menu, out: &mut Out) {
        ui.horizontal_wrapped(|ui| {
            for (id, label, current) in &menu.pane_tabs {
                let button = self.control(ui, label.clone(), id, true, *current);
                a11y::tab(ui.ctx(), button.id, *current);
                out.ids.extend(button.enabled().then_some(button.id));
                if button.clicked() && !current {
                    out.action = Some("NEXT_TAB".to_string());
                }
            }
        });
    }

    /// Rows as a monospace table under a header of the value names; a click chooses a row.
    fn table(&self, ui: &mut egui::Ui, rows: &[&MenuChoice], spec: &Spec, out: &mut Out) {
        let widths = widths(rows, spec);
        let font = egui::TextStyle::Monospace.resolve(ui.style());
        if !rows.is_empty() {
            let key = if rows.iter().any(|r| !r.hotkey.is_empty()) {
                "  "
            } else {
                ""
            };
            let names = spec
                .columns
                .iter()
                .zip(&widths[1..])
                .fold(pad("", widths[0]), |line, (key, width)| {
                    format!("{line}  {}", pad(&caption(rows, key), *width))
                });
            ui.weak(egui::RichText::new(format!("{key}{names}")).font(font.clone()));
        }
        for row in rows {
            if is_header(row) {
                ui.add_space(4.0);
                ui.strong(markup::plain(&row.label));
                continue;
            }
            let markup = row_markup(row, spec, &widths);
            let runs = self.model.styled(&markup);
            let button = ui
                .scope(|ui| {
                    list_row_style(ui);
                    ui.with_layout(egui::Layout::top_down_justified(egui::Align::Min), |ui| {
                        let text = text::job(&runs, &font);
                        self.control(ui, text, &row.id, row.enabled, row.highlighted)
                    })
                    .inner
                })
                .inner;
            out.ids.extend(button.enabled().then_some(button.id));
            ui.ctx().accesskit_node_builder(button.id, |b| {
                b.set_role(accesskit::Role::MenuItem);
                b.set_label(markup::plain(&markup));
            });
            if !row.denial.is_empty() {
                button.clone().on_disabled_hover_text(&row.denial);
            }
            if button.hovered() || (row.highlighted && out.described.is_none()) {
                out.described = Some(row.description.clone());
            }
            if button.clicked() {
                out.chosen = Some(row.id.clone());
            }
        }
    }

    /// The description of the row under the pointer or the cursor, in a block that keeps its height.
    fn description(&self, ui: &mut egui::Ui, text: Option<&str>, height: f32) {
        ui.separator();
        egui::ScrollArea::vertical()
            .id_salt("character_description")
            .max_height(height)
            .show(ui, |ui| {
                ui.set_min_height(height);
                ui.set_min_width(ui.available_width());
                if let Some(text) = text.filter(|t| !t.is_empty()) {
                    ui.label(self.rich(ui, text));
                }
            });
    }

    /// The menu's own buttons: each row of `rows` chooses it, each offered action in `actions` runs it.
    fn buttons(&self, ui: &mut egui::Ui, rows: &[&MenuChoice], actions: &[&str], out: &mut Out) {
        let offered = self.model.actions();
        ui.horizontal_wrapped(|ui| {
            for row in rows {
                let button = self.control(ui, row.label.clone(), &row.id, row.enabled, false);
                out.ids.extend(button.enabled().then_some(button.id));
                if !row.denial.is_empty() {
                    button.clone().on_disabled_hover_text(&row.denial);
                }
                if button.clicked() {
                    out.chosen = Some(row.id.clone());
                }
            }
            for id in actions {
                let Some(action) = offered.iter().find(|a| a.id == *id) else {
                    continue;
                };
                let name = markup::plain(&action.name);
                let label = match action.keys.first() {
                    Some(key) => format!("{name} [{key}]"),
                    None => name,
                };
                let button = self.control(ui, label, id, true, false);
                out.ids.extend(button.enabled().then_some(button.id));
                if button.clicked() {
                    out.action = Some((*id).to_string());
                }
            }
        });
    }

    fn close(&self, ui: &mut egui::Ui, menu: &Menu, out: &mut Out) {
        if menu.can_cancel {
            let close = ui.add_enabled(self.idle(), egui::Button::new("Close"));
            a11y::button(ui.ctx(), close.id, false);
            out.ids.extend(close.enabled().then_some(close.id));
            out.cancel |= close.clicked();
        }
    }

    /// The character sheet: the selected tab's rows beside the speed breakdown.
    fn sheet(&mut self, ui: &mut egui::Ui, menu: &Menu, entered: bool, out: &mut Out) {
        ui.label(self.rich(ui, &menu.message));
        self.tab_strip(ui, menu, entered, out);
        let pane = menu.pane.as_deref().unwrap_or_default();
        let rows = group(menu, pane);
        let spec = match pane {
            "stats" => Spec {
                columns: &["current", "base"],
            },
            "encumbrance" => Spec {
                columns: &["encumbrance", "layering", "warmth"],
            },
            "skills" => Spec {
                columns: &["level", "exercise"],
            },
            _ => Spec { columns: &[] },
        };
        let height = super::dialog_scroll_height(ui.ctx()) * 0.6;
        ui.columns(2, |cols| {
            egui::ScrollArea::vertical()
                .id_salt(("character_rows", pane))
                .max_height(height)
                .show(&mut cols[0], |ui| {
                    ui.set_min_width(ui.available_width());
                    self.table(ui, &rows, &spec, out);
                });
            self.speed(&mut cols[1], menu);
        });
        self.description(ui, out.described.clone().as_deref(), 110.0);
        self.buttons(ui, &group(menu, "controls"), &[], out);
        self.close(ui, menu, out);
    }

    /// The speed breakdown: each line's name and value in the color the native sheet prints it.
    fn speed(&self, ui: &mut egui::Ui, menu: &Menu) {
        ui.strong("SPEED");
        egui::Grid::new("character_speed")
            .spacing([12.0, 2.0])
            .show(ui, |ui| {
                for row in group(menu, "speed") {
                    ui.label(self.rich(ui, &row.label));
                    let value = colored(row.column("value"), column_color(row, "value"));
                    ui.label(self.rich(ui, &value));
                    ui.end_row();
                }
            });
    }

    /// The bionics list: power and fuel above, the active or passive tab's rows, the native actions below.
    fn bionics(&mut self, ui: &mut egui::Ui, menu: &Menu, out: &mut Out) {
        // The first line is the power, the second the key help the buttons replace; the rest are the
        // fuel and the body part slots (`Head: 0/18`), which share one line.
        let (slots, lines): (Vec<_>, Vec<_>) = menu
            .message
            .lines()
            .enumerate()
            .filter(|(i, l)| *i != 1 && !l.is_empty())
            .map(|(_, l)| l)
            .partition(|l| is_slots(l));
        for line in lines {
            ui.label(self.rich(ui, line));
        }
        if !slots.is_empty() {
            ui.weak(slots.join("   "));
        }
        self.pane_strip(ui, menu, out);
        let rows = menu.entries();
        let spec = Spec {
            columns: &["power", "state"],
        };
        egui::ScrollArea::vertical()
            .id_salt("bionic_rows")
            .max_height(super::dialog_scroll_height(ui.ctx()) * 0.6)
            .show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                self.table(ui, &rows, &spec, out);
            });
        let fuel = rows
            .iter()
            .find(|r| r.highlighted)
            .map(|r| r.column("fuel"))
            .filter(|f| !f.is_empty());
        let text = out.described.take().map(|d| match fuel {
            Some(fuel) => format!("{d}\n\nFuel: {fuel}"),
            None => d,
        });
        self.description(ui, text.as_deref(), 110.0);
        let actions = [
            "TOGGLE_EXAMINE",
            "REASSIGN",
            "TOGGLE_SAFE_FUEL",
            "TOGGLE_SPRITE",
            "TOGGLE_AUTO_START",
            "SORT",
        ];
        self.buttons(ui, &[], &actions, out);
        self.close(ui, menu, out);
    }

    /// The mutation list: passive and active traits side by side, as the native screen prints them.
    fn mutations(&mut self, ui: &mut egui::Ui, menu: &Menu, out: &mut Out) {
        ui.weak(&menu.message);
        let passive = group(menu, "passive");
        let active = group(menu, "active");
        let height = super::dialog_scroll_height(ui.ctx()) * 0.6;
        ui.columns(2, |cols| {
            let lists = [
                ("passive", &passive, &[][..]),
                ("active", &active, &["state"][..]),
            ];
            for (col, (name, rows, columns)) in cols.iter_mut().zip(lists) {
                let heading = menu.pane_tabs.iter().find(|(id, ..)| id == name);
                col.strong(heading.map_or("", |(_, label, _)| label.as_str()));
                let spec = Spec { columns };
                egui::ScrollArea::vertical()
                    .id_salt(("mutation_rows", name))
                    .max_height(height)
                    .show(col, |ui| {
                        ui.set_min_width(ui.available_width());
                        if rows.is_empty() {
                            ui.weak("None");
                        }
                        self.table(ui, rows, &spec, out);
                    });
            }
        });
        self.description(ui, out.described.clone().as_deref(), 110.0);
        self.buttons(ui, &group(menu, "actions"), &[], out);
        self.close(ui, menu, out);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bn_client_core::protocol::Column;

    fn row(label: &str, columns: &[(&str, &str)]) -> MenuChoice {
        MenuChoice {
            id: label.into(),
            label: label.into(),
            enabled: true,
            denial: String::new(),
            hotkey: String::new(),
            selected: false,
            group: None,
            tab: false,
            highlighted: false,
            selected_count: None,
            available_count: None,
            description: String::new(),
            columns: columns
                .iter()
                .map(|(k, v)| Column {
                    key: k.to_string(),
                    label: k.to_uppercase(),
                    value: v.to_string(),
                    color: None,
                })
                .collect(),
            favorite: false,
            color: String::new(),
            key_color: String::new(),
            markup: String::new(),
            action: String::new(),
            nest_color: String::new(),
            mark: None,
            editor: None,
        }
    }

    #[test]
    fn a_column_color_tints_its_value_and_a_row_color_its_name() {
        let mut stat = row("Strength:", &[("current", "8"), ("base", "8")]);
        stat.columns[0].color = Some("c_white".into());
        let spec = Spec {
            columns: &["current", "base"],
        };
        let line = row_markup(&stat, &spec, &[10, 7, 4]);
        assert_eq!(line, "Strength:   <color_c_white>8</color>        8");
        let mut trait_row = row("Quick", &[]);
        trait_row.color = "c_green".into();
        assert_eq!(
            row_markup(&trait_row, &Spec { columns: &[] }, &[5]),
            "<color_c_green>Quick</color>"
        );
    }

    #[test]
    fn a_header_names_the_column_with_the_engine_caption() {
        let stat = row("Strength:", &[("current", "8")]);
        assert_eq!(caption(&[&stat], "current"), "CURRENT");
        assert_eq!(caption(&[&stat], "base"), "");
    }

    #[test]
    fn only_the_colored_column_of_a_part_is_tinted() {
        let mut part = row("TORSO", &[("encumbrance", "3"), ("warmth", "9")]);
        part.columns[1].color = Some("c_red".into());
        assert_eq!(column_color(&part, "encumbrance"), "");
        assert_eq!(column_color(&part, "warmth"), "c_red");
    }

    #[test]
    fn a_slot_line_is_a_body_part_with_two_counts() {
        assert!(is_slots("Head: 0/18"));
        assert!(!is_slots("Bionic Power: 12/100kJ"));
        assert!(!is_slots("Available Fuel: Gasoline: 40/200 "));
    }

    #[test]
    fn a_row_without_values_or_denial_is_a_header() {
        assert!(is_header(&MenuChoice {
            enabled: false,
            ..row("Combat", &[])
        }));
        assert!(!is_header(&row("Melee", &[("level", "2")])));
    }
}
