//! Against a real engine: while the avatar waits out six hours, the top bar names the activity with how much of
//! it is done, and a bar over the avatar fills as the hours pass; pictures of both.
//!
//! `BN_ENGINE=<engine> SHOT_DIR=<dir> cargo test --release -p bn-client-egui --test live_activity -- --ignored
//! --nocapture`; `BN_ROOT` names the game root when it is not beside the engine.
mod common;
use bn_client_egui::{GameModel, MenuModel};
use common::live::{H, new_game, pick, run_until, save, settle, tiled};
use eframe::egui;
use egui_kittest::kittest::Queryable;

/// The thousandths of the avatar's activity done, if it has one with an end.
fn done(h: &H) -> Option<u16> {
    h.state().model.activity().and_then(|a| a.done)
}

#[test]
#[ignore = "requires BN_ENGINE pointing at an engine binary"]
fn waiting_six_hours_names_the_activity_and_fills_its_bar() {
    let mut h = tiled("activity", egui::vec2(1280.0, 720.0));
    new_game(&mut h, "new_game:play_now_default");
    assert!(
        h.state().model.activity().is_none(),
        "an activity before waiting"
    );
    assert!(h.state_mut().model.invoke_action("wait"));
    run_until(&mut h, "the wait menu", 60, |h| {
        !h.state().model.busy() && h.state().model.menu().is_some()
    });
    // With an alarm clock, the game first asks whether to wait or set the alarm.
    let asks = |h: &H| {
        h.state()
            .model
            .menu()
            .is_some_and(|m| m.choices.iter().any(|c| c.label == "Wait a while"))
    };
    if asks(&h) {
        pick(&mut h, "Wait a while");
    }
    // Chosen without waiting for the engine to be free again, which it is only once the wait is over.
    let six = |h: &H| {
        h.state()
            .model
            .menu()
            .and_then(|m| m.choices.into_iter().find(|c| c.label.contains("6 hours")))
    };
    run_until(&mut h, "6 hours", 120, |h| {
        !h.state().model.busy() && six(h).is_some()
    });
    let id = six(&h).expect("listed").id;
    h.state_mut().model.choose(&id);
    run_until(&mut h, "the wait under way", 120, |h| {
        done(h).is_some_and(|d| d > 0)
    });
    let first = done(&h).unwrap();
    let name = h.state().model.activity().unwrap().name;
    println!("LIVE activity {name:?} at {first}/1000");
    assert!(h.query_by_label(&name).is_some(), "{name} in the top bar");
    settle(&mut h);
    save(&mut h, "waiting-early.png");
    run_until(&mut h, "more of the wait done", 300, |h| {
        done(h).is_some_and(|d| d >= first + 100)
    });
    settle(&mut h);
    println!("LIVE activity later at {:?}/1000", done(&h));
    save(&mut h, "waiting-later.png");
}
