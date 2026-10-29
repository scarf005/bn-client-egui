//! The outline of a sprite's opaque pixels, which the sides of a solid pushed up from the sprite are drawn along.

use super::*;

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

impl Edge {
    /// The sprite's pixels just inside the edge, for a face drawn from the edge across the sprite's thickness: the
    /// page pixel at its start and the pixels it spans, given the sprite's top left corner `at` in its page.
    pub fn inside(&self, at: Vec2) -> [Vec2; 3] {
        [
            at + self.from + self.inward / 2.0,
            self.to - self.from,
            Vec2::ZERO,
        ]
    }
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
