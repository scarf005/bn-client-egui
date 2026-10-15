//! The scene of the 3D map: what the model shows in each square, as quads grouped by the page they sample.

use super::geometry::{Instance, Metrics, PageId, Placement, SpriteSource, place};
use crate::display::{STRIP, query, resolve_layer};
use crate::tileset::Tileset;
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
}

/// Where sprite number `sprite` of the tileset is: its page and the place in it.
pub fn source(tileset: &Tileset, sprite: u32) -> Option<SpriteSource> {
    let (n, local) = tileset.locate(sprite)?;
    let sheet = &tileset.sheets[n];
    let rows = (STRIP / sheet.sprite.1).max(1);
    let (col, row) = (local % sheet.columns, local / sheet.columns);
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
    season: Option<&str>,
    version: u64,
) -> Scene {
    let m = metrics(tileset);
    let size = model.size();
    let mut pages = BTreeMap::<PageId, Vec<Instance>>::new();
    let mut squares = 0;
    for y in 0..size.y {
        for x in 0..size.x {
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
            let mut order = 0;
            for layer in &cell.layers {
                let (layer, _) = resolve_layer(tileset, layer);
                let Some(chosen) = tileset.choose(&query(&layer, season)) else {
                    continue;
                };
                for placed in [chosen.bg, chosen.fg].into_iter().flatten() {
                    let Some(sprite) = source(tileset, placed.sprite) else {
                        continue;
                    };
                    let placement = Placement {
                        square: (x, y),
                        floor: -f32::from(cell.depth) * m.level,
                        terrain: layer.terrain,
                        turn: placed.turn,
                        sprite: &sprite,
                        filter,
                        haze,
                        order,
                    };
                    pages
                        .entry(sprite.page)
                        .or_default()
                        .extend(place(&m, &placement));
                    order += 1;
                }
            }
        }
    }
    Scene {
        version,
        batches: pages
            .into_iter()
            .map(|(page, instances)| Batch { page, instances })
            .collect(),
        squares,
    }
}
