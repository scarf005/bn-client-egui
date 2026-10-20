//! The 3D map view's camera: an orthographic orbit around the avatar, in world units of one map square.
//! World axes follow the map: x east, y south, z up (a left-handed frame).

use glam::camera::lh::{proj::directx, view};
use glam::{Mat4, Vec2, Vec3, Vec4};

pub const PITCH_MIN: f32 = 20.0 * std::f32::consts::PI / 180.0;
pub const PITCH_MAX: f32 = 75.0 * std::f32::consts::PI / 180.0;
/// The true isometric tilt, `atan(1 / sqrt(2))`.
pub const PITCH_ISO: f32 = 0.615_479_7;
/// The yaw the game draws an isometric tileset from: the eye south-west of the target.
pub const YAW_ISO: f32 = 3.0 * std::f32::consts::FRAC_PI_4;
/// How far the eye sits from the target; orthographic, so it only has to clear the scene.
const EYE_DISTANCE: f32 = 200.0;
/// How high the eye of the first-person view sits over the avatar's floor, in squares of a metre.
pub const EYE_HEIGHT: f32 = 1.6;
/// The vertical field of view of the first-person view.
const FOV: f32 = 70.0 * std::f32::consts::PI / 180.0;
/// The part of the orbit's tilt the first-person eye looks down by, so tilting the camera tilts the look too.
const LOOK_DOWN: f32 = 0.3;
/// Nearer than this the first-person view draws nothing: the avatar's own sprite stands around the eye.
const NEAR: f32 = 0.3;

/// The way toward the light the map's shadows are cast from: high in the north-west, so that shadows fall to the
/// south-east whatever the time, as Factorio draws them.
pub const LIGHT: Vec3 = Vec3::new(-0.6, -0.4, 1.0);

/// The view of the light over a scene of `width` by `height` squares whose sprites lie between the heights `low`
/// and `high`: an orthographic projection that holds all of it, depth 0 nearest the light.
pub fn light_view_proj(width: f32, height: f32, (low, high): (f32, f32)) -> Mat4 {
    let middle = Vec3::new(width / 2.0, height / 2.0, (low + high) / 2.0);
    let look = view::look_at_mat4(middle + LIGHT.normalize() * EYE_DISTANCE, middle, Vec3::Z);
    let corners = [0.0, width]
        .into_iter()
        .flat_map(|x| [0.0, height].map(|y| (x, y)))
        .flat_map(|(x, y)| [low, high].map(|z| look.transform_point3(Vec3::new(x, y, z))));
    let (min, max) = corners.fold((Vec3::MAX, Vec3::MIN), |(min, max), p| {
        (min.min(p), max.max(p))
    });
    directx::orthographic(min.x, max.x, min.y, max.y, min.z, max.z) * look
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Camera {
    /// Direction of the eye around the target, counter-clockwise from east on the map: 135 degrees looks from
    /// the south-west, as the game draws an isometric tileset.
    pub yaw: f32,
    /// Elevation of the eye above the ground, within `PITCH_MIN..=PITCH_MAX`.
    pub pitch: f32,
    /// The eye is the avatar's: a perspective view from its square, looking away from where the orbit's eye
    /// would be.
    pub first_person: bool,
}

impl Default for Camera {
    fn default() -> Self {
        Self {
            yaw: YAW_ISO,
            pitch: PITCH_ISO,
            first_person: false,
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
        if self.camera.first_person {
            let eye = self.target + Vec3::Z * EYE_HEIGHT;
            let (sin, cos) = (self.camera.pitch * LOOK_DOWN).sin_cos();
            let ahead = -self.toward() * cos - Vec3::Z * sin;
            let look = view::look_at_mat4(eye, eye + ahead, Vec3::Z);
            let aspect = self.size.x / self.size.y.max(1.0);
            return directx::perspective(FOV, aspect, NEAR, 2.0 * EYE_DISTANCE) * look;
        }
        let (sin, cos) = self.camera.pitch.sin_cos();
        let eye = self.target + (self.toward() * cos + Vec3::Z * sin) * EYE_DISTANCE;
        let look = view::look_at_mat4(eye, self.target, Vec3::Z);
        let half = self.size / (2.0 * self.scale);
        let proj = directx::orthographic(-half.x, half.x, -half.y, half.y, 0.0, 2.0 * EYE_DISTANCE);
        proj * look
    }

    /// Where a world point lies in the view, in points from its top left corner; none behind the eye.
    pub fn project(&self, world: Vec3) -> Option<Vec2> {
        let clip = self.view_proj() * world.extend(1.0);
        (clip.w > f32::EPSILON).then(|| {
            let ndc = clip.truncate() / clip.w;
            Vec2::new((ndc.x + 1.0) * 0.5, (1.0 - ndc.y) * 0.5) * self.size
        })
    }

    /// The ray under the view point `at` (points from the top left corner): its start at the near plane, and the
    /// way from there to the far plane.
    pub fn ray(&self, at: Vec2) -> (Vec3, Vec3) {
        let inverse = self.view_proj().inverse();
        let ndc = Vec2::new(
            at.x / self.size.x * 2.0 - 1.0,
            1.0 - at.y / self.size.y * 2.0,
        );
        let unproject = |depth: f32| {
            let p = inverse * Vec4::new(ndc.x, ndc.y, depth, 1.0);
            p.truncate() / p.w
        };
        let near = unproject(0.0);
        (near, unproject(1.0) - near)
    }

    /// The point of the plane at height `z` under the view point `at`.
    pub fn pick(&self, at: Vec2, z: f32) -> Option<Vec2> {
        let (near, along) = self.ray(at);
        (along.z.abs() > f32::EPSILON).then(|| {
            let hit = near + along * ((z - near.z) / along.z);
            Vec2::new(hit.x, hit.y)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    impl Viewport {
        fn project_seen(&self, world: Vec3) -> Vec2 {
            self.project(world).expect("in front of the eye")
        }
    }

    fn view(yaw_deg: f32, pitch_deg: f32) -> Viewport {
        Viewport {
            camera: Camera {
                yaw: yaw_deg.to_radians(),
                pitch: pitch_deg.to_radians(),
                first_person: false,
            },
            target: Vec3::new(10.5, 7.5, 0.0),
            scale: 34.0,
            size: Vec2::new(1280.0, 720.0),
        }
    }

    #[test]
    fn the_default_camera_is_the_true_isometric_view() {
        let camera = Camera::default();
        assert!((camera.yaw.to_degrees() - 135.0).abs() < 1e-4);
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
            let at = v.project_seen(v.target);
            assert!((at - v.size / 2.0).length() < 1e-2, "{yaw} {pitch}: {at}");
        }
    }

    #[test]
    fn at_the_default_yaw_east_is_up_right_and_south_is_down_right_as_the_game_draws_it() {
        let v = view(135.0, 35.264);
        let origin = v.project_seen(v.target);
        let east = v.project_seen(v.target + Vec3::X) - origin;
        let south = v.project_seen(v.target + Vec3::Y) - origin;
        assert!(east.x > 0.0 && east.y < 0.0, "{east}");
        assert!(south.x > 0.0 && south.y > 0.0, "{south}");
        // A tile is a 2:1-ish diamond: width is wider than height by 1/sin(pitch).
        assert!(
            (east.x / east.y.abs() - 1.0 / 0.5774).abs() < 1e-2 * 2.0,
            "{east}"
        );
        assert!(
            v.project_seen(v.target + Vec3::Z).y < origin.y,
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
                    let hit = v.pick(v.project_seen(centre), 0.0).unwrap();
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
        let hit = v.pick(v.project_seen(on_wall_top), 3.0).unwrap();
        assert!((hit - Vec2::new(11.5, 7.5)).length() < 1e-3);
        // The same screen point on the ground is a square further back.
        let ground = v.pick(v.project_seen(on_wall_top), 0.0).unwrap();
        assert_ne!(ground.floor(), hit.floor());
    }

    #[test]
    fn right_points_right_on_the_screen_at_any_yaw() {
        for yaw in [0.0, 45.0, 90.0, 180.0, 270.0] {
            let v = view(yaw, 35.264);
            let there = v.project_seen(v.target + v.right());
            assert!(there.x > v.size.x / 2.0, "yaw {yaw}");
            assert!((there.y - v.size.y / 2.0).abs() < 1e-2, "yaw {yaw}");
        }
    }

    #[test]
    fn the_first_person_eye_sees_ahead_of_the_avatar_and_not_behind_it() {
        let mut v = view(135.0, 35.264);
        v.camera.first_person = true;
        // The orbit's eye is south-west of the target, so the avatar looks north-east.
        let ahead = v.target + Vec3::new(3.0, -3.0, 0.0);
        let behind = v.target + Vec3::new(-3.0, 3.0, 0.0);
        let clip = |p: Vec3| v.view_proj() * p.extend(1.0);
        let seen = clip(ahead);
        assert!(seen.w > 0.0 && (seen.x / seen.w).abs() < 1.0 && (seen.y / seen.w).abs() < 1.0);
        assert!(clip(behind).w < 0.0, "behind the eye");
        // Farther along the same line lies nearer the horizon: a perspective, not an orthographic, view.
        let far = clip(v.target + Vec3::new(9.0, -9.0, 0.0));
        assert!(far.y / far.w > seen.y / seen.w);
        // The ground under the avatar's feet is nearer than the near plane and outside the view.
        let feet = clip(v.target);
        assert!(!(feet.w > 0.0 && (feet.y / feet.w).abs() <= 1.0 && feet.z / feet.w >= 0.0));
    }

    #[test]
    fn a_square_the_first_person_eye_sees_is_drawn_where_a_click_picks_it_and_one_behind_is_not_drawn()
     {
        let mut v = view(135.0, 35.264);
        v.camera.first_person = true;
        for (dx, dy) in [(3.0, -3.0), (6.0, -2.0), (2.0, -7.0)] {
            let square = v.target + Vec3::new(dx, dy, 0.0);
            let hit = v.pick(v.project_seen(square), 0.0).unwrap();
            assert!(
                (hit - square.truncate()).length() < 1e-2,
                "{dx} {dy}: {hit}"
            );
        }
        assert_eq!(v.project(v.target + Vec3::new(-3.0, 3.0, 0.0)), None);
    }
}
