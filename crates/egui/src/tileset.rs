//! The game's graphical tilesets: `gfx/<name>/tile_config.json` and its sprite sheets, read the way
//! `cata_tiles` reads them, so the sprite chosen for a cell follows the native view.

use crate::tint::{Blend, Paint};
use bn_client_core::protocol::{ModTileset, TintVarsLayer, Traits};
use bn_client_core::{Layer, Rgb};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// A tileset directory of the game, as `tileset.txt` names it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TilesetInfo {
    /// The game's `TILES` option value.
    pub name: String,
    /// The name shown to players.
    pub view: String,
    pub dir: PathBuf,
}

/// Every tileset under `<root>/gfx`, by name.
pub fn available(root: &Path) -> Vec<TilesetInfo> {
    let Ok(entries) = std::fs::read_dir(root.join("gfx")) else {
        return vec![];
    };
    let mut found = entries
        .flatten()
        .filter_map(|e| {
            let dir = e.path();
            let text = std::fs::read_to_string(dir.join("tileset.txt")).ok()?;
            let name = setting(&text, "NAME")?;
            let view = setting(&text, "VIEW").unwrap_or_else(|| name.clone());
            Some(TilesetInfo { name, view, dir })
        })
        .collect::<Vec<_>>();
    found.sort_by(|a, b| a.name.cmp(&b.name));
    found
}

/// The value of `KEY: value` in a `tileset.txt`.
fn setting(text: &str, key: &str) -> Option<String> {
    text.lines()
        .filter(|l| !l.trim_start().starts_with('#'))
        .find_map(|l| l.trim().strip_prefix(key)?.strip_prefix(':'))
        .map(|v| v.trim().to_string())
}

#[derive(Deserialize)]
struct Config {
    tile_info: Vec<Info>,
    #[serde(rename = "tiles-new", default)]
    sheets: Vec<SheetDef>,
    /// The old format: one sheet, named by `tileset.txt`.
    #[serde(default)]
    tiles: Vec<TileDef>,
    #[serde(default)]
    tints: Vec<TintDef>,
    #[serde(default)]
    tint_pairs: Vec<PairDef>,
}

#[derive(Deserialize)]
struct Info {
    width: u32,
    height: u32,
    /// The sprites are drawn from an isometric view.
    #[serde(default)]
    iso: bool,
    /// Pixels of one z-level's height on screen, for an isometric tileset.
    zlevel_height: Option<f32>,
    /// How many times the map square is drawn at its size, as the game does.
    pixelscale: Option<f32>,
}

#[derive(Deserialize)]
struct SheetDef {
    file: String,
    sprite_width: Option<u32>,
    sprite_height: Option<u32>,
    #[serde(default)]
    sprite_offset_x: i32,
    #[serde(default)]
    sprite_offset_y: i32,
    pixelscale: Option<f32>,
    transparency: Option<Key>,
    #[serde(default)]
    tiles: Vec<TileDef>,
    /// The sheet is a font of 256 symbols per entry, which draws what the tileset has no tile for.
    #[serde(default)]
    ascii: Vec<AsciiDef>,
}

/// 256 symbols of an ASCII sprite sheet in one colour.
#[derive(Deserialize)]
struct AsciiDef {
    /// The sprite of symbol 0 in the sheet.
    offset: u32,
    #[serde(default)]
    bold: bool,
    /// A curses colour name, or `DEFAULT` for all colours.
    #[serde(default = "default_color")]
    color: String,
}

fn default_color() -> String {
    "DEFAULT".into()
}

#[derive(Deserialize)]
struct Key {
    #[serde(rename = "R")]
    r: u8,
    #[serde(rename = "G")]
    g: u8,
    #[serde(rename = "B")]
    b: u8,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum Ids {
    One(String),
    Many(Vec<String>),
}

#[derive(Deserialize)]
struct TileDef {
    id: Ids,
    fg: Option<Sprites>,
    bg: Option<Sprites>,
    #[serde(default)]
    multitile: bool,
    rotates: Option<bool>,
    /// The variants are the frames of an idle animation.
    #[serde(default)]
    animated: bool,
    #[serde(default)]
    additional_tiles: Vec<SubDef>,
    #[serde(default)]
    masks: Vec<MaskDef>,
}

#[derive(Deserialize)]
struct SubDef {
    id: String,
    fg: Option<Sprites>,
    bg: Option<Sprites>,
    #[serde(default)]
    animated: bool,
    #[serde(default)]
    masks: Vec<MaskDef>,
}

/// Sprites that say where, and how much, a tile takes the tint of what it is painted with.
#[derive(Deserialize)]
struct MaskDef {
    #[serde(rename = "type")]
    kind: String,
    fg: Option<Sprites>,
    bg: Option<Sprites>,
}

/// A tileset a game file adds to the ones whose names it lists.
#[derive(Deserialize)]
struct ModConfig {
    #[serde(default)]
    compatibility: Vec<String>,
    #[serde(rename = "tiles-new", default)]
    sheets: Vec<SheetDef>,
    #[serde(default)]
    tints: Vec<TintDef>,
    #[serde(default)]
    tint_pairs: Vec<PairDef>,
}

/// The paint of a mutation: by color, or with its own blend and adjustments.
#[derive(Deserialize)]
struct TintDef {
    id: String,
    fg: Option<ColorDef>,
    bg: Option<ColorDef>,
    blend_mode: Option<String>,
    contrast: Option<f32>,
    saturation: Option<f32>,
    brightness: Option<f32>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum ColorDef {
    Plain(String),
    Detailed {
        color: Option<String>,
        blend_mode: Option<String>,
        contrast: Option<f32>,
        saturation: Option<f32>,
        brightness: Option<f32>,
    },
}

/// A mutation type whose color a tileset takes from the mutations of another type.
#[derive(Deserialize)]
struct PairDef {
    source_type: String,
    target_type: String,
    #[serde(rename = "override", default)]
    overrides: bool,
}

/// The foreground and background paint of a mutation.
pub type Paints = (Option<Paint>, Option<Paint>);

/// What picks the paint of a mutation overlay: the mutation and the character's mutations by type.
pub struct PaintQuery<'a> {
    pub id: &'a str,
    pub types: &'a [String],
    pub flags: &'a [String],
    pub traits: &'a Traits,
}

/// One sprite, the rotations of a tile (2 or 4 sprites), or weighted variants of either.
#[derive(Clone, Deserialize)]
#[serde(untagged)]
enum Sprites {
    One(i64),
    Many(Vec<i64>),
    Weighted(Vec<Weighted>),
}

#[derive(Clone, Deserialize)]
struct Weighted {
    weight: u32,
    sprite: Choice,
}

#[derive(Clone, Deserialize)]
#[serde(untagged)]
enum Choice {
    One(i64),
    Many(Vec<i64>),
}

/// Weighted variants of a sprite; each variant is one sprite or the sprites of its rotations.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Variants(Vec<(u32, Vec<u32>)>);

impl Variants {
    fn new(sprites: Option<Sprites>, offset: u32) -> Self {
        let keep = |ids: Vec<i64>| {
            ids.into_iter()
                .filter_map(|i| Some(u32::try_from(i).ok()? + offset))
                .collect::<Vec<_>>()
        };
        Self(match sprites {
            None => vec![],
            Some(Sprites::One(i)) => vec![(1, keep(vec![i]))],
            Some(Sprites::Many(ids)) => vec![(1, keep(ids))],
            Some(Sprites::Weighted(list)) => list
                .into_iter()
                .map(|w| {
                    let ids = match w.sprite {
                        Choice::One(i) => vec![i],
                        Choice::Many(ids) => ids,
                    };
                    (w.weight, keep(ids))
                })
                .collect(),
        })
    }

    /// The number of the variant `random` selects, like the game's weighted list.
    fn pick(&self, random: u32) -> Option<usize> {
        let total = self.weight();
        if total == 0 {
            return None;
        }
        if self.0.len() == 1 {
            return Some(0);
        }
        let target = random % total + 1;
        let mut seen = 0;
        self.0.iter().position(|(weight, _)| {
            seen += weight;
            seen >= target
        })
    }

    /// The sum of the variants' weights.
    fn weight(&self) -> u32 {
        self.0.iter().map(|(w, _)| *w).sum()
    }

    /// The sprite of variant `variant` for a rotation: the only one, or the rotation's own.
    fn sprite(&self, variant: usize, rotation: u8) -> Option<u32> {
        let ids = &self.0.get(variant)?.1;
        match ids.len() {
            1 => Some(ids[0]),
            n => ids.get(usize::from(rotation) % n.max(1)).copied(),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Tile {
    fg: Variants,
    bg: Variants,
    /// The tint masks of `fg` and `bg`, one per sprite; empty when the tile is painted whole.
    fg_mask: Variants,
    bg_mask: Variants,
    rotates: bool,
    multitile: bool,
    /// The variants are animation frames, each shown for its weight in frames.
    animated: bool,
    /// Multitile keys this tile defines (`center`, `edge`, ...).
    subtiles: Vec<String>,
    /// Whether this is a part of a multitile, which turns its background by itself.
    part: bool,
}

/// A sprite sheet; sprite `first + n` is the n-th cell of the image, left to right and top down.
#[derive(Clone, Debug)]
pub struct Sheet {
    pub file: PathBuf,
    pub sprite: (u32, u32),
    pub offset: (i32, i32),
    pub pixelscale: f32,
    /// Pixel colour that is transparent.
    pub key: Option<[u8; 3]>,
    pub columns: u32,
    pub first: u32,
    pub count: u32,
}

pub struct Tileset {
    /// Width and height of one map square in the tileset's pixels.
    pub tile: (u32, u32),
    /// How many times a map square is drawn at the size of `tile`.
    pub pixelscale: f32,
    /// The sprites are drawn from an isometric view: a map square is a diamond of `tile` pixels.
    pub iso: bool,
    /// Pixels of one z-level's height on screen, when the tileset says.
    pub zlevel_height: Option<f32>,
    pub sheets: Vec<Sheet>,
    tiles: HashMap<String, Tile>,
    /// Paints by mutation id, and the mutation types whose paints a type takes.
    tints: HashMap<String, Paints>,
    tint_pairs: HashMap<String, (String, bool)>,
}

/// What to draw for one thing in a cell.
#[derive(Clone, Copy)]
pub struct Query<'a> {
    pub id: &'a str,
    pub fallbacks: &'a [String],
    pub subtile: Option<&'a str>,
    pub rotation: u8,
    pub mirrored: bool,
    /// A character or monster: its sprites keep their facing whether or not the tile rotates.
    pub creature: bool,
    pub seed: u32,
    pub season: Option<&'a str>,
    /// The idle animation frame, 17 ms each as the game counts them; none while animations are off, which shows
    /// an animated tile's first frame.
    pub frame: Option<u32>,
}

/// How a sprite is turned when drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Turn {
    None,
    /// 90 degrees counter-clockwise.
    Left,
    /// 90 degrees clockwise.
    Right,
    Half,
    Mirror,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Placed {
    pub sprite: u32,
    /// The sprite that masks where a paint tints this one.
    pub mask: Option<u32>,
    pub turn: Turn,
}

/// The sprites chosen for a thing: background first, then foreground.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Chosen {
    pub bg: Option<Placed>,
    pub fg: Option<Placed>,
    /// The tile has parts for how it joins its neighbours, as walls do.
    pub joined: bool,
    /// What the tile shows changes with the animation frame.
    pub animated: bool,
}

/// How long the game shows one frame of an idle animation (cata_tiles.cpp `idle_animation_manager`).
pub const FRAME: std::time::Duration = std::time::Duration::from_millis(17);

/// The id of the tile that draws an ASCII symbol in a tileset, as `cata_tiles.cpp` names them: the symbol's code
/// and the curses colour (0-15, or -1 for all colours).
pub fn ascii_id(symbol: u32, color: i8) -> String {
    format!("ASCII_{symbol}_{color}")
}

/// The symbol code a glyph has in a tileset's ASCII sheet: the box-drawing glyphs have the codes the game gives
/// them, any other glyph its character code when that is below 256.
fn ascii_symbol(glyph: char) -> Option<u32> {
    let line = "│─└┌┐┘├┴┤┬┼".chars().position(|c| c == glyph);
    match (line, u32::from(glyph)) {
        (Some(i), _) => Some(0xa0 + i as u32),
        (None, 1..256) => Some(u32::from(glyph)),
        _ => None,
    }
}

/// The game's mix of a seed into the integer that picks a weighted variant.
pub fn mix(seed: u32) -> u32 {
    let rot = u32::rotate_left;
    let (mut a, mut b, mut c) = (seed, seed.wrapping_neg(), seed.wrapping_mul(seed));
    c ^= b;
    c = c.wrapping_sub(rot(b, 14));
    a ^= c;
    a = a.wrapping_sub(rot(c, 11));
    b ^= a;
    b = b.wrapping_sub(rot(a, 25));
    c ^= b;
    c = c.wrapping_sub(rot(b, 16));
    a ^= c;
    a = a.wrapping_sub(rot(c, 4));
    b ^= a;
    b = b.wrapping_sub(rot(a, 14));
    c ^= b;
    c.wrapping_sub(rot(b, 24))
}

const SEASONS: [&str; 4] = ["spring", "summer", "autumn", "winter"];
const MULTITILE_KEYS: [&str; 8] = [
    "center",
    "corner",
    "edge",
    "t_connection",
    "end_piece",
    "unconnected",
    "open",
    "broken",
];

impl Tileset {
    /// Reads the tileset in `dir` with the mod tilesets found in the game's data: `tileset.txt` names its JSON
    /// and, for the old format, its image.
    pub fn load(dir: &Path) -> Result<Self, String> {
        Self::load_with(dir, None)
    }

    /// Reads the tileset in `dir` with the mod tilesets the engine lists, or those found in the game's data.
    pub fn load_with(dir: &Path, mods: Option<&[ModTileset]>) -> Result<Self, String> {
        let text = std::fs::read_to_string(dir.join("tileset.txt")).unwrap_or_default();
        let json = setting(&text, "JSON").unwrap_or_else(|| "tile_config.json".into());
        let image = setting(&text, "TILESET").unwrap_or_else(|| "tiles.png".into());
        let path = dir.join(&json);
        let config: Config = serde_json::from_slice(
            &std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?,
        )
        .map_err(|e| format!("{}: {e}", path.display()))?;
        let info = config.tile_info.last().ok_or("tile_info is missing")?;
        let tile = (info.width, info.height);
        let mut sheets = config.sheets;
        if sheets.is_empty() {
            sheets.push(SheetDef {
                file: image,
                sprite_width: None,
                sprite_height: None,
                sprite_offset_x: 0,
                sprite_offset_y: 0,
                pixelscale: None,
                transparency: None,
                tiles: config.tiles,
                ascii: vec![],
            });
        }
        let mut set = Self {
            tile,
            pixelscale: info.pixelscale.filter(|p| *p > 0.0).unwrap_or(1.0),
            iso: info.iso,
            zlevel_height: info.zlevel_height,
            sheets: vec![],
            tiles: HashMap::new(),
            tints: HashMap::new(),
            tint_pairs: HashMap::new(),
        };
        set.add_sheets(sheets, dir)?;
        set.add_tints(config.tints, config.tint_pairs);
        let name = setting(&text, "NAME").unwrap_or_default();
        // Sprites of the files the game adds to this tileset come after its own.
        let found = match (mods, dir.parent().and_then(Path::parent)) {
            (Some(list), _) => list.to_vec(),
            (None, Some(root)) => scan_mods(root),
            (None, None) => vec![],
        };
        // The engine names its files from the game folder, which is where `gfx` lies.
        let game = dir.parent().and_then(Path::parent).unwrap_or(dir);
        for file in found.iter().filter(|m| m.compatibility.contains(&name)) {
            let Some(config) = mod_config(&game.join(&file.path), file.index) else {
                continue;
            };
            if set
                .add_sheets(config.sheets, &game.join(&file.base))
                .is_ok()
            {
                set.add_tints(config.tints, config.tint_pairs);
            }
        }
        Ok(set)
    }

    fn add_tints(&mut self, tints: Vec<TintDef>, pairs: Vec<PairDef>) {
        for def in tints {
            let paint = |color: &Option<ColorDef>| {
                let top = def.contrast.is_some() || def.saturation.is_some();
                match color.as_ref()? {
                    ColorDef::Plain(text) => {
                        let (color, hex) = parse_color(text)?;
                        let by = |value: Option<f32>, own: f32| {
                            value.unwrap_or(if top { 1.0 } else { own })
                        };
                        Some(Paint {
                            color,
                            blend: Blend::named(def.blend_mode.as_deref().unwrap_or("")),
                            contrast: milli(by(def.contrast, 1.0)),
                            saturation: milli(by(def.saturation, 1.0)),
                            brightness: milli(if top { by(def.brightness, 1.0) } else { hex }),
                        })
                    }
                    // An object cannot go with a top level contrast or saturation.
                    ColorDef::Detailed { .. } if top => None,
                    ColorDef::Detailed {
                        color,
                        blend_mode,
                        contrast,
                        saturation,
                        brightness,
                    } => {
                        let (color, hex) = parse_color(color.as_deref()?)?;
                        Some(Paint {
                            color,
                            blend: Blend::named(blend_mode.as_deref().unwrap_or("")),
                            contrast: milli(contrast.unwrap_or(1.0)),
                            saturation: milli(saturation.unwrap_or(1.0)),
                            brightness: milli(brightness.unwrap_or(hex)),
                        })
                    }
                }
            };
            let (bg, fg) = (paint(&def.bg), paint(&def.fg));
            if bg.is_some() || fg.is_some() {
                self.tints.insert(def.id, (bg, fg));
            }
        }
        for pair in pairs {
            self.tint_pairs
                .insert(pair.target_type, (pair.source_type, pair.overrides));
        }
    }

    /// The ids the tileset's tints are keyed by, which the engine keys the things it publishes by.
    pub fn tint_ids(&self) -> Vec<String> {
        let mut ids = self.tints.keys().cloned().collect::<Vec<_>>();
        ids.sort();
        ids
    }

    /// The paint of the first of `keys` the tileset has one for: how the game tints an item, a bionic or an effect.
    pub fn key_paints(&self, keys: &[String]) -> Paints {
        keys.iter()
            .find_map(|key| self.tints.get(key).copied())
            .unwrap_or((None, None))
    }

    /// The paint of a layer that is not a mutation, as the game works it out: the tileset's tint for the first of
    /// its keys, its own color (a vehicle part's paint) in place of that tint's color, then the tint its item's
    /// variables ask for over both.
    pub fn layer_paints(&self, layer: &Layer) -> Paints {
        let (bg, fg) = self.key_paints(&layer.tint_keys);
        let recolor = |base: Option<Paint>, own: Option<Rgb>| match own {
            Some(Rgb(r, g, b)) => Some(Paint {
                color: [r, g, b],
                ..base.unwrap_or(Paint::of([r, g, b]))
            }),
            None => base,
        };
        let (bg, fg) = (recolor(bg, layer.tint_bg), recolor(fg, layer.tint_fg));
        match &layer.tint_vars {
            Some(vars) => (with_vars(bg, &vars.bg), with_vars(fg, &vars.fg)),
            None => (bg, fg),
        }
    }

    /// The sprite ids that stand for a hair style in the character's hair color, when the tileset has no tint
    /// for hair colors and the game picks a sprite per color instead: its gendered id, then the plain one.
    pub fn colored_hair(&self, q: &PaintQuery, male: bool) -> Option<[String; 2]> {
        let controlled = |key: &String| self.tint_pairs.get(key).map(|(_, overrides)| *overrides);
        let overridden = q
            .types
            .iter()
            .chain(q.flags)
            .find_map(controlled)
            .unwrap_or(false);
        if overridden || !q.types.iter().any(|t| t == "hair_style") {
            return None;
        }
        let color = q.traits.get("hair_color")?.first()?;
        let overlay = format!("mutation_{}", q.id);
        if overlay.contains(color.as_str()) {
            return None;
        }
        let at = overlay.find("hair_")?;
        let (prefix, suffix) = overlay.split_at(at);
        let named = format!("{prefix}{color}{}", &suffix[suffix.find('_')?..]);
        let ids = [
            format!("overlay_{}_{named}", if male { "male" } else { "female" }),
            format!("overlay_{named}"),
        ];
        ids.iter()
            .any(|id| self.tiles.contains_key(id))
            .then_some(ids)
    }

    /// The paint the game gives the sprite of a mutation: that of the character's mutation of the type that
    /// the mutation's types or flags take their color from, else the mutation's own.
    pub fn mutation_paints(&self, q: &PaintQuery) -> Paints {
        let by_source = |key: &String| {
            let (source, _) = self.tint_pairs.get(key)?;
            q.traits
                .get(source)?
                .iter()
                .find_map(|id| self.tints.get(id).copied())
        };
        q.types
            .iter()
            .find_map(by_source)
            .or_else(|| q.flags.iter().find_map(by_source))
            .or_else(|| self.tints.get(q.id).copied())
            .unwrap_or((None, None))
    }

    /// Adds sheets that follow the ones loaded; their tiles number sprites from the first of them.
    fn add_sheets(&mut self, sheets: Vec<SheetDef>, dir: &Path) -> Result<(), String> {
        let tile = self.tile;
        let start = self.sheets.last().map_or(0, |s| s.first + s.count);
        let mut first = start;
        for def in sheets {
            let file = dir.join(&def.file);
            let sprite = (
                def.sprite_width.unwrap_or(tile.0),
                def.sprite_height.unwrap_or(tile.1),
            );
            // An ASCII fallback sheet without sprites may be absent, as in the game.
            let size = match image::image_dimensions(&file) {
                Ok(size) => size,
                Err(_) if def.tiles.is_empty() => continue,
                Err(e) => return Err(format!("{}: {e}", file.display())),
            };
            let columns = (size.0 / sprite.0.max(1)).max(1);
            let count = columns * (size.1 / sprite.1.max(1));
            self.sheets.push(Sheet {
                file,
                sprite,
                offset: (def.sprite_offset_x, def.sprite_offset_y),
                pixelscale: def.pixelscale.unwrap_or(1.0),
                key: def.transparency.map(|k| [k.r, k.g, k.b]),
                columns,
                first,
                count,
            });
            for set in &def.ascii {
                self.add_ascii(set, (first, count));
            }
            first += count;
            for entry in def.tiles {
                self.add(entry, start);
            }
        }
        Ok(())
    }

    /// The tiles of one ASCII set of the sheet that starts at sprite `first` and holds `count`, as the game's
    /// `load_ascii_set` makes them: the box-drawing symbols join their neighbours as walls do.
    fn add_ascii(&mut self, set: &AsciiDef, (first, count): (u32, u32)) {
        let color = match set.color.as_str() {
            "BLACK" => 0,
            "RED" => 1,
            "GREEN" => 2,
            "YELLOW" => 3,
            "BLUE" => 4,
            "MAGENTA" => 5,
            "CYAN" => 6,
            "WHITE" => 7,
            _ => -1,
        };
        let color = if color < 0 {
            -1
        } else {
            color + 8 * i8::from(set.bold)
        };
        let base = first + set.offset;
        let sprite = |at: u32| Variants(vec![(1, vec![base + at])]);
        let plain = |at: u32| Tile {
            fg: sprite(at),
            ..Tile::default()
        };
        for symbol in (0..256).filter(|s| set.offset + s < count) {
            let id = ascii_id(symbol, color);
            // The sprite a box-drawing symbol shows when it is not joined.
            let shown = match symbol {
                0xa0 => 186,
                0xa1 => 205,
                0xa2 => 200,
                0xa3 => 201,
                0xa4 => 187,
                0xa5 => 188,
                0xa6 => 208,
                0xa7 => 202,
                0xa8 => 184,
                0xa9 => 203,
                0xaa => 206,
                other => other,
            };
            let mut tile = plain(shown);
            if [0xa0, 0xa1].contains(&symbol) {
                let parts = [
                    ("center", 206),
                    ("corner", 201),
                    ("edge", 186),
                    ("t_connection", 203),
                    ("end_piece", 210),
                    ("unconnected", 219),
                ];
                for (key, at) in parts {
                    let part = Tile {
                        rotates: true,
                        part: true,
                        ..plain(at)
                    };
                    self.tiles.insert(format!("{id}_{key}"), part);
                }
                tile.multitile = true;
                tile.subtiles = parts.map(|(key, _)| key.to_string()).to_vec();
            }
            self.tiles.insert(id, tile);
        }
    }

    fn add(&mut self, entry: TileDef, offset: u32) {
        let ids = match entry.id {
            Ids::One(id) => vec![id],
            Ids::Many(ids) => ids,
        };
        for id in ids {
            let subtiles = entry
                .additional_tiles
                .iter()
                .map(|s| s.id.clone())
                .collect::<Vec<_>>();
            if entry.multitile {
                for sub in &entry.additional_tiles {
                    let part = MULTITILE_KEYS.contains(&sub.id.as_str());
                    self.tiles.insert(
                        format!("{id}_{}", sub.id),
                        Tile::new(&sub.fg, &sub.bg, &sub.masks, offset)
                            .with(|t| (t.rotates, t.part, t.animated) = (true, part, sub.animated)),
                    );
                }
            }
            self.tiles.insert(
                id,
                Tile::new(&entry.fg, &entry.bg, &entry.masks, offset).with(|t| {
                    t.rotates = entry.rotates.unwrap_or(entry.multitile);
                    t.multitile = entry.multitile;
                    t.animated = entry.animated;
                    t.subtiles = if entry.multitile { subtiles } else { vec![] };
                }),
            );
        }
    }

    /// The first tile the id or one of its fallbacks names, with the id it was found under.
    fn find(&self, q: &Query) -> Option<(String, &Tile)> {
        let season = q.season.filter(|s| SEASONS.contains(s));
        std::iter::once(q.id)
            .chain(q.fallbacks.iter().map(String::as_str))
            .filter(|id| !id.is_empty())
            .find_map(|id| {
                let seasonal = season.map(|s| format!("{id}_season_{s}"));
                seasonal
                    .into_iter()
                    .chain([id.to_string()])
                    .find_map(|key| Some((key.clone(), self.tiles.get(&key)?)))
            })
    }

    /// The sprites the game would draw for `q`; none when the tileset has no tile for it.
    pub fn choose(&self, q: &Query) -> Option<Chosen> {
        let (found, mut tile) = self.find(q)?;
        let joined = tile.multitile;
        if let Some(key) = q.subtile
            && tile.multitile
            && tile.subtiles.iter().any(|s| s == key)
            && let Some(part) = self.tiles.get(&format!("{found}_{key}"))
        {
            tile = part;
        }
        let rotation = match (q.creature, q.mirrored) {
            (true, true) => 4,
            (true, false) => 0,
            (false, _) => u8::from(tile.rotates) * q.rotation,
        };
        let varied = tile.fg.0.len() > 1 || tile.bg.0.len() > 1;
        // An animated tile shows each variant for its weight in frames, offset by its square so that the
        // squares do not all change at once (cata_tiles.cpp draw_from_id_string).
        let random = match (varied, tile.animated, q.frame) {
            (false, _, _) | (true, true, None) => 0,
            (true, false, _) => mix(q.seed),
            (true, true, Some(frame)) => {
                let frames = match tile.fg.weight() {
                    1 => tile.bg.weight(),
                    n => n,
                };
                frame.wrapping_add(mix(q.seed)) % frames.max(1)
            }
        };
        let place = |variants: &Variants, masks: &Variants, foreground: bool| {
            let variant = variants.pick(random)?;
            let sprite = variants.sprite(variant, rotation)?;
            // A single sprite turns with the tile; a background only when the tile asks for it.
            let single = variants.0[variant].1.len() == 1;
            let turns = single && (foreground || (tile.rotates && !tile.part));
            Some(Placed {
                sprite,
                mask: masks.sprite(variant, rotation),
                turn: if turns { turn_of(rotation) } else { Turn::None },
            })
        };
        Some(Chosen {
            bg: place(&tile.bg, &tile.bg_mask, false),
            fg: place(&tile.fg, &tile.fg_mask, true),
            joined,
            animated: tile.animated && varied && q.frame.is_some(),
        })
    }

    /// The sprites of `q`, else what the game draws for a thing the tileset has no tile for: the tileset's ASCII
    /// sprite of the thing's `glyph` in its curses `color`, in any colour, and its `unknown` tile. An overlay,
    /// whose glyph is NUL, draws nothing instead.
    pub fn choose_or_stand_in(&self, q: &Query, glyph: char, color: u8) -> Option<Chosen> {
        self.choose(q).or_else(|| {
            let ascii = ascii_symbol(glyph).into_iter().flat_map(|symbol| {
                let color = i8::try_from(color).unwrap_or(-1);
                [ascii_id(symbol, color), ascii_id(symbol, -1)]
            });
            let ids = ascii.chain(["unknown".to_string()]).collect::<Vec<_>>();
            let (id, fallbacks) = ids.split_first().filter(|_| glyph != '\0')?;
            self.choose(&Query {
                id,
                fallbacks,
                ..*q
            })
        })
    }

    /// The sheet holding `sprite`, and its index within the sheet.
    pub fn locate(&self, sprite: u32) -> Option<(usize, u32)> {
        let n = self
            .sheets
            .partition_point(|s| s.first <= sprite)
            .checked_sub(1)?;
        let sheet = &self.sheets[n];
        (sprite - sheet.first < sheet.count).then_some((n, sprite - sheet.first))
    }
}

impl Tile {
    fn new(fg: &Option<Sprites>, bg: &Option<Sprites>, masks: &[MaskDef], offset: u32) -> Self {
        let (fg, bg) = (
            Variants::new(fg.clone(), offset),
            Variants::new(bg.clone(), offset),
        );
        let tint = masks.iter().find(|m| m.kind == "tint");
        let mask = |sprites: &Variants, masks: Option<&Option<Sprites>>| {
            let masks = Variants::new(masks.and_then(Clone::clone), offset);
            // Masks that do not match their sprites are dropped, as the game does.
            let fits = masks.0.len() == sprites.0.len()
                && masks
                    .0
                    .iter()
                    .zip(&sprites.0)
                    .all(|(m, s)| (m.0, m.1.len()) == (s.0, s.1.len()));
            if fits { masks } else { Variants::default() }
        };
        Self {
            fg_mask: mask(&fg, tint.map(|m| &m.fg)),
            bg_mask: mask(&bg, tint.map(|m| &m.bg)),
            fg,
            bg,
            ..Self::default()
        }
    }

    fn with(mut self, set: impl FnOnce(&mut Self)) -> Self {
        set(&mut self);
        self
    }
}

/// A paint as `#rrggbb`, or `#rrggbbmm` with the brightness the last pair stands for, in thousandths.
fn parse_color(text: &str) -> Option<([u8; 3], f32)> {
    let hex = text.strip_prefix('#')?;
    if !hex.bytes().all(|b| b.is_ascii_hexdigit()) || ![6, 8].contains(&hex.len()) {
        return None;
    }
    let pair = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).ok();
    let brightness = if hex.len() == 8 {
        pair(6).map_or(1.0, |m| f32::from(m) / 128.0)
    } else {
        1.0
    };
    Some(([pair(0)?, pair(2)?, pair(4)?], brightness))
}

/// `base` with what the variables of an item set over it: without a color of its own or in `base` there is no
/// paint.
fn with_vars(base: Option<Paint>, vars: &TintVarsLayer) -> Option<Paint> {
    let mut paint = match (vars.color.as_deref().and_then(parse_color), base) {
        (Some((color, _)), base) => Paint {
            color,
            ..base.unwrap_or(Paint::of(color))
        },
        (None, base) => base?,
    };
    if let Some(name) = &vars.blend_mode {
        paint.blend = Blend::named(name);
    }
    paint.contrast = vars.contrast.map_or(paint.contrast, milli);
    paint.saturation = vars.saturation.map_or(paint.saturation, milli);
    paint.brightness = vars.brightness.map_or(paint.brightness, milli);
    Some(paint)
}

fn milli(value: f32) -> u32 {
    (value * 1000.0).round().max(0.0) as u32
}

/// The mod tilesets under `<root>/data/json/external_tileset`, in file order.
fn scan_mods(root: &Path) -> Vec<ModTileset> {
    let base = root.join("data/json");
    let dir = base.join("external_tileset");
    let base = base.display().to_string();
    let mut files = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "json"))
        .collect::<Vec<_>>();
    files.sort();
    files
        .into_iter()
        .flat_map(|file| {
            let found = mod_objects(&file)
                .iter()
                .filter_map(|v| serde_json::from_value::<ModConfig>(v.clone()).ok())
                .map(|c| c.compatibility)
                .collect::<Vec<_>>();
            let base = base.clone();
            found
                .into_iter()
                .enumerate()
                .map(move |(i, compatibility)| ModTileset {
                    path: file.display().to_string(),
                    base: base.clone(),
                    index: i as u32 + 1,
                    compatibility,
                })
        })
        .collect()
}

/// The `mod_tileset` objects of a game data file.
fn mod_objects(file: &Path) -> Vec<serde_json::Value> {
    let json = std::fs::read(file)
        .ok()
        .and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok());
    let all = match json {
        Some(serde_json::Value::Array(list)) => list,
        other => other.into_iter().collect(),
    };
    all.into_iter()
        .filter(|v| v["type"] == "mod_tileset")
        .collect()
}

/// The `index`th `mod_tileset` of a game data file, counting from 1.
fn mod_config(file: &Path, index: u32) -> Option<ModConfig> {
    let objects = mod_objects(file);
    let object = objects.get((index as usize).checked_sub(1)?)?;
    serde_json::from_value(object.clone()).ok()
}

/// The turn the game applies for its rotation index: quarter turns, 4 mirrors.
fn turn_of(rotation: u8) -> Turn {
    match rotation {
        1 => Turn::Left,
        2 => Turn::Half,
        3 => Turn::Right,
        4 => Turn::Mirror,
        _ => Turn::None,
    }
}
