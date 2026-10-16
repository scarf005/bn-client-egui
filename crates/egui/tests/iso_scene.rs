#![recursion_limit = "512"]
//! What the 3D map's scene holds for a replica: which levels, which filters, which shapes.
mod common;
use bn_client_egui::EngineModel;
use bn_client_egui::GameModel;
use bn_client_egui::iso::camera::{Camera, Viewport};
use bn_client_egui::iso::geometry::{BOARD, Instance, PageId};
use bn_client_egui::iso::pages::Pages;
use bn_client_egui::iso::scene::{self, Scene};
use bn_client_egui::tileset::Tileset;
use bn_client_egui::tint::Paint;
use bn_client_egui::transport::ScriptedTransport;
use common::{fixture, flat_root, iso_root};
use serde_json::{Value, json};

fn look(kind: &str, id: &str) -> Value {
    json!({"kind": kind, "id": id, "glyph": "#", "color": "white"})
}

fn cell(x: i32, y: i32, z: i32, terrain: &str) -> Value {
    json!({"at": {"dim": "", "x": x, "y": y, "z": z}, "known": "visible",
        "terrain": look("terrain", terrain)})
}

/// The avatar at (3, 4, 0) of a replica that covers the levels -1 to 2 and holds `cells`.
fn model(cells: Vec<Value>) -> EngineModel {
    let mut subscribe = fixture("subscribe_result");
    subscribe["result"]["coverage"]["min"]["z"] = (-1).into();
    subscribe["result"]["coverage"]["max"]["z"] = 2.into();
    subscribe["result"]["entities"] = json!([]);
    subscribe["result"]["avatar"]["look"] = json!({
        "kind": "avatar", "id": "avatar", "glyph": "@", "color": "white", "tile": "mon_a"
    });
    let mut first = fixture("snapshot_part_0");
    first["params"]["cells"] = cells.into();
    let mut last = fixture("snapshot_part_1");
    last["params"]["cells"] = json!([]);
    let (t, _) = ScriptedTransport::new();
    let t = t
        .on("bn.hello", vec![fixture("hello_result")])
        .on("bn.subscribe", vec![subscribe, first, last]);
    let mut m = EngineModel::new(Box::new(t));
    m.poll();
    m
}

fn scene_of(m: &EngineModel, tag: &str) -> Scene {
    let tileset = Tileset::load(&iso_root(tag).join("gfx/iso")).unwrap();
    scene::build(m, &tileset, None, None, 1)
}

fn all(scene: &Scene) -> Vec<Instance> {
    scene
        .batches
        .iter()
        .flat_map(|b| b.instances.clone())
        .collect()
}

/// The quads that are not boards.
fn planes(scene: &Scene) -> Vec<Instance> {
    all(scene)
        .into_iter()
        .filter(|i| i.flags[0] & BOARD == 0)
        .collect()
}

/// The instances whose origin lies in absolute square `abs`: its ground and what stands in it.
fn at(scene: &Scene, m: &EngineModel, abs: (i32, i32)) -> Vec<Instance> {
    let o = m.origin();
    let (x, y) = ((abs.0 - o.x) as f32, (abs.1 - o.y) as f32);
    all(scene)
        .into_iter()
        .filter(|i| (x..x + 1.0).contains(&i.origin[0]) && (y..y + 1.0).contains(&i.origin[1]))
        .collect()
}

fn filter(i: &Instance) -> u32 {
    i.flags[0] >> 8
}

#[test]
fn a_wall_is_a_box_and_the_ground_a_single_quad() {
    let ground = scene_of(&model(vec![cell(4, 4, 0, "t_dirt")]), "scene-ground");
    assert_eq!(planes(&ground).len(), 1);
    let wall = planes(&scene_of(
        &model(vec![cell(5, 4, 0, "t_wall")]),
        "scene-box",
    ));
    assert_eq!(wall.len(), 5, "a lid and four faces");
    assert!(
        wall.iter().any(|i| i.origin[2] > 3.0),
        "its top is over a person's head"
    );
}

#[test]
fn levels_above_the_avatar_are_cut_away() {
    let m = model(vec![
        cell(4, 4, 0, "t_dirt"),
        cell(4, 4, 1, "t_wall"),
        cell(5, 4, 0, "t_dirt"),
        cell(5, 4, 2, "t_wall"),
    ]);
    let scene = scene_of(&m, "scene-cut");
    let tops = planes(&scene).iter().filter(|i| i.origin[2] > 0.0).count();
    assert_eq!(tops, 0, "nothing stands over the avatar's level");
    // The same wall on the avatar's own level is drawn.
    let m = model(vec![cell(4, 4, 0, "t_wall")]);
    let drawn = all(&scene_of(&m, "scene-cut-control"));
    assert!(drawn.iter().any(|i| i.origin[2] > 0.0));
}

#[test]
fn open_air_shows_the_level_below_lower_and_hazed_by_how_far_down_it_is() {
    let mut air =
        json!({"at": {"dim": "", "x": 4, "y": 4, "z": 0}, "known": "visible", "below": 1});
    air["terrain"] = serde_json::Value::Null;
    let m = model(vec![cell(3, 4, 0, "t_dirt"), air, cell(4, 4, -1, "t_dirt")]);
    let scene = scene_of(&m, "scene-air");
    let level = 108.0 * 0.034_026;
    let level_below = at(&scene, &m, (4, 4));
    assert_eq!(level_below.len(), 1, "{level_below:?}");
    assert!(
        (level_below[0].origin[2] + level).abs() < 0.02,
        "{level_below:?}"
    );
    // 24 for the level and 24 more for being seen through the air of the one above.
    assert_eq!(level_below[0].flags[1], 48);
    assert_eq!(
        at(&scene, &m, (3, 4))[0].flags[1],
        0,
        "the avatar's level has no haze"
    );
}

#[test]
fn a_remembered_square_and_a_poorly_lit_one_take_the_filters_of_the_flat_map() {
    let mut remembered = cell(4, 4, 0, "t_dirt");
    remembered["known"] = "remembered".into();
    let mut dim = cell(5, 4, 0, "t_dirt");
    dim["light"] = 1.into();
    let mut lit = cell(6, 4, 0, "t_dirt");
    lit["light"] = 4.into();
    let m = model(vec![remembered, dim, lit]);
    let scene = scene_of(&m, "scene-filters");
    let filters = [(4, 4), (5, 4), (6, 4)].map(|p| filter(&at(&scene, &m, p)[0]));
    assert_eq!(filters, [2, 1, 0]);
}

#[test]
fn squares_nothing_is_known_about_are_not_drawn() {
    let m = model(vec![cell(4, 4, 0, "t_dirt")]);
    let scene = scene_of(&m, "scene-unknown");
    assert_eq!(scene.squares, 2, "the dirt, and the avatar's own square");
    assert!(at(&scene, &m, (8, 8)).is_empty());
}

#[test]
fn every_page_is_one_batch_and_sprites_that_stand_are_boards() {
    let m = model(vec![
        cell(4, 4, 0, "t_dirt"),
        cell(5, 4, 0, "t_dirt"),
        cell(3, 4, 0, "t_dirt"),
        cell(6, 4, 0, "t_wall"),
    ]);
    let scene = scene_of(&m, "scene-pages");
    let pages = scene
        .batches
        .iter()
        .map(|b| b.page)
        .collect::<Vec<PageId>>();
    let mut unique = pages.clone();
    unique.dedup();
    assert_eq!(pages, unique, "a page is drawn by one batch");
    assert!(pages.len() >= 2, "the ground sheet and the wall sheet");
    // The avatar is drawn with a sprite of its tileset, standing: a board.
    let me = at(&scene, &m, (3, 4));
    assert!(me.iter().any(|i| i.flags[0] & BOARD != 0), "{me:?}");
}

#[test]
fn tall_terrain_that_is_not_joined_to_its_neighbours_stands_upright_where_a_wall_is_a_box() {
    let m = model(vec![cell(5, 4, 0, "t_mound")]);
    let scene = scene_of(&m, "scene-mound");
    assert!(planes(&scene).len() <= 1, "no box: {:?}", planes(&scene));
    assert!(
        at(&scene, &m, (5, 4))
            .iter()
            .any(|i| i.flags[0] & BOARD != 0)
    );
}

#[test]
fn faces_where_two_walls_touch_are_not_drawn() {
    let m = model(vec![
        cell(4, 4, 0, "t_wall"),
        cell(5, 4, 0, "t_wall"),
        cell(6, 4, 0, "t_wall"),
    ]);
    // Three lids, a south and a north face each, and the two ends.
    assert_eq!(planes(&scene_of(&m, "scene-touch")).len(), 3 + 3 + 3 + 2);
    let alone = model(vec![cell(5, 4, 0, "t_wall")]);
    assert_eq!(planes(&scene_of(&alone, "scene-alone")).len(), 5);
}

/// A camera that looks at the avatar at the default tilt over a view the size of a window.
fn view(m: &EngineModel) -> Viewport {
    let me = m.player();
    Viewport {
        camera: Camera::default(),
        target: glam::Vec3::new(me.x as f32 + 0.5, me.y as f32 + 0.5, 0.0),
        scale: 34.0,
        size: glam::Vec2::new(1100.0, 800.0),
    }
}

/// The view square of absolute square `abs`.
fn square(m: &EngineModel, abs: (i32, i32)) -> (i32, i32) {
    (abs.0 - m.origin().x, abs.1 - m.origin().y)
}

fn room() -> EngineModel {
    let mut cells = vec![];
    for x in 2..=8 {
        for y in 1..=7 {
            cells.push(cell(x, y, 0, "t_dirt"));
        }
    }
    cells.retain(|c| c["at"]["x"] != 5 || c["at"]["y"] != 4);
    cells.push(cell(5, 4, 0, "t_wall"));
    model(cells)
}

#[test]
fn a_click_on_a_wall_picks_that_wall_on_its_lid_and_on_its_faces_not_the_square_behind() {
    let m = room();
    let scene = scene_of(&m, "pick-wall");
    let v = view(&m);
    let wall = square(&m, (5, 4));
    let (_, top) = scene.column(wall.0, wall.1).unwrap().solid.unwrap();
    let (x, y) = (wall.0 as f32, wall.1 as f32);
    for (what, point) in [
        ("lid", glam::Vec3::new(x + 0.5, y + 0.5, top)),
        ("south face", glam::Vec3::new(x + 0.5, y + 1.0, top / 2.0)),
        ("east face", glam::Vec3::new(x + 1.0, y + 0.5, top / 2.0)),
    ] {
        let on = v.project(point);
        assert_eq!(scene.pick(&v, on), Some(wall), "{what}");
    }
    // The ground plane alone would have named a square behind the wall for the south face.
    let on_face = v.project(glam::Vec3::new(x + 0.5, y + 1.0, top / 2.0));
    let plane = v.pick(on_face, 0.0).unwrap();
    assert_ne!(
        (plane.x.floor() as i32, plane.y.floor() as i32),
        wall,
        "the face is above ground that is not the wall's"
    );
}

#[test]
fn a_click_on_the_ground_picks_the_square_whose_ground_it_is() {
    let m = room();
    let scene = scene_of(&m, "pick-ground");
    let v = view(&m);
    for abs in [(3, 5), (7, 6), (6, 6), (2, 7)] {
        let at = square(&m, abs);
        let on = v.project(glam::Vec3::new(at.0 as f32 + 0.5, at.1 as f32 + 0.5, 0.0));
        assert_eq!(scene.pick(&v, on), Some(at), "{abs:?}");
    }
    // Outside everything the scene draws there is nothing to pick.
    assert_eq!(scene.pick(&v, glam::Vec2::new(5.0, 5.0)), None);
}

#[test]
fn a_square_seen_through_open_air_is_picked_where_its_lowered_ground_is_drawn() {
    let mut air =
        json!({"at": {"dim": "", "x": 6, "y": 4, "z": 0}, "known": "visible", "below": 1});
    air["terrain"] = serde_json::Value::Null;
    let mut cells = vec![air, cell(6, 4, -1, "t_dirt")];
    // Ground only on the far side of the lowered square, so that none stands between it and the camera.
    for x in 6..=8 {
        for y in 2..=4 {
            if (x, y) != (6, 4) {
                cells.push(cell(x, y, 0, "t_dirt"));
            }
        }
    }
    let m = model(cells);
    let scene = scene_of(&m, "pick-lowered");
    let v = view(&m);
    let lowered = square(&m, (6, 4));
    let ground = scene.column(lowered.0, lowered.1).unwrap().ground;
    assert!(ground < -1.0, "{ground}");
    let on = v.project(glam::Vec3::new(
        lowered.0 as f32 + 0.5,
        lowered.1 as f32 + 0.5,
        ground,
    ));
    assert_eq!(scene.pick(&v, on), Some(lowered));
}

#[test]
fn a_painted_sprite_is_a_page_of_its_own_with_the_paint_in_its_pixels_and_is_made_once() {
    let tileset = Tileset::load(&iso_root("paint").join("gfx/iso")).unwrap();
    let pages = Pages::start(tileset.sheets.clone(), eframe::egui::Context::default());
    for _ in 0..500 {
        if !pages.loading() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    // The light ground, sprite 0, painted red.
    let ground = scene::source(&tileset, 0).unwrap();
    let red = Paint::of([220, 30, 30]);
    let painted = pages.painted(&ground, None, red).unwrap();
    assert_ne!(painted.page, ground.page);
    assert_eq!(painted.src, ground.src);
    assert_eq!(painted.offset, ground.offset);
    assert_eq!(
        pages.painted(&ground, None, red).unwrap().page,
        painted.page,
        "once"
    );
    let other = pages
        .painted(&ground, None, Paint::of([30, 30, 220]))
        .unwrap();
    assert_ne!(other.page, painted.page);
    // The middle of the diamond was green and is red now; the corner outside the diamond is still clear.
    let (before, after) = (
        pages.get(ground.page).unwrap(),
        pages.get(painted.page).unwrap(),
    );
    let middle = after.get_pixel(24, 24).0;
    assert!(middle[0] > middle[1] && middle[0] > middle[2], "{middle:?}");
    assert_ne!(middle, before.get_pixel(24, 24).0);
    assert_eq!(after.get_pixel(0, 0).0[3], 0, "outside the diamond");
}

#[test]
fn a_sprite_loses_the_transparent_rows_above_its_first_opaque_one_and_keeps_its_place_on_the_screen()
 {
    let tileset = Tileset::load(&iso_root("trim").join("gfx/iso")).unwrap();
    let pages = Pages::start(tileset.sheets.clone(), eframe::egui::Context::default());
    for _ in 0..500 {
        if !pages.loading() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    // The ground is a diamond in the lowest 24 of its 36 rows.
    let ground = scene::source(&tileset, 0).unwrap();
    let trimmed = pages.trimmed(ground);
    assert_eq!(trimmed.src.y, 24.0);
    assert_eq!(trimmed.at, ground.at + glam::Vec2::new(0.0, 12.0));
    assert_eq!(trimmed.offset, ground.offset + glam::Vec2::new(0.0, 12.0));
    assert_eq!(trimmed.src.x, ground.src.x);
    // Its bottom edge is where it was.
    assert_eq!(
        trimmed.offset.y + trimmed.size().y,
        ground.offset.y + ground.size().y
    );
    // A trimmed sprite has no row to lose, and a sprite on a page not decoded yet stays whole.
    assert_eq!(pages.trimmed(trimmed), trimmed);
    let undecoded = Pages::start(vec![], eframe::egui::Context::default());
    assert_eq!(undecoded.trimmed(ground), ground);
}

#[test]
fn terrain_the_tileset_has_no_sprite_for_stands_as_its_glyph_where_a_looks_like_sprite_stands_as_that()
 {
    let mut bare = look("terrain", "t_unknown");
    bare["glyph"] = "%".into();
    let mut like = look("terrain", "t_unknown_too");
    like["looks_like"] = json!(["t_dirt"]);
    let mut first = cell(4, 4, 0, "t_dirt");
    first["terrain"] = bare;
    let mut second = cell(5, 4, 0, "t_dirt");
    second["terrain"] = like;
    let scene = scene_of(&model(vec![first, second]), "scene-glyph");
    assert_eq!(scene.glyphs.len(), 1, "only the bare one");
    assert_eq!(scene.glyphs[0].layer.glyph, '%');
    assert_eq!(planes(&scene).len(), 1, "the looks-like square is a quad");
}

/// The pages of `tileset`, decoded.
fn decoded(tileset: &Tileset) -> std::sync::Arc<Pages> {
    let pages = Pages::start(tileset.sheets.clone(), eframe::egui::Context::default());
    for _ in 0..500 {
        if !pages.loading() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    pages
}

/// The scene of `m` with the tileset drawn from above, its pages decoded as a running client has them.
fn flat_scene_of(m: &EngineModel, tag: &str) -> Scene {
    let tileset = Tileset::load(&flat_root(tag).join("gfx/flat")).unwrap();
    let pages = decoded(&tileset);
    scene::build(m, &tileset, Some(&pages), None, 1)
}

#[test]
fn terrain_of_a_tileset_drawn_from_above_lies_on_the_ground_even_with_clear_rows_on_top() {
    let m = model(vec![cell(4, 4, 0, "t_shrub"), cell(5, 4, 0, "t_grass")]);
    let scene = flat_scene_of(&m, "flat-shrub");
    let shrub = at(&scene, &m, (4, 4));
    assert_eq!(
        shrub.len(),
        2,
        "the grass under it and the shrub: {shrub:?}"
    );
    assert!(shrub.iter().all(|i| i.flags[0] & BOARD == 0), "{shrub:?}");
    assert!(shrub.iter().all(|i| i.origin[2] == 0.0), "{shrub:?}");
}

/// The view square of absolute square `abs`, as the corner of the ground quad that covers it.
fn corner(m: &EngineModel, abs: (i32, i32)) -> [f32; 2] {
    let (x, y) = square(m, abs);
    [x as f32, y as f32]
}

#[test]
fn a_tree_drawn_from_above_stands_on_the_ground_its_own_sprites_draw_in_its_square() {
    let m = model(vec![cell(4, 4, 0, "t_tree")]);
    let scene = flat_scene_of(&m, "flat-tree");
    let (boards, ground): (Vec<_>, Vec<_>) = at(&scene, &m, (4, 4))
        .into_iter()
        .partition(|i| i.flags[0] & BOARD != 0);
    assert_eq!(boards.len(), 1, "the tree stands: {boards:?}");
    // Its ground sprite is twice as tall as the tile with grass in the square only: it covers the square.
    assert_eq!(ground.len(), 1, "{ground:?}");
    assert_eq!(
        ground[0].origin,
        [corner(&m, (4, 4))[0], corner(&m, (4, 4))[1], 0.0]
    );
    assert_eq!(
        (ground[0].a, ground[0].b),
        ([1.0, 0.0, 0.0], [0.0, 1.0, 0.0])
    );
}

#[test]
fn a_flower_drawn_from_above_lies_over_the_ground_where_a_person_drawn_alike_stands() {
    let mut flower = cell(4, 4, 0, "t_grass");
    flower["furniture"] = look("furniture", "f_flower");
    let m = model(vec![flower, cell(3, 4, 0, "t_grass")]);
    let scene = flat_scene_of(&m, "flat-flower");
    let quads = at(&scene, &m, (4, 4));
    assert_eq!(quads.len(), 2, "{quads:?}");
    assert!(quads.iter().all(|i| i.flags[0] & BOARD == 0), "{quads:?}");
    let (grass, flower) = (quads[0], quads[1]);
    // Only its eight pixels across lie where the flat map draws them, over the grass.
    let [x, y] = corner(&m, (4, 4));
    assert_eq!(flower.origin[..2], [x + 12.0 / 32.0, y + 12.0 / 32.0]);
    assert_eq!(flower.a[0], 8.0 / 32.0);
    assert!(flower.origin[2] > grass.origin[2], "{quads:?}");
    // The avatar at (3, 4) is a person drawn with the same sprite: a creature stands.
    let me = at(&scene, &m, (3, 4));
    assert!(me.iter().any(|i| i.flags[0] & BOARD != 0), "{me:?}");
}
