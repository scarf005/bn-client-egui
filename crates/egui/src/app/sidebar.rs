//! The right-hand sidebar, like the native Tiles one: status contents on top, the message log below.

use super::App;
use bn_client_core::GameModel;
use eframe::egui::{self, accesskit};

/// The message log's share of the sidebar height.
const LOG_SHARE: f32 = 0.4;

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
        egui::Panel::bottom("log")
            .resizable(false)
            .exact_size(ui.available_height() * LOG_SHARE)
            .show(ui, |ui| self.log(ui));
        egui::CentralPanel::no_frame().show(ui, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| self.sidebar(ui));
        });
    }

    /// Slot for the status contents; the item lists stay below them.
    fn sidebar(&mut self, ui: &mut egui::Ui) {
        crate::sidebar::show(ui, &self.model.sidebar());
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
        message_log(ui, self.model.native_log(), self.model.messages());
    }
}

/// The message log panel: the game's own lines in their native colours, then the client's command
/// results and errors in a separate scroll area. The newest line of each is a polite live region,
/// so history is never re-announced.
pub(super) fn message_log(ui: &mut egui::Ui, native: &[crate::LogLine], commands: &[String]) {
    ui.heading("Message log");
    if let Some(latest) = native.last() {
        let live = ui.label(latest.shown());
        ui.ctx()
            .accesskit_node_builder(live.id, |b| b.set_live(accesskit::Live::Polite));
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
    if let Some(latest) = commands.last() {
        ui.separator();
        let live = ui.label(latest);
        ui.ctx()
            .accesskit_node_builder(live.id, |b| b.set_live(accesskit::Live::Polite));
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
