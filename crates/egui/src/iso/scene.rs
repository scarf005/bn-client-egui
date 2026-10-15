//! The scene of the 3D map: what the model shows in each square, as quads grouped by the page they sample.

use super::geometry::{
    Instance, Kind, Metrics, PageId, Placement, Shape, SpriteSource, place, shape,
};
use super::pages::Pages;
use crate::display::{STRIP, query, resolve_layer};
use crate::tileset::{Tileset, Turn};
use bn_client_core::{GameModel, Pos};
use glam::Vec2;
use std::collections::BTreeMap;

/// The quads that sample one page.
pub struct Batch {
    pub page: PageId,
    pub instances: Vec<Instance>,
}

pub struct Scene {
    /// Names this scene among those built, so a renderer knows when to upload again.
    pub version: u64,
    pub batches: Vec<Batch>,
    /// The squares of the model's view with something to draw.
    pub squares: usize,
    /// The size of the view in squares, and what each square drawn is, row by row.
    pub width: i32,
    pub height: i32,
    columns: Vec<Option<Column>>,
}

/// What a square is for picking: the ground of its level, and the box standing on it if it has one.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Column {
    pub ground: f32,
    /// The bottom and the top of the box.
    pub solid: Option<(f32, f32)>,
}

impl Column {
    /// How high the square reaches.
    pub fn top(&self) -> f32 {
        self.solid
            .map_or(self.ground, |(_, top)| top.max(self.ground))
    }
}

impl Scene {
    pub fn column(&self, x: i32, y: i32) -> Option<&Column> {
        let inside = (0..self.width).contains(&x) && (0..self.height).contains(&y);
        inside.then(|| self.columns[(y * self.width + x) as usize].as_ref())?
    }
}

/// A sprite of a square, found before any is placed: the boxes next to one another hide each other's faces.
struct Item {
    square: (i32, i32),
    floor: f32,
    kind: Kind,
    turn: Turn,
    sprite: SpriteSource,
    filter: u32,
    haze: u8,
    order: u32,
}

/// The directions of a box's faces, in the order `place` lists them: south, east, north, west.
const FACES: [(i32, i32); 4] = [(0, 1), (1, 0), (0, -1), (-1, 0)];

/// Where sprite number `sprite` of the tileset is: its page and the place in it.
pub fn source(tileset: &Tileset, sprite: u32) -> Option<SpriteSource> {
    let (n, local) = tileset.locate(sprite)?;
    let sheet = &tileset.sheets[n];
    let rows = (STRIP / sheet.sprite.1.max(1)).max(1);
    let (col, row) = (local % sheet.columns.max(1), local / sheet.columns.max(1));
    Some(SpriteSource {
        page: PageId {
            sheet: n as u16,
            strip: (row / rows) as u16,
        },
        at: Vec2::new(
            (col * sheet.sprite.0) as f32,
            ((row % rows) * sheet.sprite.1) as f32,
        ),
        src: Vec2::new(sheet.sprite.0 as f32, sheet.sprite.1 as f32),
        offset: Vec2::new(sheet.offset.0 as f32, sheet.offset.1 as f32),
        scale: sheet.pixelscale,
    })
}

pub fn metrics(tileset: &Tileset) -> Metrics {
    Metrics::new(tileset.tile, tileset.iso, tileset.zlevel_height)
}

/// Points per world unit when the tileset's tile is drawn `zoom` times its size: a diagonal of a square across
/// the tile of an isometric tileset, else a square the tile.
pub fn world_scale(tileset: &Tileset, zoom: f32) -> f32 {
    let across = tileset.tile.0 as f32 * zoom;
    match tileset.iso {
        true => across / std::f32::consts::SQRT_2,
        false => across,
    }
}

/// The scene of the squares the model shows. A square seen through open air stands lower, by the levels it lies
/// down; the levels above the avatar's are never part of the model's view, so they are cut away.
pub fn build(
    model: &(impl GameModel + ?Sized),
    tileset: &Tileset,
    pages: Option<&Pages>,
    season: Option<&str>,
    version: u64,
) -> Scene {
    let m = metrics(tileset);
    let size = model.size();
    let (width, height) = (size.x.max(0), size.y.max(0));
    let mut columns = vec![None; (width * height) as usize];
    let mut items = vec![];
    let mut squares = 0;
    for y in 0..height {
        for x in 0..width {
            let cell = model.cell(Pos { x, y });
            if cell.layers.is_empty() {
                continue;
            }
            squares += 1;
            // The same filters the flat map puts on what is remembered or poorly lit.
            let filter = match (cell.remembered, cell.light) {
                (true, _) => 2,
                (false, Some(light)) if light <= 1 => 1,
                _ => 0,
            };
            let haze = match cell.depth {
                0 => 0,
                depth => (24 * (u32::from(depth) + 1)).min(192) as u8,
            };
            let floor = -f32::from(cell.depth) * m.level;
            let mut column = Column {
                ground: floor,
                solid: None,
            };
            let mut order = 0;
            for layer in &cell.layers {
                let (layer, paints) = resolve_layer(tileset, layer);
                let Some(chosen) = tileset.choose(&query(&layer, season)) else {
                    continue;
                };
                let kind = match (layer.terrain, chosen.joined) {
                    (false, _) => Kind::Thing,
                    (true, false) => Kind::Terrain,
                    (true, true) => Kind::Joined,
                };
                for (placed, paint) in [(chosen.bg, paints.0), (chosen.fg, paints.1)] {
                    let Some(placed) = placed else {
                        continue;
                    };
                    let Some(mut sprite) = source(tileset, placed.sprite) else {
                        continue;
                    };
                    // Painted the way the flat map paints it, once the pages it is cut from are decoded.
                    if let (Some(paint), Some(pages)) = (paint, pages) {
                        let mask = placed.mask.and_then(|m| source(tileset, m));
                        sprite = pages
                            .painted(&sprite, mask.as_ref(), paint)
                            .unwrap_or(sprite);
                    }
                    if let Shape::Box { top, bottom } = shape(&m, &sprite, kind) {
                        column.solid = Some((floor + bottom, floor + top));
                    }
                    items.push(Item {
                        square: (x, y),
                        floor,
                        kind,
                        turn: placed.turn,
                        sprite,
                        filter,
                        haze,
                        order,
                    });
                    order += 1;
                }
            }
            columns[(y * width + x) as usize] = Some(column);
        }
    }
    let solid_at = |x: i32, y: i32| {
        let inside = (0..width).contains(&x) && (0..height).contains(&y);
        inside
            .then(|| columns[(y * width + x) as usize]?.solid)
            .flatten()
    };
    let mut pages = BTreeMap::<PageId, Vec<Instance>>::new();
    for item in &items {
        // A face is not seen where the next square holds a box at least as tall, down to at least as low.
        let covered = match shape(&m, &item.sprite, item.kind) {
            Shape::Box { top, bottom } => FACES.map(|(dx, dy)| {
                solid_at(item.square.0 + dx, item.square.1 + dy).is_some_and(|(low, high)| {
                    high >= item.floor + top - 1e-3 && low <= item.floor + bottom + 1e-3
                })
            }),
            _ => [false; 4],
        };
        let placement = Placement {
            square: item.square,
            floor: item.floor,
            kind: item.kind,
            turn: item.turn,
            sprite: &item.sprite,
            filter: item.filter,
            haze: item.haze,
            order: item.order,
            covered,
        };
        pages
            .entry(item.sprite.page)
            .or_default()
            .extend(place(&m, &placement));
    }
    Scene {
        version,
        batches: pages
            .into_iter()
            .map(|(page, instances)| Batch { page, instances })
            .collect(),
        squares,
        width,
        height,
        columns,
    }
}
