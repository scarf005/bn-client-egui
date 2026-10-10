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
            .default_size(ui.available_height() * LOG_SHARE)
            .size_range(80.0..=ui.available_height() * 0.8)
            .show(ui, |ui| self.log(ui));
        super::label_resize_handle(ui.ctx(), "log", "Resize message log");
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
        ui.heading("Message log");
        // Only the newest message is live, so history is never re-announced.
        if let Some(latest) = self.model.messages().last() {
            let live = ui.label(latest);
            ui.ctx()
                .accesskit_node_builder(live.id, |b| b.set_live(accesskit::Live::Polite));
        }
        egui::ScrollArea::vertical()
            .auto_shrink(false)
            .stick_to_bottom(true)
            .show(ui, |ui| {
                let messages = self.model.messages();
                for message in &messages[messages.len().saturating_sub(50)..] {
                    ui.label(message);
                }
            });
    }
}
