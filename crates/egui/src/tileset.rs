//! The game's graphical tilesets: `gfx/<name>/tile_config.json` and its sprite sheets, read the way
//! `cata_tiles` reads them, so the sprite chosen for a cell follows the native view.

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
}

#[derive(Deserialize)]
struct Info {
    width: u32,
    height: u32,
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
    #[serde(default)]
    additional_tiles: Vec<SubDef>,
}

#[derive(Deserialize)]
struct SubDef {
    id: String,
    fg: Option<Sprites>,
    bg: Option<Sprites>,
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
    fn new(sprites: Option<Sprites>) -> Self {
        let keep = |ids: Vec<i64>| {
            ids.into_iter()
                .filter_map(|i| u32::try_from(i).ok())
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

    /// The variant `random` selects, like the game's weighted list.
    fn pick(&self, random: u32) -> Option<&[u32]> {
        let total = self.0.iter().map(|(w, _)| *w).sum::<u32>();
        if total == 0 {
            return None;
        }
        if self.0.len() == 1 {
            return Some(&self.0[0].1);
        }
        let target = random % total + 1;
        let mut seen = 0;
        self.0.iter().find_map(|(weight, ids)| {
            seen += weight;
            (seen >= target).then_some(ids.as_slice())
        })
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Tile {
    fg: Variants,
    bg: Variants,
    rotates: bool,
    multitile: bool,
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
    pub pixelscale: f32,
    pub sheets: Vec<Sheet>,
    tiles: HashMap<String, Tile>,
}

/// What to draw for one thing in a cell.
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
    pub turn: Turn,
}

/// The sprites chosen for a thing: background first, then foreground.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Chosen {
    pub bg: Option<Placed>,
    pub fg: Option<Placed>,
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
    /// Reads the tileset in `dir`: `tileset.txt` names its JSON and, for the old format, its image.
    pub fn load(dir: &Path) -> Result<Self, String> {
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
            });
        }
        let mut set = Self {
            tile,
            pixelscale: 1.0,
            sheets: vec![],
            tiles: HashMap::new(),
        };
        let mut first = 0;
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
            set.sheets.push(Sheet {
                file,
                sprite,
                offset: (def.sprite_offset_x, def.sprite_offset_y),
                pixelscale: def.pixelscale.unwrap_or(1.0),
                key: def.transparency.map(|k| [k.r, k.g, k.b]),
                columns,
                first,
                count,
            });
            first += count;
            for entry in def.tiles {
                set.add(entry);
            }
        }
        Ok(set)
    }

    fn add(&mut self, entry: TileDef) {
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
                        Tile {
                            fg: Variants::new(sub.fg.clone()),
                            bg: Variants::new(sub.bg.clone()),
                            rotates: true,
                            part,
                            ..Tile::default()
                        },
                    );
                }
            }
            self.tiles.insert(
                id,
                Tile {
                    fg: Variants::new(entry.fg.clone()),
                    bg: Variants::new(entry.bg.clone()),
                    rotates: entry.rotates.unwrap_or(entry.multitile),
                    multitile: entry.multitile,
                    subtiles: if entry.multitile { subtiles } else { vec![] },
                    part: false,
                },
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
        let random = if varied { mix(q.seed) } else { 0 };
        let place = |variants: &Variants, foreground: bool| {
            let ids = variants.pick(random)?;
            let sprite = if ids.len() == 1 {
                ids[0]
            } else {
                *ids.get(usize::from(rotation) % ids.len().max(1))?
            };
            // A single sprite turns with the tile; a background only when the tile asks for it.
            let turns = ids.len() == 1 && (foreground || (tile.rotates && !tile.part));
            Some(Placed {
                sprite,
                turn: if turns { turn_of(rotation) } else { Turn::None },
            })
        };
        Some(Chosen {
            bg: place(&tile.bg, false),
            fg: place(&tile.fg, true),
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
