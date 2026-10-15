#![recursion_limit = "512"]
//! What the 3D map's scene holds for a replica: which levels, which filters, which shapes.
mod common;
use bn_client_egui::EngineModel;
use bn_client_egui::GameModel;
use bn_client_egui::iso::geometry::{BOARD, Instance, PageId};
use bn_client_egui::iso::scene::{self, Scene};
use bn_client_egui::tileset::Tileset;
use bn_client_egui::transport::ScriptedTransport;
use common::{fixture, iso_root};
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
    scene::build(m, &tileset, None, 1)
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
        .filter(|i| i.flags[0] & 0xff != BOARD)
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
    assert!(me.iter().any(|i| i.flags[0] & 0xff == BOARD), "{me:?}");
}
