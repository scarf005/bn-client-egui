//! A sprite in its square and the instances that draw it: where its picture and its outline lie, and which faces
//! a solid beside it hides.

use super::*;

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
    /// The red, green and blue coloured light adds over the sprite, times the sprite's own, as `display::light`
    /// gives them.
    pub light: [u8; 3],
    /// The sprite's place in its square, bottom first.
    pub order: u32,
    /// For a solid: which of the south, east, north and west edges of its square another solid at least as tall
    /// and as low covers whole, so that its faces there are not seen.
    pub covered: [bool; 4],
    /// For a sprite pushed up from the ground: the outline of its opaque pixels, none for the whole sprite.
    pub outline: Option<&'a Outline>,
    /// Which of the squares to the south, east, north and west stand in a line of walls: a wall, a window, a door or
    /// a fence. A door, a window pane or a fence stands along that line.
    pub joined: [bool; 4],
}

impl Placement<'_> {
    /// The north-west corner of the square.
    pub(super) fn corner(&self) -> Vec2 {
        Vec2::new(self.square.0 as f32, self.square.1 as f32)
    }

    /// How far toward the eye the sprite is drawn from the earlier ones of its square.
    fn bias(&self) -> f32 {
        LAYER_BIAS * self.order as f32
    }

    /// What every instance of the sprite carries: its filter, its haze and its coloured light.
    pub(super) fn flags(&self) -> [u32; 3] {
        let [r, g, b] = self.light;
        let light = u32::from_le_bytes([r, g, b, 0]);
        [self.filter << 8, u32::from(self.haze), light]
    }
}

/// The sprite corners `[top left, top right, bottom right, bottom left]` as they are drawn: each is the corner of
/// the sprite in its page that lands there under `turn`.
fn corners(turn: Turn, s: &SpriteSource) -> [Vec2; 4] {
    let (across, down) = (Vec2::new(s.src.x, 0.0), Vec2::new(0.0, s.src.y));
    turn.corners([s.at, s.at + across, s.at + s.src, s.at + down])
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

/// The edges of the outline of a sprite drawn from above, where `place` lays the sprite in its square; only a
/// tileset drawn from above pushes sprites up, and it turns them as `p.turn` says.
fn sides(m: &Metrics, p: &Placement) -> Vec<Side> {
    let s = p.sprite;
    let (lo, hi) = turned(p.turn, s.offset, s.offset + s.size());
    let [tl, tr, _, bl] = corners(p.turn, s);
    // How far across and down the laid sprite a page pixel is.
    let across = Mat2::from_cols(tr - tl, bl - tl).inverse();
    let square = p.corner();
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

/// Which edge of the square with its north-west corner at `square` a side lies along, to within the half pixel a
/// sprite inside its square may stand out of it, by its place among the south, east, north and west ones.
fn border(m: &Metrics, square: Vec2, side: &Side) -> Option<usize> {
    let (from, to) = (side.from - square, side.to - square);
    let near = 0.5 * m.across.max(m.up);
    let on = |a: f32, b: f32, line: f32| (a - line).abs() < near && (b - line).abs() < near;
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
        Shape::Box { .. } | Shape::Blocks { .. } => [true; 4],
        Shape::Extrude { .. } => {
            let square = p.corner();
            let mut length = [0.0; 4];
            for side in sides(m, p) {
                if let Some(n) = border(m, square, &side) {
                    length[n] += side.from.distance(side.to);
                }
            }
            length.map(|l| l > 1.0 - m.across.max(m.up))
        }
        // What is seen through the opening of a window is not hidden by it.
        Shape::Floor { .. } | Shape::Window { .. } | Shape::Board | Shape::Built { .. } => {
            [false; 4]
        }
    }
}

/// Where a point of its square, from its north-west corner, is in the page of a sprite drawn from above laid there as
/// `turn` turns its picture.
pub(super) fn page_point(m: &Metrics, s: &SpriteSource, turn: Turn, q: Vec2) -> Vec2 {
    let q = match turn {
        Turn::None => q,
        Turn::Left => Vec2::new(1.0 - q.y, q.x),
        Turn::Right => Vec2::new(q.y, 1.0 - q.x),
        Turn::Half => Vec2::ONE - q,
        Turn::Mirror => Vec2::new(1.0 - q.x, q.y),
    };
    let px = (q * m.tile - s.offset).clamp(Vec2::ZERO, s.size());
    s.at + px / s.scale
}

/// The sides of a board of `p`'s sprite: one for each edge of `outline`, running back from the board a texel of the
/// sprite, with the pixels just inside the edge. An instance's `a` is the edge's span across the screen and up and
/// the edge's start across the screen; `b[0]` is the way back from the board's step toward the eye `b[1]`; `origin`
/// is the square's centre at the edge's start height. `[bl, br, tl]` are the sprite's corners as the board draws them and `place` the board's left and bottom.
fn board_sides(
    m: &Metrics,
    p: &Placement,
    outline: &Outline,
    [bl, br, tl]: [Vec2; 3],
    flags: [u32; 3],
    [left, rest]: [f32; 2],
) -> Vec<Instance> {
    let s = p.sprite;
    let size = s.size();
    let Vec2 { x, y } = p.corner();
    let flags = marked(flags, SIDE);
    let of_page = Mat2::from_cols(br - bl, tl - bl).inverse();
    // Where a pixel of the sprite lies on the board: across from its centre and up from the ground.
    let on_board = |q: Vec2| {
        let c = of_page * (s.at + q - bl);
        Vec2::new(left + c.x * size.x * m.across, rest + c.y * size.y * m.up)
    };
    let texel = s.scale * m.across;
    let bias = p.bias();
    outline
        .edges
        .iter()
        .map(|e| {
            let (from, to) = (on_board(e.from), on_board(e.to));
            instance(
                [x + 0.5, y + 0.5, p.floor + from.y],
                ([to.x - from.x, to.y - from.y, from.x], [-texel, bias, 0.0]),
                e.inside(s.at),
                flags,
            )
        })
        .collect()
}

/// The instances that draw a sprite: one, the five faces of a box, or the faces of a solid pushed up from it.
pub fn place(m: &Metrics, p: &Placement) -> Vec<Instance> {
    let s = p.sprite;
    let size = s.size();
    let Vec2 { x, y } = p.corner();
    let flags = p.flags();
    // A point of the sprite given in tile pixels from its top left corner, in its page's pixels.
    let at = |px: f32, py: f32| s.at + Vec2::new(px, py) / s.scale;
    // A span of the sprite in tile pixels, in its page's pixels.
    let span = |px: f32, py: f32| Vec2::new(px, py) / s.scale;
    let (w, h, t) = (size.x, size.y, m.diamond);
    // The diamond of an isometric sprite whose left corner is `left` pixels down the sprite.
    let diamond = |left: f32| {
        [
            at(0.0, left),
            span(w / 2.0, -t / 2.0),
            span(w / 2.0, t / 2.0),
        ]
    };
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
    // The sprite pushed up from `low` to `high` over the ground: the top first, where it covers what is under it
    // that colour is not written again.
    let pushed = |low: f32, high: f32| -> Vec<Instance> {
        let (bottom, height) = (p.floor + p.bias() + low, high - low);
        let seen = sides(m, p)
            .into_iter()
            .filter(|side| border(m, p.corner(), side).is_none_or(|n| !p.covered[n]))
            .map(|side| {
                let (e, run) = (side.edge, side.to - side.from);
                // The pixels inside the edge, drawn all the way down.
                let uv = e.inside(s.at);
                let edges = ([run.x, run.y, 0.0], [0.0, 0.0, -height]);
                instance(
                    [side.from.x, side.from.y, bottom + height],
                    edges,
                    uv,
                    flags,
                )
            });
        std::iter::once(lying(bottom + height))
            .chain(seen)
            .chain(std::iter::once(lying(bottom)))
            .collect()
    };
    let corner = glam::Vec3::new(x, y, p.floor + p.bias());
    match shape(m, s, p.kind) {
        Shape::Blocks { top } => {
            let all = Cuboid {
                lo: glam::Vec3::ZERO,
                hi: glam::Vec3::new(1.0, 1.0, top),
                paint: Paint::Blocks,
            };
            cuboid(m, p, (corner, m.block()), &all, flags)
        }
        Shape::Built { form, block } => {
            let glow = match form {
                Form::Fire => GLOW,
                Form::Gas => SHEER,
                _ => 0,
            };
            // A door or a fence is its square's terrain: its picture lies flat under it as the flat map shows the
            // square, so nothing below shows through, over the square's earlier layers and inside its square.
            let ground = (matches!(form, Form::Door { .. } | Form::Fence) && fits(m, s))
                .then(|| lying(corner.z));
            let flags = marked(flags, glow);
            let standing = match form {
                Form::Plant | Form::Fire => crossed(corner, block, s, flags),
                _ => built(form, block, p.turn, p.joined)
                    .iter()
                    .flat_map(|c| cuboid(m, p, (corner, block), c, flags))
                    .collect(),
            };
            standing.into_iter().chain(ground).collect()
        }
        Shape::Floor { z } => {
            let z = p.floor + z;
            match m.iso {
                // The diamond of the tile lies in the lowest rows of an isometric sprite, drawn as the game draws
                // it: the square's north-west corner at its left corner, so its north-east corner is at the top.
                true => vec![instance([x, y, z], ground, diamond(h - t / 2.0), flags)],
                // Later sprites of a square lie over the earlier ones.
                false => vec![lying(z + p.bias())],
            }
        }
        Shape::Extrude { top } => pushed(0.0, top),
        // The sprite as a pane in the line of the walls, from the sill to the lintel; the blocks under and over it
        // are its wall's: see `wall_bands`.
        Shape::Window { sill, lintel, .. } => {
            let (lo, hi) = in_line(p.joined, (0.47, 0.53), (sill, lintel));
            let pane = Cuboid {
                lo,
                hi,
                paint: Paint::Blocks,
            };
            cuboid(m, p, (corner, m.block()), &pane, flags)
        }
        Shape::Box { top, bottom } => {
            let (top, drop) = (p.floor + top, top - bottom);
            let down = [0.0, 0.0, -drop];
            // The sprite shows the west face on its left and the south face on its right; the others reuse them,
            // seen from behind.
            let west = [at(0.0, t / 2.0), span(w / 2.0, t / 2.0), span(0.0, h - t)];
            let south = [at(w / 2.0, t), span(w / 2.0, -t / 2.0), span(0.0, h - t)];
            let lid = diamond(t / 2.0);
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
            let bias = p.bias();
            let edges = ([size.x * m.across, left, bias], [size.y * m.up, rest, 0.0]);
            // From the bottom left corner: along the width to the right, and up.
            let flags = marked(flags, BOARD | if m.iso { EDGE } else { 0 });
            let front = instance(
                [x + 0.5, y + 0.5, p.floor],
                edges,
                [bl, br - bl, tl - bl],
                flags,
            );
            // A creature or an item has the thickness of a texel, as a block game's item has.
            let outline = match p.kind {
                Kind::Creature | Kind::Item => p.outline,
                _ => None,
            };
            let sides = outline.map_or(vec![], |o| {
                board_sides(m, p, o, [bl, br, tl], flags, [left, rest])
            });
            std::iter::once(front).chain(sides).collect()
        }
    }
}
