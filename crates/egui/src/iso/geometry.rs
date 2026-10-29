//! What a sprite becomes in the 3D scene: the ground under a square, a box for a wall, a solid of its outline, or an
//! upright board.
//! Every shape is a parallelogram `origin + u * a + v * b` with an affine map from `(u, v)` to sprite pixels, so
//! one instance format draws all of them.

mod outline;
mod placement;
mod quad;
mod solid;
#[cfg(test)]
mod tests;

pub use outline::*;
pub use placement::*;
pub use quad::*;
pub use solid::*;

use crate::tileset::Turn;
use bytemuck::{Pod, Zeroable};
use glam::{Mat2, Vec2};

/// A texture the scene draws from: one strip of rows of a tileset sheet.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PageId {
    pub sheet: u16,
    pub strip: u16,
}

/// Where a sprite is and how the tileset places it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpriteSource {
    pub page: PageId,
    /// The sprite's top left corner in the page's pixels.
    pub at: Vec2,
    /// The sprite's size in the sheet's pixels.
    pub src: Vec2,
    /// Where the sprite's top left corner lies from the top left corner of its square's tile.
    pub offset: Vec2,
    /// How many screen pixels one sheet pixel covers.
    pub scale: f32,
}

impl SpriteSource {
    /// The size the sprite is drawn at, in tile pixels.
    pub fn size(&self) -> Vec2 {
        self.src * self.scale
    }

    /// The uv of a quad showing `w` of the sprite's width and `h` of its height from its top left corner.
    fn uv(&self, w: f32, h: f32) -> [Vec2; 3] {
        [
            self.at,
            Vec2::new(self.src.x * w, 0.0),
            Vec2::new(0.0, self.src.y * h),
        ]
    }
}

/// How tile pixels turn into world units. A square is one unit wide.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Metrics {
    /// A tile in pixels. The diamond of an isometric tileset is as wide as the tile and, whatever the tile's height,
    /// half as tall: the sprite's own height only says how far it reaches above its square.
    pub tile: Vec2,
    pub iso: bool,
    /// The height of the square's diamond in pixels: half its width for an isometric tileset, else the tile's height.
    pub diamond: f32,
    /// World units of one screen pixel across, and of one screen pixel up at the tileset's own tilt.
    pub across: f32,
    pub up: f32,
    /// The height of one z-level in world units.
    pub level: f32,
}

/// Without a tileset's own height above zero, a z-level is this many squares tall.
const LEVEL: f32 = 3.0;
/// Sprites of one square are drawn this far apart toward the eye, so the later one wins at equal depth.
const LAYER_BIAS: f32 = 0.002;
/// A vehicle part of a tileset drawn from above stands this many squares tall: a person in it shows from the chest.
const VEHICLE: f32 = 0.5;
/// An item lying on the ground of a tileset drawn from above is this many squares thick, a pixel of a block game's
/// sixteen-pixel item: the sprite's opaque pixels pushed up that far.
const ITEM: f32 = 1.0 / 16.0;
/// The blocks a wall of a tileset drawn from above stacks to a level: a storey is 3 m, a block 1 m.
const LEVEL_BLOCKS: f32 = 3.0;
/// The sill and the lintel of a window of a tileset drawn from above, as shares of the level its wall rises: a
/// block each.
const SILL: f32 = 1.0 / LEVEL_BLOCKS;
const LINTEL: f32 = 1.0 / LEVEL_BLOCKS;
/// A door of a tileset drawn from above is this many blocks tall, its wall over it.
const DOOR_BLOCKS: f32 = 2.0;

impl Metrics {
    pub fn new(tile: (u32, u32), iso: bool, zlevel_height: Option<f32>) -> Self {
        let tile = Vec2::new(tile.0.max(1) as f32, tile.1.max(1) as f32);
        let diamond = if iso { tile.x / 2.0 } else { tile.y };
        // A diamond `tile.x` wide and `diamond` tall is a square seen from a height of asin(diamond / tile.x); a
        // vertical pixel then covers cos of that less of a world unit than a pixel across the diagonal.
        let (across, up) = match iso {
            true => {
                let across = std::f32::consts::SQRT_2 / tile.x;
                (across, across / (diamond / tile.x).asin().cos())
            }
            false => (1.0 / tile.x, 1.0 / tile.y),
        };
        Self {
            tile,
            iso,
            diamond,
            across,
            up,
            // UltimateCataclysm gives 0: no height.
            level: zlevel_height.filter(|h| *h > 0.0).map_or(LEVEL, |h| h * up),
        }
    }

    /// The size of a block of a tileset drawn from above.
    fn block(&self) -> f32 {
        self.level / LEVEL_BLOCKS
    }
}

/// What a sprite is drawn as.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Shape {
    /// The ground of a square at a height.
    Floor { z: f32 },
    /// A box filling the square, between two heights.
    Box { top: f32, bottom: f32 },
    /// The sprite of a tileset drawn from above pushed up from the ground to a height, as a block game makes an item
    /// of its sprite: the sprite on top and at the bottom, and its edge pixels drawn down the sides of its outline.
    Extrude { top: f32 },
    /// A window: its sprite pushed up from the ground to its sill, and again from under its lintel to its top.
    Window { sill: f32, lintel: f32, top: f32 },
    /// An upright board that faces the eye.
    Board,
    /// Blocks of a sprite drawn from above stacked from the ground to a height, as a block game stacks them: each
    /// face of each block the whole sprite.
    Blocks { top: f32 },
    /// A thing the engine names the form of, built from blocks, slabs and crossed boards in a block's size.
    Built { form: Form, block: f32 },
}

impl Shape {
    /// The bottom and the top of what a solid shape fills, over its square's ground.
    pub fn solid(&self) -> Option<(f32, f32)> {
        match *self {
            Shape::Box { top, bottom } => Some((bottom, top)),
            Shape::Extrude { top } | Shape::Window { top, .. } | Shape::Blocks { top } => {
                Some((0.0, top))
            }
            Shape::Built { form, block } => match form {
                Form::Door { .. } => Some((0.0, LEVEL_BLOCKS * block)),
                Form::Fence | Form::Plant | Form::Fire | Form::Gas => None,
                Form::Seat | Form::Bed | Form::Table => Some((0.0, block * 0.8)),
            },
            Shape::Floor { .. } | Shape::Board => None,
        }
    }
}

/// What a sprite is the picture of.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// Anything that is not a square's terrain, a creature or a vehicle part.
    Thing,
    /// An item lying on the ground.
    Item,
    /// A monster, a person or the avatar.
    Creature,
    /// A part of a vehicle.
    Vehicle,
    /// The terrain of a square.
    Terrain,
    /// Terrain the engine joined to its neighbours, as walls are: the tileset picked the variant for how they meet.
    Joined,
    /// Terrain the engine says is a wall, and whether it joined it to its neighbours.
    Wall { joined: bool },
    /// Terrain the engine says is a window, as a tileset drawn from above shows it.
    Window,
    /// Terrain that is not a wall, or anything else that is not a creature or a vehicle part, as a tileset drawn from
    /// above shows it, covering the given percent of a person.
    Standing { coverage: u8 },
    /// What the engine says stands in a form of its own, as a tileset drawn from above shows it.
    Formed(Form),
}

/// What a thing the engine names the form of is built as from a sprite drawn from above.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Form {
    /// A door, swung open into its square or shut in the line of the walls.
    Door {
        open: bool,
    },
    Fence,
    Plant,
    Seat,
    Table,
    Bed,
    Fire,
    Gas,
}

/// Whether `sprite` draws inside its tile.
fn fits(m: &Metrics, sprite: &SpriteSource) -> bool {
    sprite.offset.cmpgt(Vec2::splat(-0.5)).all()
        && (sprite.offset + sprite.size()).cmplt(m.tile + 0.5).all()
}

/// What `sprite` is drawn as. Terrain whose sprite is as wide as the tile is the ground when it hardly rises over
/// its diamond; joined terrain that rises is a box, as a wall; anything else, a shrub or a mound as well, stands
/// upright. A tileset drawn from above draws what stays inside its square from above too, so that lies on the
/// ground, but for a creature, and a wall, a vehicle part or what covers some of a person is pushed up from it, a
/// window with its opening left out; what reaches out of its square, a tree, was drawn standing. An isometric tileset
/// draws a wall's height itself: only the joins tell.
pub fn shape(m: &Metrics, sprite: &SpriteSource, kind: Kind) -> Shape {
    let size = sprite.size();
    if !m.iso {
        let inside = fits(m, sprite);
        let block = m.block();
        return match (kind, inside) {
            (Kind::Formed(form), _) => Shape::Built { form, block },
            // A creature stands, as a block game's figure does.
            (Kind::Creature, _) | (_, false) => Shape::Board,
            (Kind::Item, true) => Shape::Extrude { top: ITEM },
            (Kind::Wall { .. }, true) => Shape::Blocks { top: m.level },
            (Kind::Window, true) => Shape::Window {
                sill: SILL * m.level,
                lintel: (1.0 - LINTEL) * m.level,
                top: m.level,
            },
            (Kind::Vehicle, true) => Shape::Extrude { top: VEHICLE },
            // A person is two blocks tall.
            (Kind::Standing { coverage }, true) => Shape::Blocks {
                top: 2.0 * block * f32::from(coverage.min(100)) / 100.0,
            },
            _ => Shape::Floor { z: 0.0 },
        };
    }
    let kind = match kind {
        Kind::Wall { joined: true } => Kind::Joined,
        Kind::Wall { joined: false } => Kind::Terrain,
        kind => kind,
    };
    let rise = size.y - m.diamond;
    // The sprite's bottom edge, over the baseline of its tile.
    let bottom = (m.tile.y - sprite.offset.y - size.y) * m.up;
    let half = m.diamond / 2.0;
    match (kind, (size.x - m.tile.x).abs() < 0.5) {
        (Kind::Joined, true) if rise > half => Shape::Box {
            top: (m.tile.y - m.diamond - sprite.offset.y) * m.up,
            bottom,
        },
        (Kind::Joined | Kind::Terrain, true) if (0.0..=half).contains(&rise) => {
            Shape::Floor { z: bottom }
        }
        _ => Shape::Board,
    }
}

/// The heights from the ground of the blocks of wall a window or a door of a tileset drawn from above stands in:
/// under the sill and over the lintel of a window drawn as its pane, over a door.
pub fn wall_bands(m: &Metrics, sprite: &SpriteSource, kind: Kind) -> Vec<(f32, f32)> {
    match shape(m, sprite, kind) {
        Shape::Window { sill, lintel, top } => vec![(0.0, sill), (lintel, top)],
        Shape::Built {
            form: Form::Door { .. },
            block,
        } => vec![(DOOR_BLOCKS * block, m.level)],
        _ => vec![],
    }
}
