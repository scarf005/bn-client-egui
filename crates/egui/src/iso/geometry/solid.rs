//! The boxes a sprite drawn from above is built of: blocks of a wall, the parts of a thing whose form the engine
//! names, and boards crossed through a square.

use super::*;

/// A whole block of the square from `low` to `high`, painted with `p`'s sprite as a wall is.
pub fn band(m: &Metrics, p: &Placement, (low, high): (f32, f32)) -> Vec<Instance> {
    let corner = glam::Vec3::new(p.square.0 as f32, p.square.1 as f32, p.floor);
    let whole = Cuboid {
        lo: glam::Vec3::new(0.0, 0.0, low),
        hi: glam::Vec3::new(1.0, 1.0, high),
        paint: Paint::Blocks,
    };
    let flags = [p.filter << 8, u32::from(p.haze), {
        let [r, g, b] = p.light;
        u32::from_le_bytes([r, g, b, 0])
    }];
    cuboid(m, p, (corner, m.level / LEVEL_BLOCKS), &whole, flags)
}

/// A box from `lo` to `hi` in a square, of world units over its ground, and how its faces are painted.
#[derive(Clone, Copy)]
pub(super) struct Cuboid {
    pub(super) lo: glam::Vec3,
    pub(super) hi: glam::Vec3,
    pub(super) paint: Paint,
}

/// How the faces of a box show its sprite.
#[derive(Clone, Copy, PartialEq)]
pub(super) enum Paint {
    /// The whole sprite per block a face spans, as a block game paints a block.
    Blocks,
    /// The whole sprite once over each side, as a door's leaf shows its picture.
    Whole,
    /// The sprite where the box stands in the square, its sides the pixels inward from their edge.
    Cut,
}

/// The faces of `c` in the square with its north-west corner at `corner`: its top and its sides, south, east, north
/// and west, but those on an edge of the square `covered` leaves out.
pub(super) fn cuboid(
    m: &Metrics,
    p: &Placement,
    (corner, block): (glam::Vec3, f32),
    c: &Cuboid,
    flags: [u32; 3],
) -> Vec<Instance> {
    let s = p.sprite;
    let at = |q: Vec2| page_point(m, s, p.turn, q);
    let (lo, hi) = (c.lo, c.hi);
    let world = |x: f32, y: f32, z: f32| (corner + glam::Vec3::new(x, y, z)).to_array();
    // The whole sprite per block across `along` and down `down`, a part of it for a part of a block.
    let tiled = |origin: glam::Vec3, along: glam::Vec3, down: glam::Vec3| -> Vec<Instance> {
        let (wide, tall) = (along.length() / block, down.length() / block);
        let (a, d) = (
            along.normalize_or_zero() * block,
            down.normalize_or_zero() * block,
        );
        let mut quads = vec![];
        let mut row = 0.0;
        while row < tall - 1e-4 {
            let h = (tall - row).min(1.0);
            let mut column = 0.0;
            while column < wide - 1e-4 {
                let w = (wide - column).min(1.0);
                let o = origin + a * column + d * row;
                quads.push(instance(
                    (corner + o).to_array(),
                    ((a * w).to_array(), (d * h).to_array()),
                    [
                        s.at,
                        Vec2::new(s.src.x * w, 0.0),
                        Vec2::new(0.0, s.src.y * h),
                    ],
                    flags,
                ));
                column += 1.0;
            }
            row += 1.0;
        }
        quads
    };
    let height = hi.z - lo.z;
    let down = glam::Vec3::new(0.0, 0.0, -height);
    // From the outside: the top left corner of each side, the way across it, and the way inward from its edge.
    let sides = [
        (
            glam::Vec3::new(lo.x, hi.y, hi.z),
            glam::Vec3::X * (hi.x - lo.x),
            Vec2::NEG_Y,
        ),
        (
            glam::Vec3::new(hi.x, hi.y, hi.z),
            glam::Vec3::NEG_Y * (hi.y - lo.y),
            Vec2::NEG_X,
        ),
        (
            glam::Vec3::new(hi.x, lo.y, hi.z),
            glam::Vec3::NEG_X * (hi.x - lo.x),
            Vec2::Y,
        ),
        (
            glam::Vec3::new(lo.x, lo.y, hi.z),
            glam::Vec3::Y * (hi.y - lo.y),
            Vec2::X,
        ),
    ];
    // The top shows the sprite where the box stands in the square.
    let o = at(lo.truncate());
    let mut quads = vec![instance(
        world(lo.x, lo.y, hi.z),
        ([hi.x - lo.x, 0.0, 0.0], [0.0, hi.y - lo.y, 0.0]),
        [
            o,
            at(Vec2::new(hi.x, lo.y)) - o,
            at(Vec2::new(lo.x, hi.y)) - o,
        ],
        flags,
    )];
    // A side on the edge of the square is hidden where `covered` says the next square's solid covers that edge.
    let on_edge = [hi.y >= 1.0, hi.x >= 1.0, lo.y <= 0.0, lo.x <= 0.0];
    for (n, (origin, along, inward)) in sides.into_iter().enumerate() {
        if p.covered[n] && on_edge[n] {
            continue;
        }
        match c.paint {
            Paint::Blocks => {
                quads.extend(tiled(origin, along, down));
                continue;
            }
            Paint::Whole => {
                quads.push(instance(
                    (corner + origin).to_array(),
                    (along.to_array(), down.to_array()),
                    [s.at, Vec2::new(s.src.x, 0.0), Vec2::new(0.0, s.src.y)],
                    flags,
                ));
                continue;
            }
            Paint::Cut => {}
        }
        let start = origin.truncate();
        let o = at(start);
        let deep = (start + inward * height).clamp(Vec2::ZERO, Vec2::ONE);
        quads.push(instance(
            (corner + origin).to_array(),
            (along.to_array(), down.to_array()),
            [o, at(start + along.truncate()) - o, at(deep) - o],
            flags,
        ));
    }
    quads
}

/// Two boards crossed through the diagonals of a square, `top` tall, each the whole sprite, as a block game draws a
/// plant.
pub(super) fn crossed(
    corner: glam::Vec3,
    top: f32,
    s: &SpriteSource,
    flags: [u32; 3],
) -> Vec<Instance> {
    let uv = [s.at, Vec2::new(s.src.x, 0.0), Vec2::new(0.0, s.src.y)];
    let down = [0.0, 0.0, -top];
    [
        (glam::Vec3::new(0.0, 1.0, top), [1.0, -1.0, 0.0]),
        (glam::Vec3::new(0.0, 0.0, top), [1.0, 1.0, 0.0]),
    ]
    .into_iter()
    .map(|(o, a)| instance((corner + o).to_array(), (a, down), uv, flags))
    .collect()
}

/// The boxes a thing of `form` is built of in its square, `block` a block's size: a door and its lintel, a fence, a
/// gas's volume, the legs, seat and back of a seat, the legs and top of a table, a bed's frame. A door and a fence
/// stand along the line of walls `joined` says, east to west unless only the north or the south one joins it; a
/// seat's back is at the top of its sprite, as the sprite turns.
pub(super) fn built(form: Form, block: f32, turn: Turn, joined: [bool; 4]) -> Vec<Cuboid> {
    use glam::Vec3;
    let along_x = joined[1] || joined[3] || !(joined[0] || joined[2]);
    // A slab `thick` across the middle of the square along the line, from `low` to `high`.
    let slab = |thick: f32, low: f32, high: f32| {
        let (a, b) = (0.5 - thick / 2.0, 0.5 + thick / 2.0);
        match along_x {
            true => (Vec3::new(0.0, a, low), Vec3::new(1.0, b, high)),
            false => (Vec3::new(a, 0.0, low), Vec3::new(b, 1.0, high)),
        }
    };
    let boxed = |(lo, hi): (Vec3, Vec3), paint: Paint| Cuboid { lo, hi, paint };
    let legs = |inset: f32, side: f32, high: f32| {
        [(0, 0), (1, 0), (0, 1), (1, 1)].map(|(i, j)| {
            let x = if i == 0 { inset } else { 1.0 - inset - side };
            let y = if j == 0 { inset } else { 1.0 - inset - side };
            boxed(
                (Vec3::new(x, y, 0.0), Vec3::new(x + side, y + side, high)),
                Paint::Cut,
            )
        })
    };
    // The wall over a door is its wall's: see `wall_bands`.
    let door = DOOR_BLOCKS * block;
    match form {
        Form::Door { open: false } => vec![boxed(slab(0.12, 0.0, door), Paint::Whole)],
        // Swung open against the west or the north edge of its square.
        Form::Door { open: true } => {
            let leaf = match along_x {
                true => (Vec3::new(0.0, 0.0, 0.0), Vec3::new(0.12, 1.0, door)),
                false => (Vec3::new(0.0, 0.0, 0.0), Vec3::new(1.0, 0.12, door)),
            };
            vec![boxed(leaf, Paint::Whole)]
        }
        Form::Fence => vec![boxed(slab(0.12, 0.0, block), Paint::Blocks)],
        Form::Gas => vec![boxed(
            (Vec3::ZERO, Vec3::new(1.0, 1.0, block)),
            Paint::Blocks,
        )],
        Form::Seat => {
            let seat = 0.45 * block;
            // The back along the sprite's top edge, wherever its turn brings that edge.
            let (lo, hi) = match turn {
                Turn::None | Turn::Mirror => {
                    (Vec3::new(0.1, 0.1, seat), Vec3::new(0.9, 0.22, block))
                }
                Turn::Half => (Vec3::new(0.1, 0.78, seat), Vec3::new(0.9, 0.9, block)),
                Turn::Left => (Vec3::new(0.1, 0.1, seat), Vec3::new(0.22, 0.9, block)),
                Turn::Right => (Vec3::new(0.78, 0.1, seat), Vec3::new(0.9, 0.9, block)),
            };
            let mut parts = legs(0.12, 0.08, seat).to_vec();
            parts.push(boxed(
                (
                    Vec3::new(0.1, 0.1, seat),
                    Vec3::new(0.9, 0.9, seat + 0.1 * block),
                ),
                Paint::Cut,
            ));
            parts.push(boxed((lo, hi), Paint::Cut));
            parts
        }
        Form::Table => {
            let top = 0.72 * block;
            let mut parts = legs(0.06, 0.09, top).to_vec();
            parts.push(boxed(
                (
                    Vec3::new(0.02, 0.02, top),
                    Vec3::new(0.98, 0.98, top + 0.1 * block),
                ),
                Paint::Cut,
            ));
            parts
        }
        Form::Bed => vec![boxed(
            (
                Vec3::new(0.03, 0.03, 0.0),
                Vec3::new(0.97, 0.97, 0.45 * block),
            ),
            Paint::Cut,
        )],
        Form::Plant | Form::Fire => vec![],
    }
}
