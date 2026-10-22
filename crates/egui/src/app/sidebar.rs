//! The right-hand sidebar, like the native Tiles one: status contents on top, the message log below.

use super::App;
use crate::a11y;
use crate::display::Display;
use bn_client_core::localise::tr;
use bn_client_core::protocol::ItemListId;
use bn_client_core::sidebar::{Body, Direction, Layout, Panel};
use bn_client_core::{Cell, GameModel, LogLine, markup};
use eframe::egui::{self, accesskit};

/// The fewest lines of the message log the sidebar keeps in view, whatever the panels above it hold.
const LOG_LINES: f32 = 6.0;
/// The share of the log panel its game lines take; the client's own lines and the sounds share the rest.
const NATIVE_SHARE: f32 = 0.7;
/// The most of what the status leaves the item lists take before the player drags the log's edge.
const ITEM_SHARE: f32 = 0.5;
/// The gap between two panels, with a rule across its middle.
const PANEL_GAP: f32 = 8.0;

/// The side of the square an item's sprite or glyph is drawn in beside its row.
const ICON: f32 = 24.0;
/// The least height an item list keeps, whatever room the window has left.
const MIN_LIST: f32 = 4.0 * ICON;

/// Drag payload: which list the item comes from and which item it is.
#[derive(Clone)]
struct Dragged {
    from: ItemListId,
    /// The engine's id of the row, so a list that changes before the drop still moves this item.
    item_id: String,
}

impl<M: GameModel> App<M> {
    /// The whole sidebar: the panels of `layout` (all of the status without one) at their natural height, the item
    /// lists below them in their own scroll areas, at most `ITEM_SHARE` of what the status leaves, and the message
    /// log taking the rest, at least `LOG_LINES` of it. The player drags the log's top edge to share it otherwise.
    pub(super) fn sidebar_panel(&mut self, ui: &mut egui::Ui, layout: Option<&Layout>) {
        let log = layout.is_none_or(Layout::has_log);
        let line = ui.text_style_height(&egui::TextStyle::Body) + ui.spacing().item_spacing.y;
        let least_log = match log {
            true => LOG_LINES * line / NATIVE_SHARE,
            false => 0.0,
        };
        // The status scrolls only when even the least log and item lists leave it too little room.
        let room = ui.available_height() - least_log - MIN_LIST - 2.0 * PANEL_GAP;
        egui::ScrollArea::vertical()
            .id_salt("status")
            .max_height(room.max(MIN_LIST))
            .auto_shrink([false, true])
            .show(ui, |ui| self.sidebar_panels(ui, layout));
        let rest = ui.available_height() - PANEL_GAP;
        let log_height = match log {
            true => self.log_height.unwrap_or(rest * (1.0 - ITEM_SHARE)),
            false => 0.0,
        };
        let room = rest - log_height.max(least_log) - PANEL_GAP;
        if !self.model.item_lists().is_empty() {
            rule(ui);
            let size = egui::vec2(ui.available_width(), room.max(MIN_LIST));
            ui.allocate_ui(size, |ui| self.items(ui));
        }
        if !log {
            return;
        }
        let edge = rule(ui);
        // Our own handle on the log's top edge, so the player can give the log more of the sidebar.
        let handle = ui.interact(edge, egui::Id::new("log_resize"), egui::Sense::drag());
        ui.ctx().accesskit_node_builder(handle.id, |b| {
            b.set_role(accesskit::Role::Splitter);
            b.set_label(tr("Resize message log"));
        });
        if handle.hovered() || handle.dragged() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeVertical);
        }
        let rect = ui.available_rect_before_wrap();
        if handle.dragged() {
            self.log_height = Some(rect.height() - handle.drag_delta().y);
        }
        let label = self.model.log_label();
        named(ui, label.as_deref(), |ui| {
            ui.set_clip_rect(rect.intersect(ui.clip_rect()));
            self.log(ui);
        });
    }

    /// The panels of `layout`, each under the engine's label for a screen reader and a rule apart, as the native
    /// sidebar draws them, with no heading; without a layout, all of the status.
    fn sidebar_panels(&mut self, ui: &mut egui::Ui, layout: Option<&Layout>) {
        let Some(layout) = layout else {
            // Without an avatar there is no status to show.
            let status = self.model.sidebar();
            if !status.is_empty() {
                crate::sidebar::show(ui, &status);
            }
            return;
        };
        let shown = layout.panels.iter().filter(|p| p.body != Body::Log);
        for (n, panel) in shown.enumerate() {
            if n > 0 {
                rule(ui);
            }
            named(ui, Some(&panel.label), |ui| self.panel(ui, panel));
        }
    }

    /// One panel of the layout. The message log is pinned below, not here.
    fn panel(&mut self, ui: &mut egui::Ui, panel: &Panel) {
        match &panel.body {
            Body::Status(rows) => {
                crate::sidebar::rows(ui, &panel.id, rows, panel.id == "Health");
            }
            Body::Compass(directions) => self.compass(ui, directions),
            Body::Log => {}
        }
    }

    /// Per direction its heading, then its creatures nearest first, each with its sprite and distance.
    fn compass(&mut self, ui: &mut egui::Ui, directions: &[Direction]) {
        let season = self.model.season();
        let text = |text: String| egui::RichText::new(text).font(crate::sidebar::FONT);
        for direction in directions {
            ui.label(text(direction.label.clone()));
            for creature in &direction.creatures {
                ui.horizontal(|ui| {
                    icon(ui, &mut self.display, &creature.cell, season.as_deref());
                    let line = format!("{} {}", creature.name, creature.distance);
                    ui.add(egui::Label::new(text(line)).truncate());
                });
            }
        }
    }

    fn items(&mut self, ui: &mut egui::Ui) {
        let mut moved = None;
        let season = self.model.season();
        for list in self.model.item_lists() {
            let column = list.id;
            let (title, verb) = (list.label.shown(), list.verb.shown());
            let (items, cells, ids) = match column {
                ItemListId::Ground => (
                    self.model.ground(),
                    self.model.ground_cells(),
                    self.model.ground_ids(),
                ),
                ItemListId::Inventory => (
                    self.model.inventory(),
                    self.model.inventory_cells(),
                    self.model.inventory_ids(),
                ),
            };
            let rows = items
                .iter()
                .zip(ids)
                .enumerate()
                .map(|(i, (name, id))| (name, self.rich(ui, name), cells.get(i), id))
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
                        ui.strong(&title);
                        // A long list scrolls inside the room the status and the log leave, so both stay in
                        // view; the ground takes at most two fifths of it for itself.
                        let room = (ui.max_rect().max.y - ui.next_widget_position().y).max(0.0);
                        let room = match column {
                            ItemListId::Ground => room * 0.4,
                            ItemListId::Inventory => room,
                        };
                        // The tops of last frame's rows, so the area ends between two of them.
                        let key = egui::Id::new(("item_list_tops", &title));
                        let (tops, total): (Vec<f32>, f32) =
                            ui.data(|d| d.get_temp(key)).unwrap_or_default();
                        let output = egui::ScrollArea::vertical()
                            .id_salt(&title)
                            .max_height(whole_lines_height(&tops, total, room))
                            .min_scrolled_height(0.0)
                            .auto_shrink([false, true])
                            .show(ui, |ui| {
                                let top = ui.next_widget_position().y;
                                let mut tops = Vec::new();
                                for (name, label, cell, item_id) in rows {
                                    tops.push(ui.next_widget_position().y - top);
                                    ui.horizontal(|ui| {
                                        if let Some(cell) = cell {
                                            icon(ui, &mut self.display, cell, season.as_deref());
                                        }
                                        // The name is cut before it would push the button out of the panel.
                                        let button_w = ui
                                            .painter()
                                            .layout_no_wrap(
                                                verb.clone(),
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
                                            item_id: item_id.clone(),
                                        });
                                        let button = ui.button(&verb);
                                        ui.ctx().accesskit_node_builder(button.id, |b| {
                                            b.set_label(format!("{verb} {}", markup::plain(name)))
                                        });
                                        if button.clicked() {
                                            moved = Some(Dragged {
                                                from: column,
                                                item_id,
                                            });
                                        }
                                        // Tabbing to a row below the fold brings it into view.
                                        if button.gained_focus() {
                                            button.scroll_to_me(None);
                                        }
                                    });
                                }
                                tops
                            });
                        ui.data_mut(|d| d.insert_temp(key, (output.inner, output.content_size.y)));
                    })
                })
                .inner;
            if let Some(d) = dropped.filter(|d| d.from != column) {
                moved = Some((*d).clone());
            }
        }
        match moved {
            Some(Dragged {
                from: ItemListId::Ground,
                item_id,
            }) => self.model.pick_up(&item_id),
            Some(Dragged {
                from: ItemListId::Inventory,
                item_id,
            }) => self.model.drop_item(&item_id),
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

/// A gap of `PANEL_GAP` between two panels with a rule across it; the gap, where a drag handle on it sits.
fn rule(ui: &mut egui::Ui) -> egui::Rect {
    let size = egui::vec2(
        ui.available_width(),
        PANEL_GAP - 2.0 * ui.spacing().item_spacing.y,
    );
    let (rect, _) = ui.allocate_exact_size(size.max(egui::Vec2::ZERO), egui::Sense::hover());
    let stroke = egui::Stroke::new(1.0, crate::theme::BORDER);
    ui.painter().hline(rect.x_range(), rect.center().y, stroke);
    rect.expand2(egui::vec2(0.0, PANEL_GAP / 2.0))
}

/// What `add` draws, as a group a screen reader names `label` in place of a painted heading.
fn named<R>(ui: &mut egui::Ui, label: Option<&str>, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    let shown = ui.scope(add);
    let rect = shown.response.rect;
    if let Some(label) = label {
        ui.ctx().accesskit_node_builder(shown.response.id, |b| {
            b.set_role(accesskit::Role::Group);
            b.set_label(label);
            b.set_bounds(accesskit::Rect::new(
                rect.min.x.into(),
                rect.min.y.into(),
                rect.max.x.into(),
                rect.max.y.into(),
            ));
        });
    }
    shown.inner
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
    if !native.is_empty() {
        a11y::live_region(ui, live[0]);
    }
    // The tops of last frame's lines, so the area ends between two of them and shows no half line.
    let key = egui::Id::new("native_log_tops");
    let (tops, total): (Vec<f32>, f32) = ui.data(|d| d.get_temp(key)).unwrap_or_default();
    // The client's own lines share the panel only when there are some.
    let share = match commands.is_empty() {
        true => 1.0,
        false => NATIVE_SHARE,
    };
    let room = whole_lines_height(&tops, total, room * share);
    let output = egui::ScrollArea::vertical()
        .id_salt("native_log")
        .max_height(room)
        .auto_shrink([false, true])
        .stick_to_bottom(true)
        .show(ui, |ui| {
            let top = ui.next_widget_position().y;
            let mut tops = Vec::new();
            let shown = &native[native.len().saturating_sub(100)..];
            for (n, line) in shown.iter().enumerate() {
                // A rule parts one turn's lines from the next turn's.
                let before = n.checked_sub(1).map(|p| &shown[p].turn);
                if before.is_some_and(|t| t.is_some() && *t != line.turn) {
                    ui.add(egui::Separator::default().spacing(2.0));
                }
                tops.push(ui.next_widget_position().y - top);
                let text = egui::RichText::new(line.shown());
                ui.label(line.color.map_or(text.clone(), |c| {
                    text.color(crate::theme::readable(c, None))
                }));
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
