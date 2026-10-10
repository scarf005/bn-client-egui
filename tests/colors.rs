use bn_client_egui::colors::ColorMap;
use eframe::egui::Color32;

const COLORS: &str = r#"[{"type":"colordef","BLACK":[0,0,0],"RED":[255,0,0],"GREEN":[0,110,0],"BROWN":[97,56,28],
"BLUE":[10,10,220],"MAGENTA":[139,58,98],"CYAN":[0,150,180],"GRAY":[150,150,150],"DGRAY":[99,99,99],
"LRED":[255,150,150],"LGREEN":[0,255,0],"YELLOW":[255,255,0],"LBLUE":[100,100,255],"LMAGENTA":[254,0,254],
"LCYAN":[0,240,255],"WHITE":[255,255,255]}]"#;

fn map() -> ColorMap {
    ColorMap::from_json(COLORS).unwrap()
}

fn rgb(r: u8, g: u8, b: u8) -> Color32 {
    Color32::from_rgb(r, g, b)
}

#[test]
fn plain_names_use_the_game_palette_with_bold_selecting_the_light_half() {
    let m = map();
    assert_eq!(m.resolve("c_light_gray"), (rgb(150, 150, 150), None));
    assert_eq!(m.resolve("c_blue"), (rgb(10, 10, 220), None));
    assert_eq!(m.resolve("c_light_blue"), (rgb(100, 100, 255), None));
    assert_eq!(m.resolve("c_white"), (rgb(255, 255, 255), None));
    assert_eq!(m.resolve("c_dark_gray"), (rgb(99, 99, 99), None));
    assert_eq!(m.resolve("c_brown"), (rgb(97, 56, 28), None));
    assert_eq!(m.resolve("c_yellow"), (rgb(255, 255, 0), None));
    assert_eq!(m.resolve("c_pink"), (rgb(254, 0, 254), None));
}

#[test]
fn highlight_and_invert_variants_carry_a_background() {
    let m = map();
    // h_X draws X on a blue background; i_X is black on X.
    assert_eq!(m.resolve("h_red"), (rgb(255, 0, 0), Some(rgb(10, 10, 220))));
    assert_eq!(
        m.resolve("i_white"),
        (rgb(0, 0, 0), Some(rgb(150, 150, 150)))
    );
    assert_eq!(
        m.resolve("c_white_red"),
        (rgb(255, 255, 255), Some(rgb(255, 0, 0)))
    );
    assert_eq!(
        m.resolve("c_light_green_red"),
        (rgb(0, 255, 0), Some(rgb(255, 0, 0)))
    );
}

#[test]
fn bare_names_without_the_c_prefix_are_accepted() {
    assert_eq!(map().resolve("light_green"), map().resolve("c_light_green"));
    assert_eq!(map().resolve("brown"), map().resolve("c_brown"));
}

#[test]
fn unknown_name_falls_back_to_light_gray() {
    assert_eq!(map().resolve("c_nonsense"), map().resolve("c_light_gray"));
}

#[test]
fn malformed_or_incomplete_json_is_rejected() {
    assert!(ColorMap::from_json("not json").is_none());
    assert!(ColorMap::from_json(r#"[{"type":"colordef","RED":[1,2,3]}]"#).is_none());
}

#[test]
fn load_reads_the_game_root_and_falls_back_to_defaults_without_it() {
    let dir = std::env::temp_dir().join(format!("bn-colors-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("data/raw")).unwrap();
    std::fs::write(dir.join("data/raw/colors.json"), COLORS).unwrap();
    assert_eq!(ColorMap::load(&dir).resolve("c_blue").0, rgb(10, 10, 220));
    std::fs::remove_dir_all(&dir).unwrap();
    assert_eq!(
        ColorMap::load(&dir).resolve("c_white").0,
        ColorMap::default().resolve("c_white").0
    );
    assert_eq!(
        ColorMap::default().resolve("c_red").0,
        rgb(170, 40, 40),
        "current default red"
    );
}
