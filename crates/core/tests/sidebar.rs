use bn_client_core::Rgb;
use bn_client_core::colors::ColorMap;
use bn_client_core::protocol::Avatar;
use bn_client_core::sidebar::lines;
use serde_json::Value;

fn avatar() -> Avatar {
    let text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/0.1.examples.json"
    ))
    .unwrap();
    let all: Value = serde_json::from_str(&text).unwrap();
    let example = all["valid"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["name"] == "avatar with sidebar")
        .unwrap();
    serde_json::from_value(example["value"].clone()).unwrap()
}

fn texts(avatar: &Avatar) -> Vec<String> {
    lines(avatar, &ColorMap::default())
        .iter()
        .map(|l| {
            l.iter()
                .map(|s| s.text.as_str())
                .filter(|t| !t.is_empty())
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect()
}

#[test]
fn rows_read_as_the_engine_words_them_in_its_order() {
    let rows = texts(&avatar());
    assert_eq!(rows[0], "HEAD ||||| Pain 3 Str 8");
    assert!(rows.contains(&"Location: field".to_string()));
    assert!(rows.contains(&"Spring, Day 1 Around noon".to_string()));
    assert!(rows.contains(&"Weather : Clear".to_string()));
    assert!(rows.contains(&"Weapon: fists".to_string()));
}

#[test]
fn spans_use_the_native_colour_and_an_empty_span_keeps_its_place_but_prints_nothing() {
    let a = avatar();
    let rows = lines(&a, &ColorMap::default());
    assert_eq!(rows[0][2].color, ColorMap::default().resolve("light_red").0);
    assert_ne!(rows[0][2].color, Rgb(0, 0, 0));
    // A need that is not felt leaves an empty span: the morale face beside it keeps its index.
    let face_at = |a: &Avatar| {
        let rows = lines(a, &ColorMap::default());
        rows.iter()
            .find_map(|r| r.iter().position(|s| s.text == ":)"))
    };
    let mut calm = a.clone();
    calm.sidebar.as_mut().unwrap().rows[1][0].text = String::new();
    assert_eq!(face_at(&a), face_at(&calm));
    assert!(face_at(&a).is_some());
}

#[test]
fn an_avatar_without_a_sidebar_has_no_lines() {
    let mut a = avatar();
    a.sidebar = None;
    assert!(lines(&a, &ColorMap::default()).is_empty());
}
