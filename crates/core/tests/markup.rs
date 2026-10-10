use bn_client_core::Rgb;
use bn_client_core::colors::ColorMap;
use bn_client_core::markup::{Run, plain, runs};

fn run(text: &str, color: Option<Rgb>) -> Run {
    Run {
        text: text.into(),
        color,
    }
}

#[test]
fn a_color_tag_colors_its_text_and_the_rest_stays_plain() {
    let colors = ColorMap::default();
    let green = colors.resolve("c_light_green").0;
    assert_eq!(
        runs("lighter<color_c_light_green> (23/25)</color>", &colors),
        [run("lighter", None), run(" (23/25)", Some(green))]
    );
}

#[test]
fn tag_names_resolve_with_or_without_the_c_prefix_and_nest() {
    let colors = ColorMap::default();
    let (white, red) = (colors.resolve("white").0, colors.resolve("red").0);
    assert_eq!(
        runs(
            "<color_white>a<color_c_red>b</color>c</color>d",
            &colors
        ),
        [
            run("a", Some(white)),
            run("b", Some(red)),
            run("c", Some(white)),
            run("d", None)
        ]
    );
}

#[test]
fn stray_closers_and_unknown_markup_stay_readable() {
    let colors = ColorMap::default();
    assert_eq!(runs("a</color>b", &colors), [run("ab", None)]);
    assert_eq!(runs("x < y <b>", &colors), [run("x < y <b>", None)]);
    assert_eq!(runs("<color_white>open", &colors).len(), 1);
    assert!(runs("", &colors).is_empty());
}

#[test]
fn plain_text_drops_the_tags_only() {
    assert_eq!(
        plain("lighter<color_c_light_green> (23/25)</color>"),
        "lighter (23/25)"
    );
    assert_eq!(plain("no tags <here>"), "no tags <here>");
}
