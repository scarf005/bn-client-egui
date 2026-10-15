//! Which square of the scene is under a point of the screen: the ray of that point walked through the squares in
//! the order it meets them, stopping at the first whose top (a box's, else its ground) the ray has gone under.

use super::camera::Viewport;
use super::scene::Scene;
use glam::Vec2;

impl Scene {
    /// The view square a ray from the camera through `at` (points from the view's top left corner) meets first,
    /// among the squares the scene draws: on a box's lid or one of its faces, or on the ground of its level.
    pub fn pick(&self, view: &Viewport, at: Vec2) -> Option<(i32, i32)> {
        let (o, d) = view.ray(at);
        let (width, height) = (self.width as f32, self.height as f32);
        // The part of the ray over the grid, by slabs.
        let (mut t0, mut t1) = (0.0f32, 1.0f32);
        for (o, d, extent) in [(o.x, d.x, width), (o.y, d.y, height)] {
            if d.abs() < f32::EPSILON {
                if o < 0.0 || o >= extent {
                    return None;
                }
                continue;
            }
            let (a, b) = ((0.0 - o) / d, (extent - o) / d);
            t0 = t0.max(a.min(b));
            t1 = t1.min(a.max(b));
        }
        if t0 >= t1 {
            return None;
        }
        let at_t = |t: f32| o + d * t;
        let start = at_t(t0 + 1e-5);
        let mut cell = [
            (start.x.floor() as i32).clamp(0, self.width - 1),
            (start.y.floor() as i32).clamp(0, self.height - 1),
        ];
        let step = [d.x.signum() as i32, d.y.signum() as i32];
        // The parameter at which the ray leaves the cell along each axis, and how far apart those are.
        let leave = |c: i32, o: f32, d: f32, s: i32| match d.abs() < f32::EPSILON {
            true => f32::INFINITY,
            false => ((c + i32::from(s > 0)) as f32 - o) / d,
        };
        let mut next = [
            leave(cell[0], o.x, d.x, step[0]),
            leave(cell[1], o.y, d.y, step[1]),
        ];
        let apart = [
            1.0 / d.x.abs().max(f32::EPSILON),
            1.0 / d.y.abs().max(f32::EPSILON),
        ];
        for _ in 0..self.width + self.height + 2 {
            let out = next[0].min(next[1]).min(t1);
            if let Some(column) = self.column(cell[0], cell[1]) {
                if at_t(out).z <= column.top() {
                    return Some((cell[0], cell[1]));
                }
            }
            if out >= t1 {
                return None;
            }
            let axis = usize::from(next[1] < next[0]);
            cell[axis] += step[axis];
            next[axis] += apart[axis];
            if cell[0] < 0 || cell[1] < 0 || cell[0] >= self.width || cell[1] >= self.height {
                return None;
            }
        }
        None
    }
}
