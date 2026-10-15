//! The right-hand sidebar, like the native Tiles one: status contents on top, the message log below.

use super::App;
use crate::a11y;
use crate::display::Display;
use bn_client_core::sidebar::{Body, Direction, Layout, Panel};
use bn_client_core::{Cell, GameModel, LogLine, markup};
use eframe::egui::{self, accesskit};

/// The message log's share of the sidebar height.
const LOG_SHARE: f32 = 0.4;
/// The log's height limits as shares of the sidebar height.
const LOG_RANGE: (f32, f32) = (0.1, 0.8);

/// The side of the square an item's sprite or glyph is drawn in beside its row.
const ICON: f32 = 24.0;
/// The least height an item list keeps, whatever room the window has left.
const MIN_LIST: f32 = 4.0 * ICON;

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
    /// The whole sidebar: the message log pinned to its bottom, the panels of `layout` (all of the status
    /// without one) filling the rest.
    pub(super) fn sidebar_panel(&mut self, ui: &mut egui::Ui, layout: Option<&Layout>) {
        let log = layout.is_none_or(Layout::has_log);
        if !log {
            return self.sidebar_panels(ui, layout);
        }
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
        egui::CentralPanel::no_frame().show(ui, |ui| self.sidebar_panels(ui, layout));
    }

    fn sidebar_panels(&mut self, ui: &mut egui::Ui, layout: Option<&Layout>) {
        egui::ScrollArea::vertical().show(ui, |ui| {
            match layout {
                Some(layout) => {
                    for panel in &layout.panels {
                        self.panel(ui, panel);
                    }
                }
                None => {
                    // Without an avatar there is no status to show.
                    let status = self.model.sidebar();
                    if !status.is_empty() {
                        crate::sidebar::show(ui, &status);
                    }
                }
            }
            self.items(ui);
        });
    }

    /// One panel of the layout under the engine's label. The message log is pinned below, not here.
    fn panel(&mut self, ui: &mut egui::Ui, panel: &Panel) {
        match &panel.body {
            Body::Status(rows) => {
                ui.strong(&panel.label);
                crate::sidebar::rows(ui, &panel.id, rows, panel.id == "Health");
            }
            Body::Compass(directions) => {
                ui.strong(&panel.label);
                self.compass(ui, directions);
            }
            Body::Log => {}
        }
    }

    /// Per direction its heading, then its creatures nearest first, each with its sprite and distance.
    fn compass(&mut self, ui: &mut egui::Ui, directions: &[Direction]) {
        let season = self.model.season();
        for direction in directions {
            ui.label(&direction.label);
            for creature in &direction.creatures {
                ui.horizontal(|ui| {
                    icon(ui, &mut self.display, &creature.cell, season.as_deref());
                    ui.add(
                        egui::Label::new(format!("{} {}", creature.name, creature.distance))
                            .truncate(),
                    );
                });
            }
        }
    }

    fn items(&mut self, ui: &mut egui::Ui) {
        let mut moved = None;
        let season = self.model.season();
        for column in [Column::Ground, Column::Inventory] {
            let (title, verb, items, cells) = match column {
                Column::Ground => (
                    "Ground",
                    "Pick up",
                    self.model.ground(),
                    self.model.ground_cells(),
                ),
                Column::Inventory => (
                    "Inventory",
                    "Drop",
                    self.model.inventory(),
                    self.model.inventory_cells(),
                ),
            };
            let rows = items
                .iter()
                .enumerate()
                .map(|(i, name)| (name, self.rich(ui, name), cells.get(i)))
                .collect::<Vec<_>>();
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
                        // A long list scrolls inside the room the window has left, so the status above it
                        // stays in view; the ground takes at most two fifths of it for itself.
                        let room =
                            (ui.clip_rect().max.y - ui.next_widget_position().y).max(MIN_LIST);
                        let room = match column {
                            Column::Ground => room * 0.4,
                            Column::Inventory => room,
                        };
                        egui::ScrollArea::vertical()
                            .id_salt(title)
                            .max_height(room)
                            .auto_shrink([false, true])
                            .show(ui, |ui| {
                                for (index, (name, label, cell)) in rows.into_iter().enumerate() {
                                    ui.horizontal(|ui| {
                                        if let Some(cell) = cell {
                                            icon(ui, &mut self.display, cell, season.as_deref());
                                        }
                                        // The name is cut before it would push the button out of the panel.
                                        let button_w = ui
                                            .painter()
                                            .layout_no_wrap(
                                                verb.into(),
                                                egui::TextStyle::Button.resolve(ui.style()),
                                                egui::Color32::WHITE,
                                            )
                                            .size()
                                            .x
                                            + 2.0 * ui.spacing().button_padding.x
                                            + ui.spacing().item_spacing.x;
                                        let grip = ui
                                            .scope(|ui| {
                                                ui.set_max_width(
                                                    (ui.available_width() - button_w).max(0.0),
                                                );
                                                ui.add(
                                                    egui::Label::new(label)
                                                        .truncate()
                                                        .sense(egui::Sense::DRAG),
                                                )
                                            })
                                            .inner;
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
                                        // Tabbing to a row below the fold brings it into view.
                                        if button.gained_focus() {
                                            button.scroll_to_me(None);
                                        }
                                    });
                                }
                            });
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
        // The log's lines, commands and sounds scroll inside the panel: content taller than the panel would
        // otherwise make the panel report a lower edge, and the panels above it would be drawn under it.
        let room = ui.available_height();
        egui::ScrollArea::vertical()
            .id_salt("log_panel")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                message_log(
                    ui,
                    LogOptions {
                        native,
                        commands: self.model.messages(),
                        live: [&native_live, &command_live],
                        room,
                    },
                );
                super::sounds::show(ui, &self.model.sounds());
            });
    }
}

/// The cell as its sprite when the tileset has one, else as its glyph, in a square of `ICON`.
fn icon(ui: &mut egui::Ui, display: &mut Display, cell: &Cell, season: Option<&str>) {
    let (rect, _) = ui.allocate_exact_size(egui::Vec2::splat(ICON), egui::Sense::hover());
    paint_cell(&ui.painter_at(rect), rect, display, cell, season);
}

/// The cell in `rect` as its sprite when the tileset has one, else as its glyph.
pub(super) fn paint_cell(
    painter: &egui::Painter,
    rect: egui::Rect,
    display: &mut Display,
    cell: &Cell,
    season: Option<&str>,
) {
    if !(display.tiles() && display.paint_icon(painter, rect, cell, season)) {
        glyph_icon(painter, rect, cell);
    }
}

/// The item as its glyph in its colour, for ASCII or an item the tileset has no sprite for.
fn glyph_icon(painter: &egui::Painter, rect: egui::Rect, cell: &Cell) {
    if let Some(bg) = cell.bg {
        painter.rect_filled(rect, 0.0, super::rgb(bg));
    }
    painter.text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        cell.glyph,
        egui::FontId::monospace(rect.height() - 4.0),
        super::rgb(cell.color),
    );
}

/// What the message log shows, and the height of the panel it sits in.
pub(super) struct LogOptions<'a> {
    pub native: &'a [LogLine],
    pub commands: &'a [String],
    pub live: [&'a str; 2],
    pub room: f32,
}

/// The message log panel: the game's own lines in their native colours, then the client's command
/// results and errors in a separate scroll area. What each list gained since the last frame is a
/// polite live region, so history is never re-announced.
pub(super) fn message_log(ui: &mut egui::Ui, options: LogOptions) {
    let LogOptions {
        native,
        commands,
        live,
        room,
    } = options;
    ui.heading("Message log");
    if !native.is_empty() {
        a11y::live_region(ui, live[0]);
    }
    // The tops of last frame's lines, so the area ends between two of them and shows no half line.
    let key = egui::Id::new("native_log_tops");
    let (tops, total): (Vec<f32>, f32) = ui.data(|d| d.get_temp(key)).unwrap_or_default();
    let room = whole_lines_height(&tops, total, room * 0.7);
    let output = egui::ScrollArea::vertical()
        .id_salt("native_log")
        .max_height(room)
        .stick_to_bottom(true)
        .show(ui, |ui| {
            let top = ui.next_widget_position().y;
            let mut tops = Vec::new();
            for line in &native[native.len().saturating_sub(100)..] {
                tops.push(ui.next_widget_position().y - top);
                let text = egui::RichText::new(line.shown());
                ui.label(
                    line.color
                        .map_or(text.clone(), |c| text.color(super::rgb(c))),
                );
            }
            tops
        });
    ui.data_mut(|d| d.insert_temp(key, (output.inner, output.content_size.y)));
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

/// The height for a scroll area of lines starting at `tops` (`total` high in all) that holds at most
/// `max`: the whole lines that fit, so no line is cut through.
fn whole_lines_height(tops: &[f32], total: f32, max: f32) -> f32 {
    if total <= max {
        return max;
    }
    let first = tops.iter().find(|t| total - **t <= max);
    first.or(tops.last()).map_or(max, |t| total - t)
}

#[cfg(test)]
mod tests {
    use super::whole_lines_height;

    #[test]
    fn a_scroll_area_is_cut_between_lines_never_through_one() {
        // Lines of 20 and 40 points: tops at 0, 20, 60, 80; 100 points in all.
        let tops = [0.0, 20.0, 60.0, 80.0];
        assert_eq!(
            whole_lines_height(&tops, 100.0, 200.0),
            200.0,
            "all of it fits"
        );
        assert_eq!(
            whole_lines_height(&tops, 100.0, 70.0),
            40.0,
            "the last two lines"
        );
        assert_eq!(
            whole_lines_height(&tops, 100.0, 45.0),
            40.0,
            "part of a line is dropped"
        );
        assert_eq!(
            whole_lines_height(&tops, 100.0, 15.0),
            20.0,
            "a line taller than the room still shows"
        );
        assert_eq!(whole_lines_height(&[], 0.0, 50.0), 50.0);
    }
}
