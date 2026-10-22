//! Whether a frame can be read: no text cut by its clip, no label of a list with columns broken over lines, and
//! every window inside the room the map leaves, off the top bar, the shortcut bar and the sidebar.
use bn_client_egui::{App, EngineModel};
use eframe::egui::{self, epaint};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use serde_json::Value;

/// The kind of a fault, which names the rule it breaks.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Kind {
    /// Text painted partly outside its clip rectangle.
    Cut,
    /// A row label of a list with columns on more than one line.
    Wrapped,
    /// A window outside the room.
    Outside,
}

/// One thing in a frame that cannot be read, and what shows it.
#[derive(Clone, Debug)]
pub struct Fault {
    pub kind: Kind,
    pub what: String,
}

/// Every painted text with the clip rectangle it is painted in.
fn texts(shapes: &[epaint::ClippedShape]) -> Vec<(egui::Rect, &epaint::TextShape)> {
    fn walk<'a>(
        clip: egui::Rect,
        shape: &'a egui::Shape,
        out: &mut Vec<(egui::Rect, &'a epaint::TextShape)>,
    ) {
        match shape {
            egui::Shape::Text(text) => out.push((clip, text)),
            egui::Shape::Vec(shapes) => shapes.iter().for_each(|s| walk(clip, s, out)),
            _ => {}
        }
    }
    let mut out = Vec::new();
    shapes
        .iter()
        .for_each(|s| walk(s.clip_rect, &s.shape, &mut out));
    out
}

/// Lines of text in `area` painted partly outside their clip rectangle: shown in part, so neither read nor known
/// to be hidden. A line wholly outside its clip, or only touching its edge, is scrolled out of view and not counted.
pub fn cut_text(shapes: &[epaint::ClippedShape], area: egui::Rect) -> Vec<String> {
    texts(shapes)
        .into_iter()
        .filter(|(clip, _)| clip.intersects(area))
        .flat_map(|(clip, text)| {
            text.galley.rows.iter().filter_map(move |row| {
                let at = row.rect().translate(text.pos.to_vec2());
                (clip.intersect(at).is_positive() && !clip.expand(1.0).contains_rect(at))
                    .then(|| format!("{:?} at {at:?} in {clip:?}", row.text()))
            })
        })
        .collect()
}

/// Labels of `labels` painted on more lines than they hold: a row label broken by the width left to it.
pub fn wrapped_labels(shapes: &[epaint::ClippedShape], labels: &[String]) -> Vec<String> {
    texts(shapes)
        .into_iter()
        .filter_map(|(_, text)| {
            let shown = text.galley.text();
            let label = labels.iter().find(|l| shown.ends_with(l.as_str()))?;
            (text.galley.rows.len() > label.lines().count())
                .then(|| format!("{shown:?} on {} lines", text.galley.rows.len()))
        })
        .collect()
}

/// Windows not wholly inside `room`.
pub fn outside_room(h: &Harness<'_, App<EngineModel>>, room: egui::Rect) -> Vec<String> {
    h.query_all_by_role(egui::accesskit::Role::Window)
        .filter(|w| !room.expand(1.0).contains_rect(w.rect()))
        .map(|w| {
            let name = w.accesskit_node().label().unwrap_or_default();
            format!("{name:?} at {:?} outside {room:?}", w.rect())
        })
        .collect()
}

/// The room windows sit in: what the map leaves in a game, the window under the top bar on the title screen.
pub fn room(h: &Harness<'_, App<EngineModel>>) -> egui::Rect {
    match h.query_by_role_and_label(egui::accesskit::Role::Canvas, "Map") {
        Some(map) => map.rect(),
        None => {
            let bar = h.get_by_label("Actions").rect();
            let window = h.ctx.content_rect();
            egui::Rect::from_min_max(egui::pos2(window.min.x, bar.max.y), window.max)
        }
    }
}

/// The plain labels of the rows of every list with columns in `screen`.
pub fn column_list_labels(screen: &Value) -> Vec<String> {
    fn walk(v: &Value, out: &mut Vec<String>) {
        match v {
            Value::Object(o) => {
                let columned = o
                    .get("columns")
                    .and_then(Value::as_array)
                    .is_some_and(|c| !c.is_empty());
                if o.get("kind").and_then(Value::as_str) == Some("list") && columned {
                    let rows = o
                        .get("rows")
                        .and_then(Value::as_array)
                        .into_iter()
                        .flatten();
                    out.extend(
                        rows.filter_map(|r| r["label"].as_str())
                            .map(bn_client_core::markup::plain)
                            .filter(|l| !l.is_empty()),
                    );
                }
                o.values().for_each(|v| walk(v, out));
            }
            Value::Array(a) => a.iter().for_each(|v| walk(v, out)),
            _ => {}
        }
    }
    let mut out = Vec::new();
    walk(screen, &mut out);
    out
}

/// Everything that keeps the last frame of `h`, showing `screen`, from being read: the whole window for the map
/// screen, the room for any other, whose own faults the map's chrome must not hide.
pub fn legible(h: &Harness<'_, App<EngineModel>>, screen: &Value) -> Vec<Fault> {
    let shapes = &h.output().shapes;
    let fault = |kind| move |what| Fault { kind, what };
    let area = match screen["role"].as_str() {
        Some("map") => h.ctx.content_rect(),
        _ => room(h),
    };
    cut_text(shapes, area)
        .into_iter()
        .map(fault(Kind::Cut))
        .chain(
            wrapped_labels(shapes, &column_list_labels(screen))
                .into_iter()
                .map(fault(Kind::Wrapped)),
        )
        .chain(
            outside_room(h, room(h))
                .into_iter()
                .map(fault(Kind::Outside)),
        )
        .collect()
}
