mod common;
use bn_client_egui::tileset::{self, Query, Tileset, Turn, mix};
use common::game_root;

fn load(tag: &str) -> Tileset {
    Tileset::load(&game_root(tag).join("gfx/test")).unwrap()
}

fn query<'a>(id: &'a str) -> Query<'a> {
    Query {
        id,
        fallbacks: &[],
        subtile: None,
        rotation: 0,
        mirrored: false,
        creature: false,
        seed: 0,
        season: None,
    }
}

fn fg(set: &Tileset, q: &Query) -> Option<(u32, Turn)> {
    let placed = set.choose(q)?.fg?;
    Some((placed.sprite, placed.turn))
}

#[test]
fn tilesets_are_listed_by_the_name_and_view_of_their_tileset_txt() {
    let found = tileset::available(&game_root("list"));
    assert_eq!(found.len(), 1);
    assert_eq!(
        (found[0].name.as_str(), found[0].view.as_str()),
        ("test", "Test Set")
    );
}

#[test]
fn sprites_are_numbered_across_sheets_and_a_missing_empty_fallback_sheet_is_skipped() {
    let set = load("numbering");
    assert_eq!(set.sheets.len(), 2);
    assert_eq!(set.tile, (4, 4));
    assert_eq!(set.locate(7), Some((0, 7)));
    assert_eq!(set.locate(8), Some((1, 0)));
    assert_eq!(set.locate(9), Some((1, 1)));
    assert_eq!(set.locate(10), None);
}

#[test]
fn an_id_without_a_tile_uses_the_first_fallback_that_has_one() {
    let set = load("fallback");
    let chain = ["t_also_missing".to_string(), "t_floor".to_string()];
    let q = Query {
        fallbacks: &chain,
        ..query("t_missing")
    };
    assert_eq!(fg(&set, &q), Some((0, Turn::None)));
    assert_eq!(fg(&set, &query("t_missing")), None);
}

#[test]
fn a_tile_that_does_not_rotate_stays_upright_but_a_creature_keeps_its_facing() {
    let set = load("rotates");
    assert_eq!(
        fg(
            &set,
            &Query {
                rotation: 3,
                ..query("t_floor")
            }
        ),
        Some((0, Turn::None))
    );
    let facing_left = Query {
        mirrored: true,
        creature: true,
        ..query("mon_a")
    };
    assert_eq!(fg(&set, &facing_left), Some((7, Turn::Mirror)));
    let facing_right = Query {
        creature: true,
        ..query("mon_b")
    };
    assert_eq!(fg(&set, &facing_right), Some((7, Turn::None)));
}

#[test]
fn multitile_parts_follow_the_engines_subtile_and_rotation() {
    let set = load("multitile");
    // Two sprites are the north/south and east/west pictures: the rotation picks, nothing is turned.
    let edge = |rotation| Query {
        subtile: Some("edge"),
        rotation,
        ..query("t_wall")
    };
    assert_eq!(fg(&set, &edge(0)), Some((2, Turn::None)));
    assert_eq!(fg(&set, &edge(1)), Some((3, Turn::None)));
    assert_eq!(fg(&set, &edge(2)), Some((2, Turn::None)));
    // One sprite is turned by quarter turns.
    let corner = |rotation| Query {
        subtile: Some("corner"),
        rotation,
        ..query("t_wall")
    };
    assert_eq!(fg(&set, &corner(1)), Some((4, Turn::Left)));
    assert_eq!(fg(&set, &corner(2)), Some((4, Turn::Half)));
    assert_eq!(fg(&set, &corner(3)), Some((4, Turn::Right)));
    // A part the tile does not define draws the tile itself, turned.
    let center = Query {
        subtile: Some("center"),
        rotation: 2,
        ..query("t_wall")
    };
    assert_eq!(fg(&set, &center), Some((1, Turn::Half)));
}

#[test]
fn seasonal_sprites_replace_the_plain_tile_in_their_season_only() {
    let set = load("season");
    let in_season = |season| Query {
        season,
        ..query("t_wall")
    };
    assert_eq!(fg(&set, &in_season(Some("winter"))).map(|c| c.0), Some(5));
    assert_eq!(fg(&set, &in_season(Some("summer"))).map(|c| c.0), Some(1));
    assert_eq!(fg(&set, &in_season(None)).map(|c| c.0), Some(1));
}

#[test]
fn the_variant_mix_equals_the_games() {
    // Values printed by the game's own mixing code (cata_tiles.cpp draw_from_id_string).
    for (seed, mixed) in [
        (0, 0),
        (1, 808_612_721),
        (12_345, 3_915_220_804),
        (196_615, 3_099_691_920),
        (u32::MAX, 1_631_742_177),
    ] {
        assert_eq!(mix(seed), mixed, "seed {seed}");
    }
}

#[test]
fn weighted_variants_are_chosen_by_seed_in_proportion_to_their_weights() {
    let set = load("weights");
    let pick = |seed| {
        fg(
            &set,
            &Query {
                seed,
                ..query("grass")
            },
        )
        .unwrap()
        .0
    };
    assert_eq!(pick(77), pick(77), "a square keeps its sprite");
    let heavy = (0..4000).filter(|&seed| pick(seed) == 6).count();
    assert!((2800..3200).contains(&heavy), "3 of 4 squares: {heavy}");
}

#[test]
fn background_and_foreground_come_from_their_own_sheets_with_the_sheets_offset() {
    let set = load("sheets");
    let chosen = set.choose(&query("two_sheets")).unwrap();
    assert_eq!(chosen.fg.map(|p| p.sprite), Some(8));
    assert_eq!(chosen.bg.map(|p| p.sprite), Some(9));
    let (sheet, _) = set.locate(8).unwrap();
    assert_eq!(set.sheets[sheet].offset, (1, -2));
}

#[test]
fn a_tileset_without_tile_info_is_refused() {
    let root = game_root("broken");
    std::fs::write(
        root.join("gfx/test/tile_config.json"),
        "{\"tiles-new\": []}",
    )
    .unwrap();
    assert!(Tileset::load(&root.join("gfx/test")).is_err());
}

#[test]
fn the_filters_for_low_light_and_memory_equal_the_games_pixel_functions() {
    use bn_client_egui::display::Fx;
    // Values printed by the game's color_pixel_grayscale and color_pixel_sepia (sdl_utils.cpp).
    // The haze laid over levels seen through open air is a flat cyan.
    assert_eq!(Fx::ZOverlay.pixel([9, 9, 9]), [128, 255, 255]);
    for (pixel, shadow, memory) in [
        ([200, 100, 50], [72; 3], [125, 107, 80]),
        ([255, 255, 255], [158; 3], [241, 220, 163]),
        ([10, 20, 30], [11; 3], [43, 26, 21]),
        ([0, 0, 0], [0; 3], [0; 3]),
        ([128, 128, 128], [79; 3], [137, 119, 89]),
    ] {
        assert_eq!(Fx::Plain.pixel(pixel), pixel);
        assert_eq!(Fx::Shadow.pixel(pixel), shadow, "{pixel:?}");
        assert_eq!(Fx::Memory.pixel(pixel), memory, "{pixel:?}");
    }
}
