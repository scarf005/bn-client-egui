//! One quad of the scene as the shader reads it, its flag bits, and the quads that draw no sprite: the dark of
//! what is unknown and the ceiling.

use super::*;

/// One quad of the scene, as the shader reads it.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
pub struct Instance {
    /// The corner `(u, v) = (0, 0)`. For a board, its square's centre on the ground.
    pub origin: [f32; 3],
    /// The edge along `u`. For a board: width, its left edge from the origin across the screen, and its offset
    /// toward the eye. For a `SIDE`: its span across the screen and up, and its start across the screen.
    pub a: [f32; 3],
    /// The edge along `v`. For a board: height and its bottom edge over the origin. For a `SIDE`: how far back from
    /// the board it runs, and the board's offset toward the eye.
    pub b: [f32; 3],
    /// The sprite pixel at `(0, 0)` and the pixels spanned by `u` and by `v`.
    pub uv: [f32; 2],
    pub uv_a: [f32; 2],
    pub uv_b: [f32; 2],
    /// Low byte: bit 0 for a board, bit 1 for a board at the near edge of its square; next byte: the visibility filter. Then the haze of a level seen
    /// through, 0 to 255. Last the coloured light added over the sprite, red, green and blue from the low byte up.
    pub flags: [u32; 3],
}

/// Flag bits of an instance's first word: a board, a board pushed to the near edge of its square, what gives light
/// (never shaded, casting no shadow), what is seen through (every other pixel drawn, casting no shadow) and what is
/// unknown but solid.
pub const BOARD: u32 = 1;
pub const EDGE: u32 = 2;
pub const GLOW: u32 = 4;
pub const SHEER: u32 = 8;
/// What nothing is known about but that it fills the space between two levels: a flat dark, its gray in the haze
/// byte, casting no shadow.
pub const VOID: u32 = 16;
/// A face of `VOID` that fades from light at its top to its dark at its foot by ordered dithering; its uv runs from 0
/// at the top to 1 at the foot.
pub const FADE: u32 = 32;
/// A side of a board, pushed back from it along an edge of its sprite's outline: see `board_sides`.
pub const SIDE: u32 = 64;

/// How far an upright sprite of an isometric tileset stands toward the eye from the middle of its square: to the
/// square's near edge, whatever the camera's yaw. `toward` is the horizontal unit vector from target to eye; the
/// shader computes the same.
pub fn edge_push(toward: [f32; 2]) -> f32 {
    0.5 / toward[0].abs().max(toward[1].abs()).max(f32::EPSILON)
}

/// How much of a quad's sprite span is cut off at each edge, so that a pixel centre on an edge samples the
/// sprite's own last texel and never a neighbour's in the sheet.
const INSET: f32 = 0.0002;

pub(super) fn instance(
    origin: [f32; 3],
    edges: ([f32; 3], [f32; 3]),
    uv: [Vec2; 3],
    flags: [u32; 3],
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

/// The gray of the ground nothing is known about and of the faces down to it, in the haze byte of its instances: dark,
/// but not the black of nothing, so that the pit reads as ground and walls.
const VOID_LID: u32 = 22;
const VOID_FACE: u32 = 10;

/// The dark ground at height `z` of `square`, whose ground nothing is known about.
pub fn void_lid(square: (i32, i32), z: f32) -> Instance {
    let ground = ([1.0, 0.0, 0.0], [0.0, 1.0, 0.0]);
    instance(
        [square.0 as f32, square.1 as f32, z],
        ground,
        [Vec2::ZERO; 3],
        [VOID, VOID_LID, 0],
    )
}

/// The gray of a ceiling, in the haze byte of its instance: a plain dark, lighter than the unknown ground.
const CEILING: u32 = 56;

/// The flat ceiling at height `z` over `square`, facing down at whoever stands under it.
pub fn ceiling_lid(square: (i32, i32), z: f32) -> Instance {
    let ceiling = ([0.0, 1.0, 0.0], [1.0, 0.0, 0.0]);
    instance(
        [square.0 as f32, square.1 as f32, z],
        ceiling,
        [Vec2::ZERO; 3],
        [VOID, CEILING, 0],
    )
}

/// The face of `square` on its south (0), east, north or west edge, from height `low` up to `high`: the step down to
/// the lower ground beside it, fading into the dark at its foot.
pub fn void_face(square: (i32, i32), face: usize, range: (f32, f32)) -> Instance {
    edge_face(
        square,
        face,
        range,
        (
            [VOID | FADE, VOID_FACE, 0],
            [Vec2::ZERO, Vec2::ZERO, Vec2::Y],
        ),
    )
}

/// The flat gray face on the `face` edge of `square` from height `low` up to `high`, which closes the room under a
/// ceiling above the walls that stop short of it.
pub fn ceiling_face(square: (i32, i32), face: usize, range: (f32, f32)) -> Instance {
    edge_face(square, face, range, ([VOID, CEILING, 0], [Vec2::ZERO; 3]))
}

fn edge_face(
    square: (i32, i32),
    face: usize,
    (low, high): (f32, f32),
    (flags, uv): ([u32; 3], [Vec2; 3]),
) -> Instance {
    let (x, y) = (square.0 as f32, square.1 as f32);
    let (from, run) = match face {
        0 => ([x, y + 1.0], [1.0, 0.0]),
        1 => ([x + 1.0, y + 1.0], [0.0, -1.0]),
        2 => ([x + 1.0, y], [-1.0, 0.0]),
        _ => ([x, y], [0.0, 1.0]),
    };
    instance(
        [from[0], from[1], high],
        ([run[0], run[1], 0.0], [0.0, 0.0, low - high]),
        uv,
        flags,
    )
}
