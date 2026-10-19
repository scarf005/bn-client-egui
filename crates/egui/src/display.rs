//! How the map is drawn: the game's tileset sprites or ASCII glyphs, the player's choice kept between runs.

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
/// Sprite rows are uploaded in strips at most this tall, within every GPU's texture limit.
pub(crate) const STRIP: u32 = 4096;
/// Remembered squares draw darkened, squares in low light slightly less.
const MEMORY: Color32 = Color32::from_gray(100);
const SHADOW: Color32 = Color32::from_gray(170);
/// Each zoom step scales the map squares by this much, for at most this many steps either way.
const ZOOM_STEP: f32 = 1.25;
const ZOOM_STEPS: i8 = 4;

/// The choice that survives a restart.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Settings {
    /// Draw sprites (true) or ASCII glyphs (false); absent until the player chooses, so the engine's
    /// `USE_TILES` option decides.
    #[serde(default)]
    pub tiles: Option<bool>,
    /// The `TILES` name of the tileset; the game's default when absent.
    pub tileset: Option<String>,
    /// Play no shot, explosion or floating-text overlays on the map.
    #[serde(default)]
    pub reduced_motion: bool,
    /// Draw the map as a 3D scene a camera can turn, or flat; an isometric tileset is drawn in 3D unless this says
    /// otherwise.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub three_d: Option<bool>,
    /// The map zoom in steps from the tileset's own size, which the zoom keys and the wheel move.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub zoom: i8,
    /// Where each kind of window sits, how large it is and whether it is collapsed.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub windows: BTreeMap<String, WindowState>,
}

fn is_zero(n: &i8) -> bool {
    *n == 0
}

/// `$XDG_CONFIG_HOME/bn-client-egui/settings.json`, or the same under `~/.config`.
pub fn settings_path() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(base.join("bn-client-egui/settings.json"))
}

/// The game's filters for what is not plainly in sight: grayscale for low light, sepia for memory, and
/// the cyan haze laid over levels seen through open air.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Fx {
    Plain,
    Shadow,
    Memory,
    ZOverlay,
}

impl Fx {
    const ALL: [Fx; 4] = [Fx::Plain, Fx::Shadow, Fx::Memory, Fx::ZOverlay];

    fn index(self) -> usize {
        self as usize
    }

    /// The colour a plain sprite is multiplied by until the filtered sheets have arrived.
    pub(crate) fn stand_in(self) -> Color32 {
        match self {
            Fx::Plain => Color32::WHITE,
            Fx::Shadow => SHADOW,
            Fx::Memory => MEMORY,
            Fx::ZOverlay => Color32::TRANSPARENT,
        }
    }

    /// The game's `color_pixel_*` function of this filter on one pixel.
    pub fn pixel(self, [r, g, b]: [u8; 3]) -> [u8; 3] {
        let average = ((85 * (u32::from(r) + u32::from(g) + u32::from(b))) >> 8) as u8;
        match self {
            Fx::Plain => [r, g, b],
            Fx::ZOverlay => [128, 255, 255],
            _ if r == 0 && g == 0 && b == 0 => [r, g, b],
            Fx::Shadow => [((u32::from(average) * 5) >> 3).max(1) as u8; 3],
            Fx::Memory => {
                let (dark, light) = ([39u32, 23, 19], [241u32, 220, 163]);
                let level = f32::from(average) / 255.0;
                let shade = ((level.powf(1.6) * 150.0).round().min(100.0)) as u32;
                [0, 1, 2].map(|i| ((dark[i] * (100 - shade) + light[i] * shade) / 100) as u8)
            }
        }
    }
}

/// A decoded sprite sheet, cut into strips ready to become textures.
struct Strips {
    sheet: usize,
    strips: Vec<egui::ColorImage>,
}

/// Wakes the UI when a background thread has finished a sheet; known once the first frame has run.
type Wake = Arc<OnceLock<egui::Context>>;

/// The sheets of a tileset under one filter, decoded in the background and uploaded as they arrive.
struct Sheets {
    incoming: Receiver<Strips>,
    /// Decoded sheets waiting for their turn: one is uploaded per frame, so no frame stalls on a copy.
    decoded: VecDeque<Strips>,
    textures: HashMap<usize, Vec<TextureHandle>>,
    loading: usize,
}

impl Sheets {
    fn start(sheets: Vec<tileset::Sheet>, fx: Fx, wake: Wake) -> Self {
        let (send, incoming) = channel();
        let loading = sheets.len();
        std::thread::spawn(move || {
            for (n, sheet) in sheets.iter().enumerate() {
                let strips = decode(sheet, fx).unwrap_or_default();
                if send.send(Strips { sheet: n, strips }).is_err() {
                    return;
                }
                wake.get().inspect(|ctx| ctx.request_repaint());
            }
        });
        Self {
            incoming,
            decoded: VecDeque::new(),
            textures: HashMap::new(),
            loading,
        }
    }

    /// Uploads the next decoded sheet; true while more are waiting for the next frame.
    fn upload(&mut self, ctx: &egui::Context, fx: Fx) -> bool {
        self.decoded.extend(self.incoming.try_iter());
        if let Some(done) = self.decoded.pop_front() {
            self.loading -= 1;
            let textures = done
                .strips
                .into_iter()
                .enumerate()
                .map(|(i, image)| {
                    let name = format!("sheet{}.{i}.{fx:?}", done.sheet);
                    ctx.load_texture(name, image, TextureOptions::NEAREST)
                })
                .collect();
            self.textures.insert(done.sheet, textures);
        }
        !self.decoded.is_empty()
    }
}

/// Where a sprite is: its texture, its rectangle in it (0..1), the sheet's offset and the filter the
/// texture already has.
#[derive(Clone, Copy)]
struct Source {
    texture: TextureId,
    uv: Rect,
    offset: (f32, f32),
    fx: Fx,
}

/// A tileset, with its plain sheets from the start and the filtered ones once a square needs them.
struct Atlas {
    tileset: Tileset,
    sheets: [Option<Sheets>; 4],
    /// The sprites to paint, for the one thread that paints them.
    jobs: Sender<Job>,
    painted: RefCell<HashMap<PaintKey, Painted>>,
    wake: Wake,
}

/// A sprite, the paint and the filter it is made under.
type PaintKey = (u32, Option<u32>, Paint, Fx);

/// A sprite painted with a vehicle part's color, made in the background.
enum Painted {
    Pending(Receiver<Option<egui::ColorImage>>),
    /// The handle keeps the texture alive.
    Ready {
        _texture: TextureHandle,
        source: Source,
    },
    Failed,
}

impl Atlas {
    fn start(tileset: Tileset) -> Self {
        let wake = Wake::default();
        let plain = Sheets::start(tileset.sheets.clone(), Fx::Plain, wake.clone());
        let (jobs, queue) = channel();
        std::thread::spawn(move || painter(queue));
        Self {
            tileset,
            sheets: [Some(plain), None, None, None],
            jobs,
            painted: RefCell::default(),
            wake,
        }
    }

    /// The sprite painted with `paint` under a filter; none while it is being made or when it cannot be.
    fn painted(
        &self,
        ctx: &egui::Context,
        (placed, paint): (Placed, Paint),
        fx: Fx,
    ) -> Option<Source> {
        let sprite = placed.sprite;
        let (n, local) = self.tileset.locate(sprite)?;
        let mut cache = self.painted.borrow_mut();
        let key = (sprite, placed.mask, paint, fx);
        let Some(entry) = cache.get_mut(&key) else {
            let mask = placed.mask.and_then(|m| {
                let (n, local) = self.tileset.locate(m)?;
                Some((n, self.tileset.sheets[n].clone(), local))
            });
            let (done, receive) = channel();
            let _ = self.jobs.send(Job {
                sheet: (n, self.tileset.sheets[n].clone(), local),
                mask,
                paint,
                fx,
                done,
            });
            cache.insert(key, Painted::Pending(receive));
            ctx.request_repaint_after(std::time::Duration::from_millis(30));
            return None;
        };
        if let Painted::Pending(receive) = entry {
            let sheet = &self.tileset.sheets[n];
            *entry = match receive.try_recv() {
                Ok(Some(image)) => {
                    let name = format!("paint{sprite}.{paint:?}.{fx:?}");
                    let texture = ctx.load_texture(name, image, TextureOptions::NEAREST);
                    let source = Source {
                        texture: texture.id(),
                        uv: Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                        offset: (sheet.offset.0 as f32, sheet.offset.1 as f32),
                        fx,
                    };
                    Painted::Ready {
                        _texture: texture,
                        source,
                    }
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    ctx.request_repaint_after(std::time::Duration::from_millis(30));
                    return None;
                }
                _ => Painted::Failed,
            };
        }
        match entry {
            Painted::Ready { source, .. } => Some(*source),
            _ => None,
        }
    }

    /// Starts decoding the sheets of a filter the first time something is drawn with it.
    fn request(&mut self, fx: Fx) {
        if self.sheets[fx.index()].is_none() {
            self.sheets[fx.index()] = Some(Sheets::start(
                self.tileset.sheets.clone(),
                fx,
                self.wake.clone(),
            ));
        }
    }

    fn upload(&mut self, ctx: &egui::Context) -> bool {
        // Set before looking for sheets, so one finished meanwhile either is seen now or wakes the UI.
        let _ = self.wake.set(ctx.clone());
        let mut more = false;
        for fx in Fx::ALL {
            if let Some(sheets) = &mut self.sheets[fx.index()] {
                more |= sheets.upload(ctx, fx);
            }
        }
        more
    }

    /// The sprite under a filter; the plain one when the filtered sheet has not arrived.
    fn source(&self, sprite: u32, fx: Fx) -> Option<Source> {
        let (n, local) = self.tileset.locate(sprite)?;
        let sheet = &self.tileset.sheets[n];
        let rows = (STRIP / sheet.sprite.1).max(1);
        let (col, row) = (local % sheet.columns, local / sheet.columns);
        let strips = |fx: Fx| self.sheets[fx.index()].as_ref()?.textures.get(&n);
        let (strips, fx) = match strips(fx) {
            Some(found) => (found, fx),
            None => (strips(Fx::Plain)?, Fx::Plain),
        };
        let texture = strips.get((row / rows) as usize)?;
        let size = texture.size_vec2();
        let min = egui::vec2(
            (col * sheet.sprite.0) as f32,
            ((row % rows) * sheet.sprite.1) as f32,
        );
        Some(Source {
            texture: texture.id(),
            uv: Rect::from_min_size(
                (min / size).to_pos2(),
                egui::vec2(sheet.sprite.0 as f32, sheet.sprite.1 as f32) / size,
            ),
            offset: (sheet.offset.0 as f32, sheet.offset.1 as f32),
            fx,
        })
    }
}

/// A sheet's pixels with its transparency key made transparent.
pub(crate) fn load(sheet: &tileset::Sheet) -> Option<image::RgbaImage> {
    let mut image = image::open(&sheet.file).ok()?.into_rgba8();
    if let Some(key) = sheet.key {
        for px in image.pixels_mut().filter(|px| px.0[..3] == key) {
            px.0[3] = 0;
        }
    }
    Some(image)
}

/// A sprite to paint: its sheet as `(sheet number, sheet, index in the sheet)` and that of the sprite masking its
/// paint, the paint and the filter, and where the painted image goes.
struct Job {
    sheet: (usize, tileset::Sheet, u32),
    mask: Option<(usize, tileset::Sheet, u32)>,
    paint: Paint,
    fx: Fx,
    done: Sender<Option<egui::ColorImage>>,
}

/// Paints the sprites asked for, one after another, until the atlas that asks is gone.
fn painter(jobs: Receiver<Job>) {
    let mut decoded = HashMap::new();
    for job in jobs {
        let _ = job.done.send(paint_sprite(&mut decoded, &job));
    }
}

/// The decoded pixels of sheet `n`, kept for the other sprites painted from it.
fn decoded_sheet(
    decoded: &mut HashMap<usize, Arc<image::RgbaImage>>,
    n: usize,
    sheet: &tileset::Sheet,
) -> Option<Arc<image::RgbaImage>> {
    match decoded.get(&n) {
        Some(image) => Some(image.clone()),
        None => Some(decoded.entry(n).or_insert(Arc::new(load(sheet)?)).clone()),
    }
}

/// One sprite of a sheet painted with `paint`, where its mask lets the paint through, and then filtered, like
/// the game's tinted sprite lookup.
fn paint_sprite(
    decoded: &mut HashMap<usize, Arc<image::RgbaImage>>,
    Job {
        sheet: (n, sheet, local),
        mask,
        paint,
        fx,
        ..
    }: &Job,
) -> Option<egui::ColorImage> {
    let (n, local, fx, paint) = (*n, *local, *fx, *paint);
    let image = decoded_sheet(decoded, n, sheet)?;
    let mask = mask.as_ref().and_then(|&(n, ref sheet, local)| {
        let image = decoded_sheet(decoded, n, sheet)?;
        let (w, h) = sheet.sprite;
        let (col, row) = (local % sheet.columns, local / sheet.columns);
        Some(image::imageops::crop_imm(&*image, col * w, row * h, w, h).to_image())
    });
    let (w, h) = sheet.sprite;
    let (col, row) = (local % sheet.columns, local / sheet.columns);
    let mut sprite = image::imageops::crop_imm(&*image, col * w, row * h, w, h).to_image();
    for (x, y, px) in sprite
        .enumerate_pixels_mut()
        .filter(|(_, _, px)| px.0[3] > 0)
    {
        let pixel = |p: &image::Rgba<u8>| [p.0[0], p.0[1], p.0[2]];
        let mask = mask
            .as_ref()
            .and_then(|m| m.get_pixel_checked(x, y))
            .map(pixel);
        let tinted = tint_pixel(pixel(px), paint, mask);
        px.0[..3].copy_from_slice(&fx.pixel(tinted));
    }
    Some(egui::ColorImage::from_rgba_unmultiplied(
        [w as usize, h as usize],
        sprite.as_raw(),
    ))
}

/// Decodes a sheet to RGBA under a filter, makes its transparency key transparent and cuts it into
/// strips of whole sprite rows.
fn decode(sheet: &tileset::Sheet, fx: Fx) -> Option<Vec<egui::ColorImage>> {
    let mut image = load(sheet)?;
    for px in image.pixels_mut() {
        let [r, g, b] = fx.pixel([px.0[0], px.0[1], px.0[2]]);
        px.0[..3].copy_from_slice(&[r, g, b]);
    }
    let rows = (STRIP / sheet.sprite.1).max(1) * sheet.sprite.1;
    let usable = image.height() / sheet.sprite.1 * sheet.sprite.1;
    Some(
        (0..usable)
            .step_by(rows as usize)
            .map(|y| {
                let h = rows.min(usable - y);
                let strip = image::imageops::crop_imm(&image, 0, y, image.width(), h).to_image();
                egui::ColorImage::from_rgba_unmultiplied(
                    [strip.width() as usize, strip.height() as usize],
                    strip.as_raw(),
                )
            })
            .collect(),
    )
}

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
        let zoom = zoom.clamp(-ZOOM_STEPS, ZOOM_STEPS);
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

    /// Mode, tileset and motion choice, kept as soon as they change.
    pub fn controls(&mut self, ui: &mut egui::Ui) {
        let before = self.settings.clone();
        ui.vertical(|ui| {
            ui.horizontal(|ui| {
                for (tiles, label) in [(true, tr("Tiles")), (false, "ASCII".into())] {
                    if ui
                        .selectable_label(self.wants_tiles() == tiles, label)
                        .clicked()
                    {
                        self.settings.tiles = Some(tiles);
                    }
                }
                ui.add_enabled_ui(self.wants_tiles(), |ui| {
                    for (three_d, label) in [(false, "2D"), (true, "3D")] {
                        if ui
                            .selectable_label(self.three_d() == three_d, label)
                            .clicked()
                        {
                            self.settings.three_d = Some(three_d);
                        }
                    }
                });
            });
            let shown = self.chosen().map_or_else(|| tr("none"), |t| t.view.clone());
            let mut pick = None;
            ui.add_enabled_ui(self.wants_tiles(), |ui| {
                egui::ComboBox::from_label(tr("Tileset"))
                    .selected_text(shown)
                    .show_ui(ui, |ui| {
                        for t in &self.tilesets {
                            let on = self.chosen().is_some_and(|c| c.name == t.name);
                            if ui.selectable_label(on, &t.view).clicked() {
                                pick = Some(t.name.clone());
                            }
                        }
                    });
            });
            if pick.is_some() {
                self.settings.tileset = pick;
            }
            if let Some(problem) = &self.problem {
                ui.label(tr_format(
                    "Tileset unavailable: %s",
                    &[Arg::Text(problem.clone())],
                ));
            }
            ui.checkbox(&mut self.settings.reduced_motion, tr("Reduced motion"));
        });
        if self.settings != before {
            self.save();
            // Another view of the same tileset keeps it.
            if (&self.settings.tiles, &self.settings.tileset) != (&before.tiles, &before.tileset) {
                self.open();
            }
        }
    }

    /// Uploads a sheet that finished decoding; a finished thread wakes the UI, so nothing polls.
    pub fn update(&mut self, ctx: &egui::Context) {
        profiling::scope!("Display::update");
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

    /// Draws an overmap square's tile into `rect`; false when no tileset is in use or it has no sprite
    /// for the tile or any of its fallbacks.
    pub fn paint_overmap(
        &mut self,
        painter: &egui::Painter,
        rect: Rect,
        tile: &OvermapTile<'_>,
    ) -> bool {
        let Some(atlas) = &mut self.atlas else {
            return false;
        };
        atlas.request(Fx::Plain);
        let scale = (
            rect.width() / atlas.tileset.tile.0 as f32,
            rect.height() / atlas.tileset.tile.1 as f32,
        );
        let query = Query {
            id: tile.id,
            fallbacks: tile.fallbacks,
            subtile: (!tile.subtile.is_empty()).then_some(tile.subtile),
            rotation: tile.rotation,
            mirrored: false,
            creature: false,
            seed: 0,
            season: None,
        };
        let chosen = atlas.tileset.choose(&query);
        let mut drawn = false;
        for placed in chosen.iter().flat_map(|c| [c.bg, c.fg]).flatten() {
            drawn |= draw_sprite(
                painter,
                atlas,
                rect,
                scale,
                (placed, None),
                (Fx::Plain, Tone::Alpha(255)),
            );
        }
        self.drawn += usize::from(drawn);
        drawn
    }

    /// Whether the tileset has a sprite for the layer.
    pub fn has_sprite(&self, layer: &Layer) -> bool {
        self.atlas
            .as_ref()
            .is_some_and(|a| a.tileset.choose(&query(layer, None)).is_some())
    }

    /// Draws one tile into `rect` when the tileset has a sprite for it; false draws nothing.
    pub fn paint_sprite(&mut self, painter: &egui::Painter, rect: Rect, layer: &Layer) -> bool {
        let Some(atlas) = &mut self.atlas else {
            return false;
        };
        atlas.request(Fx::Plain);
        let scale = (
            rect.width() / atlas.tileset.tile.0 as f32,
            rect.height() / atlas.tileset.tile.1 as f32,
        );
        let chosen = atlas.tileset.choose(&query(layer, None));
        let sprites = chosen.iter().flat_map(|c| [c.bg, c.fg]).flatten();
        let mut any = false;
        for placed in sprites {
            any |= draw_sprite(
                painter,
                atlas,
                rect,
                scale,
                (placed, None),
                (Fx::Plain, Tone::Alpha(255)),
            );
        }
        any
    }

    /// Draws a list row's item into `rect` as `paint` draws a map square, without counting it in what the map
    /// paint reports.
    pub fn paint_icon(
        &mut self,
        painter: &egui::Painter,
        rect: Rect,
        cell: &Cell,
        season: Option<&str>,
    ) -> bool {
        let counts = (self.drawn, self.hidden, self.dimmed);
        let painted = self.paint(painter, rect, cell, season);
        (self.drawn, self.hidden, self.dimmed) = counts;
        painted
    }

    /// Draws a cell's layers into `rect`, bottom first, the way the game's tile view does: sprites where
    /// the tileset has them and the glyph of the thing where it has none. A square nothing is known about is only the hidden
    /// backdrop; false when even that has no sprite.
    pub fn paint(
        &mut self,
        painter: &egui::Painter,
        rect: Rect,
        cell: &Cell,
        season: Option<&str>,
    ) -> bool {
        profiling::scope!("Display::paint");
        let Some(atlas) = &mut self.atlas else {
            return false;
        };
        let fx = match (cell.remembered, cell.light) {
            (true, _) => Fx::Memory,
            (false, Some(light)) if light <= 1 => Fx::Shadow,
            _ => Fx::Plain,
        };
        atlas.request(fx);
        let (sx, sy) = (
            rect.width() / atlas.tileset.tile.0 as f32,
            rect.height() / atlas.tileset.tile.1 as f32,
        );
        // What the game does not show plainly lies on its "hidden" lighting sprite.
        let mut backdrop = false;
        if cell.layers.is_empty() || cell.remembered {
            let hidden = Query {
                id: "lighting_hidden",
                fallbacks: &[],
                subtile: None,
                rotation: 0,
                mirrored: false,
                creature: false,
                seed: cell.square,
                season,
            };
            let sprites = atlas.tileset.choose(&hidden);
            for placed in sprites.iter().flat_map(|c| [c.bg, c.fg]).flatten() {
                backdrop |= draw_sprite(
                    painter,
                    atlas,
                    rect,
                    (sx, sy),
                    (placed, None),
                    (Fx::Plain, Tone::Alpha(255)),
                );
            }
        }
        self.hidden += usize::from(backdrop);
        if cell.layers.is_empty() {
            return backdrop;
        }
        // The game cannot see a dark square at all and can only tell a bright-only one is bright: it draws
        // the lowlight overlay in place of the square's terrain.
        let overlay = match (cell.remembered, cell.light) {
            (false, Some(0)) => Some("lighting_lowlight_dark"),
            (false, Some(2)) => Some("lighting_lowlight_light"),
            _ => None,
        };
        if let Some(id) = overlay {
            let query = Query {
                id,
                fallbacks: &[],
                subtile: None,
                rotation: 0,
                mirrored: false,
                creature: false,
                seed: cell.square,
                season,
            };
            let sprites = atlas.tileset.choose(&query);
            let mut any = false;
            for placed in sprites.iter().flat_map(|c| [c.bg, c.fg]).flatten() {
                any |= draw_sprite(
                    painter,
                    atlas,
                    rect,
                    (sx, sy),
                    (placed, None),
                    (Fx::Plain, Tone::Alpha(255)),
                );
            }
            if any {
                self.dimmed += 1;
                return true;
            }
        }
        // Coloured light falls on what is plainly or dimly in sight.
        let glow = cell
            .glow
            .filter(|_| fx == Fx::Plain || fx == Fx::Shadow)
            .map(light);
        let mut drawn = 0;
        for layer in &cell.layers {
            let (layer, paints) = resolve_layer(&atlas.tileset, layer);
            let layer = layer.as_ref();
            let chosen = atlas.tileset.choose(&query(layer, season));
            let sprites = chosen.iter().flat_map(|c| {
                [(c.bg, paints.0), (c.fg, paints.1)]
                    .into_iter()
                    .filter_map(|(placed, paint)| Some((placed?, paint)))
            });
            let mut any = false;
            for (placed, paint) in sprites {
                let tone = Tone::Alpha(255);
                any |= draw_sprite(painter, atlas, rect, (sx, sy), (placed, paint), (fx, tone));
                if let Some(light) = glow {
                    // The game adds the light's colour over the sprite it falls on.
                    let tone = Tone::Light(light);
                    draw_sprite(painter, atlas, rect, (sx, sy), (placed, paint), (fx, tone));
                }
                if cell.depth > 0 {
                    // A level seen through open air is hazed more the deeper it is.
                    let haze = (24 * (u32::from(cell.depth) + 1)).min(192) as u8;
                    atlas.request(Fx::ZOverlay);
                    let sprite = (placed, None);
                    draw_sprite(
                        painter,
                        atlas,
                        rect,
                        (sx, sy),
                        sprite,
                        (Fx::ZOverlay, Tone::Alpha(haze)),
                    );
                }
            }
            drawn += usize::from(any);
            if !any && layer.glyph != '\0' {
                glyph(painter, rect, layer, fx.stand_in());
            }
        }
        self.drawn += drawn;
        true
    }
}

/// The layer as the game draws it with this tileset, and its paint for the background and the foreground: a
/// mutation's, an item's, a bionic's or an effect's from the tileset, else the part's own. A hair style
/// goes by the sprite of the hair color where the tileset paints no hair.
pub(crate) fn resolve_layer<'a>(tileset: &Tileset, layer: &'a Layer) -> (Cow<'a, Layer>, Paints) {
    if let Some(m) = &layer.mutation {
        let q = PaintQuery {
            id: &m.id,
            types: &m.types,
            flags: &m.flags,
            traits: &m.traits,
        };
        let male = layer.id.starts_with("overlay_male_");
        return match tileset.colored_hair(&q, male) {
            Some([id, plain]) => {
                let shown = Layer {
                    id,
                    fallbacks: vec![plain],
                    ..layer.clone()
                };
                (Cow::Owned(shown), (None, None))
            }
            None => (Cow::Borrowed(layer), tileset.mutation_paints(&q)),
        };
    }
    (Cow::Borrowed(layer), tileset.layer_paints(layer))
}

pub fn query<'a>(layer: &'a Layer, season: Option<&'a str>) -> Query<'a> {
    Query {
        id: &layer.id,
        fallbacks: &layer.fallbacks,
        subtile: layer.subtile.as_deref(),
        rotation: layer.rotation,
        mirrored: layer.mirrored,
        creature: layer.creature,
        seed: layer.seed,
        season,
    }
}

/// A glyph's or its background's colour, darkened by `tint` as the filter of its square asks.
pub(crate) fn glyph_color(c: Rgb, tint: Color32) -> Color32 {
    Color32::from_rgb(c.0, c.1, c.2).gamma_multiply(f32::from(tint.r()) / 255.0)
}

/// A hurt creature's health as a dark `track` filled for its fifths in the game's colour.
pub(crate) fn health_bar(painter: &egui::Painter, track: Rect, health: Health) {
    painter.rect_filled(track, 0.0, Color32::from_black_alpha(200));
    let mut filled = track;
    filled.set_width(track.width() * f32::from(health.fifths) / 5.0);
    let Rgb(r, g, b) = health.color;
    painter.rect_filled(filled, 0.0, Color32::from_rgb(r, g, b));
}

/// The ASCII stand-in of a thing the tileset has no sprite for.
fn glyph(painter: &egui::Painter, rect: Rect, layer: &Layer, tint: Color32) {
    if let Some(bg) = layer.bg {
        painter.rect_filled(rect, 0.0, glyph_color(bg, tint));
    }
    painter.text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        layer.glyph,
        egui::FontId::monospace(rect.height() - 2.0),
        glyph_color(layer.color, tint),
    );
}

/// Whether the game at `root` would be shown with sprites: sprites are chosen in the saved settings
/// at `path` (on by default) and the game has a tileset.
pub fn tiles_wanted(root: Option<&Path>, path: Option<&Path>) -> bool {
    let settings = path
        .and_then(|p| std::fs::read(p).ok())
        .and_then(|b| serde_json::from_slice::<Settings>(&b).ok())
        .unwrap_or_default();
    settings.tiles.unwrap_or(true) && root.is_some_and(|r| !tileset::available(r).is_empty())
}

/// How a sprite is laid on what is under it.
#[derive(Clone, Copy)]
enum Tone {
    /// Blended at this opacity.
    Alpha(u8),
    /// Added, multiplied by this colour, as the game adds coloured light.
    Light(Color32),
}

/// The colour coloured light adds to a sprite: the game's tint, its alpha 40 to 176 with the square root of
/// the light's strength, with no alpha of its own so that egui adds it.
pub fn light(glow: Glow) -> Color32 {
    let alpha = (40.0 + 136.0 * (f32::from(glow.strength) / 255.0).sqrt()).round() / 255.0;
    let Rgb(r, g, b) = glow.color;
    let add = |c: u8| (f32::from(c) * alpha).round() as u8;
    Color32::from_rgba_premultiplied(add(r), add(g), add(b), 0)
}

/// One sprite at its place in the cell, turned as the tileset asks; false when its sheet is not loaded.
fn draw_sprite(
    painter: &egui::Painter,
    atlas: &Atlas,
    cell: Rect,
    scale: (f32, f32),
    (placed, paint): (Placed, Option<Paint>),
    (fx, tone): (Fx, Tone),
) -> bool {
    // A painted sprite is made in the background; the plain one stands in until it is.
    let painted = paint.and_then(|p| atlas.painted(painter.ctx(), (placed, p), fx));
    let Some(Source {
        texture,
        uv,
        offset,
        fx: shown,
    }) = painted.or_else(|| atlas.source(placed.sprite, fx))
    else {
        return false;
    };
    // A sprite whose filtered sheet is still being decoded is darkened by colour meanwhile.
    let tint = match tone {
        Tone::Light(light) => light,
        Tone::Alpha(opacity) if shown == fx => Color32::from_white_alpha(opacity),
        Tone::Alpha(_) => fx.stand_in(),
    };
    let Some((n, _)) = atlas.tileset.locate(placed.sprite) else {
        return false;
    };
    let sheet = &atlas.tileset.sheets[n];
    let size = egui::vec2(
        sheet.sprite.0 as f32 * sheet.pixelscale * scale.0,
        sheet.sprite.1 as f32 * sheet.pixelscale * scale.1,
    );
    let min = cell.min + egui::vec2(offset.0 * scale.0, offset.1 * scale.1);
    let dest = Rect::from_min_size(min, size);
    // A sample on the edge of a sprite can land in the sprite next to it in the sheet, at a zoom where the
    // renderer draws the rectangle off the pixel grid: the source stops half a texel short of each edge.
    let texel = egui::vec2(
        uv.width() / sheet.sprite.0 as f32,
        uv.height() / sheet.sprite.1 as f32,
    );
    let uv = uv.shrink2(texel / 2.0);
    // Source corners for the destination's top left, top right, bottom right and bottom left.
    let [tl, tr, br, bl] = [
        uv.left_top(),
        uv.right_top(),
        uv.right_bottom(),
        uv.left_bottom(),
    ];
    let corners = match placed.turn {
        Turn::None => [tl, tr, br, bl],
        Turn::Left => [tr, br, bl, tl],
        Turn::Right => [bl, tl, tr, br],
        Turn::Half => [br, bl, tl, tr],
        Turn::Mirror => [tr, tl, bl, br],
    };
    let mut mesh = Mesh::with_texture(texture);
    for (pos, uv) in [
        dest.left_top(),
        dest.right_top(),
        dest.right_bottom(),
        dest.left_bottom(),
    ]
    .into_iter()
    .zip(corners)
    {
        mesh.vertices.push(egui::epaint::Vertex {
            pos,
            uv,
            color: tint,
        });
    }
    mesh.indices.extend([0, 1, 2, 0, 2, 3]);
    painter.add(Shape::mesh(mesh));
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A sheet of two 2x1 sprites in `dir`: a sprite of two grey pixels and its mask, whose first pixel lets
    /// the paint through and whose second does not.
    fn sheet(dir: &Path) -> tileset::Sheet {
        let file = dir.join("masked.png");
        let mut image = image::RgbaImage::new(4, 1);
        let pixels = [[128, 128, 128], [200, 40, 40], [255, 255, 127], [0, 0, 0]];
        for (x, rgb) in pixels.into_iter().enumerate() {
            image.put_pixel(x as u32, 0, image::Rgba([rgb[0], rgb[1], rgb[2], 255]));
        }
        image.save(&file).unwrap();
        tileset::Sheet {
            file,
            sprite: (2, 1),
            offset: (0, 0),
            pixelscale: 1.0,
            key: None,
            columns: 2,
            first: 0,
            count: 2,
        }
    }

    #[test]
    fn a_sprite_is_painted_only_where_its_mask_lets_the_paint_through() {
        let dir = std::env::temp_dir().join(format!("bn-client-mask-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let sheet = sheet(&dir);
        let mut decoded = HashMap::new();
        let paint = Paint::of([247, 251, 245]);
        let mut painted = |mask: Option<(usize, tileset::Sheet, u32)>| {
            let job = Job {
                sheet: (0, sheet.clone(), 0),
                mask,
                paint,
                fx: Fx::Plain,
                done: channel().0,
            };
            paint_sprite(&mut decoded, &job).unwrap().pixels
        };
        let color = |p: [u8; 3]| Color32::from_rgb(p[0], p[1], p[2]);
        let masked = painted(Some((0, sheet.clone(), 1)));
        assert_eq!(
            masked[0],
            color(tint_pixel([128; 3], paint, Some([255, 255, 127])))
        );
        assert_ne!(masked[0], color([128; 3]));
        assert_eq!(masked[1], color([200, 40, 40]));
        // Without a mask the whole sprite is painted.
        let whole = painted(None);
        assert_eq!(whole[1], color(tint_pixel([200, 40, 40], paint, None)));
        assert_ne!(whole[1], color([200, 40, 40]));
    }
}
