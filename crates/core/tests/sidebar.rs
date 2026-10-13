use bn_client_core::Rgb;
use bn_client_core::colors::ColorMap;
use bn_client_core::protocol::Avatar;
use bn_client_core::sidebar::lines;
use serde_json::Value;

fn avatar() -> Avatar {
    let text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/0.2.examples/world.json"
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
fn the_client_lays_out_the_engines_labels_and_values_in_the_classic_order() {
    let rows = texts(&avatar());
    assert_eq!(rows[0], "HEAD ||||| Pain 3 Str 8");
    assert!(rows.contains(&"Location: field".to_string()));
    assert!(rows.contains(&"Spring, Day 1 Around noon".to_string()));
    assert!(rows.contains(&"Weather : Clear".to_string()));
    assert!(rows.contains(&"Weapon  : fists".to_string()));
    assert!(rows.contains(&"Focus 100 SAFE".to_string()));
    assert!(rows.contains(&"Spd 100 0 W Stm |||||".to_string()));
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
    calm.sidebar.as_mut().unwrap().hunger.text = String::new();
    assert_eq!(face_at(&a), face_at(&calm));
    assert!(face_at(&a).is_some());
}

#[test]
fn an_avatar_without_a_sidebar_has_no_lines() {
    let mut a = avatar();
    a.sidebar = None;
    assert!(lines(&a, &ColorMap::default()).is_empty());
}

#[test]
fn every_label_is_the_engines_text() {
    let mut a = avatar();
    let side = a.sidebar.as_mut().unwrap();
    side.focus.label = "Konzentration".into();
    side.location.label = Some("Ort:".into());
    side.time.day_label = "Tag".into();
    a.stats[0].short_label = Some("Stä".into());
    let rows = texts(&a);
    assert!(rows.contains(&"Konzentration 100 SAFE".to_string()));
    assert!(rows.contains(&"Ort: field".to_string()));
    assert!(rows.contains(&"Spring, Tag 1 Around noon".to_string()));
    assert_eq!(rows[0], "HEAD ||||| Pain 3 Stä 8");
}

#[test]
fn limb_health_is_drawn_from_the_engines_numbers() {
    let mut a = avatar();
    let limb = &mut a.sidebar.as_mut().unwrap().limbs[0];
    limb.hp = 27;
    assert!(texts(&a)[0].starts_with("HEAD |||.."));
    a.sidebar.as_mut().unwrap().limbs[0].hp = 0;
    assert!(texts(&a)[0].starts_with("HEAD ....."));
}

#[test]
fn underground_weather_has_no_label() {
    let mut a = avatar();
    let side = a.sidebar.as_mut().unwrap();
    side.weather.label = None;
    side.weather.text = "Underground".into();
    assert!(texts(&a).contains(&"Underground".to_string()));
}
