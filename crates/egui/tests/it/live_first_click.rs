//! Against a real engine: one press and release on the world of the main menu's Load tab opens it.
//!
//! `BN_ENGINE=<engine> SHOT_DIR=<dir> cargo test -p bn-client-egui --test it live_first_click:: -- --ignored --nocapture`
use crate::common::live::{H, engine, new_game, pick, root, run_until, save, settle};
use bn_client_egui::transport::{Profile, StdioTransport};
use bn_client_egui::{App, EngineModel, GameModel, MenuModel};
use eframe::egui;
use egui_kittest::kittest::{NodeT as _, Queryable};

/// The client over an English engine whose profile is `dir`.
fn client<'a>(dir: &std::path::Path) -> H<'a> {
    let config = dir.join("config");
    std::fs::create_dir_all(&config).unwrap();
    std::fs::write(
        config.join("options.json"),
        r#"[{"name":"USE_LANG","value":"en_US"}]"#,
    )
    .unwrap();
    let profile = Profile {
        fresh: false,
        userdir: Some(dir.into()),
        configdir: Some(config),
        seed: Some("2".into()),
    };
    let t = StdioTransport::spawn_with(&engine(), &profile).expect("engine starts");
    let settings = dir.join("settings.json");
    std::fs::write(&settings, r#"{"tiles":false,"tileset":null}"#).unwrap();
    let app = App::new(EngineModel::new(Box::new(t)))
        .with_settings(settings)
        .with_root(root());
    let mut h = crate::common::shot_harness(egui::vec2(1280.0, 800.0), app);
    h.input_mut().max_texture_side = Some(8192);
    h
}

fn click(h: &mut H, pos: egui::Pos2) {
    h.hover_at(pos);
    h.run_steps(2);
    for pressed in [true, false] {
        h.event(egui::Event::PointerButton {
            pos: pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        });
        h.run_steps(1);
    }
}

#[test]
#[ignore = "needs BN_ENGINE and a Vulkan adapter"]
fn one_click_on_a_world_of_the_load_tab_opens_its_characters() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target")
        .join(format!("first-click-{}", std::process::id()));
    {
        let mut h = client(&dir);
        new_game(&mut h, "new_game:play_now_default");
        assert!(h.state_mut().model.invoke_action("save"));
        run_until(&mut h, "the save question", 60, |h| {
            h.state().model.menu().is_some()
        });
        pick(&mut h, "Yes");
        run_until(&mut h, "the main menu", 120, |h| {
            h.state().model.size().x == 0
        });
    }
    let mut h = client(&dir);
    run_until(&mut h, "the main menu", 120, |h| {
        h.state()
            .model
            .screens()
            .last()
            .is_some_and(|s| s.root.is_some())
    });
    settle(&mut h);
    let at = h.get_by_label("Load").rect().center();
    click(&mut h, at);
    settle(&mut h);
    save(&mut h, "load-tab.png");
    let at = h.get_by_label_contains("(1)").rect().center();
    click(&mut h, at);
    run_until(&mut h, "the characters of the world", 20, |h| {
        h.state()
            .model
            .menu()
            .is_some_and(|m| m.message.contains("Load character"))
    });
    settle(&mut h);
    save(&mut h, "after-one-click.png");
    drop(h);
    std::fs::remove_dir_all(dir).unwrap();
}
