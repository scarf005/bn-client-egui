//! A pointer drag of a canvas grid's region: pressed inside it moves the whole region, pressed on a corner square
//! moves only that corner. The engine takes how far the top-left and bottom-right squares moved.

use super::drag::{Pointer, Square};
use bn_client_core::protocol::GridRegion;

/// A drag of one region in progress.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Grab {
    pub region: GridRegion,
    /// The square the press was on and the last one the pointer was over.
    press: Square,
    now: Square,
    /// Which edges of the region follow the pointer: west, north, east, south.
    edges: [bool; 4],
}

/// A finished drag that moved: the region's id and the engine's text for it.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct Moved {
    pub id: String,
    pub text: String,
}

fn at(square: Square) -> (i32, i32) {
    (square.0 as i32, square.1 as i32)
}

/// Which of the edges (west, north, east, south) a press on `square` drags: all four to move the whole region, or
/// the two meeting at the corner square pressed.
fn edges(region: &GridRegion, square: Square) -> [bool; 4] {
    let (x, y) = at(square);
    let (from, to) = (region.from, region.to);
    let (wide, tall) = (from.x != to.x, from.y != to.y);
    let on = |a: i32, lo: i32, hi: i32, varies: bool| (varies && a == lo, varies && a == hi);
    let ((west, east), (north, south)) = (on(x, from.x, to.x, wide), on(y, from.y, to.y, tall));
    // A corner is an end square on every axis the region spans; one spanning no axis is moved whole.
    let corner = (west || east || !wide) && (north || south || !tall) && (wide || tall);
    match corner {
        true => [west, north, east, south],
        false => [true; 4],
    }
}

/// The last region of `regions` that holds `square` (the one drawn on top), grabbed there.
pub(super) fn grab(regions: &[GridRegion], square: Square) -> Option<Grab> {
    let (x, y) = at(square);
    let region = regions
        .iter()
        .rev()
        .find(|r| (r.from.x..=r.to.x).contains(&x) && (r.from.y..=r.to.y).contains(&y))?;
    Some(Grab {
        region: region.clone(),
        press: square,
        now: square,
        edges: edges(region, square),
    })
}

impl Grab {
    /// How far the west, north, east and south edges moved.
    pub fn deltas(&self) -> [i32; 4] {
        let ((px, py), (nx, ny)) = (at(self.press), at(self.now));
        let (dx, dy) = (nx - px, ny - py);
        let [west, north, east, south] = self.edges;
        [west, north, east, south]
            .into_iter()
            .zip([dx, dy, dx, dy])
            .map(|(on, d)| if on { d } else { 0 })
            .collect::<Vec<_>>()
            .try_into()
            .unwrap()
    }

    /// The region as the drag leaves it now, as (west, north, east, south) squares.
    pub fn preview(&self) -> [i32; 4] {
        let [w, n, e, s] = self.deltas();
        let (from, to) = (self.region.from, self.region.to);
        [from.x + w, from.y + n, to.x + e, to.y + s]
    }

    fn moved(&self) -> Option<Moved> {
        let [w, n, e, s] = self.deltas();
        (self.deltas() != [0; 4]).then(|| Moved {
            id: self.region.id.clone(),
            text: format!("{w},{n},{e},{s}"),
        })
    }
}

/// Advances `grab` by one frame of `pointer`; what a release that moved the region sends. A release that did not
/// move it is a click, left to the square's own affordance.
pub(super) fn advance(
    regions: &[GridRegion],
    held: &mut Option<Grab>,
    pointer: Pointer,
) -> Option<Moved> {
    if let (true, Some(over)) = (pointer.pressed, pointer.over) {
        *held = grab(regions, over);
        return None;
    }
    let grabbed = held.as_mut()?;
    if let Some(over) = pointer.over {
        grabbed.now = over;
    }
    if !(pointer.released || !pointer.down) {
        return None;
    }
    let done = held.take()?;
    done.moved().filter(|_| pointer.released)
}

#[cfg(test)]
mod tests {
    use super::*;
    use bn_client_core::protocol::GridSquare;

    fn region(id: &str, from: (i32, i32), to: (i32, i32)) -> GridRegion {
        GridRegion {
            id: id.into(),
            from: GridSquare {
                x: from.0,
                y: from.1,
            },
            to: GridSquare { x: to.0, y: to.1 },
            label: None,
            selected: None,
        }
    }

    fn frame(over: Square, pressed: bool, released: bool) -> Pointer {
        Pointer {
            over: Some(over),
            pressed,
            down: !released,
            released,
        }
    }

    /// Presses on `from`, moves to `to`, and releases there.
    fn drag(regions: &[GridRegion], from: Square, to: Square) -> Option<Moved> {
        let mut held = None;
        advance(regions, &mut held, frame(from, true, false));
        advance(regions, &mut held, frame(to, false, false));
        advance(regions, &mut held, frame(to, false, true))
    }

    fn text(moved: Option<Moved>) -> Option<String> {
        moved.map(|m| m.text)
    }

    #[test]
    fn a_press_inside_moves_the_whole_region() {
        let regions = [region("a", (2, 2), (6, 5))];
        let moved = drag(&regions, (4, 3), (6, 2)).unwrap();
        assert_eq!((moved.id.as_str(), moved.text.as_str()), ("a", "2,-1,2,-1"));
    }

    #[test]
    fn a_press_on_the_top_left_corner_moves_that_corner() {
        let regions = [region("a", (2, 2), (6, 5))];
        assert_eq!(
            text(drag(&regions, (2, 2), (1, 4))).as_deref(),
            Some("-1,2,0,0")
        );
    }

    #[test]
    fn a_press_on_the_bottom_right_corner_moves_that_corner() {
        let regions = [region("a", (2, 2), (6, 5))];
        assert_eq!(
            text(drag(&regions, (6, 5), (8, 6))).as_deref(),
            Some("0,0,2,1")
        );
    }

    #[test]
    fn a_press_on_the_top_right_corner_moves_the_top_and_the_right_edges() {
        let regions = [region("a", (2, 2), (6, 5))];
        assert_eq!(
            text(drag(&regions, (6, 2), (7, 1))).as_deref(),
            Some("0,-1,1,0")
        );
    }

    #[test]
    fn a_press_on_the_bottom_left_corner_moves_the_left_and_the_bottom_edges() {
        let regions = [region("a", (2, 2), (6, 5))];
        assert_eq!(
            text(drag(&regions, (2, 5), (3, 7))).as_deref(),
            Some("1,0,0,2")
        );
    }

    #[test]
    fn a_press_on_an_edge_square_that_is_no_corner_moves_the_whole_region() {
        let regions = [region("a", (2, 2), (6, 5))];
        assert_eq!(
            text(drag(&regions, (4, 2), (4, 3))).as_deref(),
            Some("0,1,0,1")
        );
    }

    #[test]
    fn a_single_square_region_moves_whole() {
        let regions = [region("a", (3, 3), (3, 3))];
        assert_eq!(
            text(drag(&regions, (3, 3), (5, 4))).as_deref(),
            Some("2,1,2,1")
        );
    }

    #[test]
    fn an_end_of_a_one_wide_region_stretches_it_along_its_length_only() {
        let regions = [region("a", (3, 1), (3, 4))];
        assert_eq!(
            text(drag(&regions, (3, 4), (5, 6))).as_deref(),
            Some("0,0,0,2")
        );
    }

    #[test]
    fn a_release_where_it_pressed_is_a_click_and_sends_nothing() {
        let regions = [region("a", (2, 2), (6, 5))];
        assert_eq!(drag(&regions, (4, 3), (4, 3)), None);
        assert_eq!(
            drag(&regions, (2, 2), (4, 3)).map(|m| m.id),
            Some("a".into())
        );
        assert_eq!(drag(&regions, (2, 2), (2, 2)), None);
    }

    #[test]
    fn a_drag_that_returns_to_its_press_square_is_a_click() {
        let regions = [region("a", (2, 2), (6, 5))];
        let mut held = None;
        advance(&regions, &mut held, frame((4, 3), true, false));
        advance(&regions, &mut held, frame((7, 3), false, false));
        advance(&regions, &mut held, frame((4, 3), false, false));
        assert_eq!(
            advance(&regions, &mut held, frame((4, 3), false, true)),
            None
        );
    }

    #[test]
    fn the_region_drawn_last_takes_a_press_where_regions_overlap() {
        let regions = [
            region("under", (0, 0), (5, 5)),
            region("over", (2, 2), (4, 4)),
        ];
        assert_eq!(
            drag(&regions, (3, 3), (4, 3)).map(|m| m.id),
            Some("over".into())
        );
        assert_eq!(
            drag(&regions, (0, 3), (1, 3)).map(|m| m.id),
            Some("under".into())
        );
    }

    #[test]
    fn a_press_outside_every_region_grabs_nothing() {
        let regions = [region("a", (2, 2), (6, 5))];
        assert_eq!(drag(&regions, (0, 0), (3, 3)), None);
    }

    #[test]
    fn the_preview_follows_the_pointer_before_the_release() {
        let regions = [region("a", (2, 2), (6, 5))];
        let mut held = None;
        advance(&regions, &mut held, frame((6, 5), true, false));
        advance(&regions, &mut held, frame((9, 7), false, false));
        assert_eq!(held.unwrap().preview(), [2, 2, 9, 7]);
    }

    #[test]
    fn a_drag_that_loses_the_button_without_a_release_sends_nothing() {
        let regions = [region("a", (2, 2), (6, 5))];
        let mut held = None;
        advance(&regions, &mut held, frame((4, 3), true, false));
        let lost = Pointer {
            over: Some((5, 3)),
            ..Pointer::default()
        };
        assert_eq!(advance(&regions, &mut held, lost), None);
        assert!(held.is_none());
    }
}
