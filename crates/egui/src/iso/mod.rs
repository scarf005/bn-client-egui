//! The 3D map view: the map drawn as a scene a camera can turn and tilt, with the game's own sprites.

pub mod camera;
pub mod geometry;
pub mod gpu;
pub mod pages;
mod pick;
pub mod scene;

use crate::display::{Display, glyph_color, health_bar};
use bn_client_core::GameModel;
use bn_client_core::localise::tr;
use camera::{Camera, Viewport};
use eframe::egui;
use egui_wgpu::wgpu::TextureFormat;
use glam::{Vec2, Vec3};
use pages::Pages;
use scene::Scene;
use std::sync::Arc;

/// The squares of the map the 3D view asks the model for: at least and at most this many across and down, around
/// the avatar, as many as the view's diagonal holds.
pub const REACH: i32 = 61;
pub const MAX_REACH: i32 = 151;
/// Radians a key press turns the camera.
pub const TURN: f32 = 15.0 * std::f32::consts::PI / 180.0;
/// Radians per point of right-button drag.
const DRAG: (f32, f32) = (0.01, 0.005);

/// The 3D view's state between frames.
#[derive(Default)]
pub struct View3d {
    pub camera: Camera,
    /// The pass format of the window, known once the renderer is; none without one, and then there is no 3D view.
    pub target: Option<TextureFormat>,
    pub stats: Arc<gpu::Stats>,
    pages: Option<(u32, Arc<Pages>)>,
    scene: Option<(Key, Arc<Scene>)>,
    built: u64,
}

/// What the scene was built from.
#[derive(Clone, PartialEq, Eq)]
struct Key {
    map: Option<u64>,
    tileset: u32,
    size: (i32, i32),
    season: Option<String>,
    /// The sheets are decoded, so what is painted can be.
    decoded: bool,
    frame: Option<u32>,
}

impl View3d {
    /// The 3D view is on and can be drawn.
    pub fn active(&self, display: &Display) -> bool {
        self.target.is_some() && display.three_d()
    }

    /// Turns the camera by what the right button was dragged.
    pub fn drag(&mut self, delta: egui::Vec2) {
        self.camera.orbit(delta.x * DRAG.0, -delta.y * DRAG.1);
    }

    /// The camera looking at the avatar's square, or out of its eyes, in `size` points; `scale` is points per
    /// world unit.
    pub fn viewport(
        &self,
        avatar: bn_client_core::Pos,
        scale: f32,
        size: egui::Vec2,
        first_person: bool,
    ) -> Viewport {
        Viewport {
            camera: Camera {
                first_person,
                ..self.camera
            },
            target: Vec3::new(avatar.x as f32 + 0.5, avatar.y as f32 + 0.5, 0.0),
            scale,
            size: Vec2::new(size.x, size.y),
        }
    }

    /// The scene of what the model shows now, rebuilt only when the model's map or the tileset changed or an animated
    /// sprite in it reached another frame, and the pages it samples; none without a tileset.
    pub fn scene(
        &mut self,
        ctx: &egui::Context,
        model: &(impl GameModel + ?Sized),
        display: &Display,
        season: Option<&str>,
    ) -> Option<(Arc<Scene>, Arc<Pages>)> {
        let (tileset, epoch) = display.tileset()?;
        let pages = match &self.pages {
            Some((e, pages)) if *e == epoch => pages.clone(),
            _ => {
                let pages = Pages::start(tileset.sheets.clone(), ctx.clone());
                self.pages = Some((epoch, pages.clone()));
                pages
            }
        };
        let key = Key {
            map: model.map_version(),
            tileset: epoch,
            size: (model.size().x, model.size().y),
            season: season.map(str::to_owned),
            decoded: !pages.loading(),
            // A scene with an animated sprite is built again for each frame.
            frame: self
                .scene
                .as_ref()
                .filter(|(_, s)| s.animated)
                .and(display.frame()),
        };
        let stale = self
            .scene
            .as_ref()
            .is_none_or(|(k, _)| key.map.is_none() || *k != key);
        if stale {
            self.built += 1;
            let scene = scene::build(
                model,
                tileset,
                Some(&pages),
                season,
                self.built,
                display.frame(),
            );
            self.scene = Some((key, Arc::new(scene)));
        }
        let scene = self.scene.as_ref().map(|(_, s)| s.clone())?;
        if scene.animated {
            ctx.request_repaint_after(crate::tileset::FRAME);
        }
        Some((scene, pages))
    }
}

/// The paint callback that draws `scene` into `rect`.
pub fn callback(
    rect: egui::Rect,
    view: &Viewport,
    target: TextureFormat,
    (scene, pages): (Arc<Scene>, Arc<Pages>),
    stats: Arc<gpu::Stats>,
) -> egui::Shape {
    let light = camera::light_view_proj(scene.width as f32, scene.height as f32, scene.heights);
    egui::Shape::Callback(egui_wgpu::Callback::new_paint_callback(
        rect,
        gpu::Paint {
            camera: gpu::CameraUniform::new(view, light),
            scene,
            pages,
            size: rect.size(),
            target,
            stats,
        },
    ))
}

/// Where a way on the map points on the screen, x right and y down, as the camera's yaw turns it: up is away from
/// the eye, as the steps go (`Camera::screen_step`).
pub fn on_screen(camera: &Camera, way: Vec2) -> egui::Vec2 {
    let toward = Vec2::new(camera.yaw.cos(), camera.yaw.sin());
    egui::vec2(way.dot(Vec2::new(toward.y, -toward.x)), way.dot(toward))
}

/// The compass in the top right corner of the 3D map, its red needle to the north of the map, and a red mark at
/// the edge of the view toward each hostile creature it does not show.
pub fn paint_bearings(painter: &egui::Painter, rect: egui::Rect, view: &Viewport, scene: &Scene) {
    let radius = 18.0;
    let centre = rect.right_top() + egui::vec2(-radius - 18.0, radius + 18.0);
    let north = on_screen(&view.camera, Vec2::NEG_Y);
    let red = egui::Color32::from_rgb(220, 40, 40);
    let gray = egui::Color32::from_gray(170);
    painter.circle_stroke(centre, radius, egui::Stroke::new(1.0, gray));
    painter.line_segment(
        [centre - north * radius, centre],
        egui::Stroke::new(2.0, gray),
    );
    painter.line_segment(
        [centre, centre + north * radius],
        egui::Stroke::new(2.0, red),
    );
    painter.text(
        centre + north * (radius + 9.0),
        egui::Align2::CENTER_CENTER,
        // TRANSLATORS: the mark of north on the compass of the 3D map.
        tr("N"),
        egui::FontId::proportional(13.0),
        red,
    );
    let inner = rect.shrink(14.0);
    let eye = view.target.truncate();
    for &((x, y), floor) in &scene.hostile {
        let at = Vec3::new(x as f32 + 0.5, y as f32 + 0.5, floor + 0.5);
        let shown = view
            .project(at)
            .is_some_and(|p| inner.contains(rect.min + egui::vec2(p.x, p.y)));
        let way = on_screen(&view.camera, at.truncate() - eye).normalized();
        if shown || !way.is_finite() {
            continue;
        }
        let half = inner.size() / 2.0;
        let tip = inner.center() + way * (half.x / way.x.abs()).min(half.y / way.y.abs());
        let side = way.rot90() * 7.0;
        let back = tip - way * 14.0;
        painter.add(egui::Shape::convex_polygon(
            vec![tip, back + side, back - side],
            red,
            egui::Stroke::new(1.0, egui::Color32::BLACK),
        ));
    }
}

/// Draws the glyphs of what the tileset has no sprite for, each on its square, as the flat map draws them: the
/// background over the square's diamond and the glyph at its middle, `cell_px` points tall.
pub fn paint_glyphs(
    painter: &egui::Painter,
    rect: egui::Rect,
    view: &Viewport,
    scene: &Scene,
    cell_px: f32,
) {
    let origin = rect.min;
    let at = |x: f32, y: f32, z: f32| {
        view.project(Vec3::new(x, y, z))
            .map(|p| origin + egui::vec2(p.x, p.y))
    };
    // What has a point behind the eye is not drawn.
    let square = |x: f32, y: f32, z: f32| {
        [(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)]
            .into_iter()
            .map(|(dx, dy)| at(x + dx, y + dy, z))
            .collect::<Option<Vec<_>>>()
    };
    for mark in &scene.glyphs {
        let (x, y) = (mark.square.0 as f32, mark.square.1 as f32);
        let (Some(corners), Some(middle)) =
            (square(x, y, mark.floor), at(x + 0.5, y + 0.5, mark.floor))
        else {
            continue;
        };
        let tint = mark.filter.stand_in();
        if let Some(bg) = mark.layer.bg {
            painter.add(egui::Shape::convex_polygon(
                corners,
                glyph_color(bg, tint),
                egui::Stroke::NONE,
            ));
        }
        painter.text(
            middle,
            egui::Align2::CENTER_CENTER,
            mark.layer.glyph,
            egui::FontId::monospace(cell_px / 2.0),
            glyph_color(mark.layer.color, tint),
        );
    }
    // Under the creature, along the lowest corner of its square on the screen, a square wide.
    for hurt in &scene.hurt {
        let (x, y) = (hurt.square.0 as f32, hurt.square.1 as f32);
        let (Some(corners), Some(foot)) =
            (square(x, y, hurt.floor), at(x + 0.5, y + 0.5, hurt.floor))
        else {
            continue;
        };
        let bottom = corners.iter().map(|c| c.y).fold(foot.y, f32::max);
        let track = egui::Rect::from_min_max(
            egui::pos2(foot.x - cell_px / 2.0, bottom - 2.0),
            egui::pos2(foot.x + cell_px / 2.0, bottom),
        );
        health_bar(painter, track, hurt.health);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_compass_points_north_where_the_up_step_goes() {
        // Looking north from the south, north is up the screen; from the west, it is left.
        let from = |yaw: f32| Camera {
            yaw: yaw.to_radians(),
            ..Camera::default()
        };
        let north = on_screen(&from(90.0), Vec2::NEG_Y);
        assert!((north - egui::vec2(0.0, -1.0)).length() < 1e-5, "{north:?}");
        let north = on_screen(&from(180.0), Vec2::NEG_Y);
        assert!((north - egui::vec2(-1.0, 0.0)).length() < 1e-5, "{north:?}");
        // At every yaw the step up the screen goes the way the compass says up is.
        for yaw in (0..360).step_by(15).map(|d| d as f32) {
            let camera = from(yaw);
            let up = match camera.screen_step("UP").unwrap() {
                "UP" => Vec2::new(0.0, -1.0),
                "RIGHTUP" => Vec2::new(1.0, -1.0),
                "RIGHT" => Vec2::new(1.0, 0.0),
                "RIGHTDOWN" => Vec2::new(1.0, 1.0),
                "DOWN" => Vec2::new(0.0, 1.0),
                "LEFTDOWN" => Vec2::new(-1.0, 1.0),
                "LEFT" => Vec2::new(-1.0, 0.0),
                _ => Vec2::new(-1.0, -1.0),
            };
            let shown = on_screen(&camera, up.normalize());
            assert!(shown.y < -0.9, "yaw {yaw}: {shown:?}");
        }
    }
}
