//! The line of fire in the 3D and first-person views: where it stands in the world and on the screen.

use super::camera::{Camera, Viewport};
use glam::{Vec2, Vec3};

/// How high a creature's middle stands over its floor, in squares: where a shot flies.
pub const BODY: f32 = 0.5;

/// The yaw whose first-person eye looks from the square at `from` toward the one at `to`.
pub fn yaw_facing(_from: Vec2, _to: Vec2) -> f32 {
    0.0
}

/// Where the line of fire crosses a square: its middle, `levels` z-levels of `level` squares over the avatar's.
pub fn body_point(_square: Vec2, _levels: i32, _level: f32) -> Vec3 {
    Vec3::ZERO
}

/// The screen points of the world path `path`, in points from the view's top left corner, one for each of its
/// `STEPS` pieces a segment is cut into that lies before the eye. The line stays whole where its ends do not: the
/// part behind a first-person eye is cut off, not stretched across the view.
pub fn project(_view: &Viewport, _path: &[Vec3]) -> Vec<Vec2> {
    vec![]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn first_person(yaw: f32, avatar: Vec2) -> Viewport {
        Viewport {
            camera: Camera {
                yaw,
                pitch: 0.6,
                first_person: true,
            },
            target: Vec3::new(avatar.x, avatar.y, 0.0),
            scale: 34.0,
            size: Vec2::new(1280.0, 720.0),
        }
    }

    #[test]
    fn the_first_person_eye_faces_a_target_in_any_of_the_eight_ways_dead_ahead() {
        let avatar = Vec2::new(10.5, 10.5);
        for (dx, dy) in [
            (1, 0),
            (1, 1),
            (0, 1),
            (-1, 1),
            (-1, 0),
            (-1, -1),
            (0, -1),
            (1, -1),
        ] {
            let target = avatar + Vec2::new(dx as f32, dy as f32) * 3.0;
            let view = first_person(yaw_facing(avatar, target), avatar);
            let on = view
                .project(body_point(target, 0, 3.0))
                .unwrap_or_else(|| panic!("({dx}, {dy}) is behind the eye"));
            assert!((on.x - 640.0).abs() < 1.0, "({dx}, {dy}): {on:?}");
        }
    }

    #[test]
    fn a_line_of_fire_ahead_of_the_first_person_eye_runs_from_the_bottom_of_the_view_to_the_target()
    {
        let avatar = Vec2::new(10.5, 10.5);
        let target = Vec2::new(13.5, 10.5);
        let view = first_person(yaw_facing(avatar, target), avatar);
        let path = [body_point(avatar, 0, 3.0), body_point(target, 0, 3.0)];
        let line = project(&view, &path);
        assert!(line.len() > 2, "{line:?}");
        let end = view.project(path[1]).unwrap();
        assert!((line.last().unwrap().distance(end)) < 0.5, "{line:?}");
        assert!(line.first().unwrap().y > end.y + 100.0, "{line:?}");
        assert!(line.iter().all(|p| p.x.is_finite() && p.y.is_finite()));
    }

    #[test]
    fn a_line_of_fire_behind_the_first_person_eye_is_not_drawn() {
        let avatar = Vec2::new(10.5, 10.5);
        let view = first_person(yaw_facing(avatar, Vec2::new(13.5, 10.5)), avatar);
        let behind = [
            body_point(avatar, 0, 3.0),
            body_point(Vec2::new(7.5, 10.5), 0, 3.0),
        ];
        assert!(project(&view, &behind).is_empty());
    }

    #[test]
    fn the_line_of_fire_flies_at_the_middle_of_a_creature_and_a_level_up_stands_a_level_higher() {
        let view = Viewport {
            camera: Camera::default(),
            target: Vec3::new(10.5, 10.5, 0.0),
            scale: 34.0,
            size: Vec2::new(1280.0, 720.0),
        };
        let square = Vec2::new(12.5, 10.5);
        let ground = view.project(Vec3::new(square.x, square.y, 0.0)).unwrap();
        let body = project(&view, &[body_point(square, 0, 3.0)]);
        let above = project(&view, &[body_point(square, 1, 3.0)]);
        assert!(body[0].y < ground.y - 5.0, "{body:?} {ground:?}");
        assert!(above[0].y < body[0].y - 5.0, "{above:?} {body:?}");
    }
}
