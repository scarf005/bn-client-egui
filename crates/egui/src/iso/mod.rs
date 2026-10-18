//! The 3D map view: the map drawn as a scene a camera can turn and tilt, with the game's own sprites.

pub mod camera;
pub mod geometry;
pub mod gpu;
pub mod pages;
mod pick;
pub mod scene;

use crate::display::{Display, glyph_color};
use bn_client_core::GameModel;
use camera::{Camera, Viewport};
use eframe::egui;
use egui_wgpu::wgpu::TextureFormat;
use glam::{Vec2, Vec3};
use pages::Pages;
use scene::Scene;
use std::sync::Arc;

/// The squares of the map the 3D view asks the model for: this many across and down, around the avatar.
pub const REACH: i32 = 61;
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

    /// The camera looking at the avatar's square, in `size` points; `scale` is points per world unit.
    pub fn viewport(&self, avatar: bn_client_core::Pos, scale: f32, size: egui::Vec2) -> Viewport {
        Viewport {
            camera: self.camera,
            target: Vec3::new(avatar.x as f32 + 0.5, avatar.y as f32 + 0.5, 0.0),
            scale,
            size: Vec2::new(size.x, size.y),
        }
    }

    /// The scene of what the model shows now, rebuilt only when the model's map or the tileset changed, and the
    /// pages it samples; none without a tileset.
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
        };
        let stale = self
            .scene
            .as_ref()
            .is_none_or(|(k, _)| key.map.is_none() || *k != key);
        if stale {
            self.built += 1;
            let scene = scene::build(model, tileset, Some(&pages), season, self.built);
            self.scene = Some((key, Arc::new(scene)));
        }
        let scene = self.scene.as_ref().map(|(_, s)| s.clone())?;
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
        let p = view.project(Vec3::new(x, y, z));
        origin + egui::vec2(p.x, p.y)
    };
    for mark in &scene.glyphs {
        let (x, y) = (mark.square.0 as f32, mark.square.1 as f32);
        let tint = mark.filter.stand_in();
        if let Some(bg) = mark.layer.bg {
            let corners = [(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)]
                .map(|(dx, dy)| at(x + dx, y + dy, mark.floor));
            painter.add(egui::Shape::convex_polygon(
                corners.to_vec(),
                glyph_color(bg, tint),
                egui::Stroke::NONE,
            ));
        }
        painter.text(
            at(x + 0.5, y + 0.5, mark.floor),
            egui::Align2::CENTER_CENTER,
            mark.layer.glyph,
            egui::FontId::monospace(cell_px / 2.0),
            glyph_color(mark.layer.color, tint),
        );
    }
}
