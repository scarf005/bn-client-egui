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

#[test]
fn the_trade_screens_credit_line_keeps_its_width_and_the_lists_share_the_rest() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../core/tests/fixtures/0.2.examples/trade.json"
    );
    let all = serde_json::from_str::<Value>(&std::fs::read_to_string(path).unwrap()).unwrap();
    let trade = all["valid"][0]["value"].clone();
    let size = egui::vec2(1920.0, 1080.0);
    let app = crate::common::staged(serde_json::json!([trade])).0;
    let mut h = crate::common::themed_harness(size, app);
    h.run_steps(6);
    crate::common::settle_windows(&mut h);
    let credit = h.get_by_label("Credit $0.00").rect();
    let npc = h.get_by_label("Inventory: Daniel Arevalo, Merchant").rect();
    let you = h.get_by_label("Inventory: You").rect();
    // The pane's margin and the gap to the next pane.
    let pane = credit.width() + 2.0 * 8.0 + 8.0 + 1.0;
    assert!(npc.left() - credit.left() <= pane, "{credit:?} {npc:?}");
    // The lists share what the credit line leaves; the player's list is far wider than its title.
    assert!(you.left() - npc.left() > 400.0, "{npc:?} {you:?}");
    assert!(size.x - you.left() > 500.0, "{you:?}");
}

#[test]
fn a_terminals_answers_sit_under_its_output() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../core/tests/fixtures/0.2.examples/terminal.json"
    );
    let all = serde_json::from_str::<Value>(&std::fs::read_to_string(path).unwrap()).unwrap();
    let terminal = all["valid"][0]["value"].clone();
    for size in [egui::vec2(1280.0, 800.0), egui::vec2(1920.0, 1080.0)] {
        let app = crate::common::staged(serde_json::json!([terminal])).0;
        let mut h = crate::common::themed_harness(size, app);
        h.run_steps(6);
        crate::common::settle_windows(&mut h);
        let output = h.get_by_label_contains("ERROR: Please place").rect();
        let yes = h.get_by_label("Yes").rect();
        assert!(yes.top() >= output.bottom(), "{size:?}: {output:?} {yes:?}");
        // Within a pane's margin and a row's padding.
        assert!(
            (yes.left() - output.left()).abs() <= 16.0,
            "{size:?}: {output:?} {yes:?}"
        );
    }
}
