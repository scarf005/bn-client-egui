//! List rows as a table: a label on one line in lists with columns, keys in a column of their own so the labels
//! line up, drawn checkboxes instead of `[ ]` text. Each screen is the engine's own example, open over the map
//! as a player sees it, in the client's theme and fonts.
mod common;
use bn_client_egui::{App, EngineModel, markup};
use common::fixture;
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
    let map = fixture("subscribe_result")["result"]["interaction"]["stack"][0].clone();
    let mut subscribe = fixture("subscribe_result");
    subscribe["result"]["interaction"]["stack"] = json!([map, screen]);
    let reply = vec![
        subscribe,
        fixture("snapshot_part_0"),
        fixture("snapshot_part_1"),
    ];
    let (t, _script) = bn_client_egui::transport::ScriptedTransport::new();
    let t = t
        .on("bn.hello", vec![fixture("hello_result")])
        .on("bn.subscribe", reply.clone())
        .on("bn.subscribe", reply);
    let clock = bn_client_egui::Clock::manual();
    let model = EngineModel::open(Box::new(t), bn_client_egui::ConnectOptions::default())
        .with_clock(clock.clone());
    let app = App::new(model).with_clock(clock);
    let mut h = Harness::builder()
        .with_size(size)
        .build_ui_state(|ui, app: &mut App<EngineModel>| app.show(ui), app);
    bn_client_egui::theme::apply(&h.ctx);
    bn_client_egui::fonts::install_bundled(&h.ctx);
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
