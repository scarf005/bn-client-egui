#![recursion_limit = "512"]
//! The sidebar's sections never draw over one another, on the first frame after every state update, at the
//! default window size of a QHD monitor's quarter and beside it.
mod common;
use bn_client_egui::{App, EngineModel};
use common::{fixture, session};
use eframe::egui;
use egui_kittest::Harness;
use serde_json::{Value, json};

type H<'a> = Harness<'a, App<EngineModel>>;

fn example(name: &str) -> Value {
    let text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../core/tests/fixtures/0.2.examples/world.json"
    ))
    .unwrap();
    let all: Value = serde_json::from_str(&text).unwrap();
    let mut found = all["valid"].as_array().unwrap().iter();
    found.find(|e| e["name"] == name).unwrap()["value"].clone()
}

fn logged(sequence: u32, text: &str) -> Value {
    json!({"jsonrpc": "2.0", "method": "bn.events", "params": {"epoch": "e7f3",
        "events": [{"sequence": sequence.to_string(), "revision": "32", "type": "message.logged",
        "data": {"id": sequence.to_string(), "text": text, "kind": "info", "color": "c_light_gray", "count": 1, "label": text}}]}})
}

fn heard(sequence: u32, text: &str) -> Value {
    json!({"jsonrpc": "2.0", "method": "bn.events", "params": {"epoch": "e7f3", "events": [{
        "sequence": sequence.to_string(), "revision": "32", "type": "sound.heard",
        "data": {"text": text, "volume": 52, "logged": false}}]}})
}

/// The painted text boxes, each cut to what its clip rectangle shows.
fn painted(h: &H) -> Vec<(String, egui::Rect)> {
    h.output()
        .shapes
        .iter()
        .filter_map(|clipped| {
            let egui::Shape::Text(text) = &clipped.shape else {
                return None;
            };
            let shown = text.visual_bounding_rect().intersect(clipped.clip_rect);
            let text_shown = !text.galley.text().trim().is_empty();
            (text_shown && shown.width() > 0.0 && shown.height() > 0.0)
                .then(|| (text.galley.text().to_string(), shown))
        })
        .collect()
}

fn overlaps(h: &H) -> Vec<String> {
    let right = h.ctx.content_rect().width() * 0.7;
    let texts = painted(h)
        .into_iter()
        .filter(|(_, r)| r.min.x > right)
        .collect::<Vec<_>>();
    texts
        .iter()
        .enumerate()
        .flat_map(|(i, (a, ra))| {
            texts[i + 1..].iter().filter_map(move |(b, rb)| {
                let both = ra.intersect(*rb);
                (both.width() > 2.0 && both.height() > 2.0).then(|| format!("{a:?} over {b:?}"))
            })
        })
        .take(6)
        .collect()
}

/// The painted rows of the panels above the message log lie above its heading.
fn above_the_log(h: &H) -> Vec<String> {
    let texts = painted(h);
    let Some(log) = texts.iter().find(|(t, _)| t == "Message log") else {
        return vec!["no message log".into()];
    };
    let above = [
        "shirt number",
        "knife_combat",
        "tin_can",
        "Drop",
        "Pick up",
        "Inventory",
    ];
    texts
        .iter()
        .filter(|(t, r)| above.iter().any(|a| t.contains(a)) && r.max.y > log.1.min.y + 1.0)
        .map(|(t, r)| format!("{t:?} ends at {} under the log at {}", r.max.y, log.1.min.y))
        .take(6)
        .collect()
}

#[test]
fn no_section_is_drawn_over_another_after_each_update() {
    for (width, height) in [
        (1280.0, 720.0),
        (1100.0, 800.0),
        (1920.0, 1080.0),
        (1000.0, 500.0),
    ] {
        let (t, script) = session();
        let mut h = Harness::builder()
            .with_size(egui::vec2(width, height))
            .build_ui_state(
                |ui, app: &mut App<EngineModel>| app.show(ui),
                App::new(EngineModel::new(Box::new(t))),
            );
        h.run_steps(3);
        let mut seq = 42;
        for turn in 0..6u32 {
            let mut event = fixture("events_41_walk");
            let mut avatar = example("avatar with creatures on the compass");
            avatar["sidebar"]["layout"] = json!({"id": "labels", "side": "right", "width": 50, "panels": [
                {"id": "Health", "label": "Health", "enabled": true, "height": 7},
                {"id": "Location", "label": "Location", "enabled": true, "height": 1},
                {"id": "Compass", "label": "Compass", "enabled": true, "height": 8},
                {"id": "Log", "label": "Message log", "enabled": true}]});
            avatar["inventory"] = (0..8)
                .map(|n| {
                    let name = format!("long-sleeved shirt number {n} (worn)");
                    json!({"look": {"kind": "item", "id": "rock", "glyph": "*", "color": "light_gray"},
                        "name": name, "label": name, "slot": "worn"})
                })
                .collect();
            event["params"]["events"][0]["changes"]["avatar"] = avatar;
            if turn == 0 {
                script.push(event);
            }
            for _ in 0..4 + turn * 12 {
                script.push(logged(
                    seq,
                    &format!("Moving onto this empty window {seq} is slow!"),
                ));
                script.push(heard(seq + 1, &format!("footsteps (east, {seq})")));
                seq += 2;
            }
            // One frame is all a player sees of an update: nothing may overlap on it.
            h.run_steps(1);
            assert_eq!(
                [overlaps(&h), above_the_log(&h)].concat(),
                Vec::<String>::new(),
                "{width}x{height}, turn {turn}, first frame"
            );
            h.run_steps(4);
            assert_eq!(
                [overlaps(&h), above_the_log(&h)].concat(),
                Vec::<String>::new(),
                "{width}x{height}, turn {turn}, settled"
            );
        }
    }
}
