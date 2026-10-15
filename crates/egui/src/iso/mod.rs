//! The 3D map view: the map drawn as a scene a camera can turn and tilt, with the game's own sprites.

pub mod camera;
pub mod geometry;
pub mod gpu;
pub mod pages;
mod pick;
pub mod scene;

use crate::display::Display;
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
        };
        let stale = self
            .scene
            .as_ref()
            .is_none_or(|(k, _)| key.map.is_none() || *k != key);
        if stale {
            self.built += 1;
            let scene = scene::build(model, tileset, season, self.built);
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
    egui::Shape::Callback(egui_wgpu::Callback::new_paint_callback(
        rect,
        gpu::Paint {
            scene,
            pages,
            camera: view.into(),
            size: rect.size(),
            target,
            stats,
        },
    ))
}
