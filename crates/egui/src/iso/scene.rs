//! The scene of the 3D map: what the model shows in each square, as quads grouped by the page they sample.

use super::geometry::{
    Instance, Kind, Metrics, Outline, PageId, Placement, Shape, SpriteSource, borders, place, shape,
};
use super::pages::Pages;
use crate::display::{Fx, STRIP, light, query, resolve_layer};
use crate::tileset::{Tileset, Turn};
use bn_client_core::{GameModel, Health, Layer, Pos};
use glam::Vec2;
use std::collections::BTreeMap;
use std::sync::Arc;

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
    /// The lowest ground and a height above the highest that no sprite reaches, so the light's view holds them.
    pub heights: (f32, f32),
    columns: Vec<Option<Column>>,
    /// The things the tileset has no sprite for, which stand as their glyphs as on the flat map.
    pub glyphs: Vec<Mark>,
    /// The hurt creatures, whose health bars lie along the bottom of their squares.
    pub hurt: Vec<Hurt>,
}

/// A hurt creature standing on a square.
pub struct Hurt {
    pub square: (i32, i32),
    pub floor: f32,
    pub health: Health,
}

/// A glyph standing on a square.
pub struct Mark {
    pub square: (i32, i32),
    pub floor: f32,
    pub filter: Fx,
    pub layer: Layer,
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

/// A sprite of a square, found before any is placed: the solids next to one another hide each other's faces.
struct Item {
    square: (i32, i32),
    floor: f32,
    kind: Kind,
    turn: Turn,
    sprite: SpriteSource,
    filter: u32,
    haze: u8,
    light: [u8; 3],
    order: u32,
    form: Shape,
    outline: Option<Arc<Outline>>,
}

impl Item {
    fn placement(&self, covered: [bool; 4]) -> Placement<'_> {
        Placement {
            square: self.square,
            floor: self.floor,
            kind: self.kind,
            turn: self.turn,
            sprite: &self.sprite,
            filter: self.filter,
            haze: self.haze,
            light: self.light,
            order: self.order,
            covered,
            outline: self.outline.as_deref(),
        }
    }
}

/// The directions of the faces of a solid, in the order `Placement::covered` lists them: south, east, north, west.
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

/// An isometric sprite wider than its tile cut to the tile's columns when it draws nothing outside them, as the
/// ground under a tree on a sheet of trees does: so its shape is told by what it draws, not by its sheet.
fn narrowed(m: &Metrics, pages: &Pages, sprite: SpriteSource) -> SpriteSource {
    let inside =
        |cut: &SpriteSource| cut.offset.x > -0.5 && cut.offset.x + cut.size().x < m.tile.x + 0.5;
    match pages.cropped(sprite) {
        Some(cut) if sprite.size().x > m.tile.x + 0.5 && inside(&cut) => SpriteSource {
            at: sprite.at - Vec2::new(sprite.offset.x / sprite.scale, 0.0),
            src: Vec2::new(m.tile.x / sprite.scale, sprite.src.y),
            offset: Vec2::new(0.0, sprite.offset.y),
            ..sprite
        },
        _ => sprite,
    }
}

pub fn metrics(tileset: &Tileset) -> Metrics {
    Metrics::new(tileset.tile, tileset.iso, tileset.zlevel_height)
}

/// Points per world unit when a tile is drawn `cell_px` points wide, as the flat map draws it: a diagonal of a
/// square across the tile of an isometric tileset, else a square the tile.
pub fn world_scale(tileset: &Tileset, cell_px: f32) -> f32 {
    let across = cell_px;
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
    let mut glyphs = vec![];
    let mut hurt = vec![];
    let mut squares = 0;
    for y in 0..height {
        for x in 0..width {
            let cell = model.cell(Pos { x, y });
            if cell.layers.is_empty() {
                continue;
            }
            squares += 1;
            // The same filters the flat map puts on what is remembered or poorly lit.
            let fx = match (cell.remembered, cell.light) {
                (true, _) => Fx::Memory,
                (false, Some(light)) if light <= 1 => Fx::Shadow,
                _ => Fx::Plain,
            };
            let filter = fx as u32;
            // Coloured light falls on what is plainly or dimly in sight, as on the flat map.
            let light = match (fx, cell.glow) {
                (Fx::Plain | Fx::Shadow, Some(glow)) => {
                    let [r, g, b, _] = light(glow).to_array();
                    [r, g, b]
                }
                _ => [0; 3],
            };
            let haze = match cell.depth {
                0 => 0,
                depth => (24 * (u32::from(depth) + 1)).min(192) as u8,
            };
            let floor = -f32::from(cell.depth) * m.level;
            hurt.extend(cell.health.map(|health| Hurt {
                square: (x, y),
                floor,
                health,
            }));
            let mut column = Column {
                ground: floor,
                solid: None,
            };
            let mut order = 0;
            for layer in &cell.layers {
                let (layer, paints) = resolve_layer(tileset, layer);
                let drawn = items.len();
                let chosen = tileset.choose(&query(&layer, season));
                let kind = match (layer.terrain, chosen.as_ref().is_some_and(|c| c.joined)) {
                    (false, _) if layer.creature => Kind::Creature,
                    (false, _) if layer.vehicle => Kind::Vehicle,
                    (true, joined) if layer.wall => Kind::Wall { joined },
                    // A tileset drawn from above shows what covers some of a person as tall as that.
                    _ if layer.coverage > 0 && !m.iso => Kind::Standing {
                        coverage: layer.coverage,
                    },
                    (false, _) => Kind::Thing,
                    (true, false) => Kind::Terrain,
                    (true, true) => Kind::Joined,
                };
                for (placed, paint, under) in chosen
                    .iter()
                    .flat_map(|c| [(c.bg, paints.0, c.fg.is_some()), (c.fg, paints.1, false)])
                {
                    let Some(placed) = placed else {
                        continue;
                    };
                    // The background under a wall, a vehicle part or what stands is its ground.
                    let kind = match (kind, under) {
                        (Kind::Wall { .. }, true) => Kind::Terrain,
                        (Kind::Standing { .. }, true) if layer.terrain => Kind::Terrain,
                        (Kind::Vehicle | Kind::Standing { .. }, true) => Kind::Thing,
                        (kind, _) => kind,
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
                    // An isometric sprite loses the rows over its diamond that say nothing. A sprite drawn from
                    // above is cut to what it draws, which says whether it stays in its square and lies there,
                    // and a clear one draws nothing; what stands, or is turned about the middle of its whole
                    // sprite, keeps it whole.
                    if let Some(pages) = pages {
                        sprite = match (m.iso, placed.turn) {
                            (true, _) => pages.trimmed(narrowed(&m, pages, sprite)),
                            (false, Turn::None) => match pages.cropped(sprite) {
                                None => continue,
                                Some(cut) if shape(&m, &cut, kind) != Shape::Board => cut,
                                Some(_) => sprite,
                            },
                            (false, _) => sprite,
                        };
                    }
                    let form = shape(&m, &sprite, kind);
                    if let Some((low, high)) = form.solid() {
                        column.solid = Some((floor + low, floor + high));
                    }
                    let outline = match form {
                        Shape::Extrude { .. } => pages.and_then(|p| p.outline(&sprite)),
                        _ => None,
                    };
                    items.push(Item {
                        square: (x, y),
                        floor,
                        kind,
                        turn: placed.turn,
                        sprite,
                        filter,
                        haze,
                        light,
                        order,
                        form,
                        outline,
                    });
                    order += 1;
                }
                if items.len() == drawn && layer.glyph != '\0' {
                    glyphs.push(Mark {
                        square: (x, y),
                        floor,
                        filter: fx,
                        layer: layer.into_owned(),
                    });
                }
            }
            columns[(y * width + x) as usize] = Some(column);
        }
    }
    let index = |x: i32, y: i32| {
        let inside = (0..width).contains(&x) && (0..height).contains(&y);
        inside.then(|| (y * width + x) as usize)
    };
    // Which edges of its square the solid of each square covers whole, as its column's solid is the last one.
    let mut edges = vec![[false; 4]; columns.len()];
    for item in &items {
        if item.form.solid().is_some()
            && let Some(n) = index(item.square.0, item.square.1)
        {
            edges[n] = borders(&m, &item.placement([false; 4]));
        }
    }
    let mut pages = BTreeMap::<PageId, Vec<Instance>>::new();
    for item in &items {
        // A face is not seen where the next square holds a solid at least as tall, down to at least as low, that
        // covers the whole edge between them.
        let covered = match item.form.solid() {
            Some((bottom, top)) => std::array::from_fn(|face| {
                let (dx, dy) = FACES[face];
                index(item.square.0 + dx, item.square.1 + dy).is_some_and(|n| {
                    columns[n].and_then(|c| c.solid).is_some_and(|(low, high)| {
                        high >= item.floor + top - 1e-3 && low <= item.floor + bottom + 1e-3
                    }) && edges[n][(face + 2) % 4]
                })
            }),
            None => [false; 4],
        };
        pages
            .entry(item.sprite.page)
            .or_default()
            .extend(place(&m, &item.placement(covered)));
    }
    let (low, high) = items.iter().fold((0.0f32, 0.0f32), |(low, high), i| {
        (i.floor.min(low), i.floor.max(high))
    });
    Scene {
        version,
        batches: pages
            .into_iter()
            .map(|(page, instances)| Batch { page, instances })
            .collect(),
        squares,
        width,
        height,
        heights: (low, high + 3.0 * m.level),
        columns,
        glyphs,
        hurt,
    }
}
