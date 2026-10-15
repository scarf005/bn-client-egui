//! The 3D map view's camera: an orthographic orbit around the avatar, in world units of one map square.
//! World axes follow the map: x east, y south, z up (a left-handed frame).

use glam::camera::lh::{proj::directx, view};
use glam::{Mat4, Vec2, Vec3, Vec4};

pub const PITCH_MIN: f32 = 20.0 * std::f32::consts::PI / 180.0;
pub const PITCH_MAX: f32 = 75.0 * std::f32::consts::PI / 180.0;
/// The true isometric tilt, `atan(1 / sqrt(2))`.
pub const PITCH_ISO: f32 = 0.615_479_7;
pub const YAW_ISO: f32 = std::f32::consts::FRAC_PI_4;
/// How far the eye sits from the target; orthographic, so it only has to clear the scene.
const REACH: f32 = 200.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Camera {
    /// Direction of the eye around the target, counter-clockwise from east on the map: 45 degrees looks from
    /// the south-east, as an isometric tileset is drawn.
    pub yaw: f32,
    /// Elevation of the eye above the ground, within `PITCH_MIN..=PITCH_MAX`.
    pub pitch: f32,
}

impl Default for Camera {
    fn default() -> Self {
        Self {
            yaw: YAW_ISO,
            pitch: PITCH_ISO,
        }
    }
}

impl Camera {
    /// Turns the camera by `yaw` around the target and tilts it by `pitch`, radians.
    pub fn orbit(&mut self, yaw: f32, pitch: f32) {
        self.yaw = (self.yaw + yaw).rem_euclid(std::f32::consts::TAU);
        self.pitch = (self.pitch + pitch).clamp(PITCH_MIN, PITCH_MAX);
    }
}

/// A camera looking at `target` into a view of `size` points, `scale` points per world unit.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Viewport {
    pub camera: Camera,
    pub target: Vec3,
    pub scale: f32,
    pub size: Vec2,
}

impl Viewport {
    /// The horizontal unit vector from the target toward the eye.
    pub fn toward(&self) -> Vec3 {
        Vec3::new(self.camera.yaw.cos(), self.camera.yaw.sin(), 0.0)
    }

    /// The horizontal unit vector that points right on the screen.
    pub fn right(&self) -> Vec3 {
        let toward = self.toward();
        Vec3::new(toward.y, -toward.x, 0.0)
    }

    pub fn view_proj(&self) -> Mat4 {
        let (sin, cos) = self.camera.pitch.sin_cos();
        let eye = self.target + (self.toward() * cos + Vec3::Z * sin) * REACH;
        let look = view::look_at_mat4(eye, self.target, Vec3::Z);
        let half = self.size / (2.0 * self.scale);
        let proj = directx::orthographic(-half.x, half.x, -half.y, half.y, 0.0, 2.0 * REACH);
        proj * look
    }

    /// Where a world point lies in the view, in points from its top left corner.
    pub fn project(&self, world: Vec3) -> Vec2 {
        let clip = self.view_proj() * world.extend(1.0);
        Vec2::new((clip.x + 1.0) * 0.5, (1.0 - clip.y) * 0.5) * self.size
    }

    /// The point of the plane at height `z` under the view point `at` (points from the top left corner).
    pub fn pick(&self, at: Vec2, z: f32) -> Option<Vec2> {
        let inverse = self.view_proj().inverse();
        let ndc = Vec2::new(
            at.x / self.size.x * 2.0 - 1.0,
            1.0 - at.y / self.size.y * 2.0,
        );
        let unproject = |depth: f32| {
            let p = inverse * Vec4::new(ndc.x, ndc.y, depth, 1.0);
            p.truncate() / p.w
        };
        let (near, far) = (unproject(0.0), unproject(1.0));
        let along = far - near;
        (along.z.abs() > f32::EPSILON).then(|| {
            let hit = near + along * ((z - near.z) / along.z);
            Vec2::new(hit.x, hit.y)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn view(yaw_deg: f32, pitch_deg: f32) -> Viewport {
        Viewport {
            camera: Camera {
                yaw: yaw_deg.to_radians(),
                pitch: pitch_deg.to_radians(),
            },
            target: Vec3::new(10.5, 7.5, 0.0),
            scale: 34.0,
            size: Vec2::new(1280.0, 720.0),
        }
    }

    #[test]
    fn the_default_camera_is_the_true_isometric_view() {
        let camera = Camera::default();
        assert!((camera.yaw.to_degrees() - 45.0).abs() < 1e-4);
        assert!((camera.pitch.to_degrees() - 35.264).abs() < 1e-3);
    }

    #[test]
    fn orbiting_clamps_the_pitch_and_wraps_the_yaw() {
        let mut camera = Camera::default();
        camera.orbit(std::f32::consts::TAU + 0.1, 5.0);
        assert_eq!(camera.pitch, PITCH_MAX);
        assert!((camera.yaw - (YAW_ISO + 0.1)).abs() < 1e-4);
        camera.orbit(-9.0, -5.0);
        assert_eq!(camera.pitch, PITCH_MIN);
        assert!((0.0..std::f32::consts::TAU).contains(&camera.yaw));
    }

    #[test]
    fn the_target_is_the_middle_of_the_view() {
        for (yaw, pitch) in [(45.0, 35.264), (135.0, 35.264), (300.0, 60.0), (10.0, 20.0)] {
            let v = view(yaw, pitch);
            let at = v.project(v.target);
            assert!((at - v.size / 2.0).length() < 1e-2, "{yaw} {pitch}: {at}");
        }
    }

    #[test]
    fn at_the_default_yaw_east_is_down_right_and_south_is_down_left() {
        let v = view(45.0, 35.264);
        let origin = v.project(v.target);
        let east = v.project(v.target + Vec3::X) - origin;
        let south = v.project(v.target + Vec3::Y) - origin;
        assert!(east.x > 0.0 && east.y > 0.0, "{east}");
        assert!(south.x < 0.0 && south.y > 0.0, "{south}");
        // A tile is a 2:1-ish diamond: width is wider than height by 1/sin(pitch).
        assert!(
            (east.x / east.y - 1.0 / 0.5774).abs() < 1e-2 * 2.0,
            "{east}"
        );
        assert!(
            v.project(v.target + Vec3::Z).y < origin.y,
            "up is up the screen"
        );
    }

    #[test]
    fn a_click_on_a_projected_square_picks_that_square_at_any_yaw_and_pitch() {
        for yaw in [0.0, 45.0, 90.0, 135.0, 200.0, 315.0] {
            for pitch in [20.0, 35.264, 60.0, 75.0] {
                let v = view(yaw, pitch);
                for (x, y) in [(10, 7), (12, 5), (7, 11), (14, 9)] {
                    let centre = Vec3::new(x as f32 + 0.5, y as f32 + 0.5, 0.0);
                    let hit = v.pick(v.project(centre), 0.0).unwrap();
                    assert_eq!(
                        (hit.x.floor() as i32, hit.y.floor() as i32),
                        (x, y),
                        "yaw {yaw} pitch {pitch}"
                    );
                }
            }
        }
    }

    #[test]
    fn the_middle_of_the_screen_picks_the_target_square() {
        let v = view(135.0, 45.0);
        let hit = v.pick(v.size / 2.0, 0.0).unwrap();
        assert!((hit - Vec2::new(10.5, 7.5)).length() < 1e-3);
    }

    #[test]
    fn a_pick_on_a_raised_plane_lands_where_that_plane_is_drawn() {
        let v = view(45.0, 35.264);
        let on_wall_top = Vec3::new(11.5, 7.5, 3.0);
        let hit = v.pick(v.project(on_wall_top), 3.0).unwrap();
        assert!((hit - Vec2::new(11.5, 7.5)).length() < 1e-3);
        // The same screen point on the ground is a square further back.
        let ground = v.pick(v.project(on_wall_top), 0.0).unwrap();
        assert_ne!(ground.floor(), hit.floor());
    }

    #[test]
    fn right_points_right_on_the_screen_at_any_yaw() {
        for yaw in [0.0, 45.0, 90.0, 180.0, 270.0] {
            let v = view(yaw, 35.264);
            let there = v.project(v.target + v.right());
            assert!(there.x > v.size.x / 2.0, "yaw {yaw}");
            assert!((there.y - v.size.y / 2.0).abs() < 1e-2, "yaw {yaw}");
        }
    }
}
