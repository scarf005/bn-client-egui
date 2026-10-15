use bn_client_core::Cell;
use bn_client_core::Rgb;
use bn_client_core::colors::ColorMap;
use bn_client_core::protocol::Avatar;
use bn_client_core::protocol::{LayoutPanel, SidebarSide};
use bn_client_core::sidebar::{Body, layout, lines};
use serde_json::Value;

fn avatar() -> Avatar {
    example("avatar with sidebar")
}

fn example(name: &str) -> Avatar {
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
        .find(|e| e["name"] == name)
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

fn with_compass() -> Avatar {
    let mut a = example("avatar with creatures on the compass");
    for p in &mut a.sidebar.as_mut().unwrap().layout.as_mut().unwrap().panels {
        p.enabled = true;
    }
    a
}

fn shown(a: &Avatar) -> bn_client_core::sidebar::Layout {
    let cell = |look: &bn_client_core::protocol::Look| Cell {
        glyph: look.glyph.chars().next().unwrap(),
        desc: look.id.clone().unwrap_or_default(),
        ..Cell::default()
    };
    layout(a, &ColorMap::default(), cell).unwrap()
}

#[test]
fn the_layout_gives_the_side_the_width_and_the_enabled_panels_in_the_engines_order_and_words() {
    let mut a = with_compass();
    let panels = &mut a.sidebar.as_mut().unwrap().layout.as_mut().unwrap().panels;
    *panels = ["Weather", "Health", "Log", "Compass", "Location"]
        .iter()
        .map(|id| LayoutPanel {
            id: id.to_string(),
            label: format!("<{id}>"),
            enabled: *id != "Location",
            height: (*id != "Log").then_some(3),
        })
        .collect();
    let layout = shown(&a);
    assert_eq!((layout.side, layout.width), (SidebarSide::Right, 44));
    let labels = layout.panels.iter().map(|p| p.label.as_str());
    assert_eq!(
        labels.collect::<Vec<_>>(),
        ["<Weather>", "<Health>", "<Log>", "<Compass>"]
    );
    assert_eq!(layout.panels[2].body, Body::Log);
    assert_eq!(layout.panels[2].height, None);
    assert_eq!(layout.panels[0].height, Some(3));
}

#[test]
fn a_panel_the_client_cannot_draw_or_the_player_switched_off_is_not_shown() {
    let layout = shown(&with_compass());
    // The fixture names a Messages panel and a Compass panel; only Health and the compass have a renderer.
    let labels = layout.panels.iter().map(|p| p.label.as_str());
    assert_eq!(labels.collect::<Vec<_>>(), ["Health", "Compass"]);
    let mut off = with_compass();
    for p in &mut off
        .sidebar
        .as_mut()
        .unwrap()
        .layout
        .as_mut()
        .unwrap()
        .panels
    {
        p.enabled = p.id == "Health";
    }
    assert_eq!(shown(&off).panels.len(), 1);
}

#[test]
fn a_sidebar_without_a_layout_has_none() {
    let mut a = avatar();
    a.sidebar.as_mut().unwrap().layout = None;
    assert!(layout(&a, &ColorMap::default(), |_| Cell::default()).is_none());
}

#[test]
fn the_compass_lists_each_direction_by_its_heading_and_keeps_the_engines_nearest_first_order() {
    let layout = shown(&with_compass());
    let Body::Compass(directions) = &layout.panels[1].body else {
        panic!("the compass panel");
    };
    let labels = directions.iter().map(|d| d.label.as_str());
    assert_eq!(labels.collect::<Vec<_>>(), ["East:", "center"]);
    let east = directions[0]
        .creatures
        .iter()
        .map(|c| (c.name.as_str(), c.distance))
        .collect::<Vec<_>>();
    assert_eq!(east, [("zombie", 4), ("Rae", 9)]);
}

#[test]
fn every_creature_is_drawn_from_its_look_whatever_its_attitude() {
    let mut a = with_compass();
    a.compass.as_mut().unwrap()[0].creatures[1].attitude =
        bn_client_core::protocol::Attitude::Hostile;
    let layout = shown(&a);
    let Body::Compass(directions) = &layout.panels[1].body else {
        panic!("the compass panel");
    };
    let (zombie, rae) = (&directions[0].creatures[0], &directions[0].creatures[1]);
    assert_eq!(rae.cell.color, Cell::default().color);
    assert_eq!(zombie.cell.color, Cell::default().color);
    assert_eq!((rae.cell.glyph, zombie.cell.glyph), ('@', 'Z'));
}

#[test]
fn the_labels_layouts_split_the_health_rows_into_limbs_needs_stats_and_movement_panels() {
    let mut a = with_compass();
    let panels = &mut a.sidebar.as_mut().unwrap().layout.as_mut().unwrap().panels;
    *panels = ["Limbs", "Needs", "Stats", "Movement"]
        .iter()
        .map(|id| LayoutPanel {
            id: id.to_string(),
            label: id.to_string(),
            enabled: true,
            height: None,
        })
        .collect();
    let rows = |p: &bn_client_core::sidebar::Panel| {
        let Body::Status(lines) = &p.body else {
            panic!("status rows");
        };
        lines
            .iter()
            .map(|l| {
                let spans = l.iter().map(|s| s.text.as_str());
                spans
                    .filter(|t| !t.is_empty())
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .collect::<Vec<_>>()
    };
    let layout = shown(&a);
    let by_id = |id: &str| rows(layout.panels.iter().find(|p| p.id == id).unwrap());
    assert_eq!(by_id("Limbs"), ["HEAD |||||"]);
    assert_eq!(by_id("Needs"), ["Pain 3", "Tired Hungry", "Comfortable"]);
    assert_eq!(by_id("Stats"), ["Str 8 POWER --", "SAFE"]);
    assert_eq!(by_id("Movement"), [":) Focus 100", "Stm ||||| Spd 100 0 W"]);
}

#[test]
fn a_spans_colour_tags_split_it_into_runs_and_do_not_count_as_width() {
    let span = bn_client_core::sidebar::Span {
        text: "<color_c_magenta>|. </color>broadsword".into(),
        color: Rgb(200, 200, 200),
    };
    let runs = span.runs();
    assert_eq!(
        runs.iter().map(|r| r.text.as_str()).collect::<Vec<_>>(),
        ["|. ", "broadsword"]
    );
    assert_ne!(runs[0].color, span.color, "the tag's colour");
    assert_eq!(runs[1].color, span.color, "untagged text keeps the span's");
    assert_eq!(span.width(), "|. broadsword".len());
}
