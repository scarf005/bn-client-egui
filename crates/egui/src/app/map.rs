//! The map panel: the room it asks the engine for, what screen readers hear of it, the pointer and its
//! right-click menu, and its paint.

use super::{App, CELL, menu_item, rgb};
use crate::iso::{self, camera::Viewport, pages::Pages, scene::Scene};
use bn_client_core::{GameModel, Pos};
use eframe::egui::{self, accesskit};
use std::sync::Arc;

/// The 3D scene of the map and the pages it samples.
type Scene3d = (Arc<Scene>, Arc<Pages>);

/// Where the map lies on the screen this frame.
struct Canvas {
    /// The area the map paints into.
    rect: egui::Rect,
    /// A square's side in points.
    cell_px: f32,
    /// The view's squares across and down.
    size: Pos,
    /// The camera of the 3D map; none on the flat one.
    view: Option<Viewport>,
}

impl Canvas {
    /// A square on the screen: its rectangle on the flat map, the diamond of its ground in the 3D one.
    fn rect(&self, p: Pos) -> egui::Rect {
        match &self.view {
            Some(v) => egui::Rect::from_center_size(
                ground_point(v, self.rect, p, (0.5, 0.5), 0.5),
                egui::Vec2::splat(self.cell_px),
            ),
            None => self.flat_rect(p),
        }
    }

    /// A square's rectangle on the flat map.
    fn flat_rect(&self, p: Pos) -> egui::Rect {
        egui::Rect::from_min_size(
            self.rect.min + egui::vec2(p.x as f32, p.y as f32) * self.cell_px,
            egui::Vec2::splat(self.cell_px),
        )
    }

    fn corners(&self, v: &Viewport, p: Pos) -> Vec<egui::Pos2> {
        [(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)]
            .map(|(dx, dy)| ground_point(v, self.rect, p, (dx, dy), 0.0))
            .to_vec()
    }

    fn fill(&self, painter: &egui::Painter, p: Pos, color: egui::Color32) {
        match &self.view {
            Some(v) => {
                painter.add(egui::Shape::convex_polygon(
                    self.corners(v, p),
                    color,
                    egui::Stroke::NONE,
                ));
            }
            None => {
                painter.rect_filled(self.rect(p), 0.0, color);
            }
        }
    }

    fn outline(&self, painter: &egui::Painter, p: Pos, stroke: egui::Stroke) {
        match &self.view {
            Some(v) => {
                painter.add(egui::Shape::closed_line(self.corners(v, p), stroke));
            }
            None => {
                painter.rect_stroke(self.rect(p), 0.0, stroke, egui::StrokeKind::Inside);
            }
        }
    }

    /// The view square under the screen point `p`.
    fn square_at(&self, p: egui::Pos2, scene: Option<&Scene>) -> Pos {
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
        let now = self.map_viewport(ui, cell_px, three);
        let size = self.model.size();
        if size.x <= 0 || size.y <= 0 {
            ui.label("No map loaded");
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
        // The squares fill whole cells, so what is left over is shared by both sides, not left as a band.
        // The 3D map fills the room and turns with the right button; the flat one is its squares.
        let (extent, sense) = match three {
            true => (ui.available_size(), egui::Sense::click_and_drag()),
            false => (
                egui::vec2(size.x as f32 * cell_px, size.y as f32 * cell_px),
                egui::Sense::click(),
            ),
        };
        let (response, painter) = ui
            .with_layout(egui::Layout::top_down(egui::Align::Center), |ui| {
                ui.allocate_painter(extent, sense.union(egui::Sense::focusable_noninteractive()))
            })
            .inner;
        let view = three.then(|| {
            let scale = self
                .display
                .tileset()
                .map_or(cell_px, |(t, _)| iso::scene::world_scale(t, cell_px));
            self.iso.viewport(player, scale, response.rect.size())
        });
        let canvas = Canvas {
            rect: response.rect,
            cell_px,
            size,
            view,
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
        response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Other, true, "Map"));
        self.map_wheel(ui, &response);
        self.map_view = Some((response.rect.min, cell_px));
        let scene = shown.as_ref().map(|(scene, _)| scene.as_ref());
        let input = self.map_pointer(ui, &response, &canvas, scene, cursor);
        let cursor = input.cursor;
        self.cursor = to_abs(cursor);
        let status = self.map_status(origin, cursor, &route);
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
    fn map_viewport(&mut self, ui: &egui::Ui, cell_px: f32, three: bool) -> f64 {
        // The room left for the map, in whole cells.
        let room = ui.available_size();
        self.model.set_viewport(match three {
            true => Pos {
                x: iso::REACH,
                y: iso::REACH,
            },
            false => Pos {
                x: (room.x / cell_px) as i32,
                y: (room.y / cell_px) as i32,
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
    fn map_status(&self, origin: Pos, cursor: Pos, route: &[Pos]) -> String {
        let player = self.model.player();
        // The engine's mouse-view text when it has answered for this square; nothing for an unknown square.
        let said = self.model.hover_text(cursor).map_or_else(
            || self.model.cell(cursor).desc,
            |lines| {
                let lines = lines.iter().map(|l| l.trim()).filter(|l| !l.is_empty());
                lines.collect::<Vec<_>>().join(" ")
            },
        );
        let said = match said.trim_end_matches('.') {
            "" => String::new(),
            said => format!(": {said}"),
        };
        let mut status = format!(
            "Player at ({}, {}). Cursor ({}, {}){said}.",
            player.x + origin.x,
            player.y + origin.y,
            cursor.x + origin.x,
            cursor.y + origin.y,
        );
        if let Some(end) = route.last() {
            status += &format!(
                " Route of {} squares to ({}, {}); click it again to go.",
                route.len(),
                end.x,
                end.y
            );
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
        // Only a moving pointer moves the cursor, so it cannot undo keyboard navigation.
        let hover = response.hover_pos();
        if let Some(p) = hover.filter(|p| self.last_hover != Some(*p)) {
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
        response.clone().on_hover_ui_at_pointer(|ui| {
            if !tip {
                return;
            }
            // The engine's own mouse-view text when it has answered for this square.
            if let Some(lines) = self.model.hover_text(under) {
                for line in lines {
                    // A tooltip at the pointer gets almost no width to wrap in; one line never wraps.
                    ui.add(egui::Label::new(line).wrap_mode(egui::TextWrapMode::Extend));
                }
            } else {
                let desc = self.model.cell(under).desc;
                if !desc.is_empty() {
                    ui.add(egui::Label::new(desc).wrap_mode(egui::TextWrapMode::Extend));
                }
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
                None => response.rect.min + egui::vec2(at.x as f32, at.y as f32) * canvas.cell_px,
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
                b.set_label("Map actions");
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
                let scene = shown.0.clone();
                painter.add(iso::callback(canvas.rect, v, target, shown, stats));
                iso::paint_glyphs(painter, canvas.rect, v, &scene, canvas.cell_px);
            }
            return;
        }
        let font = egui::FontId::monospace(canvas.cell_px - 2.0);
        for y in 0..canvas.size.y {
            for x in 0..canvas.size.x {
                let cell = self.model.cell(Pos { x, y });
                let square = canvas.flat_rect(Pos { x, y });
                if tiles && self.display.paint(painter, square, &cell, season) {
                    continue;
                }
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
        // Effects stand on world squares: placed by the origin of this frame, not of when they arrived.
        let origin = self.model.origin();
        let tiles = self.display.tiles();
        self.effects.paint(
            painter,
            now,
            canvas.cell_px,
            |p| {
                canvas.rect(Pos {
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
/// screen of a 3D map drawn into `rect`.
fn ground_point(v: &Viewport, rect: egui::Rect, p: Pos, at: (f32, f32), z: f32) -> egui::Pos2 {
    let on = v.project(glam::Vec3::new(p.x as f32 + at.0, p.y as f32 + at.1, z));
    rect.min + egui::vec2(on.x, on.y)
}
