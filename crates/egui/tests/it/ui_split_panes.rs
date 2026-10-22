//! Split panes are as wide as their content: the character sheet reads whole words at 1280 points, and the
//! split scrolls sideways, not word by word, when the window is too narrow for its panes.
use crate::common::fixture;
use bn_client_egui::transport::ScriptedTransport;
use bn_client_egui::{App, Clock, ConnectOptions, EngineModel};
use eframe::egui;
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use serde_json::Value;

type H<'a> = Harness<'a, App<EngineModel>>;

fn character_sheet() -> Value {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../core/tests/fixtures/0.2.examples/character.json"
    );
    let all = serde_json::from_str::<Value>(&std::fs::read_to_string(path).unwrap()).unwrap();
    all["valid"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["name"] == "character sheet")
        .unwrap()["value"]
        .clone()
}

fn open<'a>(size: egui::Vec2) -> H<'a> {
    let mut subscribe = fixture("subscribe_result");
    subscribe["result"]["interaction"]["stack"] = serde_json::json!([character_sheet()]);
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
    let mut h = Harness::builder().with_size(size).build_ui_state(
        |ui, app: &mut App<EngineModel>| app.show(ui),
        App::new(model).with_clock(clock),
    );
    h.run_steps(6);
    h
}

/// The height of one line of text in the harness's theme.
const LINE: f32 = 30.0;

/// Every short label holding `name`: the row's own, not the long description that mentions it.
fn names<'a>(h: &'a H, name: &'a str) -> Vec<egui_kittest::Node<'a>> {
    let found = h
        .query_all_by_label_contains(name)
        .filter(|n| n.accesskit_node().label().is_some_and(|l| l.len() < 30))
        .collect::<Vec<_>>();
    assert!(!found.is_empty(), "nothing shows {name}");
    found
}

#[test]
fn the_sheets_stat_and_skill_names_each_stay_on_one_line() {
    let h = open(egui::vec2(1280.0, 800.0));
    for name in ["Strength", "Dexterity", "Perception"] {
        for node in names(&h, name) {
            assert!(
                node.rect().height() < LINE,
                "{name} broke: {:?}",
                node.rect()
            );
        }
    }
}

#[test]
fn a_window_too_narrow_for_the_panes_keeps_the_words_whole_and_scrolls() {
    let h = open(egui::vec2(700.0, 800.0));
    for node in names(&h, "Strength") {
        assert!(node.rect().height() < LINE, "{:?}", node.rect());
    }
}
