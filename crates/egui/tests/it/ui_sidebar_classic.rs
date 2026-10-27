//! The classic sidebar as a player sees it at 1280x800 and 1920x1080: every status panel at its natural height in
//! the sidebar's monospace font, under no painted heading but named for a screen reader, the ground and inventory
//! rows below it and the message log under them.
use crate::common::fixture;
use crate::common::legible::cut_text;
use bn_client_egui::{App, EngineModel};
use eframe::egui::{self, epaint};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use serde_json::{Value, json};

type H<'a> = Harness<'a, App<EngineModel>>;

/// The window sizes the sidebar is held to.
const SIZES: [egui::Vec2; 2] = [egui::vec2(1280.0, 800.0), egui::vec2(1920.0, 1080.0)];

/// The panels of the classic layout above the log, in the engine's order.
const STATUS: [&str; 6] = ["Health", "Location", "Weather", "Time", "Weapon", "Compass"];

fn item(name: &str) -> Value {
    json!({"look": {"kind": "item", "id": "rock", "glyph": "*", "color": "light_gray"}, "name": name, "label": name})
}

fn logged(sequence: u32, text: &str) -> Value {
    json!({"jsonrpc": "2.0", "method": "bn.events", "params": {"epoch": "e7f3",
        "events": [{"sequence": sequence.to_string(), "revision": "32", "type": "message.logged",
        "data": {"id": sequence.to_string(), "text": text, "kind": "info", "color": "c_light_gray", "count": 1, "label": text}}]}})
}

/// A walk step whose avatar has the classic sidebar on the right, 44 columns wide, every panel on, six limbs, a
/// few things on the ground and a full inventory.
fn classic() -> Value {
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
    let panels = STATUS
        .iter()
        .chain(&["Log"])
        .map(|id| json!({"id": id, "label": id, "enabled": true}))
        .collect::<Vec<_>>();
    avatar["sidebar"]["layout"] =
        json!({"id": "classic", "side": "right", "width": 44, "panels": panels});
    let head = avatar["sidebar"]["limbs"][0].clone();
    avatar["sidebar"]["limbs"] = ["HEAD", "TORSO", "L ARM", "R ARM", "L LEG", "R LEG"]
        .iter()
        .map(|label| {
            let mut limb = head.clone();
            limb["id"] = json!(label.to_lowercase());
            limb["label"]["text"] = json!(label);
            limb
        })
        .collect();
    avatar["ground"] = json!(["rock", "stick", "tin can"].map(item));
    avatar["inventory"] = (0..20).map(|n| item(&format!("smooth rock {n}"))).collect();
    let mut event = fixture("events_41_walk");
    event["params"]["events"][0]["changes"]["avatar"] = avatar;
    event
}

/// The map screen with the classic sidebar and a log of 30 lines, in a window of `size`, settled.
fn shown<'a>(size: egui::Vec2) -> H<'a> {
    let map = fixture("subscribe_result")["result"]["interaction"]["stack"][0].clone();
    let (app, script) = crate::common::staged(json!([map]));
    let mut h = crate::common::themed_harness(size, app);
    h.run_steps(3);
    script.push(classic());
    for n in 0..30 {
        script.push(logged(42 + n, &format!("You hear something {n}.")));
    }
    h.run_steps(4);
    crate::common::settle_windows(&mut h);
    h
}

fn rect(h: &H, label: &str) -> egui::Rect {
    h.get_by_label(label).rect()
}

/// Every painted text with its clip rectangle.
fn texts(shapes: &[epaint::ClippedShape]) -> Vec<(egui::Rect, &epaint::TextShape)> {
    shapes
        .iter()
        .filter_map(|s| match &s.shape {
            egui::Shape::Text(text) => Some((s.clip_rect, text)),
            _ => None,
        })
        .collect()
}

#[test]
fn every_status_panel_shows_whole_above_the_items_and_the_log() {
    for size in SIZES {
        let h = shown(size);
        let ground = rect(&h, "Ground");
        let log = rect(&h, "Log");
        for panel in STATUS {
            let at = rect(&h, panel);
            assert!(
                at.max.y <= ground.min.y + 1.0,
                "{size:?}: {panel} {at:?} runs under the ground list at {ground:?}"
            );
        }
        // The last status row, the nearest creature of the last direction, is in full view, not scrolled away.
        let last = rect(&h, "bat 1");
        assert!(last.max.y <= ground.min.y, "{size:?}: {last:?}");
        for row in [
            "bat 1",
            "Ground",
            "Pick up rock",
            "Inventory",
            "Drop smooth rock 0",
        ] {
            let at = rect(&h, row);
            assert!(
                at.max.y <= log.min.y,
                "{size:?}: {row} {at:?} is under the log {log:?}"
            );
        }
        let sidebar = egui::Rect::from_x_y_ranges(
            rect(&h, "Health").x_range(),
            h.ctx.content_rect().y_range(),
        );
        let cut = cut_text(&h.output().shapes, sidebar);
        assert!(cut.is_empty(), "{size:?}: {cut:#?}");
    }
}

#[test]
fn the_log_keeps_six_lines_at_the_least() {
    for size in SIZES {
        let h = shown(size);
        let line = h
            .ctx
            .fonts_mut(|f| f.row_height(&egui::FontId::proportional(15.0)));
        let log = rect(&h, "Log");
        assert!(log.height() >= 6.0 * line, "{size:?}: {log:?}");
    }
}

#[test]
fn every_status_text_is_in_the_sidebars_monospace_font() {
    for size in SIZES {
        let h = shown(size);
        let panels = STATUS.map(|p| rect(&h, p));
        let proportional = texts(&h.output().shapes)
            .into_iter()
            .filter(|(clip, text)| {
                let at = text.visual_bounding_rect().intersect(*clip);
                panels.iter().any(|p| p.contains_rect(at))
            })
            .filter(|(_, text)| {
                text.galley
                    .job
                    .sections
                    .iter()
                    .any(|s| s.format.font_id.family != egui::FontFamily::Monospace)
            })
            .map(|(_, text)| text.galley.text().to_string())
            .collect::<Vec<_>>();
        assert!(proportional.is_empty(), "{size:?}: {proportional:?}");
    }
}

#[test]
fn panel_labels_name_their_panels_and_are_not_painted() {
    for size in SIZES {
        let h = shown(size);
        let painted = texts(&h.output().shapes)
            .into_iter()
            .map(|(_, text)| text.galley.text().to_string())
            .collect::<Vec<_>>();
        for panel in STATUS.iter().chain(&["Log"]) {
            assert!(
                !painted.contains(&panel.to_string()),
                "{size:?}: {panel} painted"
            );
        }
        // Each panel's name covers what it holds.
        assert!(
            rect(&h, "Compass").contains_rect(rect(&h, "bat 1")),
            "{size:?}"
        );
        assert!(
            rect(&h, "Log").contains_rect(rect(&h, "You hear something 29.")),
            "{size:?}"
        );
    }
}

#[test]
#[ignore = "needs a Vulkan adapter and SHOT_DIR"]
fn shots() {
    for size in SIZES {
        let map = fixture("subscribe_result")["result"]["interaction"]["stack"][0].clone();
        let (app, script) = crate::common::staged(json!([map]));
        let mut h = crate::common::shot_harness(size, app);
        h.run_steps(3);
        script.push(classic());
        for n in 0..30 {
            script.push(logged(42 + n, &format!("You hear something {n}.")));
        }
        h.run_steps(4);
        crate::common::settle_windows(&mut h);
        crate::common::save_shot(&mut h, &format!("sidebar-{}x{}.png", size.x, size.y));
    }
}

/// Drags the log's top edge by `dy` and lets the sidebar settle; the edge's new top.
fn drag_log_edge(h: &mut H, dy: f32) -> f32 {
    let edge = rect(h, "Resize message log").center();
    h.hover_at(edge);
    h.run_steps(2);
    h.drag_at(edge);
    h.run_steps(2);
    for step in 1..=4 {
        h.hover_at(edge + egui::vec2(0.0, dy * step as f32 / 4.0));
        h.run_steps(2);
    }
    h.drop_at(edge + egui::vec2(0.0, dy));
    h.run_steps(5);
    rect(h, "Resize message log").min.y
}

#[test]
fn dragging_the_log_edge_up_takes_room_from_the_items_and_the_size_sticks() {
    let mut h = shown(egui::vec2(1920.0, 1080.0));
    let before = rect(&h, "Resize message log").min.y;
    let after = drag_log_edge(&mut h, -120.0);
    assert!(after < before - 60.0, "{before} -> {after}");
    h.run_steps(10);
    assert_eq!(rect(&h, "Resize message log").min.y, after);
    // Never below its least lines, however far it is dragged down.
    drag_log_edge(&mut h, 600.0);
    let line = h
        .ctx
        .fonts_mut(|f| f.row_height(&egui::FontId::proportional(15.0)));
    assert!(rect(&h, "Log").height() >= 6.0 * line);
}

/// Where every line of the message log is, as it is drawn now.
fn log_lines(h: &H) -> Vec<(String, Option<[i32; 4]>)> {
    h.query_all_by_label_contains("a long line")
        .map(|n| {
            let node = n.accesskit_node();
            let bounds = node
                .bounding_box()
                .map(|r| [r.x0, r.y0, r.x1, r.y1].map(|v| (v * 10.0) as i32));
            (node.value().unwrap_or_default().to_string(), bounds)
        })
        .collect()
}

#[test]
fn a_message_log_in_a_narrow_sidebar_stays_where_it_is_frame_after_frame() {
    let sizes = [egui::vec2(560.0, 800.0), SIZES[0]];
    let mut jumping = vec![];
    for size in sizes {
        for (columns, count) in [20, 30, 44]
            .into_iter()
            .flat_map(|c| [3, 12, 40].map(move |n| (c, n)))
        {
            let map = fixture("subscribe_result")["result"]["interaction"]["stack"][0].clone();
            let (app, script) = crate::common::staged(json!([map]));
            let mut h = crate::common::themed_harness(size, app);
            h.run_steps(3);
            let mut walk = classic();
            walk["params"]["events"][0]["changes"]["avatar"]["sidebar"]["layout"]["width"] =
                columns.into();
            script.push(walk);
            for n in 0..count {
                let text = format!(
                    "This is a long line {n} that has to wrap more than once in a narrow log panel."
                );
                script.push(logged(42 + n, &text));
            }
            h.run_steps(10);
            let seen = (0..30)
                .map(|_| {
                    h.run_steps(1);
                    log_lines(&h)
                })
                .collect::<Vec<_>>();
            let changing = seen.windows(2).filter(|w| w[0] != w[1]).count();
            if changing > 0 {
                jumping.push(format!(
                    "{size:?} {columns} columns with {count} lines: moved in {changing} of 29 frames"
                ));
            }
        }
    }
    assert!(jumping.is_empty(), "{jumping:#?}");
}

#[test]
fn the_inventory_shows_only_whole_rows_at_rest_however_far_it_is_scrolled() {
    // Rows of one and two lines, so a height measured from the bottom rows does not fit the top ones.
    let names = (0..24).map(|n| match n % 3 {
        0 => format!("a rock with a name long enough to wrap over two lines of the sidebar {n}"),
        _ => format!("smooth rock {n}"),
    });
    let mut cut = vec![];
    for size in SIZES {
        let map = fixture("subscribe_result")["result"]["interaction"]["stack"][0].clone();
        let (app, script) = crate::common::staged(json!([map]));
        let mut h = crate::common::themed_harness(size, app);
        h.run_steps(3);
        let mut walk = classic();
        walk["params"]["events"][0]["changes"]["avatar"]["inventory"] =
            names.clone().map(|n| item(&n)).collect();
        script.push(walk);
        h.run_steps(4);
        crate::common::settle_windows(&mut h);
        let sidebar = egui::Rect::from_x_y_ranges(
            rect(&h, "Health").x_range(),
            h.ctx.content_rect().y_range(),
        );
        let at = rect(&h, "Inventory").center_bottom() + egui::vec2(0.0, 40.0);
        for wheel in [0.0, -0.7, -1.3, -2.0, 1.1] {
            h.hover_at(at);
            h.run_steps(2);
            h.event(egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Line,
                delta: egui::vec2(0.0, wheel),
                phase: egui::TouchPhase::Move,
                modifiers: egui::Modifiers::NONE,
            });
            h.run_steps(30);
            let lines = cut_text(&h.output().shapes, sidebar);
            if !lines.is_empty() {
                cut.push(format!("{size:?} after a wheel of {wheel}: {lines:?}"));
            }
        }
    }
    assert!(cut.is_empty(), "{cut:#?}");
}

#[test]
fn heard_sounds_under_a_full_log_show_whole_in_the_log_panel() {
    let mut cut = vec![];
    for size in SIZES {
        let map = fixture("subscribe_result")["result"]["interaction"]["stack"][0].clone();
        let (app, script) = crate::common::staged(json!([map]));
        let mut h = crate::common::themed_harness(size, app);
        h.run_steps(3);
        script.push(classic());
        for n in 0..30 {
            script.push(logged(42 + n, &format!("You hear something {n}.")));
        }
        for (n, sound) in ["footsteps", "a door slamming", "gargling grunting"]
            .iter()
            .enumerate()
        {
            script.push(json!({"jsonrpc": "2.0", "method": "bn.events", "params": {"epoch": "e7f3",
                "events": [{"sequence": (72 + n).to_string(), "revision": "32", "type": "sound.heard",
                "data": {"text": sound, "volume": 52, "logged": false}}]}}));
        }
        h.run_steps(6);
        crate::common::settle_windows(&mut h);
        let (sound, log) = (rect(&h, "footsteps"), rect(&h, "Log"));
        if !log.contains_rect(sound) {
            cut.push(format!(
                "{size:?}: the sound {sound:?} is out of the log {log:?}"
            ));
        }
        let sidebar = egui::Rect::from_x_y_ranges(
            rect(&h, "Health").x_range(),
            h.ctx.content_rect().y_range(),
        );
        let lines = cut_text(&h.output().shapes, sidebar);
        if !lines.is_empty() {
            cut.push(format!("{size:?}: {lines:?}"));
        }
    }
    assert!(cut.is_empty(), "{cut:#?}");
}
