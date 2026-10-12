//! The native character screens as tabbed windows: the character sheet (`@`) with its stats,
//! encumbrance, skills, traits, bionics and effects tabs and the speed breakdown, the bionics list
//! (`p`) with its active and passive tabs, and the mutation list (`[`) with its passive and active
//! columns. The engine owns every list and every action; a click only chooses a row or runs an action.
use super::menu::list_row_style;
use super::{App, a11y};
use crate::text;
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

/// Which part of a row the engine's plain `Color` column colors.
#[derive(Clone, Copy, PartialEq)]
enum Tint {
    Label,
    FirstColumn,
}

/// How one list prints its rows: the key column, the value columns, and where its colors apply.
struct Spec<'a> {
    key: &'a str,
    columns: &'a [&'a str],
    tint: Tint,
}

fn column<'a>(row: &'a MenuChoice, label: &str) -> &'a str {
    let found = row.columns.iter().find(|(l, _)| l == label);
    found.map_or("", |(_, v)| v.as_str())
}

/// A row's value under the column `name`; the first column also takes the value the engine labels with the row's own name.
fn cell<'a>(row: &'a MenuChoice, index: usize, name: &str) -> &'a str {
    match column(row, name) {
        "" if index == 0 => column(row, &row.label),
        found => found,
    }
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

/// The color of a value column: its `<name> color` column, else the plain `Color` of the first column.
fn value_color<'a>(row: &'a MenuChoice, spec: &Spec, index: usize) -> &'a str {
    let name = spec.columns[index];
    match column(row, &format!("{name} color")) {
        "" if spec.tint == Tint::FirstColumn && index == 0 => column(row, "Color"),
        found => found,
    }
}

fn row_markup(row: &MenuChoice, spec: &Spec, widths: &[usize]) -> String {
    let key = match column(row, spec.key) {
        "" => String::new(),
        key => format!("{} ", colored(key, "light_green")),
    };
    let label_color = match spec.tint {
        Tint::Label => column(row, "Color"),
        Tint::FirstColumn => "",
    };
    let mut line = format!("{key}{}", pad(&colored(&name(row), label_color), widths[0]));
    for (index, name) in spec.columns.iter().enumerate() {
        let value = colored(cell(row, index, name), value_color(row, spec, index));
        line += &format!("  {}", pad(&value, widths[index + 1]));
    }
    line.trim_end().to_string()
}

/// A row's name; a bionic's label ends with the power the `Power` column already holds.
fn name(row: &MenuChoice) -> String {
    let power = column(row, "Power");
    match row.label.strip_suffix(power) {
        Some(name) if !power.is_empty() => name.trim_end_matches(", ").to_string(),
        _ => row.label.clone(),
    }
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
        .chain(spec.columns.iter().enumerate().map(|(index, name)| {
            let longest = rows.iter().map(|r| width(cell(r, index, name))).max();
            longest.unwrap_or(0).max(width(name))
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
        egui::Window::new(&title)
            .id(a11y::dialog_id(&title))
            .anchor(egui::Align2::CENTER_TOP, super::dialog_offset(ctx))
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.set_width((ctx.content_rect().width() * 0.8).clamp(560.0, 900.0));
                match menu.context.as_str() {
                    "PLAYER_INFO" => self.sheet(ui, menu, entered, &mut out),
                    "BIONICS" => self.bionics(ui, menu, entered, &mut out),
                    _ => self.mutations(ui, menu, &mut out),
                }
            });
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

    /// Rows as a monospace table under a header of the value names; a click chooses a row.
    fn table(&self, ui: &mut egui::Ui, rows: &[&MenuChoice], spec: &Spec, out: &mut Out) {
        let widths = widths(rows, spec);
        let font = egui::TextStyle::Monospace.resolve(ui.style());
        if !rows.is_empty() {
            let key = if rows.iter().any(|r| !column(r, spec.key).is_empty()) {
                "  "
            } else {
                ""
            };
            let names = spec
                .columns
                .iter()
                .zip(&widths[1..])
                .fold(pad("", widths[0]), |line, (name, width)| {
                    format!("{line}  {}", pad(name, *width))
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
            let close = ui.add_enabled(!self.model.busy(), egui::Button::new("Close"));
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
                key: "",
                columns: &["Current", "Base"],
                tint: Tint::FirstColumn,
            },
            "encumbrance" => Spec {
                key: "",
                columns: &["Encumbrance", "Layering", "Warmth"],
                tint: Tint::FirstColumn,
            },
            "skills" => Spec {
                key: "",
                columns: &["Level", "Exercise"],
                tint: Tint::FirstColumn,
            },
            _ => Spec {
                key: "",
                columns: &[],
                tint: Tint::Label,
            },
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
                    let value = colored(column(row, "Value"), column(row, "Color"));
                    ui.label(self.rich(ui, &value));
                    ui.end_row();
                }
            });
    }

    /// The bionics list: power and fuel above, the active or passive tab's rows, the native actions below.
    fn bionics(&mut self, ui: &mut egui::Ui, menu: &Menu, entered: bool, out: &mut Out) {
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
        self.tab_strip(ui, menu, entered, out);
        let rows = menu.entries();
        let spec = Spec {
            key: "Manual (shortcut)",
            columns: &["Power", "State"],
            tint: Tint::Label,
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
            .map(|r| column(r, "Fuel"))
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
                ("Passive", &passive, &[][..]),
                ("Active", &active, &["State"][..]),
            ];
            for (col, (name, rows, columns)) in cols.iter_mut().zip(lists) {
                col.strong(name);
                let spec = Spec {
                    key: "Key",
                    columns,
                    tint: Tint::Label,
                };
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
                .map(|(l, v)| (l.to_string(), v.to_string()))
                .collect(),
            editor: None,
        }
    }

    #[test]
    fn a_color_column_tints_the_first_value_of_a_stat_and_the_label_of_a_trait() {
        let stat = row(
            "Strength:",
            &[("Current", "8"), ("Base", "8"), ("Color", "c_white")],
        );
        let spec = Spec {
            key: "",
            columns: &["Current", "Base"],
            tint: Tint::FirstColumn,
        };
        let line = row_markup(&stat, &spec, &[10, 7, 4]);
        assert_eq!(line, "Strength:   <color_c_white>8</color>        8");
        let trait_row = row("Quick", &[("Color", "c_green")]);
        let spec = Spec {
            key: "",
            columns: &[],
            tint: Tint::Label,
        };
        assert_eq!(
            row_markup(&trait_row, &spec, &[5]),
            "<color_c_green>Quick</color>"
        );
    }

    #[test]
    fn height_and_age_show_the_value_the_engine_labels_with_their_name() {
        let height = row("Height:", &[("Height:", "175 cm")]);
        let spec = Spec {
            key: "",
            columns: &["Current", "Base"],
            tint: Tint::FirstColumn,
        };
        assert_eq!(row_markup(&height, &spec, &[7, 6, 4]), "Height:  175 cm");
    }

    #[test]
    fn a_named_color_column_tints_only_its_value() {
        let part = row(
            "TORSO",
            &[
                ("Encumbrance", "3"),
                ("Warmth", "9"),
                ("Warmth color", "c_red"),
            ],
        );
        let spec = Spec {
            key: "",
            columns: &["Encumbrance", "Warmth"],
            tint: Tint::FirstColumn,
        };
        assert_eq!(value_color(&part, &spec, 0), "");
        assert_eq!(value_color(&part, &spec, 1), "c_red");
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
        assert!(!is_header(&row("Melee", &[("Level", "2")])));
    }
}
