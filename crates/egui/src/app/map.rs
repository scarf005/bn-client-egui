//! The map panel: the room it asks the engine for, what screen readers hear of it, the pointer and its
//! right-click menu, and its paint.

use super::{App, CELL, menu_item, rgb};
use crate::diamond::Diamond;
use crate::display::health_bar;
use crate::iso::{self, camera::Viewport, pages::Pages, scene::Scene};
use bn_client_core::localise::{Value as Arg, tr, tr_format};
use bn_client_core::markup;
use bn_client_core::protocol::Precipitation;
use bn_client_core::protocol::{Component, MapComponent, WorldPos};
use bn_client_core::{Cell, GameModel, MapMark, MarkRole, Pos, Rgb};
use eframe::egui::{self, accesskit};
use std::sync::Arc;

/// The most squares across and down the flat map of an isometric tileset shows: the game's own view range.
const TILTED_REACH: i32 = 121;

/// The widest a square's tooltip text gets before its lines wrap; the tooltip's frame adds its margins.
const TOOLTIP_WIDTH: f32 = 420.0;
/// The share of the shorter side of the first-person view the wielded item takes.
const HELD: f32 = 0.3;

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

/// How large a shot is beside its square on the 3D map.
const SHOT_3D: f32 = 0.35;

/// The wielded item `held` in the right hand of the first-person view in `view`, its sprite pushed out as a block game
/// holds an item; none while the sheet it is cut from loads.
fn held_mesh(
    display: &crate::display::Display,
    pages: &Pages,
    held: &Cell,
    view: egui::Rect,
    season: Option<&str>,
) -> Option<egui::Mesh> {
    let (tileset, _) = display.tileset()?;
    let (layer, _) = crate::display::resolve_layer(tileset, held.layers.last()?);
    let chosen = tileset.choose(&crate::tileset::Query {
        frame: display.frame(),
        ..crate::display::query(&layer, season)
    })?;
    let sprite = chosen.fg.or(chosen.bg)?.sprite;
    let source = iso::scene::source(tileset, sprite)?;
    let outline = pages.outline(&source)?;
    let (texture, uv) = display.sprite_texture(sprite)?;
    let held = iso::held::Held {
        texture,
        uv,
        size: source.src,
        outline: &outline,
    };
    Some(iso::held::mesh(&held, view))
}

/// The 3D scene of the map and the pages it samples.
type Scene3d = (Arc<Scene>, Arc<Pages>);

/// Where the map lies on the screen this frame.
struct Canvas {
    /// The area the map lays out in; the flat squares may reach past the room, which cuts them.
    rect: egui::Rect,
    /// A square's side in points.
    cell_px: f32,
    /// The view's squares across and down.
    size: Pos,
    /// The camera of the 3D map; none on the flat one.
    view: Option<Viewport>,
    /// Where the squares of the flat map of an isometric tileset lie; none on a square grid.
    diamond: Option<Diamond>,
}

impl Canvas {
    /// A square on the screen: its rectangle on the flat map, the diamond of its ground in the 3D one and the
    /// box around the diamond on the isometric flat one.
    fn rect(&self, p: Pos) -> egui::Rect {
        if let Some(d) = &self.diamond {
            return d.tile(p);
        }
        match &self.view {
            Some(v) => ground_point(v, self.rect, p, (0.5, 0.5), 0.5)
                .map_or(egui::Rect::NOTHING, |at| {
                    egui::Rect::from_center_size(at, egui::Vec2::splat(self.cell_px))
                }),
            None => self.flat_rect(p),
        }
    }

    /// Where something the size of a square standing on square `p` is drawn: as `rect`, and on the 3D map as large
    /// as the square looks there.
    fn standing(&self, p: Pos) -> egui::Rect {
        let Some(v) = self.view.as_ref().filter(|_| self.diamond.is_none()) else {
            return self.rect(p);
        };
        let at = |z| ground_point(v, self.rect, p, (0.5, 0.5), z);
        let body = iso::aim::BODY;
        match (at(body), at(body - 0.5), at(body + 0.5)) {
            (Some(middle), Some(low), Some(high)) => {
                egui::Rect::from_center_size(middle, egui::Vec2::splat(low.distance(high)))
            }
            _ => egui::Rect::NOTHING,
        }
    }

    /// A square's rectangle on the flat map.
    fn flat_rect(&self, p: Pos) -> egui::Rect {
        egui::Rect::from_min_size(
            self.rect.min + egui::vec2(p.x as f32, p.y as f32) * self.cell_px,
            egui::Vec2::splat(self.cell_px),
        )
    }

    /// The corners of a square's ground; none when one lies behind the eye.
    fn corners(&self, v: &Viewport, p: Pos) -> Option<Vec<egui::Pos2>> {
        [(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)]
            .into_iter()
            .map(|(dx, dy)| ground_point(v, self.rect, p, (dx, dy), 0.0))
            .collect()
    }

    fn fill(&self, painter: &egui::Painter, p: Pos, color: egui::Color32) {
        if let Some(d) = &self.diamond {
            painter.add(egui::Shape::convex_polygon(
                d.corners(p).to_vec(),
                color,
                egui::Stroke::NONE,
            ));
            return;
        }
        match &self.view {
            Some(v) => {
                if let Some(corners) = self.corners(v, p) {
                    painter.add(egui::Shape::convex_polygon(
                        corners,
                        color,
                        egui::Stroke::NONE,
                    ));
                }
            }
            None => {
                painter.rect_filled(self.rect(p), 0.0, color);
            }
        }
    }

    fn outline(&self, painter: &egui::Painter, p: Pos, stroke: egui::Stroke) {
        if let Some(d) = &self.diamond {
            painter.add(egui::Shape::closed_line(d.corners(p).to_vec(), stroke));
            return;
        }
        match &self.view {
            Some(v) => {
                if let Some(corners) = self.corners(v, p) {
                    painter.add(egui::Shape::closed_line(corners, stroke));
                }
            }
            None => {
                painter.rect_stroke(self.rect(p), 0.0, stroke, egui::StrokeKind::Inside);
            }
        }
    }

    /// The screen point of the ground at (`x`, `y`) in view squares; none behind the eye.
    fn ground(&self, x: f32, y: f32) -> Option<egui::Pos2> {
        if let Some(d) = &self.diamond {
            return Some(d.ground(x, y));
        }
        match &self.view {
            Some(v) => v
                .project(glam::Vec3::new(x, y, 0.0))
                .map(|on| self.rect.min + egui::vec2(on.x, on.y)),
            None => Some(self.rect.min + egui::vec2(x, y) * self.cell_px),
        }
    }

    /// The screen points of the line of fire through `squares`, the avatar's first, over the middle of each; a z-level
    /// is `level` squares tall on the 3D map. The flat map shows one level: that of the target, the last square.
    fn fire_line(&self, squares: &[FireSquare], level: f32) -> Vec<egui::Pos2> {
        let middle = |s: &FireSquare| glam::Vec2::new(s.at.x as f32 + 0.5, s.at.y as f32 + 0.5);
        let Some(v) = &self.view else {
            let target = squares.last().map(|s| s.up);
            let on = squares.iter().filter(|s| Some(s.up) == target).map(middle);
            return on.filter_map(|p| self.ground(p.x, p.y)).collect();
        };
        let path = squares
            .iter()
            .map(|s| iso::aim::body_point(middle(s), s.up, level))
            .collect::<Vec<_>>();
        let on = iso::aim::project(v, &path).into_iter();
        on.map(|p| self.rect.min + egui::vec2(p.x, p.y)).collect()
    }

    /// The edges of the box of a creature's height around a cursor square; none off the 3D map or with a corner
    /// behind the eye.
    fn cursor_frame(&self, square: FireSquare, level: f32) -> Vec<[egui::Pos2; 2]> {
        let Some(v) = &self.view else { return vec![] };
        let floor = square.up as f32 * level;
        let corner = |(dx, dy, up): (f32, f32, bool)| {
            let z = floor + if up { iso::aim::BODY * 2.0 } else { 0.0 };
            let at = glam::Vec3::new(square.at.x as f32 + dx, square.at.y as f32 + dy, z);
            v.project(at)
                .map(|on| self.rect.min + egui::vec2(on.x, on.y))
        };
        let ring = [(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)];
        let edges = (0..4).flat_map(|i| {
            let (a, b) = (ring[i], ring[(i + 1) % 4]);
            [
                [(a.0, a.1, false), (b.0, b.1, false)],
                [(a.0, a.1, true), (b.0, b.1, true)],
                [(a.0, a.1, false), (a.0, a.1, true)],
            ]
        });
        let edges = edges.map(|[a, b]| Some([corner(a)?, corner(b)?]));
        edges.collect::<Option<Vec<_>>>().unwrap_or_default()
    }

    /// A screen's mark over the map: a selection fills its squares, a mapgen border outlines its rectangle and a
    /// mapgen stamp ticks the rectangle's corners along its edges.
    fn mark(&self, painter: &egui::Painter, mark: &MapMark) {
        let (min, max) = (mark.min, mark.max);
        let corners = [
            (min.x, min.y),
            (max.x + 1, min.y),
            (max.x + 1, max.y + 1),
            (min.x, max.y + 1),
        ]
        .map(|(x, y)| egui::vec2(x as f32, y as f32));
        let at = |p: egui::Vec2| self.ground(p.x, p.y);
        match mark.role {
            MarkRole::Selection => {
                let fill = egui::Color32::from_rgba_unmultiplied(80, 160, 255, 90);
                for y in min.y.max(0)..=max.y.min(self.size.y - 1) {
                    for x in min.x.max(0)..=max.x.min(self.size.x - 1) {
                        self.fill(painter, Pos { x, y }, fill);
                    }
                }
            }
            MarkRole::OmtBorder => {
                if let Some(points) = corners.into_iter().map(at).collect::<Option<Vec<_>>>() {
                    let stroke = egui::Stroke::new(2.0, egui::Color32::from_rgb(255, 200, 0));
                    painter.add(egui::Shape::closed_line(points, stroke));
                }
            }
            MarkRole::Stamp => {
                let stroke = egui::Stroke::new(2.0, egui::Color32::from_rgb(255, 80, 200));
                for (i, &corner) in corners.iter().enumerate() {
                    // One square along each edge that meets at the corner.
                    for next in [corners[(i + 1) % 4], corners[(i + 3) % 4]] {
                        let tick = corner + (next - corner) / (next - corner).length();
                        if let (Some(from), Some(to)) = (at(corner), at(tick)) {
                            painter.line_segment([from, to], stroke);
                        }
                    }
                }
            }
        }
    }

    /// The view square under the screen point `p`.
    fn square_at(&self, p: egui::Pos2, scene: Option<&Scene>) -> Pos {
        if let Some(d) = &self.diamond {
            let at = d.square_at(p);
            return Pos {
                x: at.x.clamp(0, self.size.x - 1),
                y: at.y.clamp(0, self.size.y - 1),
            };
        }
        let at = match &self.view {
            // The square the ray under the pointer meets first, else the ground of the avatar's level.
            Some(v) => {
                let p = p - self.rect.min;
                let p = glam::Vec2::new(p.x, p.y);
                let drawn = scene.and_then(|scene| scene.pick(v, p));
                drawn.map_or_else(
                    || v.pick(p, 0.0).map_or(glam::Vec2::ZERO, |h| h.floor()),
                    |(x, y)| glam::Vec2::new(x as f32, y as f32),
                )
            }
            None => {
                let v = (p - self.rect.min) / self.cell_px;
                glam::Vec2::new(v.x.floor(), v.y.floor())
            }
        };
        Pos {
            x: (at.x as i32).clamp(0, self.size.x - 1),
            y: (at.y as i32).clamp(0, self.size.y - 1),
        }
    }
}

/// A square the line of fire crosses: where it lies in the view, and how many z-levels over the avatar's.
#[derive(Clone, Copy)]
struct FireSquare {
    at: Pos,
    up: i32,
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
        self.display.update(ui.ctx());
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

    /// The pointer's hover, clicks and right click on the map, and the map's keyboard focus.
    fn map_pointer(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
        canvas: &Canvas,
        scene: Option<&Scene>,
        mut cursor: Pos,
    ) -> MapInput {
        let origin = self.model.origin();
        let to_abs = |view: Pos| Pos {
            x: view.x + origin.x,
            y: view.y + origin.y,
        };
        let cell_at = |p: egui::Pos2| canvas.square_at(p, scene);
        // Only a moving pointer moves the cursor, so it cannot undo keyboard navigation, nor take the cursor
        // to where it rests when a window over the map closes.
        let hover = response.hover_pos();
        let pointer = ui.input(|i| i.pointer.latest_pos());
        let moved = pointer != self.last_pointer;
        self.last_pointer = pointer;
        if let Some(p) = hover.filter(|_| moved) {
            cursor = cell_at(p);
            // The engine's own cursor follows the mouse while it is looking or aiming; otherwise the
            // native mouse view describes the square.
            match self.model.focus() {
                Some(_) => {
                    self.aim = Some(Aim {
                        at: to_abs(cursor),
                        sent: false,
                    })
                }
                None => self.model.hover(cursor),
            }
        }
        self.last_hover = hover;
        // One move is in flight at a time: the pointer's latest square goes once the engine is free, and the
        // square stands until the engine has answered it, wherever the engine then put its cursor.
        if let Some(aim) = self.aim.as_mut().filter(|_| !self.model.busy()) {
            match aim.sent {
                true => self.aim = None,
                false => {
                    aim.sent = true;
                    let at = Pos {
                        x: aim.at.x - origin.x,
                        y: aim.at.y - origin.y,
                    };
                    self.model.aim(at, false);
                }
            }
        }
        let mut menu_key = false;
        // A click goes to the square it landed on, not to where the pointer was last seen hovering.
        let clicked = response.interact_pointer_pos().map_or(cursor, cell_at);
        if response.clicked() {
            // A click while the engine owns the cursor moves or confirms it; otherwise it travels.
            if !self.model.aim(clicked, true) {
                self.go_to(clicked);
            }
            ui.ctx().request_repaint();
        }
        if response.secondary_clicked() {
            self.menu_cell = to_abs(clicked);
        }
        // With nothing focused, an arrow key would move egui's focus onto a toolbar widget and a later
        // Space would press it, so the map holds the focus.
        if (self.refocus_map || ui.memory(|m| m.focused()).is_none()) && self.model.menu().is_none()
        {
            self.refocus_map = false;
            response.request_focus();
        }
        if response.has_focus() {
            // Keep arrow keys from moving egui's focus away from the map.
            ui.memory_mut(|m| {
                m.set_focus_lock_filter(
                    response.id,
                    egui::EventFilter {
                        // Tab is the engine's here (auto-fight); it must not walk the focus over the sidebar.
                        tab: true,
                        horizontal_arrows: true,
                        vertical_arrows: true,
                        ..Default::default()
                    },
                )
            });
            // A keyboard cursor gets the same native description a hovering pointer does. The arrow keys
            // only walk: outside look and aim mode the cursor follows the mouse, else it is the player's square.
            if self.model.focus().is_none() && !response.context_menu_opened() {
                self.model.hover(cursor);
            }
            // Shift+F10, the keyboard's context-menu key, is the right click on the cursor square.
            if ui.input(|i| i.modifiers.shift && i.key_pressed(egui::Key::F10)) {
                self.menu_cell = to_abs(cursor);
                menu_key = true;
            }
        }
        MapInput { cursor, menu_key }
    }

    /// The engine's description of the square under the pointer; keyboard moves of the cursor leave it alone.
    fn map_tooltip(
        &mut self,
        response: &egui::Response,
        canvas: &Canvas,
        scene: Option<&Scene>,
        cursor: Pos,
    ) {
        // A dialog on top owns the pointer; the tooltip would hang over it.
        let tip = self.model.menu().is_none()
            && self.palette.is_none()
            && !response.context_menu_opened();
        let under = response
            .hover_pos()
            .map_or(cursor, |p| canvas.square_at(p, scene));
        // A square nothing is known about says nothing.
        let cell = self.model.cell(under);
        let known = !(cell.layers.is_empty() && cell.desc.is_empty());
        let season = self.model.season();
        response.clone().on_hover_ui_at_pointer(|ui| {
            if !tip || !known {
                return;
            }
            // The engine's own mouse-view facts when it has answered for this square. A creature's name heads
            // them beside its sprite, its health bar drawn as a bar.
            let parts = self.model.hover_parts(under).unwrap_or_default();
            let creature = parts.iter().find(|p| p.kind == "creature");
            let hp = parts
                .iter()
                .find(|p| p.kind == "hp")
                .filter(|_| creature.is_some());
            // Under a heading, the creature's name and bar are drawn as such; what more it says stays a line.
            let lines = match parts.is_empty() {
                true => vec![cell.desc.clone()],
                false => parts
                    .iter()
                    .flat_map(|p| match (creature.is_some(), p.kind.as_str()) {
                        (true, "hp") => vec![],
                        (true, "creature") => p.detail.iter().collect(),
                        _ => std::iter::once(&p.text).chain(&p.detail).collect(),
                    })
                    .filter(|text| !markup::plain(text).trim().is_empty())
                    .cloned()
                    .collect(),
            };
            // A tooltip at the pointer gets almost no width to wrap in: its text takes its widest line's width, up
            // to `TOOLTIP_WIDTH`, and wraps the longer lines there.
            let font = egui::TextStyle::Body.resolve(ui.style());
            let name = creature.map(|c| markup::plain(&c.text));
            let widest = lines
                .iter()
                .map(|l| markup::plain(l))
                .chain(name.clone())
                .map(|l| super::component::text_width(ui, &l, &font))
                .fold(0.0, f32::max);
            ui.set_width(widest.ceil().min(TOOLTIP_WIDTH));
            if let Some(name) = name {
                ui.horizontal(|ui| {
                    if self.display.tiles() {
                        let (rect, _) = ui.allocate_exact_size(
                            egui::vec2(CELL, CELL) * 2.0,
                            egui::Sense::hover(),
                        );
                        let body = Cell {
                            layers: cell
                                .layers
                                .iter()
                                .skip_while(|l| !l.creature)
                                .cloned()
                                .collect(),
                            ..Cell::default()
                        };
                        self.display
                            .paint_icon(ui.painter(), rect, &body, season.as_deref());
                    }
                    ui.label(egui::RichText::new(name).strong());
                });
            }
            if let Some(hp) = hp {
                let value = |key: &str| hp.values.get(key).map(|&v| v as f32);
                let done = match (value("hp"), value("max"), value("percent")) {
                    (Some(hp), Some(max), _) if max > 0.0 => hp / max,
                    (_, _, Some(percent)) => percent / 100.0,
                    _ => 0.0,
                };
                let color = markup::runs(&hp.text, self.model.colors())
                    .into_iter()
                    .find_map(|r| r.color)
                    .unwrap_or(Rgb(255, 255, 255));
                let bar = egui::ProgressBar::new(done.clamp(0.0, 1.0))
                    .desired_height(6.0)
                    .fill(rgb(color))
                    .corner_radius(0);
                ui.add(bar);
            }
            for line in lines {
                let line = self.rich(ui, &line);
                ui.add(egui::Label::new(line).wrap_mode(egui::TextWrapMode::Wrap));
            }
        });
    }

    /// The right click lists what the engine says the avatar can do with the square; the choice is sent as
    /// that action. A keyboard cursor opens the same menu on its square (`menu_key`). The menu sits at its
    /// square, which a keyboard has no pointer for.
    fn map_menu(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
        canvas: &Canvas,
        menu_key: bool,
    ) {
        // The menu's square is absolute: a view that scrolls under the open menu does not change it.
        let origin = self.model.origin();
        let at = Pos {
            x: self.menu_cell.x - origin.x,
            y: self.menu_cell.y - origin.y,
        };
        let mut popup = egui::Popup::context_menu(response).anchor(egui::Rect::from_min_size(
            match canvas.view {
                // The 3D scene has no square grid to place the menu on; it opens at the map's centre.
                Some(_) => response.rect.center(),
                None if canvas.diamond.is_some() => canvas.rect(at).left_top(),
                None => canvas.flat_rect(at).min,
            },
            egui::Vec2::splat(canvas.cell_px),
        ));
        if menu_key {
            popup = popup.open_memory(egui::SetOpenCommand::Bool(true));
        }
        let menu = popup.show(|ui| {
            let Some(actions) = self.model.tile_actions(at) else {
                // The engine has not answered for this square yet.
                ui.add_enabled(false, egui::Label::new("…"));
                ui.ctx().request_repaint();
                return;
            };
            if actions.is_empty() {
                ui.close();
            }
            for action in actions {
                let key = action
                    .keys
                    .first()
                    .map_or(String::new(), |k| format!(" [{k}]"));
                let label = format!(
                    "{}{key}",
                    bn_client_core::markup::plain(&action.label.shown())
                );
                if menu_item(ui, &label) {
                    self.model.tile_action(at, &action.id);
                    ui.ctx().request_repaint();
                    ui.close();
                }
            }
        });
        if let Some(menu) = menu {
            ui.ctx().accesskit_node_builder(menu.response.id, |b| {
                b.set_role(accesskit::Role::Menu);
                b.set_label(tr("Map actions"));
            });
        }
    }

    /// The squares: the 3D scene with the glyphs it has no sprite for, or the flat map's sprites and glyphs.
    fn paint_map(
        &mut self,
        painter: &egui::Painter,
        canvas: &Canvas,
        shown: Option<Scene3d>,
        season: Option<&str>,
    ) {
        let tiles = self.display.tiles();
        if tiles {
            painter.rect_filled(canvas.rect, 0.0, egui::Color32::BLACK);
        }
        profiling::scope!("map cells");
        if let (Some(v), Some(target)) = (&canvas.view, self.iso.target) {
            if let Some(shown) = shown {
                let stats = self.iso.stats.clone();
                let (scene, pages) = (shown.0.clone(), shown.1.clone());
                // The first-person view looks into the sky the engine describes; looking down, there is none.
                let sky = self.model.sky().filter(|_| v.camera.first_person);
                painter.add(iso::callback(
                    canvas.rect,
                    v,
                    target,
                    shown,
                    (stats, sky.as_ref()),
                ));
                iso::paint_glyphs(painter, canvas.rect, v, &scene, canvas.cell_px);
                if let Some(sky) = sky.as_ref().filter(|_| !iso::sheltered(&self.model)) {
                    // With reduced motion what falls stands still.
                    let still = self.display.reduced_motion();
                    let time = if still {
                        0.0
                    } else {
                        painter.ctx().input(|i| i.time)
                    };
                    iso::paint_weather(painter, canvas.rect, sky, time);
                    if sky.precipitation != Precipitation::None && !still {
                        painter
                            .ctx()
                            .request_repaint_after(std::time::Duration::from_millis(33));
                    }
                }
                iso::paint_bearings(painter, canvas.rect, v, &scene);
                if v.camera.first_person
                    && let Some(held) = self.model.wielded_cell()
                {
                    match held_mesh(&self.display, &pages, &held, canvas.rect, season) {
                        Some(mesh) => {
                            painter.add(mesh);
                        }
                        // The flat picture while the sheet it is cut from loads: the right hand holds it low, so
                        // the bottom edge cuts it.
                        None => {
                            let side = canvas.rect.size().min_elem() * HELD;
                            let corner =
                                canvas.rect.right_bottom() + egui::vec2(-1.5 * side, -0.8 * side);
                            let rect = egui::Rect::from_min_size(corner, egui::Vec2::splat(side));
                            super::sidebar::paint_cell(
                                painter,
                                rect,
                                &mut self.display,
                                &held,
                                season,
                            );
                        }
                    }
                }
            }
            return;
        }
        let squares = match &canvas.diamond {
            // The game's order: row by row down the screen, so what stands lower covers what stands behind it.
            // Sprites reach up above their square, so squares just below the canvas are drawn too.
            Some(d) => {
                let reach = egui::Rect::from_min_max(
                    canvas.rect.min - egui::vec2(canvas.cell_px, 0.0),
                    canvas.rect.max + egui::vec2(canvas.cell_px, 2.0 * canvas.cell_px),
                );
                d.order(canvas.size)
                    .filter(|p| reach.intersects(d.tile(*p)))
                    .collect::<Vec<_>>()
            }
            None => (0..canvas.size.y)
                .flat_map(|y| (0..canvas.size.x).map(move |x| Pos { x, y }))
                .collect(),
        };
        let font = egui::FontId::monospace(match canvas.diamond {
            Some(_) => canvas.cell_px / 2.0 - 2.0,
            None => canvas.cell_px - 2.0,
        });
        for p in squares {
            let cell = self.model.cell(p);
            let square = canvas.rect(p);
            if !(tiles && self.display.paint(painter, square, &cell, season)) {
                if let Some(bg) = cell.bg {
                    painter.rect_filled(square, 0.0, rgb(bg));
                }
                painter.text(
                    square.center(),
                    egui::Align2::CENTER_CENTER,
                    cell.glyph,
                    font.clone(),
                    rgb(cell.color),
                );
            }
            if let Some(health) = cell.health {
                let track = egui::Rect::from_min_max(
                    egui::pos2(square.left(), square.bottom() - 2.0),
                    square.right_bottom(),
                );
                health_bar(painter, track, health);
            }
        }
    }

    /// What the client draws over the squares: the planned route, the clicked square, the avatar where nothing
    /// else draws it, the `cursor` square and the playing animations.
    fn paint_overlays(
        &mut self,
        ui: &egui::Ui,
        painter: &egui::Painter,
        canvas: &Canvas,
        route: &[Pos],
        cursor: Option<Pos>,
        now: f64,
    ) {
        let size = canvas.size;
        for (i, p) in route.iter().enumerate() {
            let in_view = (0..size.x).contains(&p.x) && (0..size.y).contains(&p.y);
            if in_view {
                let alpha = if i + 1 == route.len() { 140 } else { 70 };
                canvas.fill(
                    painter,
                    *p,
                    egui::Color32::from_rgba_unmultiplied(0, 200, 255, alpha),
                );
            }
        }
        // The line of fire the engine publishes while aiming, from the avatar to the cursor in the map's own space.
        let origin = self.model.origin();
        let aimed = self.model.screens().last().and_then(|s| s.root.as_ref());
        if let Some(map) = aimed.and_then(Component::cursor_map)
            && let Some(source) = map.source()
        {
            let level = self
                .display
                .tileset()
                .map_or(3.0, |(t, _)| iso::scene::metrics(t).level);
            let square = |p: &WorldPos| FireSquare {
                at: Pos {
                    x: p.x - origin.x,
                    y: p.y - origin.y,
                },
                up: p.z - source.z,
            };
            let squares = std::iter::once(&source)
                .chain(&map.trajectory())
                .map(square)
                .collect::<Vec<_>>();
            let line = canvas.fire_line(&squares, level);
            if line.len() > 1 && squares.len() > 1 {
                let shade = egui::Stroke::new(4.0, egui::Color32::from_black_alpha(170));
                painter.add(egui::Shape::line(line.clone(), shade));
                let orange = egui::Stroke::new(2.0, egui::Color32::from_rgb(255, 170, 0));
                painter.add(egui::Shape::line(line, orange));
            }
            let frame = map.cursor().map(|p| canvas.cursor_frame(square(&p), level));
            let shade = egui::Stroke::new(3.5, egui::Color32::from_black_alpha(170));
            let white = egui::Stroke::new(1.5, egui::Color32::WHITE);
            let frame = frame.into_iter().flatten().collect::<Vec<_>>();
            frame.iter().for_each(|e| {
                painter.line_segment(*e, shade);
            });
            frame.iter().for_each(|e| {
                painter.line_segment(*e, white);
            });
        }
        for mark in self.model.marks() {
            canvas.mark(painter, &mark);
        }
        if let Some(at) = self.marker() {
            canvas.outline(
                painter,
                at,
                egui::Stroke::new(2.0, egui::Color32::from_rgb(0, 200, 255)),
            );
        }
        if let Some(cursor) = cursor {
            canvas.outline(
                painter,
                cursor,
                egui::Stroke::new(1.0, egui::Color32::WHITE),
            );
        }
        // How much of the avatar's activity is done, in a thin bar just over its square.
        if let Some(done) = self.model.activity().and_then(|a| a.done) {
            let square = canvas.rect(self.model.player());
            let track = egui::Rect::from_min_size(
                square.left_top() - egui::vec2(0.0, 6.0),
                egui::vec2(square.width(), 4.0),
            );
            painter.rect_filled(track, 0.0, egui::Color32::from_black_alpha(180));
            let mut filled = track;
            filled.set_width(track.width() * f32::from(done) / 1000.0);
            painter.rect_filled(filled, 0.0, ui.visuals().selection.bg_fill);
        }
        // Effects stand on world squares: placed by the origin of this frame, not of when they arrived.
        let tiles = self.display.tiles();
        // On the 3D map a shot is a small thing flying through the square, not a sprite as large as it.
        let shot = if canvas.view.is_some() && canvas.diamond.is_none() {
            SHOT_3D
        } else {
            1.0
        };
        self.effects.paint(
            painter,
            (now, canvas.cell_px, shot),
            |p| {
                canvas.standing(Pos {
                    x: p.x - origin.x,
                    y: p.y - origin.y,
                })
            },
            &mut self.display,
            !tiles,
        );
        if self.effects.active() {
            ui.ctx().request_repaint();
        }
    }
}

/// Where the point `at` (across and down the square, 0 to 1) of square `p`, `z` over its ground, lies on the
/// screen of a 3D map drawn into `rect`; none behind the eye.
fn ground_point(
    v: &Viewport,
    rect: egui::Rect,
    p: Pos,
    at: (f32, f32),
    z: f32,
) -> Option<egui::Pos2> {
    let on = v.project(glam::Vec3::new(p.x as f32 + at.0, p.y as f32 + at.1, z))?;
    Some(rect.min + egui::vec2(on.x, on.y))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::iso::camera::Camera;

    fn canvas(view: Option<Viewport>) -> Canvas {
        Canvas {
            rect: egui::Rect::from_min_size(egui::pos2(100.0, 50.0), egui::vec2(1280.0, 720.0)),
            cell_px: 32.0,
            size: Pos { x: 40, y: 40 },
            view,
            diamond: None,
        }
    }

    fn orbit() -> Viewport {
        Viewport {
            camera: Camera::default(),
            target: glam::Vec3::new(10.5, 10.5, 0.0),
            scale: 34.0,
            size: glam::Vec2::new(1280.0, 720.0),
        }
    }

    fn row(x: &[i32]) -> Vec<FireSquare> {
        x.iter()
            .map(|x| FireSquare {
                at: Pos { x: *x, y: 10 },
                up: 0,
            })
            .collect()
    }

    #[test]
    fn the_flat_line_of_fire_runs_through_the_middle_of_each_square_from_the_avatar() {
        let canvas = canvas(None);
        let line = canvas.fire_line(&row(&[10, 11, 12, 13]), 3.0);
        let middle = |x: i32| canvas.rect(Pos { x, y: 10 }).center();
        assert_eq!(line, [10, 11, 12, 13].map(middle));
    }

    #[test]
    fn the_flat_line_of_fire_keeps_only_the_squares_on_the_level_of_its_target() {
        let canvas = canvas(None);
        let mut squares = row(&[10, 11, 12, 13]);
        squares[0].up = -1;
        squares[1].up = -1;
        let line = canvas.fire_line(&squares, 3.0);
        let middle = |x: i32| canvas.rect(Pos { x, y: 10 }).center();
        assert_eq!(line, [12, 13].map(middle));
    }

    #[test]
    fn the_3d_line_of_fire_flies_over_the_ground_of_its_squares_and_ends_at_the_target() {
        let canvas = canvas(Some(orbit()));
        let line = canvas.fire_line(&row(&[10, 11, 12, 13]), 3.0);
        let ground = canvas.ground(13.5, 10.5).unwrap();
        let end = line.last().unwrap();
        assert!(end.y < ground.y - 5.0, "{end:?} over {ground:?}");
        assert!((end.x - ground.x).abs() < 1.0, "{end:?} over {ground:?}");
        assert!(
            line.len() > 4,
            "the line is a curve of the view, not four corners"
        );
    }

    #[test]
    fn the_3d_cursor_is_a_box_a_creature_tall_standing_on_its_square() {
        let canvas = canvas(Some(orbit()));
        let at = FireSquare {
            at: Pos { x: 13, y: 10 },
            up: 0,
        };
        let frame = canvas.cursor_frame(at, 3.0);
        assert_eq!(frame.len(), 12);
        let ends = || frame.iter().flatten();
        let corner = canvas.ground(13.0, 10.0).unwrap();
        assert!(
            ends().any(|p| p.distance(corner) < 0.5),
            "no edge stands on {corner:?}"
        );
        let top = ends().map(|p| p.y).fold(f32::MAX, f32::min);
        assert!(
            top < corner.y - 20.0,
            "the box is not tall: {top} over {corner:?}"
        );
        assert!(self::canvas(None).cursor_frame(at, 3.0).is_empty());
    }

    #[test]
    fn a_shot_in_first_person_is_drawn_smaller_the_farther_it_flies() {
        let avatar = glam::Vec2::new(10.5, 10.5);
        let ahead = glam::Vec2::new(30.5, 10.5);
        let canvas = canvas(Some(Viewport {
            camera: Camera {
                yaw: iso::aim::yaw_facing(avatar, ahead),
                pitch: 0.6,
                first_person: true,
            },
            target: glam::Vec3::new(avatar.x, avatar.y, 0.0),
            scale: 34.0,
            size: glam::Vec2::new(1280.0, 720.0),
        }));
        let (near, far) = (
            canvas.standing(Pos { x: 12, y: 10 }),
            canvas.standing(Pos { x: 18, y: 10 }),
        );
        assert!(near.width() > far.width() * 2.0, "{near:?} next to {far:?}");
        assert!(far.width() > 0.0, "{far:?}");
    }
}
