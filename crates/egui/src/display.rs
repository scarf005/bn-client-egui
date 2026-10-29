//! How the map is drawn: the game's tileset sprites or ASCII glyphs, the player's choice kept between runs.

mod atlas;
mod draw;
mod painted;
mod settings;

pub use atlas::*;
pub use draw::*;
use painted::*;
pub use settings::*;

use crate::tileset::{self, PaintQuery, Paints, Placed, Query, Tileset, TilesetInfo, Turn};
use crate::tint::{Paint, tint_pixel};
use crate::windows::WindowState;
use bn_client_core::localise::{Value as Arg, tr, tr_format};
use bn_client_core::protocol::ModTileset;
use bn_client_core::protocol::OvermapTile;
use bn_client_core::{Cell, Glow, Health, Layer, Rgb};
use eframe::egui::{self, Color32, Mesh, Rect, Shape, TextureHandle, TextureId, TextureOptions};
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::VecDeque;
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, OnceLock};

/// The game's default `TILES` option value.
const DEFAULT_TILESET: &str = "UNDEAD_PEOPLE_BASE";
/// Each zoom step scales the map squares by this much, for at most this many steps out and in: out until the
/// squares the engine sends no longer fill the view, in until a few squares do.
const ZOOM_STEP: f32 = 1.25;
const ZOOM_OUT: i8 = 8;
const ZOOM_IN: i8 = 16;

pub struct Display {
    settings: Settings,
    /// Window places changed and not yet written.
    unsaved: bool,
    path: Option<PathBuf>,
    /// The game's folder is known, so tilesets can be looked for.
    rooted: bool,
    tilesets: Vec<TilesetInfo>,
    atlas: Option<Atlas>,
    /// Counts the tilesets opened, so what is built from one knows when it is stale.
    epoch: u32,
    /// The ids of the tints of the tileset in use.
    tint_ids: Vec<String>,
    /// Why the chosen tileset is not in use.
    problem: Option<String>,
    /// The engine's `USE_TILES` and `TILES` options, for what the player has not chosen.
    engine: Option<(bool, String)>,
    /// The mod tilesets the engine lists, once a game is loaded; until then the game's data is searched.
    mods: Option<Vec<ModTileset>>,
    /// Sprites drawn by the last map paint.
    drawn: usize,
    /// Squares of the last map paint that lie on the hidden-lighting sprite.
    hidden: usize,
    /// Squares of the last map paint drawn as a lowlight overlay instead of their terrain.
    dimmed: usize,
    /// The idle animation frame sprites are drawn at; none with reduced motion.
    frame: Option<u32>,
}

impl Display {
    /// The display for the game at `root`, with its choice read from and kept in `path`.
    pub fn new(root: Option<&Path>, path: Option<PathBuf>) -> Self {
        let settings = path
            .as_deref()
            .and_then(|p| std::fs::read(p).ok())
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        let mut display = Self {
            settings,
            unsaved: false,
            path,
            rooted: root.is_some(),
            tilesets: root.map_or(vec![], tileset::available),
            atlas: None,
            epoch: 0,
            tint_ids: vec![],
            problem: None,
            engine: None,
            mods: None,
            drawn: 0,
            hidden: 0,
            dimmed: 0,
            frame: None,
        };
        display.open();
        display
    }

    /// The tileset to use: the chosen one, else the game's default, else the first.
    fn chosen(&self) -> Option<&TilesetInfo> {
        let by_name = |name: &str| self.tilesets.iter().find(|t| t.name == name);
        let engine = self.engine.as_ref().map(|(_, name)| name.as_str());
        self.settings
            .tileset
            .as_deref()
            .and_then(by_name)
            .or_else(|| engine.and_then(by_name))
            .or_else(|| by_name(DEFAULT_TILESET))
            .or(self.tilesets.first())
    }

    /// Loads the chosen tileset when sprites are wanted.
    fn open(&mut self) {
        self.epoch += 1;
        self.atlas = None;
        self.tint_ids.clear();
        self.problem = None;
        if !self.wants_tiles() || !self.rooted {
            return;
        }
        let Some(dir) = self.chosen().map(|t| t.dir.clone()) else {
            self.problem = Some(tr("No tileset found in the game's gfx folder."));
            return;
        };
        match Tileset::load_with(&dir, self.mods.as_deref()) {
            Ok(tileset) => {
                self.tint_ids = tileset.tint_ids();
                self.atlas = Some(Atlas::start(tileset));
            }
            Err(e) => self.problem = Some(e),
        }
    }

    /// Sprites are wanted: the player's choice, else the engine's option, else yes.
    fn wants_tiles(&self) -> bool {
        self.settings
            .tiles
            .or(self.engine.as_ref().map(|(tiles, _)| *tiles))
            .unwrap_or(true)
    }

    /// Takes the engine's `USE_TILES` and `TILES` options as the default for choices not yet made.
    pub fn follow_engine(&mut self, options: Option<(bool, String)>) {
        if self.engine != options {
            self.engine = options;
            self.open();
        }
    }

    /// Takes the mod tilesets the engine lists, which a game's active mods may add to.
    pub fn follow_mods(&mut self, listed: &[ModTileset]) {
        if !listed.is_empty() && self.mods.as_deref() != Some(listed) {
            self.mods = Some(listed.to_vec());
            self.open();
        }
    }

    fn save(&self) {
        let Some(path) = &self.path else {
            return;
        };
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(text) = serde_json::to_vec_pretty(&self.settings) {
            let _ = std::fs::write(path, text);
        }
    }

    /// Sprites are being drawn, not glyphs.
    pub fn tiles(&self) -> bool {
        self.atlas.is_some()
    }

    /// The idle animation frame sprites are drawn at; none with reduced motion.
    pub fn frame(&self) -> Option<u32> {
        self.frame
    }

    /// The tileset in use, and a number that changes whenever another one is opened.
    pub fn tileset(&self) -> Option<(&Tileset, u32)> {
        Some((&self.atlas.as_ref()?.tileset, self.epoch))
    }

    /// The map is drawn as a 3D scene: asked for, or by default when the tileset is isometric, and sprites to
    /// draw it with.
    pub fn three_d(&self) -> bool {
        self.atlas
            .as_ref()
            .is_some_and(|a| self.settings.three_d.unwrap_or(a.tileset.iso))
    }

    /// The 3D map is seen through the avatar's eyes.
    pub fn first_person(&self) -> bool {
        self.three_d() && self.settings.first_person
    }

    /// The texture of the plain sprite numbered `sprite` and its rectangle there; none while its sheet loads.
    pub fn sprite_texture(&self, sprite: u32) -> Option<(TextureId, Rect)> {
        let source = self.atlas.as_ref()?.source(sprite, Fx::Plain)?;
        Some((source.texture, source.uv))
    }

    /// The ids of the tints the tileset in use has; none without one.
    pub fn tint_ids(&self) -> &[String] {
        &self.tint_ids
    }

    /// The player asked for no map animation.
    pub fn reduced_motion(&self) -> bool {
        self.settings.reduced_motion
    }

    /// The choice kept between runs.
    pub fn settings(&self) -> &Settings {
        &self.settings
    }

    /// Sprites drawn by the last map paint.
    pub fn drawn(&self) -> usize {
        self.drawn
    }

    /// Squares of the last map paint drawn on the tileset's hidden-lighting sprite: unknown or remembered.
    pub fn hidden(&self) -> usize {
        self.hidden
    }

    /// Squares of the last map paint shown as a lowlight overlay: dark or only known to be bright.
    pub fn dimmed(&self) -> usize {
        self.dimmed
    }

    /// Whether every sheet of the tileset is ready to draw.
    pub fn ready(&self) -> bool {
        self.atlas
            .as_ref()
            .and_then(|a| a.sheets[Fx::Plain.index()].as_ref())
            .is_some_and(|s| s.loading == 0)
    }

    /// Some requested sheet is still being decoded or uploaded.
    pub fn loading(&self) -> bool {
        self.atlas
            .as_ref()
            .is_some_and(|a| a.sheets.iter().flatten().any(|s| s.loading > 0))
    }

    /// The size of the whole interface as a factor of its natural size; 100 for one the settings file holds that is
    /// none of `SCALES`.
    pub fn interface_scale(&self) -> u16 {
        let scale = self.settings.interface_scale.filter(|s| SCALES.contains(s));
        scale.unwrap_or(100)
    }

    /// The side of a map square in points: a tile of the tileset, or the glyph square.
    pub fn cell(&self, glyph: f32) -> f32 {
        self.atlas.as_ref().map_or(glyph, |a| {
            (a.tileset.tile.0 as f32 * a.tileset.pixelscale).clamp(glyph, 3.0 * glyph)
        })
    }

    /// The factor the map squares are scaled by.
    pub fn zoom(&self) -> f32 {
        ZOOM_STEP.powi(self.settings.zoom.into())
    }

    /// Zooms the map by `steps` (positive is closer), within bounds, and keeps it.
    pub fn zoom_by(&mut self, steps: i8) {
        let zoom = self.settings.zoom.saturating_add(steps);
        let zoom = zoom.clamp(-ZOOM_OUT, ZOOM_IN);
        if zoom != self.settings.zoom {
            self.settings.zoom = zoom;
            self.save();
        }
    }

    /// Where the player left windows of this kind.
    pub fn window(&self, kind: &str) -> Option<WindowState> {
        self.settings.windows.get(kind).copied()
    }

    /// Remembers where windows of this kind are; written to the file once nothing is being dragged.
    pub fn keep_window(&mut self, kind: &str, state: WindowState, settled: bool) {
        if self.window(kind) != Some(state) {
            self.settings.windows.insert(kind.into(), state);
            self.unsaved = true;
        }
        if settled && std::mem::take(&mut self.unsaved) {
            self.save();
        }
    }

    /// Uploads a sheet that finished decoding; a finished thread wakes the UI, so nothing polls.
    pub fn update(&mut self, ctx: &egui::Context) {
        profiling::scope!("Display::update");
        let frame = ctx.input(|i| i.time) / tileset::FRAME.as_secs_f64();
        self.frame = (!self.settings.reduced_motion).then_some(frame as u32);
        if let Some(atlas) = &mut self.atlas
            && atlas.upload(ctx)
        {
            ctx.request_repaint();
        }
    }

    /// Starts a paint of the map: forgets the count of sprites drawn.
    pub fn begin(&mut self) {
        self.drawn = 0;
        self.hidden = 0;
        self.dimmed = 0;
    }
}
