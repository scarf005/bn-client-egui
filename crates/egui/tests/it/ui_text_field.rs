//! A text box is outlined while idle, so the vehicle editor's part search reads as a place to type rather than as a
//! line of text.
use crate::common::fixture;
use bn_client_egui::kit::tokens::BORDER;
use bn_client_egui::transport::ScriptedTransport;
use bn_client_egui::{App, Clock, ConnectOptions, EngineModel};
use eframe::egui::{self, epaint};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use serde_json::{Value, json};

fn editor() -> Value {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../core/tests/fixtures/0.2.examples/vehicle_editor.json"
    );
    let all = serde_json::from_str::<Value>(&std::fs::read_to_string(path).unwrap()).unwrap();
    all["valid"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["name"] == "vehicle editor with a prototype open on a mount")
        .unwrap()["value"]
        .clone()
}

/// Every rectangle outline painted this frame.
fn outlines(h: &Harness<'_, App<EngineModel>>) -> Vec<(egui::Rect, egui::Stroke)> {
    fn flat(shape: &egui::Shape, out: &mut Vec<(egui::Rect, egui::Stroke)>) {
        match shape {
            egui::Shape::Vec(all) => all.iter().for_each(|s| flat(s, out)),
            egui::Shape::Rect(epaint::RectShape { rect, stroke, .. }) if stroke.width > 0.0 => {
                out.push((*rect, *stroke))
            }
            _ => {}
        }
    }
    let mut out = vec![];
    h.output()
        .shapes
        .iter()
        .for_each(|c| flat(&c.shape, &mut out));
    out
}

#[test]
fn the_part_search_box_is_outlined_while_idle() {
    let mut subscribe = fixture("subscribe_result");
    subscribe["result"]["interaction"]["stack"] = json!([editor()]);
    let reply = vec![
        subscribe,
        fixture("snapshot_part_0"),
        fixture("snapshot_part_1"),
    ];
    let (t, _) = ScriptedTransport::new();
    let t = t
        .on("bn.hello", vec![fixture("hello_result")])
        .on("bn.subscribe", reply.clone())
        .on("bn.subscribe", reply);
    let clock = Clock::manual();
    let model = EngineModel::open(Box::new(t), ConnectOptions::default()).with_clock(clock.clone());
    let mut h = Harness::builder()
        .with_size(egui::vec2(1280.0, 800.0))
        .build_ui_state(
            |ui, app: &mut App<EngineModel>| app.show(ui),
            App::new(model).with_clock(clock),
        );
    h.run_steps(8);
    let search = h
        .get_all_by_role(egui::accesskit::Role::TextInput)
        // The box that filters the part list is named by the list.
        .find(|n| n.accesskit_node().label().as_deref() == Some("Parts"))
        .expect("the part search box")
        .rect();
    let framed = outlines(&h)
        .into_iter()
        .any(|(rect, stroke)| stroke.color == BORDER && rect.expand(2.0).contains_rect(search));
    assert!(framed, "the search box {search:?} has no outline");
}
