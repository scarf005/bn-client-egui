//! What a tileset draws for a thing it has no tile for: its ASCII sprite of the glyph in the thing's colour, else in
//! any colour, else its `unknown` tile, as the game does.
use bn_client_egui::tileset::{Query, Tileset};
use serde_json::json;

/// A tileset of 4x4 sprites with an ASCII sheet of three sets (all colours, red, bold red) of 256 symbols each, and
/// `unknown` as the sprite after the sheet; `with_unknown` says whether it defines that tile.
fn load(tag: &str, with_unknown: bool) -> Tileset {
    let root = std::env::temp_dir().join(format!("bn-client-ascii-{}-{tag}", std::process::id()));
    let dir = root.join("gfx/ascii");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("tileset.txt"),
        "NAME: ascii\nVIEW: Ascii\nJSON: tile_config.json\nTILESET: font.png\n",
    )
    .unwrap();
    // 16 symbols across, 48 rows: 768 ASCII sprites, then one more.
    image::RgbaImage::new(64, 4 * 49)
        .save(dir.join("font.png"))
        .unwrap();
    let unknown = json!({"id": "unknown", "fg": 768});
    let config = json!({
        "tile_info": [{"width": 4, "height": 4}],
        "tiles-new": [{
            "file": "font.png",
            "tiles": if with_unknown { vec![unknown] } else { vec![] },
            "ascii": [
                {"offset": 0, "color": "DEFAULT"},
                {"offset": 256, "color": "RED"},
                {"offset": 512, "color": "RED", "bold": true}
            ]
        }]
    });
    std::fs::write(dir.join("tile_config.json"), config.to_string()).unwrap();
    Tileset::load(&dir).unwrap()
}

fn query(subtile: Option<&str>) -> Query<'_> {
    Query {
        id: "t_missing",
        fallbacks: &[],
        subtile,
        rotation: 0,
        mirrored: false,
        creature: false,
        seed: 0,
        season: None,
        frame: None,
    }
}

fn drawn(set: &Tileset, glyph: char, color: u8, subtile: Option<&str>) -> Option<u32> {
    Some(
        set.choose_or_stand_in(&query(subtile), glyph, color)?
            .fg?
            .sprite,
    )
}

#[test]
fn a_tile_the_tileset_has_is_drawn_not_the_glyph() {
    let set = load("has", true);
    let q = Query {
        id: "unknown",
        ..query(None)
    };
    assert_eq!(
        set.choose_or_stand_in(&q, '+', 9)
            .unwrap()
            .fg
            .unwrap()
            .sprite,
        768
    );
}

#[test]
fn a_missing_tile_is_drawn_by_the_ascii_sprite_of_its_glyph_in_its_colour() {
    let set = load("colour", true);
    // Colour 1 is red, 9 is bold red: the sets at offsets 256 and 512.
    assert_eq!(drawn(&set, '+', 1, None), Some(256 + 43));
    assert_eq!(drawn(&set, '+', 9, None), Some(512 + 43));
}

#[test]
fn a_colour_without_an_ascii_set_is_drawn_in_the_set_for_all_colours() {
    let set = load("any", true);
    assert_eq!(drawn(&set, '>', 4, None), Some(62));
}

#[test]
fn box_drawing_glyphs_join_their_neighbours_by_the_subtile_the_engine_chose() {
    let set = load("box", true);
    // The unjoined horizontal line, then the edge and corner of the vertical and horizontal ones.
    assert_eq!(drawn(&set, '─', 4, None), Some(205));
    assert_eq!(drawn(&set, '─', 4, Some("edge")), Some(186));
    assert_eq!(drawn(&set, '─', 4, Some("corner")), Some(201));
    assert_eq!(drawn(&set, '│', 4, Some("center")), Some(206));
    // A corner glyph is one sprite; the game gives it no parts.
    assert_eq!(drawn(&set, '┌', 4, Some("edge")), Some(201));
}

#[test]
fn a_glyph_without_an_ascii_code_is_drawn_as_the_unknown_tile() {
    let set = load("unknown", true);
    assert_eq!(drawn(&set, '\u{4e00}', 1, None), Some(768));
}

#[test]
fn a_tileset_without_an_unknown_tile_draws_nothing_for_a_glyph_it_has_no_sprite_for() {
    let set = load("none", false);
    assert_eq!(drawn(&set, '\u{4e00}', 1, None), None);
    // Its ASCII sprites still draw the glyphs they have.
    assert_eq!(drawn(&set, 'H', 1, None), Some(256 + 72));
}

#[test]
fn an_overlay_without_a_tile_draws_nothing_even_when_the_tileset_has_an_unknown_tile() {
    let set = load("overlay", true);
    assert_eq!(drawn(&set, '\0', 1, None), None);
}
