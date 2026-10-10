//! How the map is drawn: the game's tileset sprites or ASCII glyphs, the player's choice kept between runs.

use crate::tileset::{self, Placed, Query, Tileset, TilesetInfo, Turn};
use bn_client_core::{Cell, Layer};
use eframe::egui::{self, Color32, Mesh, Rect, Shape, TextureHandle, TextureId, TextureOptions};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, channel};
use std::sync::{Arc, OnceLock};

/// The game's default `TILES` option value.
const DEFAULT_TILESET: &str = "UNDEAD_PEOPLE_BASE";
/// Sprite rows are uploaded in strips at most this tall, within every GPU's texture limit.
const STRIP: u32 = 4096;
/// Remembered squares draw darkened, squares in low light slightly less.
const MEMORY: Color32 = Color32::from_gray(100);
const SHADOW: Color32 = Color32::from_gray(170);

/// The choice that survives a restart.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Settings {
    /// Draw sprites; false draws ASCII glyphs.
    pub tiles: bool,
    /// The `TILES` name of the tileset; the game's default when absent.
    pub tileset: Option<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            tiles: true,
            tileset: None,
        }
    }
}

/// `$XDG_CONFIG_HOME/bn-client-egui/settings.json`, or the same under `~/.config`.
pub fn settings_path() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(base.join("bn-client-egui/settings.json"))
}

/// The game's filters for what is not plainly in sight: grayscale for low light, sepia for memory.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fx {
    Plain,
    Shadow,
    Memory,
}

impl Fx {
    const ALL: [Fx; 3] = [Fx::Plain, Fx::Shadow, Fx::Memory];

    fn index(self) -> usize {
        self as usize
    }

    /// The colour a plain sprite is multiplied by until the filtered sheets have arrived.
    fn stand_in(self) -> Color32 {
        match self {
            Fx::Plain => Color32::WHITE,
            Fx::Shadow => SHADOW,
            Fx::Memory => MEMORY,
        }
    }

    /// The game's `color_pixel_*` function of this filter on one pixel.
    pub fn pixel(self, [r, g, b]: [u8; 3]) -> [u8; 3] {
        let average = ((85 * (u32::from(r) + u32::from(g) + u32::from(b))) >> 8) as u8;
        match self {
            Fx::Plain => [r, g, b],
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
struct Source {
    texture: TextureId,
    uv: Rect,
    offset: (f32, f32),
    fx: Fx,
}

/// A tileset, with its plain sheets from the start and the filtered ones once a square needs them.
struct Atlas {
    tileset: Tileset,
    sheets: [Option<Sheets>; 3],
    wake: Wake,
}

impl Atlas {
    fn start(tileset: Tileset) -> Self {
        let wake = Wake::default();
        let plain = Sheets::start(tileset.sheets.clone(), Fx::Plain, wake.clone());
        Self {
            tileset,
            sheets: [Some(plain), None, None],
            wake,
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

/// Decodes a sheet to RGBA under a filter, makes its transparency key transparent and cuts it into
/// strips of whole sprite rows.
fn decode(sheet: &tileset::Sheet, fx: Fx) -> Option<Vec<egui::ColorImage>> {
    let mut image = image::open(&sheet.file).ok()?.into_rgba8();
    for px in image.pixels_mut() {
        if sheet.key.is_some_and(|key| px.0[..3] == key) {
            px.0[3] = 0;
        }
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
    path: Option<PathBuf>,
    /// The game's folder is known, so tilesets can be looked for.
    rooted: bool,
    tilesets: Vec<TilesetInfo>,
    atlas: Option<Atlas>,
    /// Why the chosen tileset is not in use.
    problem: Option<String>,
    /// Sprites drawn by the last map paint.
    drawn: usize,
    /// Squares of the last map paint that lie on the hidden-lighting sprite.
    hidden: usize,
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
            path,
            rooted: root.is_some(),
            tilesets: root.map_or(vec![], tileset::available),
            atlas: None,
            problem: None,
            drawn: 0,
            hidden: 0,
        };
        display.open();
        display
    }

    /// The tileset to use: the chosen one, else the game's default, else the first.
    fn chosen(&self) -> Option<&TilesetInfo> {
        let by_name = |name: &str| self.tilesets.iter().find(|t| t.name == name);
        self.settings
            .tileset
            .as_deref()
            .and_then(by_name)
            .or_else(|| by_name(DEFAULT_TILESET))
            .or(self.tilesets.first())
    }

    /// Loads the chosen tileset when sprites are wanted.
    fn open(&mut self) {
        self.atlas = None;
        self.problem = None;
        if !self.settings.tiles || !self.rooted {
            return;
        }
        let Some(dir) = self.chosen().map(|t| t.dir.clone()) else {
            self.problem = Some("no tileset found in the game's gfx folder".into());
            return;
        };
        match Tileset::load(&dir) {
            Ok(tileset) => self.atlas = Some(Atlas::start(tileset)),
            Err(e) => self.problem = Some(e),
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
            (a.tileset.tile.0 as f32).clamp(glyph, 3.0 * glyph)
        })
    }

    /// Mode and tileset choice, kept as soon as they change.
    pub fn controls(&mut self, ui: &mut egui::Ui) {
        let before = self.settings.clone();
        ui.horizontal(|ui| {
            for (tiles, label) in [(true, "Tiles"), (false, "ASCII")] {
                if ui
                    .selectable_label(self.settings.tiles == tiles, label)
                    .clicked()
                {
                    self.settings.tiles = tiles;
                }
            }
            let shown = self.chosen().map_or("none".to_string(), |t| t.view.clone());
            let mut pick = None;
            ui.add_enabled_ui(self.settings.tiles, |ui| {
                egui::ComboBox::from_label("Tileset")
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
                ui.label(format!("Tileset unavailable: {problem}"));
            }
        });
        if self.settings != before {
            self.save();
            self.open();
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
                backdrop |= draw_sprite(painter, atlas, rect, (sx, sy), placed, Fx::Plain);
            }
        }
        self.hidden += usize::from(backdrop);
        if cell.layers.is_empty() {
            return backdrop;
        }
        let mut drawn = 0;
        for layer in &cell.layers {
            let chosen = atlas.tileset.choose(&query(layer, season));
            let sprites = chosen.iter().flat_map(|c| [c.bg, c.fg]).flatten();
            let mut any = false;
            for placed in sprites {
                any |= draw_sprite(painter, atlas, rect, (sx, sy), placed, fx);
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

fn query<'a>(layer: &'a Layer, season: Option<&'a str>) -> Query<'a> {
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

/// The ASCII stand-in of a thing the tileset has no sprite for.
fn glyph(painter: &egui::Painter, rect: Rect, layer: &Layer, tint: Color32) {
    let c = layer.color;
    if let Some(bg) = layer.bg {
        painter.rect_filled(
            rect,
            0.0,
            Color32::from_rgb(bg.0, bg.1, bg.2).gamma_multiply(f32::from(tint.r()) / 255.0),
        );
    }
    painter.text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        layer.glyph,
        egui::FontId::monospace(rect.height() - 2.0),
        Color32::from_rgb(c.0, c.1, c.2).gamma_multiply(f32::from(tint.r()) / 255.0),
    );
}

/// One sprite at its place in the cell, turned as the tileset asks; false when its sheet is not loaded.
fn draw_sprite(
    painter: &egui::Painter,
    atlas: &Atlas,
    cell: Rect,
    scale: (f32, f32),
    placed: Placed,
    fx: Fx,
) -> bool {
    let Some(Source {
        texture,
        uv,
        offset,
        fx: shown,
    }) = atlas.source(placed.sprite, fx)
    else {
        return false;
    };
    // A sprite whose filtered sheet is still being decoded is darkened by colour meanwhile.
    let tint = if shown == fx {
        Color32::WHITE
    } else {
        fx.stand_in()
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
