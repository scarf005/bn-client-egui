use bn_client_egui::{App, Effect, FakeModel, GameModel, Pos, Shape};
use eframe::egui;
use egui_kittest::Harness;

type H<'a> = Harness<'a, App<FakeModel>>;

fn shot() -> Effect {
    Effect {
        shape: Shape::Projectile {
            path: vec![Pos { x: 6, y: 5 }, Pos { x: 7, y: 5 }],
            glyph: '*',
            sprite: None,
            color: bn_client_egui::Rgb(255, 0, 0),
        },
        duration_ms: 20,
    }
}

fn harness<'a>(app: App<FakeModel>) -> H<'a> {
    Harness::builder()
        .with_size(egui::vec2(1100.0, 800.0))
        .with_step_dt(0.01)
        .build_ui_state(|ui, app: &mut App<FakeModel>| app.show(ui), app)
}

fn with_queued(settings: Option<&str>) -> App<FakeModel> {
    let mut model = FakeModel::default();
    model.queue_effect(shot());
    let app = App::new(model);
    let Some(json) = settings else { return app };
    let path = std::env::temp_dir().join(format!(
        "bn-effects-{}-{}.json",
        std::process::id(),
        json.len()
    ));
    std::fs::write(&path, json).unwrap();
    app.with_settings(path)
}

#[test]
fn a_shot_from_the_game_plays_on_the_map_and_is_handed_out_once() {
    let mut h = harness(with_queued(None));
    h.run_steps(1);
    assert_eq!(h.state().playing_effects(), 1);
    assert!(h.state_mut().model.take_effects().is_empty());
}

#[test]
fn reduced_motion_drops_the_animations_unseen() {
    let mut h = harness(with_queued(Some(
        r#"{"tiles":false,"tileset":null,"reduced_motion":true}"#,
    )));
    h.run_steps(3);
    assert_eq!(h.state().playing_effects(), 0);
}

#[test]
fn a_finished_animation_leaves_the_map() {
    let mut h = harness(with_queued(None));
    h.run_steps(1);
    assert_eq!(h.state().playing_effects(), 1);
    h.run_steps(30);
    assert_eq!(h.state().playing_effects(), 0);
}
