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
        light: [0; 3],
        order: 0,
        covered: [false; 4],
        outline: None,
        joined: [false; 4],
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
    // A creature stands, inside its square or reaching out of it.
    assert_eq!(shape(&flat, &small, Kind::Creature), Shape::Board);
    assert_eq!(shape(&flat, &tall, Kind::Creature), Shape::Board);
}

#[test]
fn a_wall_or_a_vehicle_part_drawn_from_above_is_pushed_up_from_its_sprite_where_an_isometric_wall_goes_by_its_joins()
 {
    let flat = Metrics::new((32, 32), false, None);
    let whole = sprite((32.0, 32.0), (0.0, 0.0));
    let wall = Kind::Wall { joined: false };
    assert_eq!(shape(&flat, &whole, wall), Shape::Blocks { top: 3.0 });
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
        ..square(&whole, Kind::Vehicle)
    };
    let layers = place(&flat, &p);
    assert_eq!(layers.len(), 6);
    assert_eq!(layers[0].origin, [3.0, 5.0, 1.0 + VEHICLE]);
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
fn items_drawn_from_above_lie_a_pixel_of_a_block_game_thick_with_sides_only_on_the_opaque_edges() {
    let flat = Metrics::new((32, 32), false, None);
    let whole = sprite((32.0, 32.0), (0.0, 0.0));
    let reach = sprite((32.0, 64.0), (0.0, -32.0));
    // An L of the top and left halves: six straight edges.
    let l = Outline::of([32, 32], |x, y| x < 16 || y < 16);
    // As thick as a pixel of a block game's sixteen-pixel item.
    let (kind, thick) = (Kind::Item, 1.0 / 16.0);
    assert_eq!(shape(&flat, &whole, kind), Shape::Extrude { top: thick });
    // What reaches out of its square was drawn standing.
    assert_eq!(shape(&flat, &reach, kind), Shape::Board);
    let p = Placement {
        outline: Some(&l),
        ..square(&whole, kind)
    };
    let quads = place(&flat, &p);
    // The top, the bottom and a side per edge of the outline, each as deep as the sprite is thick.
    assert_eq!(quads.len(), 2 + l.edges.len(), "{kind:?}");
    assert_eq!(quads[0].origin[2], thick);
    let sides = quads.iter().filter(|q| q.b[2] != 0.0).collect::<Vec<_>>();
    assert_eq!(sides.len(), 6);
    assert!(sides.iter().all(|q| (q.b[2] + thick).abs() < 1e-6));
    // An isometric tileset keeps its things as boards, and a clear sprite is no solid.
    let iso = sprite((48.0, 48.0), (0.0, -24.0));
    assert_eq!(shape(&ultica(), &iso, Kind::Item), Shape::Board);
    assert_eq!(shape(&flat, &whole, Kind::Thing), Shape::Floor { z: 0.0 });
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
    let mut p = square(&whole, Kind::Vehicle);
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
fn every_side_shows_the_pixels_the_top_shows_at_its_edge_however_the_sprite_is_turned_or_scaled() {
    let flat = Metrics::new((32, 32), false, None);
    let plain = sprite((20.0, 10.0), (4.0, 2.0));
    let scaled = SpriteSource {
        src: Vec2::new(10.0, 5.0),
        scale: 2.0,
        ..plain
    };
    for s in [plain, scaled] {
        for turn in [
            Turn::None,
            Turn::Left,
            Turn::Right,
            Turn::Half,
            Turn::Mirror,
        ] {
            let mut p = square(&s, Kind::Vehicle);
            p.turn = turn;
            let faces = place(&flat, &p);
            assert_eq!(faces.len(), 6, "{turn:?}");
            let top = faces[0];
            // The page pixel the top draws at a point of the ground.
            let drawn = |x: f32, y: f32| {
                let u = (x - top.origin[0]) / top.a[0];
                let v = (y - top.origin[1]) / top.b[1];
                Vec2::from(top.uv) + Vec2::from(top.uv_a) * u + Vec2::from(top.uv_b) * v
            };
            for side in faces.iter().filter(|i| i.b[2] < 0.0) {
                for u in [0.0, 1.0] {
                    let (x, y) = (
                        side.origin[0] + side.a[0] * u,
                        side.origin[1] + side.a[1] * u,
                    );
                    let page = Vec2::from(side.uv) + Vec2::from(side.uv_a) * u;
                    // Half a pixel in from the edge, at the middle of the pixel the top shows there.
                    let off = (page - drawn(x, y)).length();
                    assert!(
                        (off - 0.5).abs() < 0.15,
                        "{turn:?} {}: {page} {off}",
                        s.scale
                    );
                }
            }
        }
    }
}

#[test]
fn a_solid_drawn_from_above_covers_only_the_edges_of_its_square_it_is_opaque_all_along() {
    let flat = Metrics::new((32, 32), false, None);
    // The top half of the square, cut to its opaque pixels.
    let half = sprite((32.0, 16.0), (0.0, 0.0));
    let p = square(&half, Kind::Vehicle);
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
            first_person: false,
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
    view.project(world).unwrap()
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
        offset: Vec2::new(0.0, -4.0),
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
        (edge_push([std::f32::consts::FRAC_1_SQRT_2; 2]) - std::f32::consts::FRAC_1_SQRT_2).abs()
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

/// A sprite of a tileset drawn from above filling its 32 pixel tile, in a square of 1 m blocks.
fn block_sprite() -> (Metrics, SpriteSource) {
    (
        Metrics::new((32, 32), false, None),
        sprite((32.0, 32.0), (0.0, 0.0)),
    )
}

fn formed(s: &SpriteSource, form: Form) -> Placement<'_> {
    square(s, Kind::Formed(form))
}

/// The lowest and highest corner of the quads.
fn bounds(quads: &[Instance]) -> (glam::Vec3, glam::Vec3) {
    quads
        .iter()
        .flat_map(|i| {
            let (o, a, b) = (
                glam::Vec3::from(i.origin),
                glam::Vec3::from(i.a),
                glam::Vec3::from(i.b),
            );
            [o, o + a, o + b, o + a + b]
        })
        .fold((glam::Vec3::MAX, glam::Vec3::MIN), |(lo, hi), p| {
            (lo.min(p), hi.max(p))
        })
}

#[test]
fn a_wall_drawn_from_above_is_three_blocks_each_face_the_whole_sprite_at_its_own_aspect() {
    let (m, s) = block_sprite();
    let quads = place(&m, &square(&s, Kind::Wall { joined: true }));
    // The top, and four sides of three blocks.
    assert_eq!(quads.len(), 1 + 4 * 3);
    for side in quads.iter().filter(|i| i.b[2] < 0.0) {
        let (a, b) = (glam::Vec3::from(side.a), glam::Vec3::from(side.b));
        assert!((a.length() - 1.0).abs() < 1e-5 && (b.length() - 1.0).abs() < 1e-5);
        // The whole sprite, as many pixels across as down: no pixel is stretched.
        let (ua, ub) = (
            Vec2::from(side.uv_a).length(),
            Vec2::from(side.uv_b).length(),
        );
        assert!(
            (ua - 32.0).abs() < 0.1 && (ub - 32.0).abs() < 0.1,
            "{ua} {ub}"
        );
    }
    let (lo, hi) = bounds(&quads);
    assert_eq!((lo.z, hi.z), (0.0, 3.0));
    // Walls on every side leave only the top.
    let shut = Placement {
        covered: [true; 4],
        ..square(&s, Kind::Wall { joined: true })
    };
    assert_eq!(place(&m, &shut).len(), 1);
}

#[test]
fn a_creature_or_an_item_board_is_one_texel_thick_along_its_opaque_edges_and_nothing_else_is() {
    let m = ultica();
    let s = sprite((16.0, 32.0), (16.0, 0.0));
    let opaque = Outline::of([16, 32], |x, y| x >= 4 && y >= 8);
    for kind in [Kind::Creature, Kind::Item] {
        let quads = place(
            &m,
            &Placement {
                outline: Some(&opaque),
                ..square(&s, kind)
            },
        );
        // The sprite, and a side along each of the four edges of its opaque block.
        assert_eq!(quads.len(), 5, "{kind:?}");
        assert!(quads.iter().all(|q| q.flags[0] & BOARD != 0), "{kind:?}");
        // The sides run back from the sprite by a texel of the world, whatever way the board turns to the eye.
        let back = quads.iter().filter(|q| (q.b[0] + m.across).abs() < 1e-6);
        assert_eq!(back.count(), 4, "{kind:?} {quads:?}");
    }
    // Standing terrain and a wall are as thin as before, with or without an outline.
    for kind in [Kind::Terrain, Kind::Thing] {
        let quads = place(
            &m,
            &Placement {
                outline: Some(&opaque),
                ..square(&s, kind)
            },
        );
        assert_eq!(quads.len(), 1, "{kind:?}");
    }
    // Without the outline of its pixels a creature is the flat board it was.
    assert_eq!(place(&m, &square(&s, Kind::Creature)).len(), 1);
}

#[test]
fn the_sides_of_a_board_drawn_over_others_start_where_the_board_stands_toward_the_eye() {
    let m = ultica();
    let s = sprite((16.0, 32.0), (16.0, 0.0));
    let opaque = Outline::of([16, 32], |x, y| x >= 4 && y >= 8);
    let quads = place(
        &m,
        &Placement {
            order: 3,
            outline: Some(&opaque),
            ..square(&s, Kind::Creature)
        },
    );
    let board = quads[0].a[2];
    assert!((board - 3.0 * LAYER_BIAS).abs() < 1e-6);
    // No gap between the picture and its sides.
    assert!(
        quads[1..].iter().all(|q| (q.b[1] - board).abs() < 1e-6),
        "{quads:?}"
    );
}

#[test]
fn the_sides_of_a_mirrored_board_run_along_the_edges_of_the_mirrored_picture() {
    let m = ultica();
    let s = sprite((16.0, 32.0), (16.0, 0.0));
    // Opaque in the left quarter of the sprite's pixels only.
    let left = Outline::of([16, 32], |x, _| x < 4);
    let reach = |turn| {
        let quads = place(
            &m,
            &Placement {
                turn,
                outline: Some(&left),
                ..square(&s, Kind::Creature)
            },
        );
        // Where the sides start across the screen from the board's centre, the leftmost first.
        let mut starts = quads[1..].iter().map(|q| q.a[2]).collect::<Vec<_>>();
        starts.sort_by(f32::total_cmp);
        (starts[0], starts[starts.len() - 1])
    };
    let (near, far) = reach(Turn::None);
    let (mirrored_near, mirrored_far) = reach(Turn::Mirror);
    // The picture turned about the board's middle: its left quarter becomes the right quarter.
    assert!((near + mirrored_far).abs() < 1e-5, "{near} {mirrored_far}");
    assert!((far + mirrored_near).abs() < 1e-5, "{far} {mirrored_near}");
}

#[test]
fn a_plant_and_a_fire_stand_as_two_crossed_boards_and_fire_and_gas_cast_no_shadow() {
    let (m, s) = block_sprite();
    for form in [Form::Plant, Form::Fire] {
        let quads = place(&m, &formed(&s, form));
        assert_eq!(quads.len(), 2, "{form:?}");
        for q in &quads {
            // Upright through a diagonal of the square, a block tall.
            assert_eq!(q.b, [0.0, 0.0, -1.0]);
            assert!((q.a[0].abs() - 1.0).abs() < 1e-6 && (q.a[1].abs() - 1.0).abs() < 1e-6);
            assert_eq!(q.flags[0] & GLOW != 0, form == Form::Fire);
        }
    }
    let gas = place(&m, &formed(&s, Form::Gas));
    assert!(gas.iter().all(|q| q.flags[0] & SHEER != 0));
    assert_eq!(bounds(&gas).1.z, 1.0);
    // Nothing of a plant's is solid.
    assert_eq!(shape(&m, &s, Kind::Formed(Form::Plant)).solid(), None);
}

#[test]
fn a_shut_door_stands_as_a_slab_along_the_line_of_its_walls_and_an_open_one_across_it() {
    let (m, s) = block_sprite();
    let door = |open: bool, joined: [bool; 4]| {
        let p = Placement {
            joined,
            ..formed(&s, Form::Door { open })
        };
        // Below the lintel.
        let quads = place(&m, &p)
            .into_iter()
            .filter(|q| q.origin[2] <= 2.0 + 1e-4 && q.b[2] < 0.0)
            .collect::<Vec<_>>();
        let (lo, hi) = bounds(&quads);
        (hi - lo, lo)
    };
    let east_west = [false, true, false, true];
    let north_south = [true, false, true, false];
    let (size, _) = door(false, east_west);
    assert!(
        (size.x - 1.0).abs() < 1e-4 && size.y < 0.2 && (size.z - 2.0).abs() < 1e-4,
        "{size}"
    );
    let (size, _) = door(false, north_south);
    assert!(size.x < 0.2 && (size.y - 1.0).abs() < 1e-4, "{size}");
    // Open, it lies against the west edge of a door in an east-west wall.
    let (size, lo) = door(true, east_west);
    assert!(
        size.x < 0.2 && (size.y - 1.0).abs() < 1e-4 && (lo.x - 3.0).abs() < 1e-4,
        "{size} {lo}"
    );
}

#[test]
fn a_door_or_a_fence_of_a_tileset_drawn_from_above_lies_on_its_picture_of_the_ground() {
    let (m, s) = block_sprite();
    for form in [
        Form::Door { open: true },
        Form::Door { open: false },
        Form::Fence,
    ] {
        let p = Placement {
            joined: [false, true, false, true],
            ..formed(&s, form)
        };
        // The whole square, flat at its ground, as the flat map shows the square: nothing below shows through.
        let ground = place(&m, &p).into_iter().find(|i| {
            i.a[2] == 0.0
                && i.b[2] == 0.0
                && i.origin[2].abs() < 0.01
                && i.a[0] == 1.0
                && i.b[1] == 1.0
        });
        assert!(ground.is_some(), "{form:?}");
    }
}

#[test]
fn a_shut_door_between_walls_hides_the_ends_against_them_and_keeps_its_faces_inside_the_square() {
    let (m, s) = block_sprite();
    let p = Placement {
        joined: [false, true, false, true],
        covered: [false, true, false, true],
        ..formed(&s, Form::Door { open: false })
    };
    let quads = place(&m, &p);
    // The square spans x 3 to 4: nothing upright lies in its east or west edge.
    let upright = |i: &&Instance| i.b[2] < 0.0 && i.a[0] == 0.0;
    assert!(
        quads
            .iter()
            .filter(upright)
            .all(|i| i.origin[0] > 3.0 + 1e-4 && i.origin[0] < 4.0 - 1e-4),
        "{quads:?}"
    );
    // The leaf's faces along the wall stay, though the walls cover the square's other edges.
    let along = quads
        .iter()
        .filter(|i| i.b[2] < 0.0 && i.a[1] == 0.0 && i.origin[2] <= 2.0 + 1e-4)
        .count();
    assert_eq!(along, 2, "two faces, each the whole sprite: {quads:?}");
}

#[test]
fn a_window_drawn_from_above_is_its_pane_between_blocks_of_its_wall() {
    let (m, s) = block_sprite();
    let p = Placement {
        joined: [false, true, false, true],
        ..square(&s, Kind::Window)
    };
    // The window's sprite is only the pane: thin, along the wall, from its sill to its lintel.
    let pane = place(&m, &p);
    let (lo, hi) = bounds(&pane);
    assert!(
        (hi.x - lo.x - 1.0).abs() < 1e-4 && hi.y - lo.y < 0.1,
        "{lo} {hi}"
    );
    assert!(
        (lo.z - 1.0).abs() < 1e-4 && (hi.z - 2.0).abs() < 1e-4,
        "{lo} {hi}"
    );
    // Under the sill and over the lintel, whole blocks painted with the wall beside it.
    let bands = wall_bands(&m, &s, Kind::Window);
    let expected = [(0.0, 1.0), (2.0, 3.0)];
    assert!(
        bands.len() == 2
            && bands
                .iter()
                .zip(expected)
                .all(|(b, e)| { (b.0 - e.0).abs() < 1e-4 && (b.1 - e.1).abs() < 1e-4 }),
        "{bands:?}"
    );
    let wall = SpriteSource {
        at: Vec2::new(64.0, 0.0),
        ..s
    };
    let blocks = wall_bands(&m, &s, Kind::Window)
        .into_iter()
        .flat_map(|b| band(&m, &Placement { sprite: &wall, ..p }, b))
        .collect::<Vec<_>>();
    let (lo, hi) = bounds(&blocks);
    assert!(lo.z.abs() < 1e-4 && (hi.z - 3.0).abs() < 1e-4, "{lo} {hi}");
    assert!(blocks.iter().all(|q| q.uv[0] >= 64.0), "{blocks:?}");
}

#[test]
fn a_window_drawn_standing_out_of_its_square_stands_in_no_blocks_of_wall() {
    let (m, _) = block_sprite();
    let tall = sprite((32.0, 64.0), (0.0, -32.0));
    assert!(wall_bands(&m, &tall, Kind::Window).is_empty());
}

#[test]
fn a_door_drawn_from_above_shows_its_whole_sprite_once_up_its_leaf_under_a_lintel_of_its_wall() {
    let (m, s) = block_sprite();
    let p = Placement {
        joined: [false, true, false, true],
        ..formed(&s, Form::Door { open: false })
    };
    let quads = place(&m, &p);
    assert!(bounds(&quads).1.z <= 2.0 + 1e-4, "the lintel is the wall's");
    // Each face along the wall is the sprite once, two blocks tall.
    let faces = quads
        .iter()
        .filter(|q| q.b[2] < 0.0 && q.a[1] == 0.0)
        .collect::<Vec<_>>();
    assert_eq!(faces.len(), 2, "{faces:?}");
    for q in faces {
        assert_eq!(q.b[2], -2.0);
        assert!((Vec2::from(q.uv_b).y - 32.0).abs() < 0.1, "{q:?}");
    }
    assert_eq!(
        wall_bands(&m, &s, Kind::Formed(Form::Door { open: false })),
        vec![(2.0, 3.0)]
    );
}

#[test]
fn a_seat_has_legs_a_seat_and_a_back_at_the_top_of_its_sprite_and_a_table_a_top_on_legs() {
    let (m, s) = block_sprite();
    let back = |turn: Turn| {
        let p = Placement {
            turn,
            ..formed(&s, Form::Seat)
        };
        let quads = place(&m, &p);
        let (lo, hi) = bounds(&quads);
        assert!((hi.z - 1.0).abs() < 1e-3 && lo.z.abs() < 1e-3);
        // Where the parts over the seat stand.
        let high = quads
            .iter()
            .filter(|q| q.origin[2] > 0.6)
            .copied()
            .collect::<Vec<_>>();
        let (lo, hi) = bounds(&high);
        (lo.truncate() + hi.truncate()) / 2.0 - Vec2::new(3.0, 5.0)
    };
    assert!(back(Turn::None).y < 0.3, "north");
    assert!(back(Turn::Half).y > 0.7, "south");
    assert!(back(Turn::Left).x < 0.3, "west");
    assert!(back(Turn::Right).x > 0.7, "east");
    let table = place(&m, &formed(&s, Form::Table));
    let (_, hi) = bounds(&table);
    assert!((hi.z - 0.82).abs() < 1e-3, "{hi}");
    // Four legs and a top, five faces each.
    assert_eq!(table.len(), 5 * 5);
}
