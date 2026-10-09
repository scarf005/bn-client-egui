use crate::model::{GameModel, Pos};
use eframe::egui::{self, accesskit};

/// Side length of one map cell in points.
pub const CELL: f32 = 16.0;

#[derive(Clone, Copy, PartialEq)]
enum Column {
    Ground,
    Inventory,
}

/// Drag payload: which column the item comes from and its index there.
#[derive(Clone, Copy)]
struct Dragged {
    from: Column,
    index: usize,
}

pub struct App<M: GameModel> {
    pub model: M,
    cursor: Pos,
    menu_cell: Pos,
    destination: Option<Pos>,
    last_hover: Option<egui::Pos2>,
}

impl<M: GameModel> App<M> {
    pub fn new(model: M) -> Self {
        let cursor = model.player();
        Self {
            model,
            cursor,
            menu_cell: cursor,
            destination: None,
            last_hover: None,
        }
    }

    pub fn show(&mut self, ui: &mut egui::Ui) {
        self.walk(ui.ctx());
        egui::Panel::bottom("log").show(ui, |ui| self.log(ui));
        egui::Panel::right("items")
            .resizable(false)
            .show(ui, |ui| self.items(ui));
        self.map(ui);
    }

    /// One step per frame toward the destination.
    fn walk(&mut self, ctx: &egui::Context) {
        let Some(target) = self.destination else {
            return;
        };
        self.model.step_toward(target);
        if self.model.player() == target {
            self.destination = None;
        }
        ctx.request_repaint();
    }

    fn look(&mut self, at: Pos) {
        let desc = self.model.cell(at).desc;
        self.model
            .say(format!("You see {desc} at ({}, {}).", at.x, at.y));
    }

    fn map(&mut self, ui: &mut egui::Ui) {
        let size = self.model.size();
        let player = self.model.player();
        ui.label(format!(
            "Player at ({}, {}). Cursor ({}, {}): {}.",
            player.x,
            player.y,
            self.cursor.x,
            self.cursor.y,
            self.model.cell(self.cursor).desc
        ));
        let (response, painter) = ui.allocate_painter(
            egui::vec2(size.x as f32 * CELL, size.y as f32 * CELL),
            egui::Sense::click().union(egui::Sense::focusable_noninteractive()),
        );
        response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Other, true, "Map"));
        let cell_at = |p: egui::Pos2| {
            let v = (p - response.rect.min) / CELL;
            Pos {
                x: (v.x as i32).clamp(0, size.x - 1),
                y: (v.y as i32).clamp(0, size.y - 1),
            }
        };
        // Only a moving pointer moves the cursor, so it cannot undo keyboard navigation.
        let hover = response.hover_pos();
        if let Some(p) = hover.filter(|p| self.last_hover != Some(*p)) {
            self.cursor = cell_at(p);
        }
        self.last_hover = hover;
        if response.clicked() {
            self.destination = Some(self.cursor);
        }
        if response.secondary_clicked() {
            self.menu_cell = self.cursor;
        }
        if response.has_focus() {
            // Keep arrow keys from moving egui's focus away from the map.
            ui.memory_mut(|m| {
                m.set_focus_lock_filter(
                    response.id,
                    egui::EventFilter {
                        horizontal_arrows: true,
                        vertical_arrows: true,
                        ..Default::default()
                    },
                )
            });
            let (dx, dy, look) = ui.input(|i| {
                let k = |key| i.key_pressed(key) as i32;
                (
                    k(egui::Key::ArrowRight) - k(egui::Key::ArrowLeft),
                    k(egui::Key::ArrowDown) - k(egui::Key::ArrowUp),
                    i.key_pressed(egui::Key::L),
                )
            });
            self.cursor = Pos {
                x: (self.cursor.x + dx).clamp(0, size.x - 1),
                y: (self.cursor.y + dy).clamp(0, size.y - 1),
            };
            if look {
                self.look(self.cursor);
            }
        }
        response.clone().on_hover_ui_at_pointer(|ui| {
            ui.label(self.model.cell(self.cursor).desc);
        });
        let menu = response.context_menu(|ui| {
            let at = self.menu_cell;
            if ui.button("Look").clicked() {
                self.look(at);
                ui.close();
            }
            if ui.button("Move here").clicked() {
                self.destination = Some(at);
                ui.close();
            }
        });
        if let Some(menu) = menu {
            ui.ctx().accesskit_node_builder(menu.response.id, |b| {
                b.set_role(accesskit::Role::Menu);
                b.set_label("Map actions");
            });
        }
        let font = egui::FontId::monospace(CELL - 2.0);
        for y in 0..size.y {
            for x in 0..size.x {
                let cell = self.model.cell(Pos { x, y });
                let center = response.rect.min + egui::vec2(x as f32 + 0.5, y as f32 + 0.5) * CELL;
                painter.text(
                    center,
                    egui::Align2::CENTER_CENTER,
                    cell.glyph,
                    font.clone(),
                    cell.color,
                );
            }
        }
        let rect = |p: Pos| {
            egui::Rect::from_min_size(
                response.rect.min + egui::vec2(p.x as f32, p.y as f32) * CELL,
                egui::Vec2::splat(CELL),
            )
        };
        let center = rect(player).center();
        painter.text(
            center,
            egui::Align2::CENTER_CENTER,
            '@',
            font,
            egui::Color32::YELLOW,
        );
        painter.rect_stroke(
            rect(self.cursor),
            0.0,
            egui::Stroke::new(1.0, egui::Color32::WHITE),
            egui::StrokeKind::Inside,
        );
    }

    fn items(&mut self, ui: &mut egui::Ui) {
        let mut moved = None;
        ui.columns(2, |cols| {
            for (ui, column) in cols.iter_mut().zip([Column::Ground, Column::Inventory]) {
                let (title, verb, items) = match column {
                    Column::Ground => ("Ground", "Pick up", self.model.ground()),
                    Column::Inventory => ("Inventory", "Drop", self.model.inventory()),
                };
                ui.heading(title);
                let (_, dropped) =
                    ui.dnd_drop_zone::<Dragged, _>(egui::Frame::group(ui.style()), |ui| {
                        ui.set_min_size(egui::vec2(ui.available_width(), 200.0));
                        for (index, name) in items.iter().enumerate() {
                            ui.horizontal(|ui| {
                                let grip =
                                    ui.add(egui::Label::new(name).sense(egui::Sense::drag()));
                                grip.dnd_set_drag_payload(Dragged {
                                    from: column,
                                    index,
                                });
                                let button = ui.button(verb);
                                ui.ctx().accesskit_node_builder(button.id, |b| {
                                    b.set_label(format!("{verb} {name}"))
                                });
                                if button.clicked() {
                                    moved = Some(Dragged {
                                        from: column,
                                        index,
                                    });
                                }
                            });
                        }
                    });
                if let Some(d) = dropped.filter(|d| d.from != column) {
                    moved = Some(*d);
                }
            }
        });
        match moved {
            Some(Dragged {
                from: Column::Ground,
                index,
            }) => self.model.pick_up(index),
            Some(Dragged {
                from: Column::Inventory,
                index,
            }) => self.model.drop_item(index),
            None => {}
        }
    }

    fn log(&mut self, ui: &mut egui::Ui) {
        ui.heading("Message log");
        egui::ScrollArea::vertical()
            .max_height(100.0)
            .stick_to_bottom(true)
            .show(ui, |ui| {
                let messages = self.model.messages();
                let text = messages[messages.len().saturating_sub(50)..].join("\n");
                let log = ui.label(text);
                ui.ctx()
                    .accesskit_node_builder(log.id, |b| b.set_live(accesskit::Live::Polite));
            });
    }
}

impl<M: GameModel> eframe::App for App<M> {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.show(ui);
    }
}
