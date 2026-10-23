//! The item the avatar holds, drawn over the first-person view as a block game draws the item in the hand: its
//! sprite's opaque pixels pushed out a pixel of a sixteen-pixel item thick, low on the right, its far corner reaching
//! up and ahead toward the middle of the view.

use super::geometry::Outline;
use eframe::egui;
use glam::{EulerRot, Mat4, Quat, Vec2, Vec3};

/// A sprite of the tileset as the flat map paints it, with the outline of its opaque pixels.
pub struct Held<'a> {
    pub texture: egui::TextureId,
    /// The sprite's rectangle in its texture.
    pub uv: egui::Rect,
    /// The sprite's size in its sheet's pixels.
    pub size: Vec2,
    pub outline: &'a Outline,
}

/// The vertical field of view a block game draws the hand with.
const FOV: f32 = 70.0 * std::f32::consts::PI / 180.0;
/// The shape of the screen the hand is laid out for: as wide as a wide screen's 16 by 9.
const WIDE: f32 = 16.0 / 9.0;
/// A pixel of a sixteen-pixel item: how thick the item is, of the square it fills.
const THICK: f32 = 1.0 / 16.0;
/// The way the faces are lit from: above, ahead and a little left of the eye.
const LIGHT: Vec3 = Vec3::new(-0.3, 0.8, 0.5);

/// From the item's unit square, x right, y up and its thickness along z, to the eye's frame, x right, y up and z
/// toward the eye: a block game's right hand (ItemInHandRenderer's arm offset, then the handheld model's
/// first-person transform: turned 25 degrees in its plane and a quarter about the vertical so its far corner leads,
/// 0.68 times as large).
fn hand() -> Mat4 {
    let turn = Quat::from_euler(EulerRot::XYZ, 0.0, 90f32.to_radians(), 25f32.to_radians());
    Mat4::from_translation(Vec3::new(0.56, -0.52, -0.72))
        * Mat4::from_scale_rotation_translation(
            Vec3::splat(0.68),
            turn,
            Vec3::new(1.13, 3.2, 1.13) / 16.0,
        )
        * Mat4::from_translation(Vec3::splat(-0.5))
}

/// One face of the item: its corners in the eye's frame, where each samples the texture, and how lit it is.
struct Face {
    corners: [Vec3; 4],
    uv: [egui::Pos2; 4],
    shade: f32,
}

/// The mesh of `held` in the hand of the eye that sees `view`.
pub fn mesh(held: &Held, view: egui::Rect) -> egui::Mesh {
    let points = held.outline.edges.iter().flat_map(|e| [e.from, e.to]);
    let (lo, hi) = points.fold((Vec2::MAX, Vec2::MIN), |(lo, hi), p| (lo.min(p), hi.max(p)));
    if lo.cmpgt(hi).any() {
        return egui::Mesh::with_texture(held.texture);
    }
    // What the sprite draws fills the hand's square, in the middle of its longer side.
    let side = (hi - lo).max_element();
    let corner = (lo + hi - Vec2::splat(side)) / 2.0;
    let hand = hand();
    let at = |p: Vec2, z: f32| {
        let q = (p - corner) / side;
        hand.transform_point3(Vec3::new(q.x, 1.0 - q.y, 0.5 + z))
    };
    let uv = |p: Vec2| {
        let q = p / held.size;
        held.uv.min + egui::vec2(q.x, q.y) * held.uv.size()
    };
    let turned = |n: Vec3| hand.transform_vector3(n).normalize();
    // The two pictures, a hair inside what they draw so no neighbour in the sheet shows.
    let inset = Vec2::splat(0.01);
    let (a, b) = (lo + inset, hi - inset);
    let picture = [
        Vec2::new(a.x, a.y),
        Vec2::new(b.x, a.y),
        Vec2::new(b.x, b.y),
        Vec2::new(a.x, b.y),
    ];
    let pictures = [(THICK / 2.0, Vec3::Z), (-THICK / 2.0, -Vec3::Z)]
        .map(|(z, n)| (picture.map(|p| at(p, z)), picture.map(uv), turned(n)));
    // Each edge of the outline, the pixels just inside it drawn across the thickness.
    let sides = held.outline.edges.iter().map(|e| {
        let inside = |p: Vec2| uv(p + e.inward / 2.0);
        let run = (e.to - e.from).normalize_or_zero() * 0.01;
        let (from, to) = (e.from + run, e.to - run);
        let (z0, z1) = (THICK / 2.0, -THICK / 2.0);
        (
            [at(e.from, z0), at(e.to, z0), at(e.to, z1), at(e.from, z1)],
            [inside(from), inside(to), inside(to), inside(from)],
            turned(Vec3::new(-e.inward.x, e.inward.y, 0.0)),
        )
    });
    let light = LIGHT.normalize();
    let mut faces = pictures
        .into_iter()
        .chain(sides)
        // Only what faces the eye, which sits at the origin.
        .filter(|(c, _, n)| n.dot(-(c[0] + c[2]) / 2.0) > 0.0)
        .map(|(corners, uv, n)| Face {
            corners,
            uv,
            shade: 0.6 + 0.4 * n.dot(light).max(0.0),
        })
        .collect::<Vec<_>>();
    // The farthest first, so the nearer faces cover them.
    let depth = |f: &Face| f.corners.iter().map(|c| c.z).sum::<f32>();
    faces.sort_by(|a, b| depth(a).total_cmp(&depth(b)));
    // The hand of a wide screen; in a narrower view, that screen as wide as the view in its bottom right corner.
    let height = view.height().min(view.width() / WIDE);
    let middle = view.right_bottom() - egui::vec2(height * WIDE, height) / 2.0;
    let focal = height / 2.0 / (FOV / 2.0).tan();
    let screen = |p: Vec3| middle + egui::vec2(p.x, -p.y) * focal / -p.z;
    let mut mesh = egui::Mesh::with_texture(held.texture);
    for face in faces {
        let gray = (255.0 * face.shade).round() as u8;
        let color = egui::Color32::from_rgb(gray, gray, gray);
        let first = mesh.vertices.len() as u32;
        for (p, uv) in face.corners.into_iter().zip(face.uv) {
            mesh.vertices.push(egui::epaint::Vertex {
                pos: screen(p),
                uv,
                color,
            });
        }
        mesh.indices.extend([0, 1, 2, 0, 2, 3].map(|i| first + i));
    }
    mesh
}

#[cfg(test)]
mod tests {
    use super::*;

    const VIEW: egui::Rect =
        egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1600.0, 900.0));

    /// What `opaque` says of a sixteen-pixel sprite, drawn in the hand of the eye that sees `view`.
    fn drawn_in(opaque: impl Fn(u32, u32) -> bool, view: egui::Rect) -> egui::Mesh {
        let outline = Outline::of([16, 16], opaque);
        let held = Held {
            texture: egui::TextureId::default(),
            uv: egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
            size: Vec2::splat(16.0),
            outline: &outline,
        };
        mesh(&held, view)
    }

    fn drawn(opaque: impl Fn(u32, u32) -> bool) -> egui::Mesh {
        drawn_in(opaque, VIEW)
    }

    /// Where the picture's corner `uv` is drawn on the face toward the eye, the last drawn.
    fn corner(mesh: &egui::Mesh, uv: egui::Pos2) -> egui::Pos2 {
        let near = |v: &&egui::epaint::Vertex| v.uv.distance(uv) < 0.01;
        mesh.vertices
            .iter()
            .rev()
            .find(near)
            .map(|v| v.pos)
            .unwrap()
    }

    #[test]
    fn a_held_item_is_large_and_low_on_the_right() {
        let bounds = drawn(|_, _| true).calc_bounds();
        assert!(bounds.height() > 0.35 * VIEW.height(), "{bounds:?}");
        assert!(bounds.center().x > VIEW.center().x, "{bounds:?}");
        assert!(
            bounds.max.y > VIEW.center().y + 0.3 * VIEW.height(),
            "{bounds:?}"
        );
    }

    #[test]
    fn a_held_item_in_a_narrower_view_is_laid_out_as_in_the_corner_of_a_wide_screen() {
        let narrow = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(900.0, 800.0));
        let tip = |view| corner(&drawn_in(|_, _| true, view), egui::pos2(1.0, 0.0));
        // The wide screen is as wide as the narrow view and stands in its bottom right corner.
        let shrink = narrow.width() / VIEW.width();
        let expected = narrow.right_bottom() - (VIEW.right_bottom() - tip(VIEW)) * shrink;
        assert!(
            tip(narrow).distance(expected) < 0.5,
            "{:?} {expected:?}",
            tip(narrow)
        );
    }

    #[test]
    fn the_picture_of_a_held_item_bends_with_the_perspective_of_its_squares() {
        let mesh = drawn(|_, _| true);
        let middle = corner(&mesh, egui::pos2(0.5, 0.5));
        let corners = [(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)]
            .map(|(u, v)| corner(&mesh, egui::pos2(u, v)));
        let flat = corners
            .iter()
            .fold(egui::Vec2::ZERO, |sum, c| sum + c.to_vec2())
            / 4.0;
        // Drawn flat, the middle of the picture would be the middle of its corners; seen in perspective the nearer half
        // is larger.
        assert!(middle.distance(flat.to_pos2()) > 2.0, "{middle:?} {flat:?}");
    }

    #[test]
    fn a_held_item_shows_its_thickness_in_darker_sides() {
        let mesh = drawn(|x, y| x + y == 15 || x + y == 16);
        let shades = mesh
            .vertices
            .iter()
            .map(|v| v.color.r())
            .collect::<std::collections::BTreeSet<_>>();
        // The face and at least one side, lit differently.
        assert!(shades.len() >= 2, "{shades:?}");
        assert!(mesh.indices.len() > 12, "{}", mesh.indices.len());
    }

    #[test]
    fn the_far_corner_of_a_held_item_reaches_up_and_toward_the_middle() {
        let mesh = drawn(|_, _| true);
        let tip = corner(&mesh, egui::pos2(1.0, 0.0));
        let grip = corner(&mesh, egui::pos2(0.0, 1.0));
        assert!(tip.y < grip.y, "{tip:?} {grip:?}");
        assert!(tip.x < grip.x, "{tip:?} {grip:?}");
    }
}
