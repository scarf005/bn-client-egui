//! Keys pressed while a command runs wait their turn instead of being dropped.
mod common;
use bn_client_egui::transport::ScriptHandle;
use bn_client_egui::{App, EngineModel};
use common::{fixture, session};
use eframe::egui;
use egui_kittest::Harness;

type H<'a> = Harness<'a, App<EngineModel>>;

fn walking<'a>() -> (H<'a>, ScriptHandle) {
    let (t, script) = session();
    let t = (0..6).fold(t, |t, _| {
        t.on("bn.command.submit", vec![fixture("submit_result")])
    });
    let app = App::new(EngineModel::new(Box::new(t)));
    let h = Harness::builder()
        .with_size(egui::vec2(1100.0, 800.0))
        .build_ui_state(|ui, app: &mut App<EngineModel>| app.show(ui), app);
    (h, script)
}

fn submits(script: &ScriptHandle) -> usize {
    script
        .sent_methods()
        .iter()
        .filter(|m| *m == "bn.command.submit")
        .count()
}

/// The key repeats the OS sends while a held key's step is still running.
fn repeat(h: &mut H, key: egui::Key, times: usize) {
    for _ in 0..times {
        h.key_press(key);
        h.run_steps(1);
    }
}

#[test]
fn a_held_key_sends_the_next_step_the_moment_the_previous_one_finishes() {
    let (mut h, script) = walking();
    h.run_steps(3);
    repeat(&mut h, egui::Key::ArrowRight, 6);
    assert_eq!(submits(&script), 1, "one command in flight at a time");
    script.push(fixture("command_completed"));
    h.run_steps(1);
    assert_eq!(
        submits(&script),
        2,
        "no frame or key event is needed between steps"
    );
}

#[test]
fn releasing_a_held_key_ends_the_walk_within_the_queue_cap() {
    let (mut h, script) = walking();
    h.run_steps(3);
    repeat(&mut h, egui::Key::ArrowRight, 20);
    for _ in 0..6 {
        script.push(fixture("command_completed"));
        h.run_steps(2);
    }
    assert_eq!(
        submits(&script),
        3,
        "the step that ran plus the two that waited"
    );
}

#[test]
fn a_key_that_runs_nothing_is_not_kept_for_later() {
    let (mut h, script) = walking();
    h.run_steps(3);
    repeat(&mut h, egui::Key::ArrowRight, 1);
    h.event(egui::Event::Text("z".into()));
    h.run_steps(1);
    script.push(fixture("command_completed"));
    h.run_steps(2);
    assert_eq!(submits(&script), 1);
}

fn key_event(key: egui::Key, pressed: bool, repeat: bool) -> egui::Event {
    egui::Event::Key {
        key,
        physical_key: Some(key),
        pressed,
        repeat,
        modifiers: egui::Modifiers::NONE,
    }
}

#[test]
fn releasing_a_held_key_drops_its_repeats_still_waiting() {
    let (mut h, script) = walking();
    h.run_steps(3);
    h.event(key_event(egui::Key::ArrowRight, true, false));
    h.run_steps(1);
    for _ in 0..5 {
        h.event(key_event(egui::Key::ArrowRight, true, true));
        h.run_steps(1);
    }
    h.event(key_event(egui::Key::ArrowRight, false, false));
    h.run_steps(1);
    for _ in 0..4 {
        script.push(fixture("command_completed"));
        h.run_steps(2);
    }
    assert_eq!(
        submits(&script),
        1,
        "only the step in flight when it was released"
    );
}

#[test]
fn tapping_a_key_while_a_command_runs_is_kept_after_release() {
    let (mut h, script) = walking();
    h.run_steps(3);
    repeat(&mut h, egui::Key::ArrowRight, 2);
    script.push(fixture("command_completed"));
    h.run_steps(2);
    assert_eq!(submits(&script), 2, "a tap's key-up does not cancel it");
}
