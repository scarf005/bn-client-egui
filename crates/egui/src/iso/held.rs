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

/// The mesh of `held` in the hand of the eye that sees `view`.
pub fn mesh(_held: &Held, _view: egui::Rect) -> egui::Mesh {
    egui::Mesh::default()
}

#[cfg(test)]
mod tests {
    use super::*;

    const VIEW: egui::Rect =
        egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1600.0, 900.0));

    /// The bounds of what `opaque` says of a sixteen-pixel sprite, drawn in the hand.
    fn drawn(opaque: impl Fn(u32, u32) -> bool) -> (egui::Mesh, egui::Rect) {
        let outline = Outline::of([16, 16], opaque);
        let held = Held {
            texture: egui::TextureId::default(),
            uv: egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
            size: Vec2::splat(16.0),
            outline: &outline,
        };
        let mesh = mesh(&held, VIEW);
        let bounds = mesh.calc_bounds();
        (mesh, bounds)
    }

    #[test]
    fn a_held_item_is_large_and_low_on_the_right() {
        let (_, bounds) = drawn(|_, _| true);
        assert!(bounds.height() > 0.35 * VIEW.height(), "{bounds:?}");
        assert!(bounds.center().x > VIEW.center().x, "{bounds:?}");
        assert!(
            bounds.max.y > VIEW.center().y + 0.3 * VIEW.height(),
            "{bounds:?}"
        );
    }

    #[test]
    fn a_held_item_shows_its_thickness_in_darker_sides() {
        let (mesh, _) = drawn(|x, y| x + y == 15 || x + y == 16);
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
        let (_, grip) = drawn(|x, y| x < 2 && y > 13);
        let (_, tip) = drawn(|x, y| x > 13 && y < 2);
        assert!(tip.center().y < grip.center().y, "{tip:?} {grip:?}");
        assert!(tip.center().x < grip.center().x, "{tip:?} {grip:?}");
        // Farther from the eye, so smaller.
        assert!(tip.area() < grip.area(), "{tip:?} {grip:?}");
    }
}
