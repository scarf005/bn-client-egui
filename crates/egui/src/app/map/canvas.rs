//! Where the map lies on the screen: the rectangle, diamond or projected ground of each square on the flat, the
//! isometric and the 3D map, and the marks, lines and frames drawn over them.

use super::*;

/// Where the map lies on the screen this frame.
pub(super) struct Canvas {
    /// The area the map lays out in; the flat squares may reach past the room, which cuts them.
    pub(super) rect: egui::Rect,
    /// A square's side in points.
    pub(super) cell_px: f32,
    /// The view's squares across and down.
    pub(super) size: Pos,
    /// The camera of the 3D map; none on the flat one.
    pub(super) view: Option<Viewport>,
    /// Where the squares of the flat map of an isometric tileset lie; none on a square grid.
    pub(super) diamond: Option<Diamond>,
}

impl Canvas {
    /// A square on the screen: its rectangle on the flat map, the diamond of its ground in the 3D one and the
    /// box around the diamond on the isometric flat one.
    pub(super) fn rect(&self, p: Pos) -> egui::Rect {
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
    pub(super) fn standing(&self, p: Pos) -> egui::Rect {
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
    pub(super) fn flat_rect(&self, p: Pos) -> egui::Rect {
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

    pub(super) fn fill(&self, painter: &egui::Painter, p: Pos, color: egui::Color32) {
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

    pub(super) fn outline(&self, painter: &egui::Painter, p: Pos, stroke: egui::Stroke) {
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
    pub(super) fn ground(&self, x: f32, y: f32) -> Option<egui::Pos2> {
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
    pub(super) fn fire_line(&self, squares: &[FireSquare], level: f32) -> Vec<egui::Pos2> {
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
    pub(super) fn cursor_frame(&self, square: FireSquare, level: f32) -> Vec<[egui::Pos2; 2]> {
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
    pub(super) fn mark(&self, painter: &egui::Painter, mark: &MapMark) {
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
    pub(super) fn square_at(&self, p: egui::Pos2, scene: Option<&Scene>) -> Pos {
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
pub(super) struct FireSquare {
    pub(super) at: Pos,
    pub(super) up: i32,
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
