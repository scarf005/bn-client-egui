//! Against a real engine: Esc in a game opens the main menu, tall enough for all its rows or the whole room, at
//! the size of a client window and after an earlier session remembered a short window for the same kind.
//!
//! `BN_ENGINE=<engine> SHOT_DIR=<dir> cargo test -p bn-client-egui --test it live_menu_height:: -- --ignored --nocapture`
use crate::common::live::{H, english_transport, new_game, root, save, settle};
use bn_client_egui::{App, EngineModel, GameModel};
use eframe::egui;
use egui_kittest::kittest::Queryable;

const SIZES: [(f32, f32); 3] = [(1290.0, 830.0), (1280.0, 800.0), (1920.0, 1080.0)];

fn client<'a>(size: (f32, f32), remembered: bool) -> H<'a> {
    let dir = std::env::temp_dir().join(format!(
        "bn-menu-height-{}-{}-{remembered}",
        std::process::id(),
        size.0
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let windows = match remembered {
        true => format!(
            r#","windows":{{"screen:uilist":{{"pos":[60.0,120.0],"size":[{},210.0],"collapsed":false}}}}"#,
            size.0 * 0.6
        ),
        false => String::new(),
    };
    let settings = dir.join("settings.json");
    std::fs::write(
        &settings,
        format!(r#"{{"tiles":false,"tileset":null{windows}}}"#),
    )
    .unwrap();
    let app = App::new(EngineModel::new(Box::new(english_transport())))
        .with_settings(settings)
        .with_root(root());
    let mut h = crate::common::shot_harness(egui::vec2(size.0, size.1), app);
    h.input_mut().max_texture_side = Some(8192);
    h
}

/// The last row label of the first list the top screen has.
fn last_row(h: &H) -> String {
    fn find(v: &serde_json::Value) -> Option<String> {
        match v {
            serde_json::Value::Object(o) => o
                .get("rows")
                .and_then(|r| r.as_array())
                .and_then(|r| r.last())
                .and_then(|r| r["label"].as_str().map(str::to_string))
                .or_else(|| o.values().find_map(find)),
            serde_json::Value::Array(a) => a.iter().find_map(find),
            _ => None,
        }
    }
    let screen = h.state().model.screens().last().cloned().expect("a screen");
    find(&serde_json::to_value(screen).unwrap()).expect("a list with rows")
}

#[test]
#[ignore = "needs BN_ENGINE"]
fn the_main_menu_in_a_game_opens_showing_all_its_rows_or_the_room() {
    let mut misses = vec![];
    for remembered in [false, true] {
        for size in SIZES {
            let mut h = client(size, remembered);
            new_game(&mut h, "new_game:play_now_default");
            h.key_press(egui::Key::Escape);
            settle(&mut h);
            let id = egui::Id::new(("dialog", "screen:uilist"));
            let window = h.ctx.memory(|m| m.area_rect(id)).expect("the menu window");
            let last = last_row(&h);
            let row = h
                .query_all_by_label_contains(&last)
                .last()
                .map(|n| n.rect());
            let name = format!(
                "{}x{}{}",
                size.0,
                size.1,
                if remembered { "-remembered" } else { "" }
            );
            save(&mut h, &format!("main-menu-{name}.png"));
            let room = egui::vec2(size.0, size.1).y - 47.0 - 16.0;
            let shows = row.is_some_and(|r| window.contains_rect(r.shrink(1.0)));
            if !(shows || window.height() >= 0.9 * room - 2.0) {
                misses.push(format!(
                    "{name}: {window:?} does not show {last:?} ({row:?})"
                ));
            }
        }
    }
    assert!(misses.is_empty(), "{misses:#?}");
}
