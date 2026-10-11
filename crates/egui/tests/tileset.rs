#![recursion_limit = "512"]
mod common;
use bn_client_core::protocol::{ModTileset, Traits};
use bn_client_egui::tileset::{self, PaintQuery, Query, Tileset, Turn, mix};
use bn_client_egui::tint::{Blend, Paint};
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

/// Adds a mod tileset file listing `compatibility` with one sheet of four sprites (0 and 2 are sprites,
/// 1 and 3 their tint masks) and the given tiles.
fn add_mod(root: &std::path::Path, file: &str, compatibility: &[&str], tiles: serde_json::Value) {
    let dir = root.join("data/json/external_tileset");
    std::fs::create_dir_all(&dir).unwrap();
    image::RgbaImage::new(16, 4)
        .save(dir.join("mod.png"))
        .unwrap();
    let config = serde_json::json!([
        {"type": "mod_tileset", "compatibility": compatibility, "tiles-new": [{"file": "external_tileset/mod.png", "tiles": tiles}]}
    ]);
    std::fs::write(dir.join(file), config.to_string()).unwrap();
}

#[test]
fn a_mod_tileset_numbers_its_sprites_after_the_tilesets_own_and_masks_its_tint() {
    let root = game_root("mod-mask");
    add_mod(
        &root,
        "vehicles.json",
        &["other", "test"],
        serde_json::json!([
            {"id": "vp_glass", "fg": 0, "masks": [{"type": "tint", "fg": 1}]},
            {"id": "vp_open", "fg": 2},
            // A mask that does not match its sprites is dropped.
            {"id": "vp_odd", "fg": 2, "masks": [{"type": "tint", "fg": [1, 3]}]},
            // A mod tile replaces the tileset's own.
            {"id": "t_floor", "fg": 2}
        ]),
    );
    let set = Tileset::load(&root.join("gfx/test")).unwrap();
    let placed = |id: &str| set.choose(&query(id)).unwrap().fg.unwrap();
    // The tileset has sprites 0 to 9, so the mod's sheet starts at 10.
    assert_eq!(
        (placed("vp_glass").sprite, placed("vp_glass").mask),
        (10, Some(11))
    );
    assert_eq!(
        (placed("vp_open").sprite, placed("vp_open").mask),
        (12, None)
    );
    assert_eq!(placed("vp_odd").mask, None);
    assert_eq!(placed("t_floor").sprite, 12);
    assert_eq!(set.locate(13), Some((2, 3)));
}

#[test]
fn a_mod_tileset_for_another_tileset_adds_nothing() {
    let root = game_root("mod-other");
    add_mod(
        &root,
        "other.json",
        &["other"],
        serde_json::json!([{"id": "vp_glass", "fg": 0}]),
    );
    let set = Tileset::load(&root.join("gfx/test")).unwrap();
    assert!(set.choose(&query("vp_glass")).is_none());
    assert_eq!(set.sheets.len(), 2);
}

fn traits(pairs: &[(&str, &[&str])]) -> Traits {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.iter().map(|s| s.to_string()).collect()))
        .collect()
}

/// A mod tileset of the root that only holds tints.
fn add_tints(root: &std::path::Path, tints: serde_json::Value, pairs: serde_json::Value) {
    let dir = root.join("data/json/external_tileset");
    std::fs::create_dir_all(&dir).unwrap();
    let config = serde_json::json!([
        {"type": "mod_tileset", "compatibility": ["test"], "tints": tints, "tint_pairs": pairs}
    ]);
    std::fs::write(dir.join("tints.json"), config.to_string()).unwrap();
}

#[test]
fn a_mutation_takes_the_paint_of_the_character_mutation_its_type_is_paired_with() {
    let root = game_root("tints");
    add_tints(
        &root,
        serde_json::json!([
            {"id": "hair_black", "fg": "#383838", "blend_mode": "multiply"},
            {"id": "SKIN_DARK", "fg": "#55381c80", "bg": {"color": "#102030", "blend_mode": "additive", "contrast": 1.5}}
        ]),
        serde_json::json!([{"source_type": "hair_color", "target_type": "hair_style", "override": true}]),
    );
    let set = Tileset::load(&root.join("gfx/test")).unwrap();
    let query = |id: &'static str, types: &'static [String], traits: &'static Traits| PaintQuery {
        id,
        types,
        flags: &[],
        traits,
    };
    let (style, none): (&'static [String], &'static Traits) = (
        Box::leak(vec!["hair_style".to_string()].into_boxed_slice()),
        Box::leak(Box::new(Traits::new())),
    );
    let black: &'static Traits = Box::leak(Box::new(traits(&[("hair_color", &["hair_black"])])));
    // The hair takes the color of the character's hair color mutation, multiplied in.
    let (bg, fg) = set.mutation_paints(&query("hair_long", style, black));
    assert_eq!(bg, None);
    assert_eq!(
        fg,
        Some(Paint {
            blend: Blend::Multiply,
            ..Paint::of([56, 56, 56])
        })
    );
    // Without a hair color mutation, or for another type, a mutation has only its own paint, if any.
    assert_eq!(
        set.mutation_paints(&query("hair_long", style, none)),
        (None, None)
    );
    assert_eq!(
        set.mutation_paints(&query("hair_long", &[], black)),
        (None, None)
    );
    let (bg, fg) = set.mutation_paints(&query("SKIN_DARK", &[], none));
    assert_eq!(
        bg,
        Some(Paint {
            blend: Blend::Additive,
            contrast: 1500,
            ..Paint::of([16, 32, 48])
        })
    );
    // The fourth pair of a color is its brightness in 128ths.
    assert_eq!(
        fg.map(|p| (p.color, p.brightness)),
        Some(([85, 56, 28], 1000))
    );
}

#[test]
fn the_engines_list_of_mod_tilesets_replaces_the_search_of_the_data() {
    let root = game_root("mod-list");
    add_mod(
        &root,
        "found.json",
        &["test"],
        serde_json::json!([{"id": "vp_found", "fg": 0}]),
    );
    add_mod(
        &root,
        "listed.json",
        &["test"],
        serde_json::json!([{"id": "vp_listed", "fg": 0}]),
    );
    let dir = root.join("gfx/test");
    // The engine names its files from the game folder, through the folder of the mod that loads them.
    std::fs::create_dir_all(root.join("data/mods/bn")).unwrap();
    let only = |name: &str| ModTileset {
        path: format!("data/mods/bn/../../json/external_tileset/{name}"),
        base: "data/mods/bn/../../json".into(),
        index: 1,
        compatibility: vec!["test".into()],
    };
    let set = Tileset::load_with(&dir, Some(&[only("listed.json")])).unwrap();
    assert!(set.choose(&query("vp_listed")).is_some());
    assert!(set.choose(&query("vp_found")).is_none());
    // The search of the data finds both, in file order.
    let both = Tileset::load(&dir).unwrap();
    let sprite = |id: &str| both.choose(&query(id)).unwrap().fg.unwrap().sprite;
    assert_eq!((sprite("vp_found"), sprite("vp_listed")), (10, 14));
}

#[test]
fn an_item_takes_the_paint_of_its_first_flag_or_else_of_its_id() {
    let root = game_root("key-paints");
    add_tints(
        &root,
        serde_json::json!([
            {"id": "jeans", "fg": "#112233"},
            {"id": "FANCY", "fg": "#445566"},
            {"id": "WATERPROOF", "fg": "#778899"}
        ]),
        serde_json::json!([]),
    );
    let set = Tileset::load(&root.join("gfx/test")).unwrap();
    let keys = |k: &[&str]| k.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    let color = |k: &[&str]| set.key_paints(&keys(k)).1.map(|p| p.color);
    // The flags come first, in the order the game lists them, then the id.
    assert_eq!(
        color(&["VARSIZE", "FANCY", "WATERPROOF", "jeans"]),
        Some([68, 85, 102])
    );
    assert_eq!(color(&["VARSIZE", "jeans"]), Some([17, 34, 51]));
    assert_eq!(color(&["VARSIZE", "shirt"]), None);
}

#[test]
fn a_hair_style_goes_by_the_sprite_of_the_hair_color_where_the_tileset_paints_no_hair() {
    let root = game_root("legacy-hair");
    add_mod(
        &root,
        "hair.json",
        &["test"],
        serde_json::json!([{"id": "overlay_female_mutation_hair_black_long", "fg": 0}]),
    );
    let set = Tileset::load(&root.join("gfx/test")).unwrap();
    let style = vec!["hair_style".to_string()];
    let black = traits(&[("hair_color", &["hair_black"])]);
    let blond = traits(&[("hair_color", &["hair_blond"])]);
    let ask = |id: &str, traits: &Traits, male: bool| {
        set.colored_hair(
            &PaintQuery {
                id,
                types: &style,
                flags: &[],
                traits,
            },
            male,
        )
    };
    assert_eq!(
        ask("hair_long", &black, false),
        Some([
            "overlay_female_mutation_hair_black_long".to_string(),
            "overlay_mutation_hair_black_long".to_string()
        ])
    );
    // No sprite for the color, a style that names its color already, or no hair color: the ordinary overlay.
    assert_eq!(ask("hair_long", &blond, false), None);
    assert_eq!(ask("hair_black_long", &black, false), None);
    assert_eq!(ask("hair_long", &Traits::new(), false), None);
    // A tileset that pairs hair styles with colors paints them instead.
    add_tints(
        &root,
        serde_json::json!([]),
        serde_json::json!([
            {"source_type": "hair_color", "target_type": "hair_style", "override": true}
        ]),
    );
    let painted = Tileset::load(&root.join("gfx/test")).unwrap();
    let q = PaintQuery {
        id: "hair_long",
        types: &style,
        flags: &[],
        traits: &black,
    };
    assert_eq!(painted.colored_hair(&q, false), None);
}
