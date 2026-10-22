//! List rows as a table: a label on one line in lists with columns, keys in a column of their own so the labels
//! line up, drawn checkboxes instead of `[ ]` text. Each screen is the engine's own example, open over the map
//! as a player sees it, in the client's theme and fonts.
use crate::common::fixture;
use bn_client_egui::{App, EngineModel, markup};
use eframe::egui::{self, epaint};
use egui_kittest::Harness;
use serde_json::{Value, json};

type H<'a> = Harness<'a, App<EngineModel>>;

fn example(file: &str, name: &str) -> Value {
    let path = format!(
        "{}/../core/tests/fixtures/0.2.examples/{file}.json",
        env!("CARGO_MANIFEST_DIR")
    );
    let all = serde_json::from_str::<Value>(&std::fs::read_to_string(path).unwrap()).unwrap();
    all["valid"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["name"] == name)
        .unwrap_or_else(|| panic!("no example {name}"))["value"]
        .clone()
}

/// The client in a window of `size` with `screen` open over the fixture's map.
fn open<'a>(screen: Value, size: egui::Vec2) -> H<'a> {
    opened(Harness::builder().with_size(size), screen)
}

/// `open` on the harness `builder` makes. The screen opens once the client's theme and fonts are in place, as
/// in a game.
fn opened<'a>(builder: egui_kittest::HarnessBuilder<App<EngineModel>>, screen: Value) -> H<'a> {
    let map = fixture("subscribe_result")["result"]["interaction"]["stack"][0].clone();
    let reply = vec![
        fixture("subscribe_result"),
        fixture("snapshot_part_0"),
        fixture("snapshot_part_1"),
    ];
    let (t, script) = bn_client_egui::transport::ScriptedTransport::new();
    let t = t
        .on("bn.hello", vec![fixture("hello_result")])
        .on("bn.subscribe", reply.clone())
        .on("bn.subscribe", reply);
    let clock = bn_client_egui::Clock::manual();
    let model = EngineModel::open(Box::new(t), bn_client_egui::ConnectOptions::default())
        .with_clock(clock.clone());
    let app = App::new(model).with_clock(clock);
    let mut h = builder.build_ui_state(|ui, app: &mut App<EngineModel>| app.show(ui), app);
    bn_client_egui::kit::tokens::apply(&h.ctx);
    bn_client_egui::fonts::install_bundled(&h.ctx);
    h.run_steps(4);
    script.push(json!({"jsonrpc": "2.0", "method": "bn.events", "params": {"epoch": "e7f3", "events": [{
        "sequence": "41", "revision": "32", "type": "interaction.changed", "changes": {"interaction": {
        "boundary_id": "b:41",
        "readiness": {"phase": "playing", "game_ready": true, "accepts_interaction_commands": true,
            "accepts_registered_actions": true},
        "actions": [], "interaction": null, "stack": [map, screen]}}}]}}));
    h.run_steps(8);
    h
}

/// Every painted text.
fn texts(h: &H) -> Vec<epaint::TextShape> {
    fn walk(shape: &egui::Shape, out: &mut Vec<epaint::TextShape>) {
        match shape {
            egui::Shape::Text(text) => out.push(text.clone()),
            egui::Shape::Vec(shapes) => shapes.iter().for_each(|s| walk(s, out)),
            _ => {}
        }
    }
    let mut out = vec![];
    h.output()
        .shapes
        .iter()
        .for_each(|s| walk(&s.shape, &mut out));
    out
}

/// The painted texts that are exactly one of `labels`.
fn painted_labels(h: &H, labels: &[String]) -> Vec<epaint::TextShape> {
    texts(h)
        .into_iter()
        .filter(|t| labels.iter().any(|l| t.galley.text() == l))
        .collect()
}

/// The plain labels of the rows of the list `id` in `screen` that are not category headers.
fn row_labels(screen: &Value, id: &str) -> Vec<String> {
    fn find<'a>(c: &'a Value, id: &str) -> Option<&'a Value> {
        if c["id"] == id {
            return Some(c);
        }
        let children = c["children"].as_array().into_iter().flatten();
        children
            .chain(c.get("child"))
            .find_map(|child| find(child, id))
    }
    let list = find(&screen["root"], id).unwrap_or_else(|| panic!("no list {id}"));
    list["rows"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["header"] != true)
        .map(|r| markup::plain(r["label"].as_str().unwrap()))
        .collect()
}

fn left(t: &epaint::TextShape) -> f32 {
    t.pos.x + t.galley.rect.min.x
}

#[test]
fn colour_manager_labels_stay_on_one_row() {
    let screen = example("colors", "colour manager");
    let labels = row_labels(&screen, "colors:items");
    let h = open(screen, egui::vec2(1280.0, 800.0));
    let painted = painted_labels(&h, &labels);
    let broken = painted
        .iter()
        .filter(|t| t.galley.rows.len() > 1)
        .map(|t| format!("{:?} on {} rows", t.galley.text(), t.galley.rows.len()))
        .collect::<Vec<_>>();
    assert!(broken.is_empty(), "{broken:?}");
    assert!(painted.len() >= 5, "{} labels painted", painted.len());
}

#[test]
fn row_keys_have_a_column_and_the_labels_line_up() {
    let mut screen = example("generic", "filterable list screen");
    let rows = &mut screen["root"]["children"][1]["rows"];
    // A row without a key and a wider key among the narrow ones.
    rows[1].as_object_mut().unwrap().remove("hotkey");
    rows[2]["hotkey"] = json!("F1");
    let labels = row_labels(&screen, "list:choices");
    let keys = screen["root"]["children"][1]["rows"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|r| r["hotkey"].as_str().map(str::to_string))
        .collect::<Vec<_>>();
    let h = open(screen, egui::vec2(1280.0, 800.0));
    let painted = painted_labels(&h, &labels);
    assert!(painted.len() >= 4, "{} labels painted", painted.len());
    let lefts = painted.iter().map(left).collect::<Vec<_>>();
    let spread = lefts.iter().fold(f32::MIN, |a, &b| a.max(b))
        - lefts.iter().fold(f32::MAX, |a, &b| a.min(b));
    assert!(spread < 0.5, "label left edges {lefts:?}");
    // Each key sits before its label, the keys' right edges in one column.
    let rights = texts(&h)
        .into_iter()
        .filter(|t| keys.iter().any(|k| t.galley.text() == k))
        .filter(|k| {
            painted.iter().any(|l| {
                let (k_y, l_y) = (
                    k.pos.y + k.galley.rect.center().y,
                    l.pos.y + l.galley.rect.center().y,
                );
                (k_y - l_y).abs() < 4.0 && k.pos.x < left(l)
            })
        })
        .map(|t| t.pos.x + t.galley.rect.max.x)
        .collect::<Vec<_>>();
    assert!(rights.len() >= 3, "keys beside labels: {rights:?}");
    let spread = rights.iter().fold(f32::MIN, |a, &b| a.max(b))
        - rights.iter().fold(f32::MAX, |a, &b| a.min(b));
    assert!(spread < 0.5, "key right edges {rights:?}");
}

#[test]
fn picks_and_switches_are_drawn_checkboxes_not_bracket_text() {
    for (file, name) in [
        ("generic", "list of switches screen"),
        ("trade", "trade screen"),
    ] {
        let h = open(example(file, name), egui::vec2(1280.0, 800.0));
        let brackets = texts(&h)
            .into_iter()
            .map(|t| t.galley.text().to_string())
            .filter(|t| t.contains("[ ]") || t.contains("[x]"))
            .collect::<Vec<_>>();
        assert!(brackets.is_empty(), "{name}: {brackets:?}");
    }
}

#[test]
fn trade_rows_stay_on_one_row_at_1920() {
    let screen = example("trade", "trade screen");
    let labels = ["list:npc", "list:you"]
        .into_iter()
        .flat_map(|id| row_labels(&screen, id))
        .collect::<Vec<_>>();
    let h = open(screen, egui::vec2(1920.0, 1080.0));
    let painted = painted_labels(&h, &labels);
    assert!(painted.len() >= 4, "{} labels painted", painted.len());
    let broken = painted
        .iter()
        .filter(|t| t.galley.rows.len() > 1 || t.galley.elided)
        .map(|t| format!("{:?} on {} rows", t.galley.text(), t.galley.rows.len()))
        .collect::<Vec<_>>();
    assert!(broken.is_empty(), "{broken:?}");
}

/// Writes PNGs of the screens these tests read to `$SHOT_DIR` for a person to look at:
/// `cargo test --test it ui_kit_table:: -- --ignored`.
#[test]
#[ignore = "needs a Vulkan adapter and SHOT_DIR"]
fn shots() {
    let dir = std::path::PathBuf::from(std::env::var("SHOT_DIR").expect("SHOT_DIR"));
    let screens = [
        ("colors", "colour manager", "colors"),
        ("generic", "filterable list screen", "filterable-list"),
        ("generic", "list of switches screen", "switches"),
        ("generic", "query popup screen", "query-popup"),
        ("trade", "trade screen", "trade"),
        ("pickup", "pickup screen", "pickup"),
        ("inventory", "drop selector screen", "inventory-drop"),
    ];
    for (w, h) in [(1280.0, 800.0), (1920.0, 1080.0)] {
        for (file, name, png) in screens {
            let builder = Harness::builder().with_size(egui::vec2(w, h)).wgpu();
            let mut harness = opened(builder, example(file, name));
            let path = dir.join(format!("{png}-{w}x{h}.png"));
            std::fs::create_dir_all(&dir).unwrap();
            harness.render().expect("renders").save(path).unwrap();
        }
    }
}

#[test]
fn a_trade_label_cut_at_1280_shows_its_whole_text_on_hover() {
    let screen = example("trade", "trade screen");
    let labels = ["list:npc", "list:you"]
        .into_iter()
        .flat_map(|id| row_labels(&screen, id))
        .collect::<Vec<_>>();
    let mut h = open(screen, egui::vec2(1280.0, 800.0));
    let cut = painted_labels(&h, &labels)
        .into_iter()
        .find(|t| t.galley.elided)
        .expect("a label cut short at 1280");
    let whole = cut.galley.text().to_string();
    h.hover_at(cut.pos + cut.galley.rect.center().to_vec2());
    h.run_steps(4);
    let shown = texts(&h)
        .into_iter()
        .any(|t| t.galley.text() == whole && !t.galley.elided);
    assert!(shown, "{whole:?} on hover");
}

#[test]
fn colour_manager_keeps_every_value_column_in_sight() {
    let h = open(
        example("colors", "colour manager"),
        egui::vec2(1280.0, 800.0),
    );
    let captions = [
        "Normal",
        "Custom Normal color:",
        "Invert",
        "Custom Invert color:",
    ];
    let shown = |caption: &str| {
        h.output().shapes.iter().any(|c| match &c.shape {
            egui::Shape::Text(t) if t.galley.text() == caption => c
                .clip_rect
                .expand(0.5)
                .contains_rect(t.visual_bounding_rect()),
            _ => false,
        })
    };
    let hidden = captions
        .into_iter()
        .filter(|c| !shown(c))
        .collect::<Vec<_>>();
    assert!(hidden.is_empty(), "{hidden:?}");
}
