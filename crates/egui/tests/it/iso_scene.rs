//! What the 3D map's scene holds for a replica: which levels, which filters, which shapes.
use crate::common::{fixture, flat_root, iso_root};
use bn_client_egui::EngineModel;
use bn_client_egui::GameModel;
use bn_client_egui::iso::camera::{Camera, Viewport};
use bn_client_egui::iso::geometry::{BOARD, FADE, GLOW, Instance, PageId, SHEER, VOID};
use bn_client_egui::iso::pages::Pages;
use bn_client_egui::iso::scene::{self, Scene};
use bn_client_egui::tileset::Tileset;
use bn_client_egui::tint::Paint;
use bn_client_egui::transport::ScriptedTransport;
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
    scene::build(m, &tileset, None, None, 1, None)
}

fn all(scene: &Scene) -> Vec<Instance> {
    scene
        .batches
        .iter()
        .flat_map(|b| b.instances.clone())
        .collect()
}

/// The quads that are not boards and not the avatar's, who stands in every model.
fn planes(scene: &Scene) -> Vec<Instance> {
    let (x, y) = square(&model(vec![]), (3, 4));
    all(scene)
        .into_iter()
        .filter(|i| i.flags[0] & BOARD == 0)
        .filter(|i| i.origin[0].floor() != x as f32 || i.origin[1].floor() != y as f32)
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

fn open_air(x: i32, y: i32) -> Value {
    let mut air =
        json!({"at": {"dim": "", "x": x, "y": y, "z": 0}, "known": "visible", "below": 1});
    air["terrain"] = serde_json::Value::Null;
    air
}

/// The quads of solid unknown in the scene.
fn voids(scene: &Scene) -> Vec<Instance> {
    all(scene)
        .into_iter()
        .filter(|i| i.flags[0] & VOID != 0)
        .collect()
}

#[test]
fn open_air_over_ground_nothing_is_known_about_is_dark_ground_at_its_depth_walled_up_to_the_level()
{
    let level = 108.0 * 0.034_026;
    let m = model(vec![
        cell(4, 4, 0, "t_dirt"),
        open_air(5, 4),
        cell(6, 4, 0, "t_dirt"),
    ]);
    let scene = scene_of(&m, "scene-void");
    let void = voids(&scene);
    // Dark ground one level down, no lid at the level, and a wall from it up to each slab beside it.
    let grounds = void.iter().filter(|i| i.b[2] == 0.0).collect::<Vec<_>>();
    let walls = void.iter().filter(|i| i.b[2] < 0.0).collect::<Vec<_>>();
    assert_eq!(grounds.len(), 1, "{void:?}");
    assert!((grounds[0].origin[2] + level).abs() < 0.02, "{grounds:?}");
    assert_eq!(walls.len(), 2, "{void:?}");
    assert!(
        walls
            .iter()
            .all(|w| w.origin[2] == 0.0 && (w.b[2] + level).abs() < 0.02),
        "{walls:?}"
    );
    assert!(
        void.iter().all(|i| i.flags[1] > 0 && i.flags[1] < 64),
        "a dark that is not the black of nothing: {void:?}"
    );
    // The walls fade from light at their top to the dark at their foot; the ground does not.
    assert!(
        walls
            .iter()
            .all(|w| w.flags[0] & FADE != 0 && w.uv[1] < 0.01 && w.uv[1] + w.uv_b[1] > 0.99),
        "{walls:?}"
    );
    assert!(
        grounds.iter().all(|g| g.flags[0] & FADE == 0),
        "{grounds:?}"
    );
    // Nothing known about a square that does not look through to ground is no block.
    assert!(at(&scene, &m, (8, 8)).is_empty());
}

#[test]
fn dark_ground_meets_known_ground_as_deep_without_a_face_and_a_slab_edge_walls_down_to_either() {
    let level = 108.0 * 0.034_026;
    let mut lowered = vec![open_air(7, 4), cell(7, 4, -1, "t_dirt")];
    lowered[1]["known"] = "remembered".into();
    let mut cells = vec![open_air(6, 4), cell(5, 4, 0, "t_floor")];
    cells.append(&mut lowered);
    let m = model(cells);
    let scene = scene_of(&m, "scene-void-faces");
    // The dark ground at (6, 4) lies as deep as the ground east of it: no face between them, and the slab at (5, 4)
    // walls down to it.
    let void = voids(&scene);
    let walls = void.iter().filter(|i| i.b[2] < 0.0).collect::<Vec<_>>();
    assert_eq!(void.len(), 2, "its ground and the slab's wall: {void:?}");
    assert_eq!(walls.len(), 1, "{void:?}");
    let ground = void.iter().find(|i| i.b[2] == 0.0).expect("its ground");
    assert_eq!(
        walls[0].origin[0], ground.origin[0],
        "on the west edge of it: {void:?}"
    );
    assert!((walls[0].b[2] + level).abs() < 0.02, "{walls:?}");
    // A slab beside lowered ground is cut off by a face down to it.
    let m = model(vec![
        cell(5, 4, 0, "t_floor"),
        open_air(6, 4),
        cell(6, 4, -1, "t_dirt"),
    ]);
    let scene = scene_of(&m, "scene-slab-face");
    let edge = voids(&scene);
    assert_eq!(edge.len(), 1, "{edge:?}");
    assert!((edge[0].b[2] + level).abs() < 0.02, "{edge:?}");
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
fn coloured_light_on_a_square_in_sight_is_added_over_its_sprites_as_on_the_flat_map() {
    let fire = json!({"color": "#ff8000", "strength": 255});
    let mut lit = cell(4, 4, 0, "t_dirt");
    lit["light"] = 3.into();
    lit["glow"] = fire.clone();
    let mut dim = cell(5, 4, 0, "t_dirt");
    dim["light"] = 1.into();
    dim["glow"] = fire;
    let plain = cell(6, 4, 0, "t_dirt");
    let m = model(vec![lit, dim, plain]);
    let scene = scene_of(&m, "scene-glow");
    let lights = [(4, 4), (5, 4), (6, 4)].map(|p| at(&scene, &m, p)[0].flags[2]);
    // Red 255 and green 128 at the game's highest alpha, 176.
    let fire = u32::from_le_bytes([176, 88, 0, 0]);
    assert_eq!(lights, [fire, fire, 0]);
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
        let on = v.project(point).unwrap();
        assert_eq!(scene.pick(&v, on), Some(wall), "{what}");
    }
    // The ground plane alone would have named a square behind the wall for the south face.
    let on_face = v
        .project(glam::Vec3::new(x + 0.5, y + 1.0, top / 2.0))
        .unwrap();
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
        let on = v
            .project(glam::Vec3::new(at.0 as f32 + 0.5, at.1 as f32 + 0.5, 0.0))
            .unwrap();
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
    let on = v
        .project(glam::Vec3::new(
            lowered.0 as f32 + 0.5,
            lowered.1 as f32 + 0.5,
            ground,
        ))
        .unwrap();
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
    assert!(!pages.loading(), "the sheets are decoded");
    pages
}

/// The scene of `m` with the tileset drawn from above, its pages decoded as a running client has them.
fn flat_scene_of(m: &EngineModel, tag: &str) -> Scene {
    let tileset = Tileset::load(&flat_root(tag).join("gfx/flat")).unwrap();
    let pages = decoded(&tileset);
    scene::build(m, &tileset, Some(&pages), None, 1, None)
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
    assert!(shrub.iter().all(|i| i.origin[2].abs() < 0.01), "{shrub:?}");
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
    // Furniture is neither an item nor a creature: it lies.
    assert_eq!(quads.len(), 2, "{quads:?}");
    assert!(quads.iter().all(|i| i.flags[0] & BOARD == 0), "{quads:?}");
    let (grass, flower) = (quads[0], quads[1]);
    // Only its eight pixels across lie where the flat map draws them, over the grass.
    let [x, y] = corner(&m, (4, 4));
    assert_eq!(flower.origin[..2], [x + 12.0 / 32.0, y + 12.0 / 32.0]);
    assert_eq!(flower.a[0], 8.0 / 32.0);
    assert!(flower.origin[2] < 0.01, "{quads:?}");
    assert!(flower.origin[2] > grass.origin[2], "{quads:?}");
    // The avatar at (3, 4) is a person drawn with the same sprite: a creature stands as a board.
    let me = at(&scene, &m, (3, 4));
    assert!(me.iter().any(|i| i.flags[0] & BOARD != 0), "{me:?}");
}

#[test]
fn an_item_lying_on_the_ground_of_a_tileset_drawn_from_above_has_the_thickness_of_its_pixels() {
    let mut pile = cell(4, 4, 0, "t_grass");
    pile["items"] = json!([look("item", "f_flower")]);
    let m = model(vec![pile]);
    let scene = flat_scene_of(&m, "flat-item");
    let quads = at(&scene, &m, (4, 4));
    // The grass, and the item's top, bottom and the four sides of its eight pixels across.
    assert_eq!(quads.len(), 1 + 2 + 4, "{quads:?}");
    assert!(quads.iter().all(|i| i.flags[0] & BOARD == 0), "{quads:?}");
    assert_eq!(sides(&quads).len(), 4, "{quads:?}");
    assert!(
        sides(&quads)
            .iter()
            .all(|i| (i.b[2] + 1.0 / 16.0).abs() < 0.01),
        "{quads:?}"
    );
}

#[test]
fn fields_traps_and_furniture_drawn_from_above_are_not_thick_and_an_item_keeps_its_walls_solid() {
    let thing = |x: i32, slot: &str, kind: &str, form: Option<&str>| {
        let mut c = cell(x, 4, 0, "t_grass");
        let mut l = look(kind, "f_flower");
        if let Some(form) = form {
            l["form"] = form.into();
        }
        c[slot] = match slot {
            "furniture" => l,
            "fields" => json!([{"look": l, "intensity": 1}]),
            _ => json!([l]),
        };
        c
    };
    let mut walled = wall(4, 6, "t_wall");
    walled["items"] = json!([look("item", "f_flower")]);
    let m = model(vec![
        thing(4, "traps", "trap", None),
        thing(5, "fields", "field", None),
        thing(6, "furniture", "furniture", None),
        thing(7, "fields", "field", Some("fire")),
        thing(8, "fields", "field", Some("gas")),
        thing(9, "furniture", "furniture", Some("seat")),
        walled,
    ]);
    let scene = flat_scene_of(&m, "flat-not-thick");
    // What is built in a square may start on its far edges.
    let within = |abs: (i32, i32)| {
        let (x, y) = square(&m, abs);
        let (x, y) = (x as f32, y as f32);
        all(&scene)
            .into_iter()
            .filter(|i| {
                (x..=x + 1.0).contains(&i.origin[0]) && (y..=y + 1.0).contains(&i.origin[1])
            })
            .collect::<Vec<_>>()
    };
    // A trap, blood and furniture without a form lie on the grass.
    for x in [4, 5, 6] {
        let quads = at(&scene, &m, (x, 4));
        assert_eq!(quads.len(), 2, "{x}: {quads:?}");
        assert!(quads.iter().all(|i| i.origin[2] < 0.01), "{x}: {quads:?}");
    }
    // A fire is two crossed boards that give light, a gas is seen through.
    let fire = within((7, 4))
        .into_iter()
        .filter(|i| i.flags[0] & GLOW != 0)
        .collect::<Vec<_>>();
    assert_eq!(fire.len(), 2, "{fire:?}");
    assert!(fire.iter().all(|i| i.b == [0.0, 0.0, -1.0]), "{fire:?}");
    assert!(within((8, 4)).iter().any(|i| i.flags[0] & SHEER != 0));
    // A seat is built as one: its back reaches a block up.
    let seat = within((9, 4))
        .into_iter()
        .map(|i| i.origin[2])
        .fold(f32::MIN, f32::max);
    assert!((seat - 1.0).abs() < 0.01, "{seat}");
    // An item on a wall's square leaves the wall its solid.
    let (x, y) = square(&m, (4, 6));
    assert_eq!(scene.column(x, y).unwrap().solid, Some((0.0, 3.0)));
}

/// A square of terrain `id` the engine calls a wall.
fn wall(x: i32, y: i32, id: &str) -> Value {
    let mut walled = cell(x, y, 0, id);
    walled["terrain"]["wall"] = true.into();
    walled
}

/// The upright side faces among `quads`: those that drop straight down.
fn sides(quads: &[Instance]) -> Vec<Instance> {
    quads
        .iter()
        .filter(|i| i.flags[0] & BOARD == 0 && i.b[2] < 0.0)
        .copied()
        .collect()
}

#[test]
fn an_opaque_wall_drawn_from_above_is_three_blocks_of_its_top_and_four_sides_a_level_tall() {
    let m = model(vec![wall(4, 4, "t_wall")]);
    let scene = flat_scene_of(&m, "flat-solid");
    let quads = planes(&scene);
    // The top, and four sides of three 1 m blocks stacked to the 3 m level.
    assert_eq!(quads.len(), 1 + 4 * 3, "{quads:?}");
    let (x, y) = square(&m, (4, 4));
    let (x, y) = (x as f32, y as f32);
    // The top is the whole sprite over the whole square, a level up.
    let lids = quads
        .iter()
        .filter(|i| i.b[2] == 0.0)
        .map(|i| (i.origin, i.a, i.b))
        .collect::<Vec<_>>();
    assert_eq!(lids, [([x, y, 3.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0])]);
    // Each side spans a whole edge of the square, a block tall, one block at each height from the top down.
    let sides = sides(&quads);
    assert_eq!(sides.len(), 4 * 3, "{sides:?}");
    assert!(
        sides.iter().all(
            |i| (i.b[2] + 1.0).abs() < 1e-4 && (i.a[0].abs() + i.a[1].abs() - 1.0).abs() < 1e-4
        ),
        "{sides:?}"
    );
    let mut tops = sides.iter().map(|i| i.origin[2]).collect::<Vec<_>>();
    tops.sort_by(f32::total_cmp);
    tops.dedup();
    assert_eq!(tops, [1.0, 2.0, 3.0]);
    assert_eq!(
        scene.column(x as i32, y as i32).unwrap().solid,
        Some((0.0, 3.0))
    );
}

#[test]
fn a_side_face_shows_the_whole_sprite_at_its_own_aspect_on_each_block() {
    let m = model(vec![wall(4, 4, "t_wall")]);
    let scene = flat_scene_of(&m, "flat-side-uv");
    let sides = sides(&planes(&scene));
    assert!(!sides.is_empty());
    // The wall is sprite 2 of its sheet: 64 pixels from the left of its page, 32 pixels square. Each block's face
    // shows it from its top left corner, as many pixels across as down: no row is stretched.
    for i in &sides {
        let (corner, across, down) = (
            glam::Vec2::from(i.uv),
            glam::Vec2::from(i.uv_a),
            glam::Vec2::from(i.uv_b),
        );
        assert!(
            (corner - glam::Vec2::new(64.0, 0.0)).length() < 0.15,
            "{i:?}"
        );
        assert!(
            (across - glam::Vec2::new(32.0, 0.0)).length() < 0.15,
            "{i:?}"
        );
        assert!((down - glam::Vec2::new(0.0, 32.0)).length() < 0.15, "{i:?}");
    }
}

#[test]
fn a_wall_with_a_hole_is_whole_blocks_whose_faces_show_the_hole() {
    let m = model(vec![wall(4, 4, "t_wall_holed")]);
    let scene = flat_scene_of(&m, "flat-hole");
    let quads = planes(&scene);
    // As a block game draws a block with clear pixels: the top and the blocks' faces, nothing round the hole.
    assert_eq!(quads.len(), 1 + 4 * 3, "{quads:?}");
    let (x, y) = square(&m, (4, 4));
    let inner = |v: f32, base: i32| (0.01..=0.99).contains(&(v - base as f32));
    let around = sides(&quads)
        .into_iter()
        .filter(|i| inner(i.origin[0], x) && inner(i.origin[1], y))
        .count();
    assert_eq!(around, 0, "{quads:?}");
    // Each face is the whole sprite, its hole with it.
    assert!(
        quads
            .iter()
            .all(|i| (glam::Vec2::from(i.uv_a).length() - 32.0).abs() < 0.15),
        "{quads:?}"
    );
}

#[test]
fn walls_side_by_side_leave_out_the_faces_they_share() {
    let row = model(vec![
        wall(4, 4, "t_wall"),
        wall(5, 4, "t_wall"),
        wall(6, 4, "t_wall"),
    ]);
    let scene = flat_scene_of(&row, "flat-row");
    // Each end wall's top and three sides of three blocks, the middle one's top and two sides.
    assert_eq!(
        planes(&scene).len(),
        (1 + 3 * 3) + (1 + 2 * 3) + (1 + 3 * 3)
    );
    let (x, _) = square(&row, (5, 4));
    let shared = sides(&planes(&scene))
        .into_iter()
        .filter(|i| i.a[0] == 0.0 && [x, x + 1].contains(&(i.origin[0] as i32)))
        .count();
    assert_eq!(shared, 0, "no face between two walls");
    // A wall whose sprite fills half its square is still whole blocks: it and the wall beside it hide the faces
    // they share.
    let half = model(vec![wall(4, 4, "t_wall"), wall(5, 4, "t_wall_half")]);
    let scene = flat_scene_of(&half, "flat-half");
    assert_eq!(planes(&scene).len(), 2 * (1 + 3 * 3));
}

#[test]
fn a_vehicle_part_drawn_from_above_is_a_solid_half_a_square_tall_on_its_ground() {
    let mut car = cell(5, 4, 0, "t_grass");
    car["vehicle"] = look("vehicle_part", "vp_frame");
    let m = model(vec![car, cell(6, 4, 0, "t_wall")]);
    let scene = flat_scene_of(&m, "flat-car");
    // The grass, the part's own grass background lying under it, the part's six faces and the flat terrain beside it.
    let quads = planes(&scene);
    assert_eq!(quads.len(), 1 + 1 + 6 + 1, "{quads:?}");
    let top = quads.iter().map(|i| i.origin[2]).fold(f32::MIN, f32::max);
    assert!((top - 0.5).abs() < 0.01, "{quads:?}");
}

#[test]
fn a_vehicle_part_beside_a_wall_hides_its_side_there_and_the_taller_wall_keeps_its_own() {
    let mut car = cell(5, 4, 0, "t_grass");
    car["vehicle"] = look("vehicle_part", "vp_frame");
    let m = model(vec![wall(4, 4, "t_wall"), car]);
    let scene = flat_scene_of(&m, "flat-car-wall");
    // The wall's top and four sides of three blocks, the grass and the part's grass background, and the part's faces
    // but its west one.
    assert_eq!(planes(&scene).len(), (1 + 4 * 3) + 1 + 1 + 5);
    let (x, _) = square(&m, (5, 4));
    let between = sides(&planes(&scene))
        .into_iter()
        .filter(|i| i.a[0] == 0.0 && i.origin[0] == x as f32)
        .collect::<Vec<_>>();
    assert_eq!(between.len(), 3, "the wall's three blocks: {between:?}");
    assert!(
        between.iter().all(|i| (i.b[2] + 1.0).abs() < 1e-4),
        "the wall's: {between:?}"
    );
}

#[test]
fn a_tree_of_an_isometric_tileset_stands_on_its_ground_which_lies_in_its_square() {
    let m = model(vec![cell(4, 4, 0, "t_tree")]);
    let tileset = Tileset::load(&iso_root("iso-tree").join("gfx/iso")).unwrap();
    let pages = decoded(&tileset);
    let scene = scene::build(&m, &tileset, Some(&pages), None, 1, None);
    let (boards, ground): (Vec<_>, Vec<_>) = at(&scene, &m, (4, 4))
        .into_iter()
        .partition(|i| i.flags[0] & BOARD != 0);
    // The ground is a sprite as wide as the tree's, but it draws only in its tile's columns.
    assert_eq!(boards.len(), 1, "the tree stands: {boards:?}");
    assert_eq!(ground.len(), 1, "its ground lies: {ground:?}");
    let [x, y] = corner(&m, (4, 4));
    assert_eq!(ground[0].origin, [x, y, 0.0]);
}

#[test]
fn what_covers_some_of_a_person_drawn_from_above_stands_that_share_of_a_person_on_its_ground() {
    let mut shrub = cell(4, 4, 0, "t_shrub");
    shrub["terrain"]["coverage"] = 40.into();
    let mut flower = cell(6, 4, 0, "t_grass");
    flower["furniture"] = look("furniture", "f_flower");
    flower["furniture"]["coverage"] = 80.into();
    let m = model(vec![shrub, flower]);
    let scene = flat_scene_of(&m, "flat-standing");
    // A person is two blocks tall: what covers 40% of one is 0.8 of a block.
    let (x, y) = square(&m, (4, 4));
    let (fx, fy) = (x as f32, y as f32);
    let quads = all(&scene)
        .into_iter()
        .filter(|i| {
            (fx..=fx + 1.0).contains(&i.origin[0]) && (fy..=fy + 1.0).contains(&i.origin[1])
        })
        .collect::<Vec<_>>();
    // The grass under it lies; the shrub is its top and its four sides.
    let ground = quads
        .iter()
        .filter(|i| i.origin[2].abs() < 0.01 && i.b[2] == 0.0)
        .count();
    assert_eq!(quads.len(), 1 + 1 + 4, "{quads:?}");
    assert_eq!(ground, 1, "the grass: {quads:?}");
    let top = quads.iter().map(|i| i.origin[2]).fold(f32::MIN, f32::max);
    assert!((top - 0.8).abs() < 0.01, "{quads:?}");
    assert!(
        sides(&quads).iter().all(|i| (i.b[2] + 0.8).abs() < 0.01),
        "{quads:?}"
    );
    let solid = scene.column(x, y).unwrap().solid.unwrap();
    assert!((solid.1 - 0.8).abs() < 0.01, "{solid:?}");
    // Furniture stands by its own coverage.
    let furniture = at(&scene, &m, (6, 4));
    let top = furniture
        .iter()
        .map(|i| i.origin[2])
        .fold(f32::MIN, f32::max);
    assert!((top - 1.6).abs() < 0.01, "{furniture:?}");
    assert!(
        furniture.iter().all(|i| i.flags[0] & BOARD == 0),
        "{furniture:?}"
    );
}

#[test]
fn what_covers_nothing_of_a_person_drawn_from_above_still_lies_on_the_ground() {
    let mut shrub = cell(4, 4, 0, "t_shrub");
    shrub["terrain"]["coverage"] = 0.into();
    let m = model(vec![shrub]);
    let scene = flat_scene_of(&m, "flat-covers-nothing");
    let quads = at(&scene, &m, (4, 4));
    assert_eq!(quads.len(), 2, "{quads:?}");
    assert!(quads.iter().all(|i| i.origin[2].abs() < 0.01), "{quads:?}");
}

/// A square of terrain `id` the engine calls a window in a wall's line.
fn window(x: i32, y: i32, id: &str) -> Value {
    let mut glazed = wall(x, y, id);
    glazed["terrain"]["window"] = true.into();
    glazed
}

#[test]
fn a_window_drawn_from_above_is_a_sill_block_and_a_lintel_block_of_its_wall_with_a_thin_pane_between()
 {
    let m = model(vec![window(4, 4, "t_wall")]);
    let scene = flat_scene_of(&m, "flat-window");
    let quads = planes(&scene);
    // Each of the sill, the pane and the lintel: a top and four sides.
    assert_eq!(quads.len(), 3 * 5, "{quads:?}");
    let mut lids = quads
        .iter()
        .filter(|i| i.b[2] == 0.0)
        .map(|i| i.origin[2])
        .collect::<Vec<_>>();
    lids.sort_by(f32::total_cmp);
    let expected = [1.0, 2.0, 3.0];
    assert!(
        lids.len() == 3 && lids.iter().zip(expected).all(|(a, b)| (a - b).abs() < 0.01),
        "{lids:?}"
    );
    // In the opening only the pane is drawn, thin and along the line of the wall.
    let (x, y) = square(&m, (4, 4));
    let opening = quads
        .iter()
        .filter(|i| i.origin[2] > 1.0 + 0.01 && i.origin[2] <= 2.0 + 0.01)
        .collect::<Vec<_>>();
    assert_eq!(opening.len(), 5, "{opening:?}");
    assert!(
        opening.iter().all(|i| {
            let ys = [i.origin[1], i.origin[1] + i.a[1] + i.b[1]];
            ys.iter().all(|v| (v - y as f32 - 0.5).abs() < 0.05)
        }),
        "{opening:?}"
    );
    assert_eq!(scene.column(x, y).unwrap().solid, Some((0.0, 3.0)));
}

#[test]
fn a_wall_beside_a_window_keeps_the_face_seen_through_its_opening_and_hides_the_windows() {
    let m = model(vec![window(4, 4, "t_wall"), wall(5, 4, "t_wall")]);
    let scene = flat_scene_of(&m, "flat-window-wall");
    // The wall's top and four sides of three blocks; the sill, the pane and the lintel without their east sides
    // against it.
    assert_eq!(planes(&scene).len(), (1 + 4 * 3) + 3 * 4);
    let (x, _) = square(&m, (5, 4));
    let between = sides(&planes(&scene))
        .into_iter()
        .filter(|i| i.a[0] == 0.0 && i.origin[0] == x as f32)
        .collect::<Vec<_>>();
    assert_eq!(between.len(), 3, "the wall's three blocks: {between:?}");
    assert!(
        between.iter().all(|i| (i.b[2] + 1.0).abs() < 1e-4),
        "the wall's: {between:?}"
    );
}

#[test]
fn a_window_of_an_isometric_tileset_is_its_walls_box() {
    let m = model(vec![window(5, 4, "t_wall")]);
    let wall = planes(&scene_of(&m, "scene-window-box"));
    assert_eq!(wall.len(), 5, "a lid and four faces");
}

#[test]
fn a_fire_in_the_3d_scene_shows_the_sprite_of_its_animation_frame() {
    let m = model(vec![cell(4, 4, 0, "fd_fire")]);
    let tileset = Tileset::load(&crate::common::game_root("iso-fire").join("gfx/test")).unwrap();
    let at_frame = |frame| scene::build(&m, &tileset, None, None, 1, frame);
    let frames = (0..5).map(|f| all(&at_frame(Some(f)))).collect::<Vec<_>>();
    assert!(at_frame(Some(0)).animated);
    assert!(
        frames.iter().any(|f| *f != frames[0]),
        "the fire changes over a loop"
    );
    assert!(!at_frame(None).animated, "still with reduced motion");
    let grass = model(vec![cell(4, 4, 0, "grass")]);
    assert!(!scene::build(&grass, &tileset, None, None, 1, Some(0)).animated);
}

#[test]
fn every_step_down_between_lowered_grounds_has_a_fading_face_from_the_lower_up_to_the_higher() {
    let level = 108.0 * 0.034_026;
    let mut deep = open_air(6, 4);
    deep["below"] = 2.into();
    // Dark ground one level down, dark ground two levels down, and ground one level down.
    let m = model(vec![
        open_air(5, 4),
        deep,
        open_air(7, 4),
        cell(7, 4, -1, "t_dirt"),
    ]);
    let scene = scene_of(&m, "scene-void-steps");
    let walls = voids(&scene)
        .into_iter()
        .filter(|i| i.b[2] < 0.0)
        .collect::<Vec<_>>();
    // One face on each side of the deeper square, each a level tall from two levels down.
    assert_eq!(walls.len(), 2, "{walls:?}");
    assert!(
        walls.iter().all(|w| (w.origin[2] + level).abs() < 0.02
            && (w.b[2] + level).abs() < 0.02
            && w.flags[0] & FADE != 0),
        "{walls:?}"
    );
}

fn roofed(x: i32, y: i32) -> Value {
    let mut square = cell(x, y, 0, "t_floor");
    square["roofed"] = true.into();
    square
}

#[test]
fn a_roofed_square_has_a_flat_dark_ceiling_one_level_up_that_faces_down() {
    let level = 108.0 * 0.034_026;
    let m = model(vec![roofed(4, 4), cell(5, 4, 0, "t_floor")]);
    let scene = scene_of(&m, "scene-ceiling");
    let o = m.origin();
    let lids = scene
        .ceiling
        .iter()
        .filter(|i| i.b[2] == 0.0)
        .collect::<Vec<_>>();
    assert_eq!(lids.len(), 1, "{:?}", scene.ceiling);
    let lid = *lids[0];
    assert_eq!(
        (lid.origin[0], lid.origin[1]),
        ((4 - o.x) as f32, (4 - o.y) as f32)
    );
    assert!((lid.origin[2] - level).abs() < 0.02, "{lid:?}");
    let normal_z = lid.a[0] * lid.b[1] - lid.a[1] * lid.b[0];
    assert!(normal_z < 0.0, "faces down: {lid:?}");
    assert!(lid.flags[0] & VOID != 0 && lid.flags[1] > 0, "{lid:?}");
    // The ceiling is not part of the scene the orbit camera looks into from above.
    assert!(
        all(&scene).iter().all(|i| i.origin[2] < level - 0.02),
        "{:?}",
        all(&scene)
    );
}

#[test]
fn a_square_seen_through_open_air_gets_no_ceiling() {
    let mut air = open_air(4, 4);
    air["roofed"] = true.into();
    let m = model(vec![air, cell(4, 4, -1, "t_dirt")]);
    assert!(scene_of(&m, "scene-ceiling-air").ceiling.is_empty());
}

#[test]
fn weather_does_not_fall_on_an_avatar_under_a_roof() {
    assert!(!bn_client_egui::iso::sheltered(&model(vec![cell(
        3, 4, 0, "t_floor"
    )])));
    assert!(bn_client_egui::iso::sheltered(&model(vec![roofed(3, 4)])));
}

#[test]
fn a_ceiling_is_closed_down_to_the_wall_beside_it_and_to_the_lintel_of_a_gap() {
    let level = 108.0 * 0.034_026;
    // A wall to the east of the roofed square, and open floor to the south.
    let m = model(vec![
        roofed(4, 4),
        cell(5, 4, 0, "t_wall"),
        cell(4, 5, 0, "t_floor"),
    ]);
    let scene = scene_of(&m, "scene-ceiling-walls");
    let o = m.origin();
    let faces = scene
        .ceiling
        .iter()
        .filter(|i| i.b[2] < 0.0)
        .collect::<Vec<_>>();
    let wall_top = scene.column(5 - o.x, 4 - o.y).unwrap().top();
    // The faces on the south edge run along x, and those on the east edge run toward the north.
    let east = faces.iter().filter(|f| f.a[1] < 0.0);
    let south = faces.iter().filter(|f| f.a[0] > 0.0);
    // Walls as tall as the ceiling need no face; a shorter one is closed above.
    assert_eq!(
        east.count(),
        usize::from(wall_top < level - 1e-3),
        "{faces:?}"
    );
    let gap = south.collect::<Vec<_>>();
    assert_eq!(gap.len(), 1, "{faces:?}");
    assert!(
        (gap[0].origin[2] - level).abs() < 1e-3 && gap[0].origin[2] + gap[0].b[2] > 0.0,
        "closed above a doorway's lintel: {gap:?}"
    );
}

#[test]
fn a_ceiling_beside_squares_nothing_is_known_about_is_closed_from_the_floor_so_no_sky_shows() {
    let level = 108.0 * 0.034_026;
    // Only the roofed square is known, away from the avatar: every edge of it faces the unknown.
    let m = model(vec![roofed(8, 8)]);
    let scene = scene_of(&m, "scene-ceiling-unknown");
    let faces = scene
        .ceiling
        .iter()
        .filter(|i| i.b[2] < 0.0)
        .collect::<Vec<_>>();
    assert_eq!(faces.len(), 4, "{faces:?}");
    assert!(
        faces
            .iter()
            .all(|f| (f.origin[2] - level).abs() < 1e-3 && (f.origin[2] + f.b[2]).abs() < 1e-3),
        "from the floor up to the ceiling: {faces:?}"
    );
}
