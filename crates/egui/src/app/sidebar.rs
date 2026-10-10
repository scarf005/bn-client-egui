//! The right-hand sidebar, like the native Tiles one: status contents on top, the message log below.

use super::App;
use crate::a11y;
use bn_client_core::{GameModel, LogLine, markup};
use eframe::egui::{self, accesskit};

/// The message log's share of the sidebar height.
const LOG_SHARE: f32 = 0.4;
/// The log's height limits as shares of the sidebar height.
const LOG_RANGE: (f32, f32) = (0.1, 0.8);

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

impl<M: GameModel> App<M> {
    /// The whole sidebar: the message log pinned to its bottom, `sidebar` filling the rest.
    pub(super) fn sidebar_panel(&mut self, ui: &mut egui::Ui) {
        let room = ui.available_height();
        let height = self.log_height.unwrap_or(room * LOG_SHARE);
        let height = height.clamp(room * LOG_RANGE.0, room * LOG_RANGE.1);
        let log = egui::Panel::bottom("log")
            .resizable(false)
            .exact_size(height)
            .show(ui, |ui| self.log(ui));
        // Our own handle on the panel's top edge: egui's nested panel handle never gets the pointer.
        let edge = log.response.rect.expand2(egui::vec2(0.0, 3.0));
        let handle = ui.interact(
            egui::Rect::from_min_max(edge.left_top(), edge.right_top() + egui::vec2(0.0, 3.0)),
            egui::Id::new("log_resize"),
            egui::Sense::drag(),
        );
        ui.ctx().accesskit_node_builder(handle.id, |b| {
            b.set_role(accesskit::Role::Splitter);
            b.set_label("Resize message log");
        });
        if handle.hovered() || handle.dragged() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeVertical);
        }
        if handle.dragged() {
            self.log_height = Some(height - handle.drag_delta().y);
        }
        egui::CentralPanel::no_frame().show(ui, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| self.sidebar(ui));
        });
    }

    /// Slot for the status contents; the item lists stay below them.
    fn sidebar(&mut self, ui: &mut egui::Ui) {
        // Without an avatar there is no status to show.
        let status = self.model.sidebar();
        if !status.is_empty() {
            crate::sidebar::show(ui, &status);
        }
        self.items(ui);
    }

    fn items(&mut self, ui: &mut egui::Ui) {
        let mut moved = None;
        for column in [Column::Ground, Column::Inventory] {
            let (title, verb, items) = match column {
                Column::Ground => ("Ground", "Pick up", self.model.ground()),
                Column::Inventory => ("Inventory", "Drop", self.model.inventory()),
            };
            // The heading is part of the drop zone, so dropping on it works too.
            // Only a drag in progress gives the zone a fill; idle zones are bare.
            let (_, dropped) = ui
                .scope(|ui| {
                    let idle = &mut ui.visuals_mut().widgets.inactive;
                    idle.bg_fill = egui::Color32::TRANSPARENT;
                    idle.bg_stroke = egui::Stroke::NONE;
                    ui.dnd_drop_zone::<Dragged, _>(egui::Frame::NONE, |ui| {
                        ui.set_min_size(egui::vec2(ui.available_width(), 0.0));
                        ui.heading(title);
                        for (index, name) in items.iter().enumerate() {
                            ui.horizontal(|ui| {
                                let grip = ui.add(
                                    egui::Label::new(self.rich(ui, name)).sense(egui::Sense::DRAG),
                                );
                                grip.dnd_set_drag_payload(Dragged {
                                    from: column,
                                    index,
                                });
                                let button = ui.button(verb);
                                ui.ctx().accesskit_node_builder(button.id, |b| {
                                    b.set_label(format!("{verb} {}", markup::plain(name)))
                                });
                                if button.clicked() {
                                    moved = Some(Dragged {
                                        from: column,
                                        index,
                                    });
                                }
                            });
                        }
                    })
                })
                .inner;
            if let Some(d) = dropped.filter(|d| d.from != column) {
                moved = Some(*d);
            }
        }
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
        let native = self.model.native_log();
        let shown = native.iter().map(LogLine::shown).collect::<Vec<_>>();
        let native_live = self.native_live.update(&shown).to_owned();
        let command_live = self.command_live.update(self.model.messages()).to_owned();
        message_log(
            ui,
            native,
            self.model.messages(),
            [&native_live, &command_live],
        );
    }
}

/// The message log panel: the game's own lines in their native colours, then the client's command
/// results and errors in a separate scroll area. What each list gained since the last frame is a
/// polite live region, so history is never re-announced.
pub(super) fn message_log(
    ui: &mut egui::Ui,
    native: &[LogLine],
    commands: &[String],
    live: [&str; 2],
) {
    ui.heading("Message log");
    if !native.is_empty() {
        a11y::live_region(ui, live[0]);
    }
    egui::ScrollArea::vertical()
        .id_salt("native_log")
        .max_height(ui.available_height() * 0.7)
        .stick_to_bottom(true)
        .show(ui, |ui| {
            for line in &native[native.len().saturating_sub(100)..] {
                let text = egui::RichText::new(line.shown());
                ui.label(
                    line.color
                        .map_or(text.clone(), |c| text.color(super::rgb(c))),
                );
            }
        });
    if !commands.is_empty() {
        ui.separator();
        a11y::live_region(ui, live[1]);
    }
    egui::ScrollArea::vertical()
        .id_salt("command_log")
        .max_height(100.0)
        .stick_to_bottom(true)
        .show(ui, |ui| {
            for message in &commands[commands.len().saturating_sub(50)..] {
                ui.label(message);
            }
        });
}

#[cfg(test)]
mod tests {
    use super::whole_lines_height;

    #[test]
    fn a_scroll_area_is_cut_between_lines_never_through_one() {
        // Lines of 20 and 40 points: tops at 0, 20, 60, 80; 100 points in all.
        let tops = [0.0, 20.0, 60.0, 80.0];
        assert_eq!(whole_lines_height(&tops, 100.0, 200.0), 200.0, "all of it fits");
        assert_eq!(whole_lines_height(&tops, 100.0, 70.0), 40.0, "the last two lines");
        assert_eq!(whole_lines_height(&tops, 100.0, 45.0), 40.0, "part of a line is dropped");
        assert_eq!(whole_lines_height(&tops, 100.0, 15.0), 20.0, "a line taller than the room still shows");
        assert_eq!(whole_lines_height(&[], 0.0, 50.0), 50.0);
    }
}
