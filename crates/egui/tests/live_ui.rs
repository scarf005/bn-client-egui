//! Needs a real engine build: `BN_ENGINE=/path/to/cataclysm-bn-tiles cargo test -p bn-client-egui --test live_ui -- --ignored --nocapture`
//! Set `BN_CLIENT_TRACE=<file>` to log the JSON-RPC traffic.
use bn_client_egui::transport::StdioTransport;
use bn_client_egui::watchdog::Watchdog;
use bn_client_egui::{App, EngineModel, GameModel};
use eframe::egui;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use std::time::{Duration, Instant};

type H<'a> = Harness<'a, App<EngineModel>>;

fn run_until(h: &mut H, what: &str, secs: u64, mut done: impl FnMut(&mut H) -> bool) {
    let mut watchdog = Watchdog::new(Duration::from_secs(secs), Duration::from_secs(600));
    while !done(h) {
        let model = &h.state().model;
        assert!(
            !watchdog.expired(model.received()),
            "timed out waiting for {what}; status: {:?}; menu: {:?}; last messages: {:?}",
            model.command(),
            model.menu().map(|m| (m.context, m.title)),
            model.messages().iter().rev().take(5).collect::<Vec<_>>()
        );
        h.run_steps(1);
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[test]
#[ignore = "requires BN_ENGINE pointing at an engine binary"]
fn clicking_new_game_shows_the_command_status() {
    let engine = std::env::var_os("BN_ENGINE").expect("set BN_ENGINE");
    let transport = StdioTransport::spawn_fresh(engine.as_ref()).expect("engine starts");
    let model = EngineModel::new(Box::new(transport));
    let mut h = Harness::builder()
        .with_size(egui::vec2(1300.0, 800.0))
        .build_ui_state(
            |ui, app: &mut App<EngineModel>| app.show(ui),
            App::new(model),
        );
    run_until(&mut h, "the main menu", 60, |h| {
        h.query_by_label("New Game").is_some()
    });
    h.get_by_label("New Game").click();
    let clicked = Instant::now();
    run_until(&mut h, "the status line to follow the click", 60, |h| {
        h.query_by_label("New Game: done").is_some()
    });
    // The engine answers with the same flattened main menu, so the status line is the only reaction.
    println!("New Game done after {:?}", clicked.elapsed());
}

fn crafting(h: &H) -> Option<bn_client_egui::Menu> {
    h.state().model.menu().filter(|m| m.context == "CRAFTING")
}

fn recipes(h: &H) -> Vec<bn_client_egui::MenuChoice> {
    let rows = crafting(h).map(|m| m.choices).unwrap_or_default();
    rows.into_iter()
        .filter(|c| c.id.starts_with("recipe:"))
        .collect()
}

#[test]
#[ignore = "requires BN_ENGINE pointing at an engine binary"]
fn live_crafting_menu_switches_tabs_searches_shows_details_and_closes() {
    let engine = std::env::var_os("BN_ENGINE").expect("set BN_ENGINE");
    let transport = StdioTransport::spawn_fresh(engine.as_ref()).expect("engine starts");
    let model = EngineModel::new(Box::new(transport));
    let mut h = Harness::builder()
        .with_size(egui::vec2(1300.0, 800.0))
        .build_ui_state(
            |ui, app: &mut App<EngineModel>| app.show(ui),
            App::new(model),
        );
    run_until(&mut h, "the main menu", 60, |h| {
        h.query_by_label("New Game").is_some()
    });
    h.get_by_label("New Game").click();
    // A click can land before the window has settled its layout or while the tab command is still in
    // flight; it is then ignored, so press Tutorial again until the engine reports it was chosen.
    run_until(&mut h, "the tutorial map", 300, |h| {
        let chosen = h
            .state()
            .model
            .command()
            .is_some_and(|c| c.target.as_deref() == Some("new_game:tutorial"));
        if !chosen && !h.state().model.busy() && h.query_by_label("Tutorial").is_some() {
            h.get_by_label("Tutorial").click();
        }
        h.state().model.size().x > 0
    });
    // Tutorial pop-ups close with their only choice.
    let mut quiet = 0;
    while quiet < 10 {
        let popup = h.state().model.menu();
        match popup.and_then(|m| m.choices.first().map(|c| c.id.clone())) {
            Some(id) if !h.state().model.busy() => {
                h.state_mut().model.choose(&id);
                quiet = 0;
            }
            Some(_) => quiet = 0,
            None => quiet += 1,
        }
        h.run_steps(1);
        std::thread::sleep(Duration::from_millis(100));
    }

    assert!(h.state_mut().model.invoke_action("craft"));
    run_until(&mut h, "the crafting menu", 60, |h| crafting(h).is_some());
    let menu = crafting(&h).unwrap();
    let tabs = menu.tabs();
    assert!(tabs.len() > 1, "category tabs are listed");
    let other = menu
        .choices
        .iter()
        .filter(|c| c.id.starts_with("tab:") && !c.selected)
        .map(|c| (c.id.clone(), c.label.clone()))
        .collect::<Vec<_>>();
    let mut found = false;
    for (id, label) in other {
        run_until(&mut h, "idle", 30, |h| !h.state().model.busy());
        h.get_by_label(&label).click();
        run_until(&mut h, "the tab to change", 30, |h| {
            crafting(h).is_some_and(|m| m.choices.iter().any(|c| c.id == id && c.selected))
        });
        run_until(&mut h, "idle", 30, |h| !h.state().model.busy());
        if !recipes(&h).is_empty() {
            found = true;
            break;
        }
    }
    assert!(found, "some tab lists recipes");

    // The selected recipe's native detail text is on screen.
    let first = recipes(&h).remove(0);
    run_until(&mut h, "the detail text", 30, |h| {
        recipes(h)
            .first()
            .is_some_and(|r| !r.description.is_empty())
    });
    let detail = recipes(&h).remove(0).description;
    assert!(detail.contains("Time to complete"), "{detail}");
    h.get_by_label(&detail);

    // Searching narrows the list by a word of the first recipe.
    let word = first.label.split(' ').next().unwrap().to_owned();
    h.get_by_label("Search").click();
    h.get_by_label("Search").type_text(&word);
    h.run_steps(2);
    h.key_press(egui::Key::Enter);
    run_until(&mut h, "the search result", 30, |h| {
        crafting(h).is_some_and(|m| m.field.is_some_and(|f| f.value == word))
    });
    assert!(recipes(&h).iter().any(|r| r.label.contains(&word)));

    // Closing returns to the map.
    run_until(&mut h, "idle", 30, |h| !h.state().model.busy());
    h.key_press(egui::Key::Escape);
    run_until(&mut h, "the crafting menu to close", 30, |h| {
        h.state().model.menu().is_none()
    });
}
