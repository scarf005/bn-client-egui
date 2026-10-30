//! A pointer drag over the squares of a canvas grid: which squares it runs the grid's affordance on.

use bn_client_core::protocol::GridDrag;

pub(super) type Square = (u32, u32);

/// The pointer in one frame, over `over` when it is on a square of the grid.
#[derive(Clone, Copy, Default)]
pub(super) struct Pointer {
    pub over: Option<Square>,
    /// How far across and down `over` the pointer is, from 0 to 1; unknown counts as its middle.
    pub spot: Option<(f32, f32)>,
    pub pressed: bool,
    pub down: bool,
    pub released: bool,
}

/// A drag in progress: the square it began on and the last square it reached.
#[derive(Clone, Copy)]
pub(super) struct Drag {
    from: Square,
    last: Square,
}

impl Drag {
    /// The square it began on and the last one it reached.
    pub(super) fn ends(&self) -> (Square, Square) {
        (self.from, self.last)
    }
}

/// Advances `drag` by one frame of `pointer`; the squares to run, in order.
pub(super) fn advance(mode: GridDrag, drag: &mut Option<Drag>, pointer: Pointer) -> Vec<Square> {
    if let (true, Some(over)) = (pointer.pressed, pointer.over) {
        *drag = Some(Drag {
            from: over,
            last: over,
        });
        return vec![over];
    }
    let Some(held) = drag.as_mut() else {
        return vec![];
    };
    let moved = pointer.over.filter(|&at| at != held.last);
    let run = match (mode, moved, pointer.over) {
        (GridDrag::Trail, Some(over), _) if pointer.down || pointer.released => {
            crossed(held.last, over)
        }
        (GridDrag::Span, _, Some(over)) if pointer.released && over != held.from => vec![over],
        _ => vec![],
    };
    if let Some(over) = moved {
        held.last = over;
    }
    if pointer.released || !pointer.down {
        *drag = None;
    }
    run
}

/// The squares on the line from `from` to `to`, `from` left out.
fn crossed(from: Square, to: Square) -> Vec<Square> {
    let (dx, dy) = (to.0 as i64 - from.0 as i64, to.1 as i64 - from.1 as i64);
    let steps = dx.abs().max(dy.abs());
    let at = |a: u32, d: i64, i: i64| (a as i64 + (d * i + steps / 2).div_euclid(steps)) as u32;
    (1..=steps)
        .map(|i| (at(from.0, dx, i), at(from.1, dy, i)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(mode: GridDrag, frames: &[Pointer]) -> Vec<Square> {
        let mut drag = None;
        frames
            .iter()
            .flat_map(|&p| advance(mode, &mut drag, p))
            .collect()
    }

    fn over(at: Square) -> Pointer {
        Pointer {
            over: Some(at),
            down: true,
            ..Pointer::default()
        }
    }

    fn press(at: Square) -> Pointer {
        Pointer {
            pressed: true,
            ..over(at)
        }
    }

    fn release(at: Square) -> Pointer {
        Pointer {
            released: true,
            down: false,
            ..over(at)
        }
    }

    #[test]
    fn a_trail_runs_every_square_between_frames() {
        let got = run(
            GridDrag::Trail,
            &[press((1, 1)), over((2, 1)), over((5, 1)), release((5, 1))],
        );
        assert_eq!(got, [(1, 1), (2, 1), (3, 1), (4, 1), (5, 1)]);
    }

    #[test]
    fn a_trail_follows_a_diagonal_without_gaps() {
        let got = run(GridDrag::Trail, &[press((0, 0)), over((3, 1))]);
        assert_eq!(got, [(0, 0), (1, 0), (2, 1), (3, 1)]);
    }

    #[test]
    fn a_span_runs_the_press_and_the_release_only() {
        let got = run(
            GridDrag::Span,
            &[press((2, 2)), over((4, 2)), over((9, 7)), release((9, 7))],
        );
        assert_eq!(got, [(2, 2), (9, 7)]);
    }

    #[test]
    fn a_span_released_where_it_began_runs_once() {
        assert_eq!(
            run(GridDrag::Span, &[press((2, 2)), release((2, 2))]),
            [(2, 2)]
        );
    }

    #[test]
    fn moving_without_a_press_runs_nothing() {
        assert!(run(GridDrag::Trail, &[over((1, 1)), over((2, 1))]).is_empty());
    }

    #[test]
    fn a_trail_ends_with_the_release() {
        let got = run(
            GridDrag::Trail,
            &[press((1, 1)), release((1, 1)), over((2, 1))],
        );
        assert_eq!(got, [(1, 1)]);
    }
}
