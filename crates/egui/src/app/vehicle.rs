//! The native vehicle interaction window: the grid around the cursor over the parts on its square,
//! the requirements of the highlighted part, the task list with the install tabs, the stat lines and
//! the owner line. The engine owns the cursor and the highlight; arrows and the game's own keys go to it.
use super::{App, a11y, rgb};
use crate::text;
use crate::windows::{self, WindowSpec};
use bn_client_core::protocol::{Vehicle, VehicleRequirement};
use bn_client_core::{GameModel, Menu, MenuChoice, action_hint};
use eframe::egui::{self, accesskit};

const GREEN: egui::Color32 = egui::Color32::from_rgb(90, 255, 90);
const RED: egui::Color32 = egui::Color32::from_rgb(255, 110, 110);
const LIGHT_GREEN: egui::Color32 = egui::Color32::from_rgb(150, 255, 150);
/// Lines per column of the native stat block.
const STAT_ROWS: usize = 8;
/// The directions of the cursor, as the arrow buttons under the grid.
const ARROWS: [(&str, &str); 4] = [
    ("LEFT", "\u{25c0}"),
    ("UP", "\u{25b2}"),
    ("DOWN", "\u{25bc}"),
    ("RIGHT", "\u{25b6}"),
];

/// What a frame of the window asks the engine for.
#[derive(Default)]
struct Out {
    ids: Vec<egui::Id>,
    chosen: Option<String>,
    cancel: bool,
    focused: bool,
}

fn is_task_row(choice: &&MenuChoice) -> bool {
    choice.id.starts_with("part:")
}

impl<M: GameModel> App<M> {
    /// The colour of a game colour name; light gray for an unknown one.
    fn game_color(&self, name: &str) -> egui::Color32 {
        let runs = self.model.styled(&format!("<color_{name}>x</color>"));
        runs.first()
            .and_then(|run| run.color)
            .map_or(egui::Color32::LIGHT_GRAY, rgb)
    }

    /// Draws the window; true when it took keyboard focus (on `entered`) this frame.
    pub(super) fn vehicle(
        &mut self,
        ctx: &egui::Context,
        menu: &Menu,
        vehicle: &Vehicle,
        entered: bool,
    ) -> bool {
        a11y::trap_tab(
            ctx,
            self.trapped_by_menu(),
            super::menu_forwards_keys(Some(menu)),
        );
        let mut out = Out::default();
        let cancel = self.palette.is_none()
            && ctx.input(|i| i.key_pressed(egui::Key::Escape))
            && menu.can_cancel;
        let title = bn_client_core::markup::plain(&vehicle.name);
        let spec = WindowSpec {
            closable: menu.can_cancel,
            id: a11y::dialog_id("Vehicle"),
            width: Some(1000.0),
            max_width: Some(ctx.content_rect().width() - 16.0),
            ..self.window_spec(ctx, "vehicle", &title)
        };
        let shown = windows::show(ctx, spec, |ui| {
            self.vehicle_modes(ui, menu, vehicle, entered, &mut out);
            ui.separator();
            ui.columns(3, |cols| {
                self.vehicle_grid(&mut cols[0], menu, vehicle, &mut out);
                self.vehicle_requirements(&mut cols[1], menu, vehicle);
                self.vehicle_list(&mut cols[2], menu, vehicle, &mut out);
            });
            ui.separator();
            self.vehicle_stats(ui, vehicle);
        });
        if self.keep_window(ctx, "vehicle", &shown) {
            self.model.cancel_menu();
        }
        self.menu_ids = out.ids;
        a11y::dialog(ctx, &title, &menu.message);
        if let Some(id) = out.chosen {
            ctx.request_repaint();
            self.model.choose(&id);
        } else if cancel || out.cancel {
            self.model.cancel_menu();
        }
        out.focused
    }

    /// The native top line: the actions of the main screen, or the task's title with Back.
    fn vehicle_modes(
        &mut self,
        ui: &mut egui::Ui,
        menu: &Menu,
        vehicle: &Vehicle,
        entered: bool,
        out: &mut Out,
    ) {
        ui.horizontal_wrapped(|ui| {
            if vehicle.mode != "main" {
                ui.label(self.rich(ui, &menu.title));
                if menu.can_cancel {
                    let back = self.control(ui, "Back", "action:QUIT", true, false);
                    out.ids.extend(back.enabled().then_some(back.id));
                    out.cancel |= back.clicked();
                }
                return;
            }
            let actions = self.model.actions();
            let tasks = menu
                .choices
                .iter()
                .filter(|c| c.id.starts_with("action:") && !is_arrow(&c.id));
            for (i, choice) in tasks.enumerate() {
                let key = action_hint(&actions, &choice.id).unwrap_or_default();
                let label = bn_client_core::markup::plain(&choice.label);
                let text = if key.is_empty() {
                    label
                } else {
                    format!("{label} [{key}]")
                };
                let mut button = self.control(ui, text, &choice.id, choice.enabled, false);
                if !choice.denial.is_empty() {
                    button = button.on_disabled_hover_text(&choice.denial);
                }
                out.ids.extend(button.enabled().then_some(button.id));
                if entered && i == 0 {
                    button.request_focus();
                    out.focused = button.has_focus();
                }
                if button.clicked() {
                    out.chosen = Some(choice.id.clone());
                }
            }
            if menu.can_cancel {
                let back = self.control(ui, "Back", "action:QUIT", true, false);
                out.ids.extend(back.enabled().then_some(back.id));
                out.cancel |= back.clicked();
            }
        });
        if vehicle.mode == "main" && !menu.message.is_empty() {
            ui.label(self.rich(ui, &menu.message));
        }
    }

    /// The vehicle around the cursor in the native fixed-width grid, with its guide lines, and under it
    /// the parts on the cursor square.
    fn vehicle_grid(&mut self, ui: &mut egui::Ui, menu: &Menu, vehicle: &Vehicle, out: &mut Out) {
        let reach = |axis: fn(&bn_client_core::protocol::VehicleCell) -> i32, least: i32| {
            vehicle
                .cells
                .iter()
                .map(|c| axis(c).abs())
                .fold(least, i32::max)
        };
        let font = egui::TextStyle::Monospace.resolve(ui.style());
        let cell_size = ui
            .painter()
            .layout_no_wrap("0".into(), font.clone(), egui::Color32::WHITE)
            .size();
        // The grid stays centred on the cursor and is cut at the column, as the native window cuts it.
        let fits = |room: f32, size: f32| ((room / size).floor() as i32 - 1).max(0) / 2;
        let rx = reach(|c| c.dx, 6).min(fits(ui.available_width(), cell_size.x));
        let ry = reach(|c| c.dy, 3).min(fits(ui.ctx().content_rect().height() * 0.4, cell_size.y));
        let dark = egui::Color32::DARK_GRAY;
        let mut job = egui::text::LayoutJob::default();
        for y in -ry..=ry {
            for x in -rx..=rx {
                let cell = vehicle.cells.iter().find(|c| (c.dx, c.dy) == (x, y));
                let (glyph, color, background) = match (cell, (x, y)) {
                    (Some(c), (0, 0)) => {
                        let back = if vehicle.blocked {
                            RED
                        } else {
                            self.game_color(&c.color)
                        };
                        (c.glyph.as_str(), egui::Color32::BLACK, back)
                    }
                    (Some(c), _) => (
                        c.glyph.as_str(),
                        self.game_color(&c.color),
                        egui::Color32::TRANSPARENT,
                    ),
                    (None, (0, 0)) if vehicle.blocked => (" ", RED, RED),
                    (None, (0, 0)) => (" ", dark, egui::Color32::from_gray(70)),
                    (None, (0, _)) => ("\u{2502}", dark, egui::Color32::TRANSPARENT),
                    (None, (_, 0)) => ("\u{2500}", dark, egui::Color32::TRANSPARENT),
                    (None, _) => (" ", dark, egui::Color32::TRANSPARENT),
                };
                job.append(
                    glyph,
                    0.0,
                    egui::TextFormat {
                        font_id: font.clone(),
                        color,
                        background,
                        ..Default::default()
                    },
                );
            }
            job.append("\n", 0.0, egui::TextFormat::simple(font.clone(), dark));
        }
        ui.add(egui::Label::new(job).wrap_mode(egui::TextWrapMode::Extend));
        ui.horizontal(|ui| {
            for (action, glyph) in ARROWS {
                let id = format!("action:{action}");
                let Some(row) = menu.choices.iter().find(|c| c.id == id) else {
                    continue;
                };
                let button = self.control(ui, glyph, &id, true, false);
                let name = row.label.clone();
                ui.ctx()
                    .accesskit_node_builder(button.id, |b| b.set_label(name));
                out.ids.extend(button.enabled().then_some(button.id));
                if button.clicked() {
                    out.chosen = Some(id);
                }
            }
            ui.weak(format!("{}, {}", vehicle.cursor.x, vehicle.cursor.y));
        });
        for part in &vehicle.parts {
            let color = if part.broken {
                RED
            } else {
                egui::Color32::LIGHT_GRAY
            };
            let text = format!("{}{}{}", part.open, part.name, part.close);
            ui.label(egui::RichText::new(text).monospace().color(color));
        }
    }

    /// The requirements of the highlighted part, each in the green or red of its status.
    fn vehicle_requirements(&mut self, ui: &mut egui::Ui, menu: &Menu, vehicle: &Vehicle) {
        let Some(detail) = &vehicle.detail else {
            if vehicle.mode != "main" {
                ui.label(self.rich(ui, &menu.message));
            }
            return;
        };
        egui::ScrollArea::vertical()
            .id_salt("vehicle_requirements")
            .max_height(ui.ctx().content_rect().height() * 0.5)
            .show(ui, |ui| {
                ui.label(egui::RichText::new(&detail.name).color(LIGHT_GREEN));
                if let Some(time) = &detail.time {
                    ui.label(detail.time_heading.as_deref().unwrap_or_default());
                    ui.label(format!("> {time}"));
                }
                for group in &detail.groups {
                    let rows = detail
                        .requirements
                        .iter()
                        .filter(|r| r.kind == group.kind)
                        .collect::<Vec<_>>();
                    if rows.is_empty() && group.empty_text.is_empty() {
                        continue;
                    }
                    ui.label(&group.heading);
                    if rows.is_empty() {
                        ui.colored_label(GREEN, format!("> {}", group.empty_text));
                    }
                    for row in rows {
                        ui.colored_label(status(row), format!("> {}", row.text));
                    }
                }
                if !detail.stats.is_empty() {
                    ui.separator();
                    egui::Grid::new("vehicle_part_stats").show(ui, |ui| {
                        for stat in &detail.stats {
                            ui.label(&stat.label);
                            ui.weak(&stat.value);
                            ui.end_row();
                        }
                    });
                }
                if !detail.description.is_empty() {
                    ui.separator();
                    ui.weak(&detail.description);
                }
            });
    }

    /// The tabs and candidate parts of a task, with the button that starts it on the highlighted one.
    fn vehicle_list(&mut self, ui: &mut egui::Ui, menu: &Menu, vehicle: &Vehicle, out: &mut Out) {
        let rows = menu.choices.iter().filter(is_task_row).collect::<Vec<_>>();
        if vehicle.mode == "main" {
            return;
        }
        if let Some(tabs) = &vehicle.tabs {
            ui.horizontal_wrapped(|ui| {
                for (i, label) in tabs.labels.iter().enumerate() {
                    let id = format!("tab:{i}");
                    let on = i == tabs.selected;
                    let button = self.control(ui, label.clone(), &id, true, on);
                    a11y::tab(ui.ctx(), button.id, on);
                    out.ids.extend(button.enabled().then_some(button.id));
                    if button.clicked() {
                        out.chosen = Some(id);
                    }
                }
            });
            if !tabs.filter.is_empty() {
                ui.weak(&tabs.filter);
            }
        }
        egui::ScrollArea::vertical()
            .id_salt(("vehicle_list", &vehicle.mode))
            .max_height(ui.ctx().content_rect().height() * 0.4)
            .show(ui, |ui| {
                for row in &rows {
                    let name = bn_client_core::markup::plain(&row.label);
                    let hotkey = match row.hotkey.as_str() {
                        "" => String::new(),
                        key => format!("{key} "),
                    };
                    let text = self.rich(
                        ui,
                        &super::component::tagged(
                            Some(row.color.as_str()).filter(|c| !c.is_empty()),
                            &format!("{hotkey}{name}"),
                        ),
                    );
                    let response = ui.selectable_label(row.highlighted, text);
                    if row.highlighted && !response.is_pointer_button_down_on() {
                        response.scroll_to_me(None);
                    }
                    ui.ctx().accesskit_node_builder(response.id, |b| {
                        b.set_role(accesskit::Role::MenuItem);
                        b.set_label(name.clone());
                    });
                    out.ids.push(response.id);
                    if response.clicked() && !self.model.busy() {
                        out.chosen = Some(row.id.clone());
                    }
                }
            });
        let Some(row) = rows.iter().find(|r| r.highlighted) else {
            return;
        };
        let usable = row.enabled && vehicle.detail.as_ref().is_none_or(|d| d.can_do);
        let name = bn_client_core::markup::plain(&row.label);
        let button = self.control(
            ui,
            format!("{} {name}", vehicle.verb),
            &row.id,
            usable,
            false,
        );
        out.ids.extend(button.enabled().then_some(button.id));
        if button.clicked() {
            out.chosen = Some(row.id.clone());
        }
    }

    /// The native stat block in columns of eight lines, and the name and owner line under it.
    fn vehicle_stats(&self, ui: &mut egui::Ui, vehicle: &Vehicle) {
        let font = egui::TextStyle::Monospace.resolve(ui.style());
        ui.columns(3, |cols| {
            for (col, lines) in cols.iter_mut().zip(vehicle.stats.chunks(STAT_ROWS)) {
                let runs = self.model.styled(&lines.join("\n"));
                let job = text::job(&runs, &font);
                col.label(job);
            }
        });
        let owner = if vehicle.owned { GREEN } else { RED };
        ui.horizontal(|ui| {
            ui.colored_label(owner, format!("{} ({})", vehicle.name, vehicle.owner));
        });
    }
}

fn is_arrow(id: &str) -> bool {
    ARROWS
        .iter()
        .any(|(action, _)| id == format!("action:{action}"))
}

/// The native status colour of a requirement: green when met, red when not.
fn status(row: &VehicleRequirement) -> egui::Color32 {
    if row.met { GREEN } else { RED }
}
