//! What a sprite becomes in the 3D scene: the ground under a square, a box for a wall, a solid of its outline, or an
//! upright board.
//! Every shape is a parallelogram `origin + u * a + v * b` with an affine map from `(u, v)` to sprite pixels, so
//! one instance format draws all of them.

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
    /// An upright board that faces the eye.
    Board,
}

impl Shape {
    /// The bottom and the top of what a solid shape fills, over its square's ground.
    pub fn solid(&self) -> Option<(f32, f32)> {
        match *self {
            Shape::Box { top, bottom } => Some((bottom, top)),
            Shape::Extrude { top } => Some((0.0, top)),
            Shape::Floor { .. } | Shape::Board => None,
        }
    }
}

/// The outline of a sprite's opaque pixels, those the fragment shader draws: the straight runs of the edges
/// between an opaque pixel and a clear one or the outside of the sprite.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Outline {
    pub edges: Vec<Edge>,
}

/// A straight run of an outline in the sprite's pixels from its top left corner, from `from` to `to`, with the
/// opaque pixels on the side `inward` points to.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Edge {
    pub from: Vec2,
    pub to: Vec2,
    pub inward: Vec2,
}

impl Outline {
    /// The outline of a sprite `size` pixels big whose pixel `(x, y)` is opaque where `opaque` says so.
    pub fn of(size: [u32; 2], opaque: impl Fn(u32, u32) -> bool) -> Self {
        let [w, h] = size.map(|n| n as i32);
        let at = |x: i32, y: i32| {
            (0..w).contains(&x) && (0..h).contains(&y) && opaque(x as u32, y as u32)
        };
        // Along each line between two rows, and between two columns, the runs over which the pixels on its two
        // sides differ the same way: true where the opaque one is after the line.
        let rows = (0..=h).flat_map(|y| {
            let side = move |x| (at(x, y - 1) != at(x, y)).then(|| at(x, y));
            runs(w, side)
                .into_iter()
                .map(move |(from, to, after)| Edge {
                    from: Vec2::new(from as f32, y as f32),
                    to: Vec2::new(to as f32, y as f32),
                    inward: Vec2::new(0.0, if after { 1.0 } else { -1.0 }),
                })
        });
        let columns = (0..=w).flat_map(|x| {
            let side = move |y| (at(x - 1, y) != at(x, y)).then(|| at(x, y));
            runs(h, side)
                .into_iter()
                .map(move |(from, to, after)| Edge {
                    from: Vec2::new(x as f32, from as f32),
                    to: Vec2::new(x as f32, to as f32),
                    inward: Vec2::new(if after { 1.0 } else { -1.0 }, 0.0),
                })
        });
        Self {
            edges: rows.chain(columns).collect(),
        }
    }
}

/// The runs `from..to` of `0..len` over which `side` is some and the same, with what it is.
fn runs(len: i32, side: impl Fn(i32) -> Option<bool>) -> Vec<(i32, i32, bool)> {
    let mut found = Vec::<(i32, i32, bool)>::new();
    for n in 0..len {
        match (side(n), found.last_mut()) {
            (Some(s), Some(last)) if last.1 == n && last.2 == s => last.1 = n + 1,
            (Some(s), _) => found.push((n, n + 1, s)),
            (None, _) => {}
        }
    }
    found
}

/// What a sprite is the picture of.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// Anything that is not a square's terrain, a creature or a vehicle part.
    Thing,
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
}

/// What `sprite` is drawn as. Terrain whose sprite is as wide as the tile is the ground when it hardly rises over
/// its diamond; joined terrain that rises is a box, as a wall; anything else, a shrub or a mound as well, stands
/// upright. A tileset drawn from above draws what stays inside its square from above too, so that lies on the
/// ground, but for a creature, and a wall or a vehicle part is pushed up from it; what reaches out of its square, a
/// tree, was drawn standing. An isometric tileset draws a wall's height itself: only the joins tell.
pub fn shape(m: &Metrics, sprite: &SpriteSource, kind: Kind) -> Shape {
    let size = sprite.size();
    if !m.iso {
        let inside = sprite.offset.cmpgt(Vec2::splat(-0.5)).all()
            && (sprite.offset + size).cmplt(m.tile + 0.5).all();
        return match (kind, inside) {
            (Kind::Creature, _) | (_, false) => Shape::Board,
            (Kind::Wall { .. }, true) => Shape::Extrude { top: m.level },
            (Kind::Vehicle, true) => Shape::Extrude { top: VEHICLE },
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

/// One quad of the scene, as the shader reads it.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
pub struct Instance {
    /// The corner `(u, v) = (0, 0)`. For a board, its square's centre on the ground.
    pub origin: [f32; 3],
    /// The edge along `u`. For a board: width, its left edge from the origin across the screen, and its offset
    /// toward the eye.
    pub a: [f32; 3],
    /// The edge along `v`. For a board: height and its bottom edge over the origin.
    pub b: [f32; 3],
    /// The sprite pixel at `(0, 0)` and the pixels spanned by `u` and by `v`.
    pub uv: [f32; 2],
    pub uv_a: [f32; 2],
    pub uv_b: [f32; 2],
    /// Low byte: bit 0 for a board, bit 1 for a board at the near edge of its square; next byte: the visibility filter. Then the haze of a level seen
    /// through, 0 to 255.
    pub flags: [u32; 2],
}

/// Flag bits of an instance's first word: a board, and a board pushed to the near edge of its square.
pub const BOARD: u32 = 1;
pub const EDGE: u32 = 2;

/// How far an upright sprite of an isometric tileset stands toward the eye from the middle of its square: to the
/// square's near edge, whatever the camera's yaw. `toward` is the horizontal unit vector from target to eye; the
/// shader computes the same.
pub fn edge_push(toward: [f32; 2]) -> f32 {
    0.5 / toward[0].abs().max(toward[1].abs()).max(f32::EPSILON)
}

/// A sprite in a square.
pub struct Placement<'a> {
    pub square: (i32, i32),
    /// The height of the square's ground.
    pub floor: f32,
    pub kind: Kind,
    pub turn: Turn,
    pub sprite: &'a SpriteSource,
    /// The visibility filter: 0 plain, 1 low light, 2 remembered.
    pub filter: u32,
    pub haze: u8,
    /// The sprite's place in its square, bottom first.
    pub order: u32,
    /// For a solid: which of the south, east, north and west edges of its square another solid as tall covers whole,
    /// so that its faces there are not seen.
    pub covered: [bool; 4],
    /// For a sprite pushed up from the ground: the outline of its opaque pixels, none for the whole sprite.
    pub outline: Option<&'a Outline>,
}

/// How much of a quad's sprite span is cut off at each edge, so that a pixel centre on an edge samples the
/// sprite's own last texel and never a neighbour's in the sheet.
const INSET: f32 = 0.0002;

fn instance(
    origin: [f32; 3],
    edges: ([f32; 3], [f32; 3]),
    uv: [Vec2; 3],
    flags: [u32; 2],
) -> Instance {
    let inner = uv[0] + (uv[1] + uv[2]) * INSET;
    Instance {
        origin,
        a: edges.0,
        b: edges.1,
        uv: inner.to_array(),
        uv_a: (uv[1] * (1.0 - 2.0 * INSET)).to_array(),
        uv_b: (uv[2] * (1.0 - 2.0 * INSET)).to_array(),
        flags,
    }
}

/// The sprite corners `[top left, top right, bottom right, bottom left]` as they are drawn: each is the corner of
/// the sprite in its page that lands there under `turn`.
fn corners(turn: Turn, s: &SpriteSource) -> [Vec2; 4] {
    let [tl, tr, br, bl] = [
        s.at,
        s.at + Vec2::new(s.src.x, 0.0),
        s.at + s.src,
        s.at + Vec2::new(0.0, s.src.y),
    ];
    match turn {
        Turn::None => [tl, tr, br, bl],
        Turn::Left => [tr, br, bl, tl],
        Turn::Right => [bl, tl, tr, br],
        Turn::Half => [br, bl, tl, tr],
        Turn::Mirror => [tr, tl, bl, br],
    }
}

/// Where a sprite drawn from above from `lo` to `hi` in its tile, in tile pixels from the tile's top left corner,
/// lies once turned about its own middle as `turn` turns its picture, as the game's tiles view turns it.
fn turned(turn: Turn, lo: Vec2, hi: Vec2) -> (Vec2, Vec2) {
    let middle = (lo + hi) / 2.0;
    let take = |p: Vec2| {
        let d = p - middle;
        middle
            + match turn {
                Turn::None => d,
                Turn::Left => Vec2::new(d.y, -d.x),
                Turn::Right => Vec2::new(-d.y, d.x),
                Turn::Half => -d,
                Turn::Mirror => Vec2::new(-d.x, d.y),
            }
    };
    let (a, b) = (take(lo), take(hi));
    (a.min(b), a.max(b))
}

/// An edge of a sprite's outline where it lies in the world, from `from` to `to`.
struct Side {
    from: Vec2,
    to: Vec2,
    edge: Edge,
}

/// The edges of the outline of a sprite drawn from above, where `place` lays the sprite in its square.
fn sides(m: &Metrics, p: &Placement) -> Vec<Side> {
    let s = p.sprite;
    let (lo, hi) = turned(p.turn, s.offset, s.offset + s.size());
    let [tl, tr, _, bl] = corners(p.turn, s);
    // How far across and down the laid sprite a page pixel is.
    let across = Mat2::from_cols(tr - tl, bl - tl).inverse();
    let square = Vec2::new(p.square.0 as f32, p.square.1 as f32);
    let laid = |q: Vec2| {
        square + (lo + (hi - lo) * (across * (s.at + q - tl))) * Vec2::new(m.across, m.up)
    };
    let whole;
    let outline = match p.outline {
        Some(outline) => outline,
        None => {
            whole = Outline::of([s.src.x as u32, s.src.y as u32], |_, _| true);
            &whole
        }
    };
    outline
        .edges
        .iter()
        .map(|e| Side {
            from: laid(e.from),
            to: laid(e.to),
            edge: *e,
        })
        .collect()
}

/// Which edge of the square with its north-west corner at `square` a side lies along, by its place among the
/// south, east, north and west ones.
fn border(square: Vec2, side: &Side) -> Option<usize> {
    let (from, to) = (side.from - square, side.to - square);
    let on = |a: f32, b: f32, line: f32| (a - line).abs() < 1e-4 && (b - line).abs() < 1e-4;
    [
        on(from.y, to.y, 1.0),
        on(from.x, to.x, 1.0),
        on(from.y, to.y, 0.0),
        on(from.x, to.x, 0.0),
    ]
    .iter()
    .position(|on| *on)
}

/// Which of the south, east, north and west edges of its square a solid covers whole: all of them for a box, and
/// for a sprite pushed up from the ground those it is opaque all along.
pub fn borders(m: &Metrics, p: &Placement) -> [bool; 4] {
    match shape(m, p.sprite, p.kind) {
        Shape::Box { .. } => [true; 4],
        Shape::Extrude { .. } => {
            let square = Vec2::new(p.square.0 as f32, p.square.1 as f32);
            let mut length = [0.0; 4];
            for side in sides(m, p) {
                if let Some(n) = border(square, &side) {
                    length[n] += side.from.distance(side.to);
                }
            }
            length.map(|l| l > 1.0 - 1e-3)
        }
        Shape::Floor { .. } | Shape::Board => [false; 4],
    }
}

/// The instances that draw a sprite: one, the five faces of a box, or the faces of a solid pushed up from it.
pub fn place(m: &Metrics, p: &Placement) -> Vec<Instance> {
    let s = p.sprite;
    let size = s.size();
    let (x, y) = (p.square.0 as f32, p.square.1 as f32);
    let flags = [p.filter << 8, u32::from(p.haze)];
    // A point of the sprite given in tile pixels from its top left corner, in its page's pixels.
    let at = |px: f32, py: f32| s.at + Vec2::new(px, py) / s.scale;
    // A span of the sprite in tile pixels, in its page's pixels.
    let span = |px: f32, py: f32| Vec2::new(px, py) / s.scale;
    // An isometric tile is only ever mirrored, never turned.
    let turn = match (m.iso, p.turn) {
        (true, Turn::Mirror) => Turn::Mirror,
        (true, _) => Turn::None,
        (false, turn) => turn,
    };
    let ground = ([1.0, 0.0, 0.0], [0.0, 1.0, 0.0]);
    // A sprite drawn from above lying at a height, where the flat map draws it in its square.
    let lying = |z: f32| {
        let (lo, hi) = turned(turn, s.offset, s.offset + size);
        let [tl, tr, _, bl] = corners(turn, s);
        let (w, h) = ((hi.x - lo.x) * m.across, (hi.y - lo.y) * m.up);
        instance(
            [x + lo.x * m.across, y + lo.y * m.up, z],
            ([w, 0.0, 0.0], [0.0, h, 0.0]),
            [tl, tr - tl, bl - tl],
            flags,
        )
    };
    match shape(m, s, p.kind) {
        Shape::Floor { z } => {
            let z = p.floor + z;
            match m.iso {
                // The diamond of the tile lies in the lowest rows of an isometric sprite, drawn as the game draws
                // it: the square's north-west corner at its left corner, so its north-east corner is at the top.
                true => {
                    let (w, h, t) = (size.x, size.y, m.diamond);
                    let uv = [
                        at(0.0, h - t / 2.0),
                        span(w / 2.0, -t / 2.0),
                        span(w / 2.0, t / 2.0),
                    ];
                    vec![instance([x, y, z], ground, uv, flags)]
                }
                // Later sprites of a square lie over the earlier ones.
                false => vec![lying(z + LAYER_BIAS * p.order as f32)],
            }
        }
        Shape::Extrude { top } => {
            // The top first: where it covers what is under it, that colour is not written again.
            let bottom = p.floor + LAYER_BIAS * p.order as f32;
            let square = Vec2::new(x, y);
            let seen = sides(m, p)
                .into_iter()
                .filter(|side| border(square, side).is_none_or(|n| !p.covered[n]))
                .map(|side| {
                    let (e, run) = (side.edge, side.to - side.from);
                    // The pixels inside the edge, drawn all the way down.
                    let uv = [s.at + e.from + e.inward / 2.0, e.to - e.from, Vec2::ZERO];
                    let edges = ([run.x, run.y, 0.0], [0.0, 0.0, -top]);
                    instance([side.from.x, side.from.y, bottom + top], edges, uv, flags)
                });
            std::iter::once(lying(bottom + top))
                .chain(seen)
                .chain(std::iter::once(lying(bottom)))
                .collect()
        }
        Shape::Box { top, bottom } => {
            let (w, h, t) = (size.x, size.y, m.diamond);
            let (top, drop) = (p.floor + top, top - bottom);
            let down = [0.0, 0.0, -drop];
            // The sprite shows the west face on its left and the south face on its right; the others reuse them,
            // seen from behind.
            let west = [at(0.0, t / 2.0), span(w / 2.0, t / 2.0), span(0.0, h - t)];
            let south = [at(w / 2.0, t), span(w / 2.0, -t / 2.0), span(0.0, h - t)];
            let lid = [
                at(0.0, t / 2.0),
                span(w / 2.0, -t / 2.0),
                span(w / 2.0, t / 2.0),
            ];
            let faces = [
                ([x, y + 1.0, top], ([1.0, 0.0, 0.0], down), south),
                ([x + 1.0, y + 1.0, top], ([0.0, -1.0, 0.0], down), west),
                ([x + 1.0, y, top], ([-1.0, 0.0, 0.0], down), south),
                ([x, y, top], ([0.0, 1.0, 0.0], down), west),
            ];
            let seen = faces
                .into_iter()
                .zip(p.covered)
                .filter(|(_, covered)| !covered)
                .map(|((origin, edges, uv), _)| instance(origin, edges, uv, flags));
            std::iter::once(instance([x, y, top], ground, lid, flags))
                .chain(seen)
                .collect()
        }
        Shape::Board => {
            let [tl, _, br, bl] = corners(turn, s);
            // The sprite's bottom edge rests on the ground where its tile's bottom edge would.
            let left = (s.offset.x - m.tile.x / 2.0) * m.across;
            let rest = (m.tile.y - (s.offset.y + size.y)) * m.up;
            let bias = LAYER_BIAS * p.order as f32;
            let edges = ([size.x * m.across, left, bias], [size.y * m.up, rest, 0.0]);
            // From the bottom left corner: along the width to the right, and up.
            let flags = [flags[0] | BOARD | if m.iso { EDGE } else { 0 }, flags[1]];
            vec![instance(
                [x + 0.5, y + 0.5, p.floor],
                edges,
                [bl, br - bl, tl - bl],
                flags,
            )]
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::iso::camera::{Camera, Viewport, YAW_ISO};
    use glam::Vec3;

    const PAGE: PageId = PageId { sheet: 0, strip: 0 };

    fn ultica() -> Metrics {
        Metrics::new((48, 24), true, Some(108.0))
    }

    /// A sprite of the Ultica_iso sheets: `size` pixels at `(at)` in its page, placed `offset` from its tile.
    fn sprite(size: (f32, f32), offset: (f32, f32)) -> SpriteSource {
        SpriteSource {
            page: PAGE,
            at: Vec2::new(96.0, 240.0),
            src: Vec2::new(size.0, size.1),
            offset: Vec2::new(offset.0, offset.1),
            scale: 1.0,
        }
    }

    fn square(s: &SpriteSource, kind: Kind) -> Placement<'_> {
        Placement {
            square: (3, 5),
            floor: 0.0,
            kind,
            turn: Turn::None,
            sprite: s,
            filter: 0,
            haze: 0,
            order: 0,
            covered: [false; 4],
            outline: None,
        }
    }

    /// Two page points are the same up to the sliver the quads cut off their edges.
    fn near(a: Vec2, b: Vec2) -> bool {
        (a - b).length() < 0.15
    }

    #[test]
    fn a_pixel_covers_the_world_the_tilt_of_the_tileset_implies() {
        let m = ultica();
        // 48 pixels across one diagonal of a square; 29.4 pixels up for one unit at 30 degrees.
        assert!((m.across - std::f32::consts::SQRT_2 / 48.0).abs() < 1e-6);
        assert!((1.0 / m.up - 29.394).abs() < 1e-2, "{}", 1.0 / m.up);
        // A z-level of 108 pixels is 3.67 squares, about the 3.6 the tileset's author counts.
        assert!((m.level - 3.674).abs() < 1e-2, "{}", m.level);
        let flat = Metrics::new((16, 16), false, None);
        assert_eq!(
            (flat.across, flat.up, flat.level),
            (1.0 / 16.0, 1.0 / 16.0, 3.0)
        );
    }

    #[test]
    fn a_level_height_of_zero_as_in_ultimatecataclysm_is_no_height_given() {
        let given = Metrics::new((32, 32), false, Some(0.0));
        assert_eq!(given.level, Metrics::new((32, 32), false, None).level);
        assert_eq!(given.level, LEVEL);
    }

    #[test]
    fn the_diamond_of_an_isometric_tile_is_half_as_tall_as_wide_however_tall_the_tile_is() {
        // HitButton_iso: 16 x 20 tiles whose diamond is 16 x 8, as the game draws it.
        let m = Metrics::new((16, 20), true, None);
        assert_eq!(m.diamond, 8.0);
        let expected = std::f32::consts::SQRT_2 / 16.0 / 30f32.to_radians().cos();
        assert!((m.up - expected).abs() < 1e-6, "{}", m.up);
        assert_eq!(Metrics::new((16, 16), false, None).diamond, 16.0);
    }

    /// A sprite of HitButton_iso: 16 x 20 pixels from the top left of the tile.
    fn hit_button(size: (f32, f32), offset: (f32, f32)) -> (Metrics, SpriteSource) {
        (Metrics::new((16, 20), true, None), sprite(size, offset))
    }

    #[test]
    fn a_floor_whose_sprite_is_as_tall_as_the_tile_takes_its_diamond_from_the_bottom_rows() {
        // Sprite rows 12 to 19 hold the diamond; the transparent rows above are trimmed off.
        let (m, s) = hit_button((16.0, 8.0), (0.0, 12.0));
        assert!(matches!(shape(&m, &s, Kind::Terrain), Shape::Floor { z } if z.abs() < 1e-6));
        let [floor] = place(&m, &square(&s, Kind::Terrain))[..] else {
            panic!("one quad")
        };
        let corner = |u: f32, v: f32| {
            Vec2::from(floor.uv) + Vec2::from(floor.uv_a) * u + Vec2::from(floor.uv_b) * v
        };
        let at = Vec2::new(96.0, 240.0);
        assert!(near(corner(0.0, 0.0), at + Vec2::new(0.0, 4.0)));
        assert!(near(corner(1.0, 0.0), at + Vec2::new(8.0, 0.0)));
        assert!(near(corner(1.0, 1.0), at + Vec2::new(16.0, 4.0)));
        assert!(near(corner(0.0, 1.0), at + Vec2::new(8.0, 8.0)));
        // Left untrimmed it is a box: the sprite alone does not say there is nothing in the top rows.
        let (m, s) = hit_button((16.0, 20.0), (0.0, 0.0));
        assert!(matches!(shape(&m, &s, Kind::Joined), Shape::Box { .. }));
        assert!(matches!(shape(&m, &s, Kind::Terrain), Shape::Board));
    }

    #[test]
    fn a_wall_as_tall_as_the_tile_rises_by_what_is_over_its_diamond() {
        let (m, s) = hit_button((16.0, 20.0), (0.0, 0.0));
        // Rows 0 to 7 are the lid, rows 12 to 19 the foot of the faces: 12 pixels from the ground to the lid.
        assert_eq!(
            shape(&m, &s, Kind::Joined),
            Shape::Box {
                top: 12.0 * m.up,
                bottom: 0.0
            }
        );
        let faces = place(&m, &square(&s, Kind::Joined));
        assert_eq!(faces.len(), 5);
        let at = Vec2::new(96.0, 240.0);
        let corner = |i: &Instance, u: f32, v: f32| {
            Vec2::from(i.uv) + Vec2::from(i.uv_a) * u + Vec2::from(i.uv_b) * v - at
        };
        assert!(near(corner(&faces[0], 0.0, 0.0), Vec2::new(0.0, 4.0)));
        assert!(near(corner(&faces[0], 1.0, 1.0), Vec2::new(16.0, 4.0)));
        // The south face, on the right of the sprite, from its bottom corner to its right corner.
        assert!(near(corner(&faces[1], 0.0, 0.0), Vec2::new(8.0, 8.0)));
        assert!(near(corner(&faces[1], 1.0, 1.0), Vec2::new(16.0, 16.0)));
        assert_eq!(faces[1].origin[2], 12.0 * m.up);
        assert!((faces[1].b[2] + 12.0 * m.up).abs() < 1e-5);
    }

    #[test]
    fn terrain_is_ground_or_a_box_by_how_far_its_sprite_rises_over_its_diamond() {
        let m = ultica();
        let ground = sprite((48.0, 36.0), (0.0, -12.0));
        assert!(matches!(shape(&m, &ground, Kind::Terrain), Shape::Floor { z } if z.abs() < 1e-6));
        assert!(matches!(
            shape(&m, &ground, Kind::Joined),
            Shape::Floor { .. }
        ));
        let wall = sprite((48.0, 120.0), (0.0, -96.0));
        assert_eq!(
            shape(&m, &wall, Kind::Joined),
            Shape::Box {
                top: 96.0 * m.up,
                bottom: 0.0
            }
        );
        // A wall whose sprite reaches below its square's baseline goes down there.
        let deep = sprite((48.0, 132.0), (0.0, -96.0));
        assert_eq!(
            shape(&m, &deep, Kind::Joined),
            Shape::Box {
                top: 96.0 * m.up,
                bottom: -12.0 * m.up
            }
        );
        // Tall terrain the engine did not join to its neighbours, a shrub or a mound, stands upright.
        assert_eq!(shape(&m, &wall, Kind::Terrain), Shape::Board);
        // Not terrain, or not as wide as the tile: it stands upright.
        assert_eq!(shape(&m, &wall, Kind::Thing), Shape::Board);
        assert_eq!(
            shape(&m, &sprite((200.0, 240.0), (-76.0, -216.0)), Kind::Joined),
            Shape::Board
        );
        // A sprite lower than its diamond has no ground to cut from it.
        assert_eq!(
            shape(&m, &sprite((48.0, 12.0), (0.0, 0.0)), Kind::Terrain),
            Shape::Board
        );
    }

    #[test]
    fn a_sprite_drawn_from_above_lies_flat_inside_its_square_and_stands_when_it_reaches_out() {
        let flat = Metrics::new((32, 32), false, None);
        let whole = sprite((32.0, 32.0), (0.0, 0.0));
        let small = sprite((20.0, 20.0), (6.0, 0.0));
        let tall = sprite((32.0, 64.0), (0.0, -32.0));
        // One pixel out of the square.
        let wide = sprite((33.0, 32.0), (0.0, 0.0));
        for kind in [Kind::Terrain, Kind::Joined, Kind::Thing] {
            assert_eq!(shape(&flat, &whole, kind), Shape::Floor { z: 0.0 });
            assert_eq!(shape(&flat, &small, kind), Shape::Floor { z: 0.0 });
            assert_eq!(shape(&flat, &tall, kind), Shape::Board);
            assert_eq!(shape(&flat, &wide, kind), Shape::Board);
        }
        // A creature stands however small it is.
        assert_eq!(shape(&flat, &small, Kind::Creature), Shape::Board);
    }

    #[test]
    fn a_wall_or_a_vehicle_part_drawn_from_above_is_pushed_up_from_its_sprite_where_an_isometric_wall_goes_by_its_joins()
     {
        let flat = Metrics::new((32, 32), false, None);
        let whole = sprite((32.0, 32.0), (0.0, 0.0));
        let wall = Kind::Wall { joined: false };
        assert_eq!(shape(&flat, &whole, wall), Shape::Extrude { top: 3.0 });
        assert_eq!(
            shape(&flat, &whole, Kind::Vehicle),
            Shape::Extrude { top: VEHICLE }
        );
        // A wall sprite that reaches out of its square was drawn standing.
        let tall = sprite((32.0, 64.0), (0.0, -32.0));
        assert_eq!(shape(&flat, &tall, wall), Shape::Board);
        // The top comes first and the bottom lies on the ground: four sides between them, however tall it is.
        let p = Placement {
            floor: 1.0,
            ..square(&whole, wall)
        };
        let layers = place(&flat, &p);
        assert_eq!(layers.len(), 6);
        assert_eq!(layers[0].origin, [3.0, 5.0, 4.0]);
        assert_eq!(layers[layers.len() - 1].origin, [3.0, 5.0, 1.0]);
        // An isometric wall is a box only where the tileset joined it: a lone rock stands.
        let m = ultica();
        let iso = sprite((48.0, 120.0), (0.0, -96.0));
        assert!(matches!(
            shape(&m, &iso, Kind::Wall { joined: true }),
            Shape::Box { .. }
        ));
        assert_eq!(shape(&m, &iso, wall), Shape::Board);
        assert_eq!(shape(&m, &iso, Kind::Vehicle), Shape::Board);
    }

    #[test]
    fn an_outline_is_one_run_per_straight_edge_and_rings_a_hole_too() {
        let count = |opaque: fn(u32, u32) -> bool| Outline::of([32, 32], opaque).edges.len();
        assert_eq!(count(|_, _| true), 4);
        assert_eq!(count(|_, _| false), 0);
        // An L of the top and left halves: six straight edges.
        assert_eq!(count(|x, y| x < 16 || y < 16), 6);
        // A hole of 4 x 4 pixels in the middle: four more.
        let holed = |x: u32, y: u32| !(14..18).contains(&x) || !(14..18).contains(&y);
        assert_eq!(count(holed), 8);
        let hole = Outline::of([32, 32], holed);
        // Round the hole the opaque pixels lie away from it.
        let left = hole
            .edges
            .iter()
            .find(|e| e.from.x == 14.0 && e.to.x == 14.0)
            .unwrap();
        assert_eq!(
            (left.from.y, left.to.y, left.inward),
            (14.0, 18.0, Vec2::new(-1.0, 0.0))
        );
    }

    #[test]
    fn a_side_of_a_turned_sprite_drawn_from_above_shows_the_edge_its_turn_brings_there() {
        let flat = Metrics::new((32, 32), false, None);
        let whole = sprite((32.0, 32.0), (0.0, 0.0));
        let mut p = square(&whole, Kind::Wall { joined: false });
        p.turn = Turn::Left;
        let faces = place(&flat, &p);
        assert_eq!(faces.len(), 6);
        // Turned counter-clockwise, the sprite's left column lies along the south edge of the square, (3, 6) to
        // (4, 6), its top at the west end.
        let south = faces
            .iter()
            .find(|i| i.b[2] < 0.0 && i.origin[1] == 6.0 && i.a[1] == 0.0)
            .unwrap();
        for u in [0.0, 1.0] {
            let x = south.origin[0] + south.a[0] * u - 3.0;
            let page = Vec2::from(south.uv) + Vec2::from(south.uv_a) * u;
            assert!(
                near(page, Vec2::new(96.5, 240.0 + x * 32.0)),
                "{page} at {x}"
            );
        }
        // A side covered by a solid beside it is left out.
        p.covered = [true, false, false, false];
        let faces = place(&flat, &p);
        assert_eq!(faces.len(), 5);
        assert!(
            !faces
                .iter()
                .any(|i| i.b[2] < 0.0 && i.origin[1] == 6.0 && i.a[1] == 0.0)
        );
        assert_eq!(borders(&flat, &p), [true; 4]);
    }

    #[test]
    fn a_solid_drawn_from_above_covers_only_the_edges_of_its_square_it_is_opaque_all_along() {
        let flat = Metrics::new((32, 32), false, None);
        // The top half of the square, cut to its opaque pixels.
        let half = sprite((32.0, 16.0), (0.0, 0.0));
        let p = square(&half, Kind::Wall { joined: false });
        assert_eq!(borders(&flat, &p), [false, false, true, false]);
        let outline = Outline::of([32, 16], |x, _| x < 31);
        let p = Placement {
            outline: Some(&outline),
            ..p
        };
        assert_eq!(borders(&flat, &p), [false; 4]);
        assert_eq!(borders(&flat, &square(&half, Kind::Thing)), [false; 4]);
    }

    #[test]
    fn a_sprite_drawn_from_above_lies_where_the_flat_map_draws_it_and_turns_about_its_own_middle() {
        let flat = Metrics::new((32, 32), false, None);
        // 20 x 10 pixels, 4 from the left and 2 from the top of its tile.
        let s = sprite((20.0, 10.0), (4.0, 2.0));
        let lies = |turn| {
            let mut p = square(&s, Kind::Thing);
            p.turn = turn;
            place(&flat, &p)[0]
        };
        let plain = lies(Turn::None);
        assert_eq!(plain.origin, [3.0 + 4.0 / 32.0, 5.0 + 2.0 / 32.0, 0.0]);
        assert_eq!(
            (plain.a, plain.b),
            ([20.0 / 32.0, 0.0, 0.0], [0.0, 10.0 / 32.0, 0.0])
        );
        // Turned counter-clockwise about its own middle, (14, 7): 10 wide from 9 and 20 tall from -3.
        let left = lies(Turn::Left);
        assert_eq!(left.origin, [3.0 + 9.0 / 32.0, 5.0 - 3.0 / 32.0, 0.0]);
        assert_eq!((left.a[0], left.b[1]), (10.0 / 32.0, 20.0 / 32.0));
        // The sprite's top right corner is drawn at the top left.
        assert!(near(Vec2::from(left.uv), s.at + Vec2::new(20.0, 0.0)));
    }

    #[test]
    fn ground_maps_the_diamond_of_the_sprite_onto_the_square() {
        let m = ultica();
        let s = sprite((48.0, 36.0), (0.0, -12.0));
        let [floor] = place(&m, &square(&s, Kind::Terrain))[..] else {
            panic!("one quad")
        };
        assert_eq!(floor.origin, [3.0, 5.0, 0.0]);
        // The diamond's corners in the page, from the lowest 24 rows: the north-west corner is the left one, the
        // north-east the top one, the south-east the right one and the south-west the bottom one.
        let corner = |u: f32, v: f32| {
            Vec2::from(floor.uv) + Vec2::from(floor.uv_a) * u + Vec2::from(floor.uv_b) * v
        };
        let at = Vec2::new(96.0, 240.0);
        assert!(near(corner(0.0, 0.0), at + Vec2::new(0.0, 24.0)));
        assert!(near(corner(1.0, 0.0), at + Vec2::new(24.0, 12.0)));
        assert!(near(corner(1.0, 1.0), at + Vec2::new(48.0, 24.0)));
        assert!(near(corner(0.0, 1.0), at + Vec2::new(24.0, 36.0)));
    }

    #[test]
    fn a_box_takes_its_top_from_the_top_diamond_and_its_sides_from_the_left_and_right_faces() {
        let m = ultica();
        let s = sprite((48.0, 120.0), (0.0, -96.0));
        let faces = place(&m, &square(&s, Kind::Joined));
        assert_eq!(faces.len(), 5);
        let at = Vec2::new(96.0, 240.0);
        let corner = |i: &Instance, u: f32, v: f32| {
            Vec2::from(i.uv) + Vec2::from(i.uv_a) * u + Vec2::from(i.uv_b) * v - at
        };
        let top = 96.0 * m.up;
        // Lid: the top diamond, rows 0 to 24, its north-west corner at the left.
        assert_eq!(faces[0].origin, [3.0, 5.0, top]);
        assert!(near(corner(&faces[0], 0.0, 0.0), Vec2::new(0.0, 12.0)));
        assert!(near(corner(&faces[0], 1.0, 0.0), Vec2::new(24.0, 0.0)));
        assert!(near(corner(&faces[0], 1.0, 1.0), Vec2::new(48.0, 12.0)));
        // South face: the right half of the faces, from the south corner to the east corner.
        assert_eq!(faces[1].origin, [3.0, 6.0, top]);
        assert!(near(corner(&faces[1], 0.0, 0.0), Vec2::new(24.0, 24.0)));
        assert!(near(corner(&faces[1], 1.0, 0.0), Vec2::new(48.0, 12.0)));
        assert!(near(corner(&faces[1], 1.0, 1.0), Vec2::new(48.0, 108.0)));
        // East face, behind: the left half, as the west face has it.
        assert_eq!(faces[2].origin, [4.0, 6.0, top]);
        assert!(near(corner(&faces[2], 0.0, 0.0), Vec2::new(0.0, 12.0)));
        assert!(near(corner(&faces[2], 1.0, 0.0), Vec2::new(24.0, 24.0)));
        assert!(near(corner(&faces[2], 1.0, 1.0), Vec2::new(24.0, 120.0)));
        // West face: the left half, from the west corner to the south corner.
        assert_eq!(faces[4].origin, [3.0, 5.0, top]);
        assert!(near(corner(&faces[4], 0.0, 0.0), Vec2::new(0.0, 12.0)));
        assert!(near(corner(&faces[4], 1.0, 0.0), Vec2::new(24.0, 24.0)));
        // Each side face drops from the top to the bottom of the box.
        assert!(faces[1..].iter().all(|f| (f.b[2] + top).abs() < 1e-5));
    }

    #[test]
    fn a_scaled_sprite_is_cut_from_the_page_by_its_own_pixels() {
        // A 24 x 18 sprite drawn at twice the size is a 48 x 36 ground.
        let m = ultica();
        let mut s = sprite((24.0, 18.0), (0.0, -12.0));
        s.scale = 2.0;
        let [floor] = place(&m, &square(&s, Kind::Terrain))[..] else {
            panic!("one quad")
        };
        assert!(near(Vec2::from(floor.uv), s.at + Vec2::new(0.0, 12.0)));
        assert!(near(Vec2::from(floor.uv_a), Vec2::new(12.0, -6.0)));
    }

    /// Where the corner `(u, v)` of an instance lies on a screen that draws the world as the tileset does
    /// (the game's yaw, the 2:1 tilt, one tile pixel per point), from the top left of the square's tile.
    fn screen(i: &Instance, m: &Metrics, u: f32, v: f32) -> Vec2 {
        let tilt = (m.diamond / m.tile.x).asin();
        let view = Viewport {
            camera: Camera {
                yaw: YAW_ISO,
                pitch: tilt,
            },
            target: Vec3::ZERO,
            scale: m.tile.x / std::f32::consts::SQRT_2,
            size: Vec2::new(1000.0, 1000.0),
        };
        let world = if i.flags[0] & BOARD != 0 {
            let (a, b) = (i.a, i.b);
            let toward = view.toward();
            let edge = if i.flags[0] & EDGE != 0 {
                edge_push([toward.x, toward.y])
            } else {
                0.0
            };
            Vec3::from(i.origin)
                + view.right() * (a[1] + u * a[0])
                + Vec3::Z * (b[1] + v * b[0])
                + toward * (a[2] + edge)
        } else {
            Vec3::from(i.origin) + Vec3::from(i.a) * u + Vec3::from(i.b) * v
        };
        view.project(world)
    }

    /// The translation that takes the page's pixels to the screen: equal at every corner of a quad drawn as the
    /// tileset's artist drew it.
    fn drawn_as_in_the_sprite(i: &Instance, m: &Metrics) {
        let shifts = [(0.0, 0.0), (1.0, 0.0), (0.0, 1.0), (1.0, 1.0)].map(|(u, v)| {
            let page = Vec2::from(i.uv) + Vec2::from(i.uv_a) * u + Vec2::from(i.uv_b) * v;
            screen(i, m, u, v) - page
        });
        for shift in shifts {
            assert!((shift - shifts[0]).length() < 0.15, "{shifts:?}");
        }
    }

    #[test]
    fn at_the_tilesets_own_view_every_quad_lands_where_its_sprite_pixels_are() {
        let m = ultica();
        let ground = sprite((48.0, 36.0), (0.0, -12.0));
        let wall = sprite((48.0, 120.0), (0.0, -96.0));
        let furniture = sprite((48.0, 72.0), (0.0, -48.0));
        let item = sprite((32.0, 32.0), (8.0, -15.0));
        let quads = [
            (&ground, Kind::Terrain),
            (&wall, Kind::Joined),
            (&furniture, Kind::Thing),
            (&item, Kind::Thing),
        ]
        .iter()
        .flat_map(|(s, kind)| place(&m, &square(s, *kind)))
        .collect::<Vec<_>>();
        assert_eq!(quads.len(), 1 + 5 + 1 + 1);
        // The east and north faces are behind the box, seen from inside.
        for quad in quads
            .iter()
            .enumerate()
            .filter(|(n, _)| ![3, 4].contains(n))
        {
            drawn_as_in_the_sprite(quad.1, &m);
        }
    }

    #[test]
    fn at_the_tilesets_own_view_a_tile_taller_than_its_diamond_lands_where_its_sprite_pixels_are() {
        // HitButton_iso: a floor trimmed to its diamond, a wall and a piece of furniture, all 16 pixels wide.
        let m = Metrics::new((16, 20), true, None);
        let ground = sprite((16.0, 8.0), (0.0, 12.0));
        let wall = sprite((16.0, 20.0), (0.0, 0.0));
        let furniture = sprite((16.0, 20.0), (0.0, 0.0));
        let quads = [
            (&ground, Kind::Terrain),
            (&wall, Kind::Joined),
            (&furniture, Kind::Thing),
        ]
        .iter()
        .flat_map(|(s, kind)| place(&m, &square(s, *kind)))
        .collect::<Vec<_>>();
        assert_eq!(quads.len(), 1 + 5 + 1);
        for quad in quads
            .iter()
            .enumerate()
            .filter(|(n, _)| ![3, 4].contains(n))
        {
            drawn_as_in_the_sprite(quad.1, &m);
        }
    }

    #[test]
    fn a_board_stands_at_the_near_edge_of_its_square_and_later_sprites_come_nearer_the_eye() {
        let m = ultica();
        let s = sprite((48.0, 72.0), (0.0, -48.0));
        let mut first = square(&s, Kind::Thing);
        let a = place(&m, &first)[0];
        first.order = 3;
        let b = place(&m, &first)[0];
        assert_eq!(a.origin, [3.5, 5.5, 0.0]);
        assert_eq!(a.flags[0] & 0xff, BOARD | EDGE);
        assert_eq!(a.a[2], 0.0);
        assert!((b.a[2] - a.a[2] - 3.0 * LAYER_BIAS).abs() < 1e-6);
        // 48 pixels wide is one diagonal of a square; 72 pixels tall at the tileset's tilt.
        assert!((a.a[0] - std::f32::consts::SQRT_2).abs() < 1e-5);
        assert!((a.b[0] - 72.0 * m.up).abs() < 1e-5);
        // Its bottom edge rests on the ground at the baseline of the tile.
        assert!(a.b[1].abs() < 1e-6);
    }

    #[test]
    fn a_turned_or_mirrored_sprite_swaps_the_corners_it_is_cut_from() {
        let flat = Metrics::new((16, 16), false, None);
        let s = SpriteSource {
            page: PAGE,
            at: Vec2::new(32.0, 16.0),
            src: Vec2::new(16.0, 16.0),
            offset: Vec2::ZERO,
            scale: 1.0,
        };
        let board = |turn| {
            let mut p = square(&s, Kind::Creature);
            p.turn = turn;
            place(&flat, &p)[0]
        };
        let bottom_left = |i: Instance| Vec2::from(i.uv);
        assert!(near(bottom_left(board(Turn::None)), Vec2::new(32.0, 32.0)));
        assert!(near(
            bottom_left(board(Turn::Mirror)),
            Vec2::new(48.0, 32.0)
        ));
        // An isometric tile is never turned, only mirrored.
        let iso = ultica();
        let mut p = square(&s, Kind::Thing);
        p.turn = Turn::Left;
        assert!(near(
            Vec2::from(place(&iso, &p)[0].uv),
            Vec2::new(32.0, 32.0)
        ));
        p.turn = Turn::Mirror;
        assert!(near(
            Vec2::from(place(&iso, &p)[0].uv),
            Vec2::new(48.0, 32.0)
        ));
    }

    #[test]
    fn a_board_never_leaves_its_square_at_any_yaw() {
        for degrees in 0..360 {
            let yaw = (degrees as f32).to_radians();
            let toward = [yaw.cos(), yaw.sin()];
            let push = edge_push(toward);
            // The point it stands at is on the square's border, not past it.
            let (x, y) = (push * toward[0], push * toward[1]);
            assert!(
                x.abs() <= 0.5 + 1e-4 && y.abs() <= 0.5 + 1e-4,
                "{degrees}: {x} {y}"
            );
            assert!(
                x.abs().max(y.abs()) >= 0.5 - 1e-4,
                "{degrees}: it reaches the border"
            );
        }
        assert!(
            (edge_push([std::f32::consts::FRAC_1_SQRT_2; 2]) - std::f32::consts::FRAC_1_SQRT_2)
                .abs()
                < 1e-6
        );
    }

    #[test]
    fn faces_that_touch_another_box_are_left_out() {
        let m = ultica();
        let s = sprite((48.0, 120.0), (0.0, -96.0));
        let mut p = square(&s, Kind::Joined);
        p.covered = [true, false, true, false];
        let faces = place(&m, &p);
        // The lid and the east and west faces: [x + 1.0, y + 1.0] and [x, y] edges.
        assert_eq!(faces.len(), 3);
        assert_eq!(faces[1].origin[0], 4.0);
        assert_eq!(faces[2].origin[0], 3.0);
        p.covered = [true; 4];
        assert_eq!(place(&m, &p).len(), 1, "only the lid");
    }
}
