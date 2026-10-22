//! Every screen of the engine's 0.2 examples as a player sees it, at 1280x800 and 1920x1080: the map with the
//! classic sidebar, each other screen on top of it, or alone where the title screen shows it. Each must be
//! `legible` but for the faults `KNOWN` names; `shots` writes them all as PNGs to `$SHOT_DIR/<width>x<height>/` for
//! a person to look at, which needs a Vulkan adapter, so it only runs when asked:
//! `cargo test --test ui_shots_all -- --ignored`.
mod common;
use bn_client_egui::{App, EngineModel};
use common::fixture;
use common::legible::{Kind, cut_text, legible, wrapped_labels};
use eframe::egui;
use egui_kittest::Harness;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

/// The window sizes every screen is shown at.
const SIZES: [egui::Vec2; 2] = [egui::vec2(1280.0, 800.0), egui::vec2(1920.0, 1080.0)];

/// Examples the title screen shows before a game runs (`file` or `file/name`): alone, with no map or sidebar.
const TITLE_FLOW: &[&str] = &[
    "title",
    "handshake",
    "newgame",
    "editor_menu/title screen on the editor tab",
];

/// Faults of the UI/UX plan's findings (`F<n>`) not fixed yet: `file/name@size`, the kind of fault and the
/// finding. A fixed one must leave the list.
#[rustfmt::skip]
const KNOWN: &[(&str, Kind, &str)] = &[
    ("aim/advanced inventory screen@1280x800", Kind::Cut, "F21"),
    ("aim/advanced inventory screen@1920x1080", Kind::Cut, "F6"),
    ("armor/armor layers screen of the torso@1280x800", Kind::Cut, "F1"),
    ("armor/armor layers screen@1280x800", Kind::Cut, "F1"),
    ("character/character sheet@1280x800", Kind::Cut, "F6"),
    ("colors/colour manager with a custom colour set@1280x800", Kind::Cut, "F1"),
    ("colors/colour manager@1280x800", Kind::Cut, "F1"),
    ("editor/map editor with a drawn room and a new mapping@1280x800", Kind::Cut, "F22"),
    ("editor/map editor with a drawn room and a new mapping@1920x1080", Kind::Cut, "F22"),
    ("editor/map editor with a nested chunk open@1280x800", Kind::Cut, "F22"),
    ("editor/map editor with a nested chunk open@1920x1080", Kind::Cut, "F22"),
    ("editor/map editor with a placement picked@1280x800", Kind::Cut, "F22"),
    ("editor/map editor with a placement picked@1920x1080", Kind::Cut, "F22"),
    ("editor_menu/map editor reopened on the document it was left with@1280x800", Kind::Cut, "F22"),
    ("editor_menu/map editor reopened on the document it was left with@1920x1080", Kind::Cut, "F22"),
    ("info/scores@1280x800", Kind::Cut, "F6"),
    ("info/scores@1920x1080", Kind::Cut, "F6"),
    ("newgame/character creation on the overview page@1280x800", Kind::Cut, "F6"),
    ("newgame/world creation on the finalize page@1280x800", Kind::Cut, "F6"),
    ("overmap/overmap screen for a client that draws tiles@1280x800", Kind::Cut, "F19"),
    ("overmap/overmap screen@1280x800", Kind::Cut, "F19"),
    ("title/title screen on the credits tab@1280x800", Kind::Cut, "F6"),
];

/// One screen of the examples, keyed `file/name`.
struct Example {
    key: String,
    screen: Value,
}

impl Example {
    /// The stack the screen is shown in.
    fn stack(&self) -> Value {
        let file = self.key.split('/').next().unwrap();
        let alone = TITLE_FLOW.contains(&file) || TITLE_FLOW.contains(&self.key.as_str());
        if alone || self.screen["role"] == "map" {
            json!([self.screen])
        } else {
            common::over_map(self.screen.clone())
        }
    }

    /// The PNG's name: the key with every run of other than letters and digits as one `-`.
    fn file_name(&self) -> String {
        let slug = self
            .key
            .split(|c: char| !c.is_ascii_alphanumeric())
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join("-");
        format!("{slug}.png")
    }
}

/// Every screen the engine's 0.2 examples hold, but the client's panel windows' views.
fn examples() -> Vec<Example> {
    let dir = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../core/tests/fixtures/0.2.examples"
    );
    let mut files = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect::<Vec<_>>();
    files.sort();
    files
        .iter()
        .flat_map(|path| {
            let file = path.file_stem().unwrap().to_str().unwrap().to_string();
            let all =
                serde_json::from_str::<Value>(&std::fs::read_to_string(path).unwrap()).unwrap();
            all["valid"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|e| {
                    let v = &e["value"];
                    let role = v["role"].as_str();
                    v["screen_id"]
                        .as_str()
                        .is_some_and(|id| !id.starts_with("panel:"))
                        && matches!(role, Some("map" | "screen" | "dialog"))
                })
                .map(|e| Example {
                    key: format!("{file}/{}", e["name"].as_str().unwrap()),
                    screen: e["value"].clone(),
                })
                .collect::<Vec<_>>()
        })
        .collect()
}

/// The avatar of a walk step with the classic sidebar on the right, 44 columns wide, every panel on.
fn classic_sidebar() -> Value {
    let world = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../core/tests/fixtures/0.2.examples/world.json"
    );
    let all = serde_json::from_str::<Value>(&std::fs::read_to_string(world).unwrap()).unwrap();
    let mut avatar = all["valid"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["name"] == "avatar with creatures on the compass")
        .unwrap()["value"]
        .clone();
    let panels = [
        "Health", "Location", "Weather", "Time", "Weapon", "Compass", "Log",
    ]
    .map(|id| json!({"id": id, "label": id, "enabled": true}));
    avatar["sidebar"]["layout"] =
        json!({"id": "classic", "side": "right", "width": 44, "panels": panels});
    let mut event = fixture("events_41_walk");
    event["params"]["events"][0]["changes"]["avatar"] = avatar;
    event
}

/// `example` shown in a harness `make` builds at `size`, on its first settled frame.
fn shown<'a>(
    example: &Example,
    size: egui::Vec2,
    make: fn(egui::Vec2, App<EngineModel>) -> Harness<'a, App<EngineModel>>,
) -> Harness<'a, App<EngineModel>> {
    let (app, script) = common::staged(example.stack());
    let mut h = make(size, app);
    h.run_steps(3);
    script.push(classic_sidebar());
    h.run_steps(4);
    common::settle_windows(&mut h);
    h
}

fn size_name(size: egui::Vec2) -> String {
    format!("{}x{}", size.x, size.y)
}

#[test]
fn every_example_screen_is_legible_but_for_the_known_faults() {
    let mut found = BTreeMap::<(String, Kind), Vec<String>>::new();
    for example in examples() {
        for size in SIZES {
            let h = shown(&example, size, common::themed_harness);
            let key = format!("{}@{}", example.key, size_name(size));
            for fault in legible(&h, &example.screen) {
                found
                    .entry((key.clone(), fault.kind))
                    .or_default()
                    .push(fault.what);
            }
        }
    }
    let known = KNOWN
        .iter()
        .map(|(key, kind, _)| (key.to_string(), *kind))
        .collect::<BTreeSet<_>>();
    for (key, kind, finding) in KNOWN {
        eprintln!("known {finding}: {key} {kind:?}");
    }
    let new = found
        .iter()
        .filter(|(fault, _)| !known.contains(*fault))
        .collect::<Vec<_>>();
    let fixed = known
        .iter()
        .filter(|fault| !found.contains_key(*fault))
        .collect::<Vec<_>>();
    assert!(
        new.is_empty() && fixed.is_empty(),
        "new faults {new:#?}\nfixed, to leave KNOWN: {fixed:#?}"
    );
}

#[test]
#[ignore = "needs a Vulkan adapter and SHOT_DIR"]
fn shots() {
    for example in examples() {
        for size in SIZES {
            let mut h = shown(&example, size, common::shot_harness);
            common::save_shot(
                &mut h,
                &format!("{}/{}", size_name(size), example.file_name()),
            );
        }
    }
}

/// A frame of `labels` at `width`, the second of them half out of the clip rectangle and the third wholly out.
fn clipped<'a>(width: f32, labels: [&'static str; 3]) -> Harness<'a> {
    Harness::builder()
        .with_size(egui::vec2(400.0, 200.0))
        .build_ui(move |ui| {
            ui.set_max_width(width);
            let top = ui.max_rect().top();
            ui.set_clip_rect(egui::Rect::from_x_y_ranges(0.0..=400.0, top..=top + 30.0));
            for label in labels {
                ui.label(label);
            }
        })
}

#[test]
fn a_line_half_out_of_its_clip_is_cut_and_one_wholly_out_is_scrolled_away() {
    let h = clipped(400.0, ["first", "second", "third"]);
    let cut = cut_text(&h.output().shapes, h.ctx.content_rect());
    assert_eq!(cut.len(), 1, "{cut:?}");
    assert!(cut[0].starts_with("\"second\""), "{cut:?}");
}

#[test]
fn a_label_on_more_lines_than_it_holds_is_wrapped() {
    let labels = ["a long row label".to_string()];
    let narrow = clipped(40.0, ["a long row label", "", ""]);
    let wide = clipped(400.0, ["a long row label", "", ""]);
    assert_eq!(wrapped_labels(&narrow.output().shapes, &labels).len(), 1);
    assert!(wrapped_labels(&wide.output().shapes, &labels).is_empty());
}
