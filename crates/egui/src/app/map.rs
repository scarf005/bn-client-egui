//! The map panel: the room it asks the engine for, what screen readers hear of it, the pointer and its
//! right-click menu, and its paint.

mod canvas;
mod paint;
mod pointer;

use super::{App, CELL, menu_item, rgb};
use crate::diamond::Diamond;
use crate::display::health_bar;
use crate::iso::{self, camera::Viewport, pages::Pages, scene::Scene};
use bn_client_core::localise::{Value as Arg, tr, tr_format};
use bn_client_core::markup;
use bn_client_core::protocol::Precipitation;
use bn_client_core::protocol::{Component, MapComponent, WorldPos};
use bn_client_core::{Cell, GameModel, MapMark, MarkRole, Pos, Rgb};
use canvas::*;
use eframe::egui::{self, accesskit};
use std::sync::Arc;

/// The most squares across and down the flat map of an isometric tileset shows: the game's own view range.
const TILTED_REACH: i32 = 121;

/// Where the squares of the map were drawn in the last frame, to place a window off the cursor.
#[derive(Clone, Copy)]
pub(super) struct MapView {
    /// The map canvas's top left corner.
    pub(super) origin: egui::Pos2,
    /// A square's side in points.
    pub(super) cell: f32,
    pub(super) diamond: Option<Diamond>,
}

impl MapView {
    /// The screen rectangle of a view square.
    pub(super) fn square(&self, at: Pos) -> egui::Rect {
        match self.diamond {
            Some(d) => d.tile(at),
            None => egui::Rect::from_min_size(
                self.origin + egui::vec2(at.x as f32, at.y as f32) * self.cell,
                egui::Vec2::splat(self.cell),
            ),
        }
    }
}

/// The absolute square a moving pointer asked the engine's cursor (look, aim) to go to.
#[derive(Clone, Copy)]
pub(super) struct Aim {
    pub(super) at: Pos,
    /// The move went to the engine; once it is no longer busy, it has answered.
    pub(super) sent: bool,
}

/// What the pointer and the keyboard did on the map this frame.
struct MapInput {
    /// The cursor's view square, moved by a moving pointer.
    cursor: Pos,
    /// Shift+F10 asked for the right-click menu on the cursor's square.
    menu_key: bool,
}

impl<M: GameModel> App<M> {
    pub(super) fn map(&mut self, ui: &mut egui::Ui) {
        let cell_px = self.display.cell(CELL) * self.display.zoom();
        self.map_view = None;
        self.display.begin();
        let three = self.iso.active(&self.display);
        // An isometric tileset on the flat map lays its diamonds out as the game's tile view does.
        let tilted =
            !three && self.display.tiles() && self.display.tileset().is_some_and(|(t, _)| t.iso);
        let now = self.map_viewport(ui, cell_px, (three, tilted));
        let size = self.model.size();
        if size.x <= 0 || size.y <= 0 {
            ui.label(tr("No map loaded"));
            return;
        }
        // The cursor is kept on a map square, so it stays there when the view scrolls; the engine's own
        // cursor (look mode) wins. The map may also have been replaced by a smaller one since it was set.
        let origin = self.model.origin();
        let to_abs = |view: Pos| Pos {
            x: view.x + origin.x,
            y: view.y + origin.y,
        };
        let looking = self.model.focus();
        match looking {
            // The square the pointer asked for stands until the engine has answered, so no frame shows the
            // square the engine's cursor is leaving.
            Some(focus) => self.cursor = self.aim.map_or(to_abs(focus), |aim| aim.at),
            None => self.aim = None,
        }
        let player = self.model.player();
        // Native shows no cursor outside look mode and the mouse: without a pointer it is the player's square.
        if looking.is_none() && self.last_hover.is_none() {
            self.cursor = to_abs(player);
        }
        let cursor = Pos {
            x: (self.cursor.x - origin.x).clamp(0, size.x - 1),
            y: (self.cursor.y - origin.y).clamp(0, size.y - 1),
        };
        let route = self.model.route();
        // The cursor's square is said politely and never drawn; the map's own description says it too.
        let said = ui.interact(
            egui::Rect::from_min_size(ui.max_rect().min, egui::Vec2::ZERO),
            ui.id().with("map_status"),
            egui::Sense::hover(),
        );
        // The 3D map fills the room and turns with the right button; the flat one is its squares, cut at the
        // room's edges.
        let (extent, sense) = match three {
            true => (ui.available_size(), egui::Sense::click_and_drag()),
            false if tilted => (ui.available_size(), egui::Sense::click()),
            false => (
                (egui::vec2(size.x as f32, size.y as f32) * cell_px).min(ui.available_size()),
                egui::Sense::click(),
            ),
        };
        let first_person = self.display.first_person();
        let (response, painter) = ui
            .with_layout(egui::Layout::top_down(egui::Align::Center), |ui| {
                ui.allocate_painter(extent, sense.union(egui::Sense::focusable_noninteractive()))
            })
            .inner;
        self.face_aim(first_person);
        let view = three.then(|| {
            let scale = self
                .display
                .tileset()
                .map_or(cell_px, |(t, _)| iso::scene::world_scale(t, cell_px));
            self.iso
                .viewport(player, scale, response.rect.size(), first_person)
        });
        let diamond = self
            .display
            .tileset()
            .filter(|_| tilted)
            .map(|(t, _)| Diamond {
                centre: response.rect.center(),
                at: self.model.player(),
                width: cell_px,
                ratio: t.tile.1 as f32 / t.tile.0 as f32,
            });
        // The flat squares are laid out around the middle one, the avatar's, and cut at the room's edges.
        let grid = egui::Rect::from_center_size(
            response.rect.center(),
            egui::vec2(size.x as f32, size.y as f32) * cell_px,
        );
        let canvas = Canvas {
            rect: if three || tilted { response.rect } else { grid },
            cell_px,
            size,
            view,
            diamond,
        };
        let season = self.model.season();
        // The scene is built before the pointer is read: picking casts its ray into it.
        let shown = match (&canvas.view, self.iso.target) {
            (Some(_), Some(_)) => {
                let ctx = ui.ctx().clone();
                self.iso
                    .scene(&ctx, &self.model, &self.display, season.as_deref())
            }
            _ => None,
        };
        if canvas.view.is_some() && response.dragged_by(egui::PointerButton::Secondary) {
            self.iso.drag(response.drag_delta());
            ui.ctx().request_repaint();
        }
        response
            .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Other, true, tr("Map")));
        self.map_wheel(ui, &response);
        self.map_view = Some(MapView {
            origin: canvas.rect.min,
            cell: cell_px,
            diamond,
        });
        let scene = shown.as_ref().map(|(scene, _)| scene.as_ref());
        let input = self.map_pointer(ui, &response, &canvas, scene, cursor);
        let cursor = input.cursor;
        self.cursor = to_abs(cursor);
        let status = self.map_status(origin, cursor);
        said.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Label, true, &status));
        ui.ctx()
            .accesskit_node_builder(said.id, |b| b.set_live(accesskit::Live::Polite));
        ui.ctx().accesskit_node_builder(response.id, |b| {
            b.set_role(accesskit::Role::Canvas);
            b.set_description(status.as_str());
        });
        self.map_tooltip(&response, &canvas, scene, cursor);
        self.map_menu(ui, &response, &canvas, input.menu_key);
        self.paint_map(&painter, &canvas, shown, season.as_deref());
        let marked = looking.is_some() || response.hover_pos().is_some();
        self.paint_overlays(ui, &painter, &canvas, &route, marked.then_some(cursor), now);
    }

    /// While aiming, the first-person eye looks at the square the cursor moves to, so it is on the screen.
    fn face_aim(&mut self, first_person: bool) {
        let aimed = self.model.screens().last().and_then(|s| s.root.as_ref());
        let aimed = aimed.and_then(Component::cursor_map);
        let aimed = aimed.filter(|m| first_person && !m.trajectory().is_empty());
        let cursor = aimed.and_then(MapComponent::cursor);
        if let (Some(m), Some(to), true) = (aimed, &cursor, cursor != self.faced) {
            let middle = |p: &WorldPos| glam::Vec2::new(p.x as f32 + 0.5, p.y as f32 + 0.5);
            m.source()
                .iter()
                .for_each(|from| self.iso.face(middle(from), middle(to)));
        }
        self.faced = cursor;
    }

    /// A click on a map square: the engine plans the route, a repeat walks it.
    fn go_to(&mut self, view: Pos) {
        self.model.travel(view);
        let origin = self.model.origin();
        self.marker = self.model.busy().then_some(Pos {
            x: view.x + origin.x,
            y: view.y + origin.y,
        });
    }

    /// Asks the engine for as many squares as the room holds, and takes the animation facts; returns the time
    /// they are played from.
    fn map_viewport(&mut self, ui: &egui::Ui, cell_px: f32, (three, tilted): (bool, bool)) -> f64 {
        // The room left for the map, in whole cells.
        let room = ui.available_size();
        // The 3D map asks for the squares across the room's diagonal, so a turned camera finds none missing.
        let scale = self
            .display
            .tileset()
            .map_or(cell_px, |(t, _)| iso::scene::world_scale(t, cell_px));
        let reach =
            ((room.length() / scale).ceil() as i32 + 2).clamp(iso::REACH, iso::MAX_REACH) | 1;
        self.model.set_viewport(match (three, tilted) {
            (true, _) => Pos { x: reach, y: reach },
            // The squares that reach the corners of the room: as many as it holds diagonally, from its middle.
            (_, true) => {
                let around = ((room.x + 2.0 * room.y) / cell_px).ceil() as i32 + 3;
                Pos {
                    x: around.clamp(iso::REACH, TILTED_REACH),
                    y: around.clamp(iso::REACH, TILTED_REACH),
                }
            }
            // Enough squares to cover the room, an odd number so the avatar's is the middle one.
            _ => Pos {
                x: (room.x / cell_px).ceil() as i32 | 1,
                y: (room.y / cell_px).ceil() as i32 | 1,
            },
        });
        // Facts are always taken, so a long pause never replays them; reduced motion drops them.
        let now = ui.input(|i| i.time);
        let produced = self.model.take_effects();
        self.effects
            .add(now, produced, self.display.reduced_motion());
        now
    }

    /// What a screen reader says of the map: the player's and the cursor's squares, the planned route and the aim.
    fn map_status(&self, origin: Pos, cursor: Pos) -> String {
        let player = self.model.player();
        // The engine's mouse-view text when it has answered for this square; nothing for an unknown square.
        let said = self.model.hover_text(cursor).map_or_else(
            || self.model.cell(cursor).desc,
            |lines| {
                let lines = lines.iter().map(|l| l.trim()).filter(|l| !l.is_empty());
                lines.collect::<Vec<_>>().join(" ")
            },
        );
        let mut args = vec![
            Arg::Int((player.x + origin.x).into()),
            Arg::Int((player.y + origin.y).into()),
            Arg::Int((cursor.x + origin.x).into()),
            Arg::Int((cursor.y + origin.y).into()),
        ];
        let mut status = match said.trim_end_matches('.') {
            "" => tr_format("Player at (%d, %d). Cursor (%d, %d).", &args),
            said => {
                args.push(Arg::Text(said.into()));
                tr_format("Player at (%d, %d). Cursor (%d, %d): %s.", &args)
            }
        };
        if let Some(route) = self.model.route_text() {
            status += &format!(" {route}");
        }
        if let Some(aiming) = self.model.aiming() {
            status = format!("{aiming}. {status}");
        }
        status
    }

    /// One zoom step per wheel line over the map, not per smoothed frame; a touchpad's points add up to lines.
    fn map_wheel(&mut self, ui: &egui::Ui, response: &egui::Response) {
        if !response.hovered() {
            return;
        }
        let line = ui.ctx().options(|o| o.input_options.line_scroll_speed);
        self.wheel += ui.input(|i| {
            i.raw
                .events
                .iter()
                .map(|e| match e {
                    egui::Event::MouseWheel {
                        unit: egui::MouseWheelUnit::Point,
                        delta,
                        ..
                    } => delta.y / line,
                    // A page is one step too: the zoom has no page.
                    egui::Event::MouseWheel { delta, .. } => delta.y,
                    _ => 0.0,
                })
                .sum::<f32>()
        });
        let steps = self.wheel.trunc();
        self.wheel -= steps;
        if steps != 0.0 {
            self.display.zoom_by(steps as i8);
            ui.ctx().request_repaint();
        }
    }
}
