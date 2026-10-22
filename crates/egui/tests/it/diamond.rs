//! The flat map of an isometric tileset: its squares are diamonds that tile the plane, and the pointer picks the
//! diamond it is over.
use bn_client_egui::Pos;
use bn_client_egui::diamond::Diamond;
use eframe::egui::{Pos2, pos2};

fn diamond() -> Diamond {
    Diamond {
        centre: pos2(400.0, 300.0),
        at: Pos { x: 10, y: 10 },
        width: 48.0,
        ratio: 0.5,
    }
}

/// Whether `p` lies inside the convex quadrilateral `corners` (clockwise on the screen).
fn inside(corners: [Pos2; 4], p: Pos2) -> bool {
    (0..4).all(|i| {
        let (a, b) = (corners[i], corners[(i + 1) % 4]);
        (b.x - a.x) * (p.y - a.y) - (b.y - a.y) * (p.x - a.x) >= -1e-3
    })
}

#[test]
fn a_square_is_a_diamond_as_wide_as_the_tile_and_half_as_tall() {
    let d = diamond();
    let [left, top, right, bottom] = d.corners(Pos { x: 10, y: 10 });
    assert_eq!((right.x - left.x, bottom.y - top.y), (48.0, 24.0));
    // The centre square's diamond is centred on the centre and fills its tile box.
    assert_eq!(top.x, 400.0);
    assert_eq!(left.y, 300.0);
    let tile = d.tile(Pos { x: 10, y: 10 });
    assert_eq!(
        (tile.center(), tile.width(), tile.height()),
        (d.centre, 48.0, 24.0)
    );
}

#[test]
fn the_squares_east_and_south_step_half_a_tile_sideways_and_a_quarter_up_or_down() {
    let d = diamond();
    let at = |x, y| d.tile(Pos { x, y }).center() - d.centre;
    assert_eq!(at(11, 10), eframe::egui::vec2(24.0, -12.0));
    assert_eq!(at(10, 11), eframe::egui::vec2(24.0, 12.0));
    assert_eq!(at(11, 11), eframe::egui::vec2(48.0, 0.0));
}

#[test]
fn every_point_of_the_floor_is_in_exactly_the_diamond_the_pointer_picks() {
    let d = diamond();
    let mut gaps = 0;
    for ix in 0..160 {
        for iy in 0..100 {
            // A point off the diamonds' edges.
            let p = pos2(ix as f32 * 5.0 + 0.37, iy as f32 * 6.0 + 0.61);
            let square = d.square_at(p);
            if !inside(d.corners(square), p) {
                gaps += 1;
            }
            let others = [(1, 0), (-1, 0), (0, 1), (0, -1)]
                .map(|(x, y)| Pos {
                    x: square.x + x,
                    y: square.y + y,
                })
                .into_iter()
                .filter(|n| inside(d.corners(*n), p))
                .count();
            assert_eq!(others, 0, "{p:?} lies in the neighbours of {square:?}");
        }
    }
    assert_eq!(gaps, 0);
}

#[test]
fn a_diamonds_middle_and_nearby_points_pick_its_square() {
    let d = diamond();
    for (x, y) in [(10, 10), (0, 0), (30, 4), (7, 25)] {
        let p = Pos { x, y };
        let middle = d.tile(p).center();
        assert_eq!(d.square_at(middle), p);
        // Just inside each corner.
        for corner in d.corners(p) {
            let inward = middle + (corner - middle) * 0.9;
            assert_eq!(d.square_at(inward), p);
        }
    }
}

#[test]
fn squares_are_drawn_from_the_top_of_the_screen_down_and_left_to_right() {
    let d = diamond();
    // A square view, one wider than tall and one taller than wide.
    for (x, y) in [(21, 21), (7, 4), (3, 9), (1, 1)] {
        let order = d.order(Pos { x, y });
        let mut seen = order.clone();
        seen.sort_by_key(|p| (p.y, p.x));
        seen.dedup();
        assert_eq!(
            (order.len(), seen.len()),
            ((x * y) as usize, (x * y) as usize)
        );
        let tops = order
            .iter()
            .map(|p| (d.tile(*p).top(), d.tile(*p).left()))
            .collect::<Vec<_>>();
        assert!(
            tops.windows(2)
                .all(|w| w[0].0 < w[1].0 || (w[0].0 == w[1].0 && w[0].1 < w[1].1))
        );
        assert!(
            order
                .iter()
                .all(|p| (0..x).contains(&p.x) && (0..y).contains(&p.y))
        );
    }
}
