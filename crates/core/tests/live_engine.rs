//! Needs a real engine build: `BN_ENGINE=/path/to/cataclysm-bn-tiles cargo test --test live_engine -- --ignored`
use bn_client_core::keys::KeyMap;
use bn_client_core::transport::{Profile, StdioTransport};
use bn_client_core::watchdog::Watchdog;
use bn_client_core::{EngineModel, GameModel, Pos};
use std::time::Duration;

fn wait_for(model: &mut EngineModel, what: &str, mut done: impl FnMut(&EngineModel) -> bool) {
    let mut watchdog = Watchdog::live();
    while !done(model) {
        assert!(
            !watchdog.expired(model.received()),
            "timed out waiting for {what}: {:?}",
            model.messages()
        );
        model.poll();
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn choose_label(model: &mut EngineModel, label: &str) {
    wait_for(model, label, |m| {
        m.menu()
            .is_some_and(|menu| menu.choices.iter().any(|c| c.label.contains(label)))
    });
    let menu = model.menu().expect("menu");
    let id = menu
        .choices
        .iter()
        .find(|c| c.label.contains(label))
        .expect("choice")
        .id
        .clone();
    // Input is ignored while a command is in flight, so wait for the previous one to finish.
    wait_for(model, "the previous command", |m| !m.busy());
    model.choose(&id);
    assert!(model.busy(), "{label} was submitted");
    wait_for(model, &format!("{label} to finish"), |m| !m.busy());
}

/// The tutorial greets and teaches with pop-ups; they are menus too and each closes with its only choice.
fn dismiss_popups(model: &mut EngineModel) {
    while let Some(menu) = model.menu() {
        let id = menu.choices.first().map(|c| c.id.clone());
        match id {
            Some(id) if !menu.can_cancel => model.choose(&id),
            _ => model.cancel_menu(),
        }
        wait_for(model, "popup closing", |m| !m.busy());
        std::thread::sleep(Duration::from_millis(500));
        model.poll();
    }
}

#[test]
#[ignore = "requires BN_ENGINE pointing at an engine binary"]
fn live_engine_starts_the_tutorial_and_walks_one_step() {
    let engine = std::env::var_os("BN_ENGINE").expect("set BN_ENGINE");
    let transport = StdioTransport::spawn_fresh(engine.as_ref()).expect("engine starts");
    let mut model = EngineModel::new(Box::new(transport));

    wait_for(&mut model, "subscription", |m| m.live());
    choose_label(&mut model, "New Game");
    choose_label(&mut model, "Tutorial");
    wait_for(&mut model, "the tutorial map", |m| m.size().x > 0);

    // The view follows the avatar, so only the absolute square tells that it moved.
    let square = |m: &EngineModel| (m.player().x + m.origin().x, m.player().y + m.origin().y);
    let start = square(&model);
    let me = model.player();
    assert!(
        model.step_toward(Pos {
            x: me.x + 1,
            y: me.y
        }),
        "movement was submitted"
    );
    wait_for(&mut model, "one step east", |m| square(m) != start);
    assert_eq!(square(&model), (start.0 + 1, start.1));
}

#[test]
#[ignore = "requires BN_ENGINE pointing at an engine binary"]
fn live_engine_non_movement_action_opens_a_menu_that_can_be_cancelled() {
    let engine = std::env::var_os("BN_ENGINE").expect("set BN_ENGINE");
    let transport = StdioTransport::spawn_fresh(engine.as_ref()).expect("engine starts");
    let root = bn_client_core::transport::game_root(engine.as_ref(), None);
    let keys = root.as_deref().map_or_else(KeyMap::default, KeyMap::load);
    let mut model = EngineModel::new(Box::new(transport)).with_keys(keys);

    wait_for(&mut model, "subscription", |m| m.live());
    choose_label(&mut model, "New Game");
    choose_label(&mut model, "Tutorial");
    wait_for(&mut model, "the tutorial map", |m| m.size().x > 0);
    wait_for(&mut model, "idle", |m| !m.busy());
    dismiss_popups(&mut model);

    let actions = model.actions();
    let inventory = actions
        .iter()
        .find(|a| a.id == "inventory")
        .expect("inventory action offered");
    assert_eq!(inventory.keys, ["i"], "key comes from keybindings.json");
    assert!(model.press("i"), "the game key runs the action");
    wait_for(&mut model, "the inventory menu", |m| m.menu().is_some());
    let menu = model.menu().unwrap();
    println!(
        "inventory menu: {:?} with {} choices",
        menu.title,
        menu.choices.len()
    );
    wait_for(&mut model, "the command to finish", |m| !m.busy());
    model.cancel_menu();
    wait_for(&mut model, "the menu to close", |m| m.menu().is_none());
}

#[test]
#[ignore = "requires BN_ENGINE built with bn.loading support"]
fn live_engine_shows_a_loading_screen_while_the_tutorial_loads() {
    let engine = std::env::var_os("BN_ENGINE").expect("set BN_ENGINE");
    let transport = StdioTransport::spawn_fresh(engine.as_ref()).expect("engine starts");
    let mut model = EngineModel::new(Box::new(transport));
    wait_for(&mut model, "subscription", |m| m.live());
    choose_label(&mut model, "New Game");
    let mut seen = vec![];
    // Tutorial is chosen inside this loop's first poll; the command finishes only after loading.
    wait_for(&mut model, "the world menu", |m| m.menu().is_some());
    let id = model
        .menu()
        .unwrap()
        .choices
        .iter()
        .find(|c| c.label.contains("Tutorial"))
        .expect("Tutorial choice")
        .id
        .clone();
    wait_for(&mut model, "idle", |m| !m.busy());
    model.choose(&id);
    wait_for(&mut model, "the tutorial map", |m| {
        if let Some(l) = m.loading().filter(|l| seen.last() != Some(l)) {
            seen.push(l);
        }
        m.size().x > 0
    });
    assert!(!seen.is_empty(), "a loading screen appeared");
    assert!(seen.iter().all(|l| !l.title.is_empty()));
    assert!(
        model.loading().is_none(),
        "and went away once the map showed"
    );
    println!("{} loading states, last: {:?}", seen.len(), seen.last());
}

#[test]
#[ignore = "requires BN_ENGINE built with the travel operation"]
fn live_engine_travel_click_plans_a_route_and_a_second_click_walks_it() {
    let engine = std::env::var_os("BN_ENGINE").expect("set BN_ENGINE");
    let transport = StdioTransport::spawn_fresh(engine.as_ref()).expect("engine starts");
    let mut model = EngineModel::new(Box::new(transport));

    wait_for(&mut model, "subscription", |m| m.live());
    choose_label(&mut model, "New Game");
    choose_label(&mut model, "Tutorial");
    wait_for(&mut model, "the tutorial map", |m| m.size().x > 0);
    wait_for(&mut model, "idle", |m| !m.busy());
    dismiss_popups(&mut model);

    let start = model.player();
    assert!(model.route().is_empty(), "nothing is planned yet");
    // Known non-wall squares around; the engine answers a click on a reachable one with a route.
    let (w, h) = (model.size().x, model.size().y);
    let reach = |p: Pos| (p.x - start.x).abs().max((p.y - start.y).abs());
    let mut candidates = (0..h)
        .flat_map(|y| (0..w).map(move |x| Pos { x, y }))
        .filter(|&p| (2..=12).contains(&reach(p)))
        .filter(|&p| {
            let desc = model.cell(p).desc;
            desc != "unknown" && !desc.contains("wall")
        })
        .collect::<Vec<_>>();
    candidates.sort_by_key(|&p| std::cmp::Reverse(reach(p)));
    assert!(!candidates.is_empty(), "no known square in view");
    // Prefer a square the straight walk cannot reach: its route goes around walls or through a door.
    let mut best: Option<(Pos, i32)> = None;
    for at in candidates.into_iter().take(60) {
        wait_for(&mut model, "idle", |m| !m.busy());
        assert!(model.travel(at), "the click was handled");
        wait_for(&mut model, "the click to finish", |m| !m.busy());
        if model.route().last() != Some(&at) {
            continue;
        }
        let detour = model.route().len() as i32 - reach(at);
        if best.is_none_or(|(_, d)| detour > d) {
            best = Some((at, detour));
        }
        if detour > 0 {
            break;
        }
    }
    let (target, detour) = best.expect("the engine planned a route to some square");
    println!("target {target:?}, route detour {detour} squares beyond the straight distance");
    assert_eq!(model.player(), start, "the first click only plans");
    assert!(model.route().len() >= 2, "{:?}", model.route());

    // The tutorial pops up lessons on the way, which stops a native auto-move like any interrupt;
    // dismiss them and click again until the avatar stands on the square.
    let (ox, oy) = (model.origin().x, model.origin().y);
    let goal = (target.x + ox, target.y + oy);
    let here = |m: &EngineModel| (m.player().x + m.origin().x, m.player().y + m.origin().y);
    for _ in 0..8 {
        if here(&model) == goal {
            break;
        }
        let at = Pos {
            x: goal.0 - model.origin().x,
            y: goal.1 - model.origin().y,
        };
        assert!(model.travel(at));
        wait_for(&mut model, "the click to finish", |m| !m.busy());
        dismiss_popups(&mut model);
    }
    assert_eq!(
        here(&model),
        goal,
        "the avatar stands on the clicked square"
    );
    assert!(model.route().is_empty(), "arriving clears the route");
}

#[test]
#[ignore = "requires BN_ENGINE built with remembered furniture overlays"]
fn live_engine_remembered_furniture_stays_drawn_after_walking_past_it() {
    let engine = std::env::var_os("BN_ENGINE").expect("set BN_ENGINE");
    let transport = StdioTransport::spawn_fresh(engine.as_ref()).expect("engine starts");
    let mut model = EngineModel::new(Box::new(transport));

    wait_for(&mut model, "subscription", |m| m.live());
    choose_label(&mut model, "New Game");
    choose_label(&mut model, "Tutorial");
    wait_for(&mut model, "the tutorial map", |m| m.size().x > 0);
    wait_for(&mut model, "idle", |m| !m.busy());
    dismiss_popups(&mut model);

    let step = |model: &mut EngineModel, dy: i32| {
        let me = model.player();
        wait_for(model, "idle", |m| !m.busy());
        model.step_toward(Pos {
            x: me.x,
            y: me.y + dy,
        });
        wait_for(model, "the step to finish", |m| !m.busy());
        dismiss_popups(model);
    };
    let racks = |model: &EngineModel| {
        let (w, h) = (model.size().x, model.size().y);
        (0..h)
            .flat_map(|y| (0..w).map(move |x| Pos { x, y }))
            .filter(|&p| model.cell(p).desc == "f_rack")
            .count()
    };

    // Walking south, a loaded rack in the room ahead is seen, and walking back it is only remembered.
    for _ in 0..15 {
        step(&mut model, 1);
    }
    assert_eq!(racks(&model), 0, "items on the rack show while it is seen");
    // Back up the corridor the rack drops out of sight; memory must keep drawing it as furniture.
    for _ in 0..14 {
        if racks(&model) > 0 {
            break;
        }
        step(&mut model, -1);
    }
    assert_eq!(racks(&model), 1, "the rack is drawn from map memory");
}

#[test]
#[ignore = "requires BN_ENGINE that sizes its terrain window from the client's viewport"]
fn live_engine_map_follows_the_declared_viewport_and_accepts_clicks_across_it() {
    let engine = std::env::var_os("BN_ENGINE").expect("set BN_ENGINE");
    let transport = StdioTransport::spawn_fresh(engine.as_ref()).expect("engine starts");
    let mut model = EngineModel::new(Box::new(transport));

    wait_for(&mut model, "subscription", |m| m.live());
    choose_label(&mut model, "New Game");
    choose_label(&mut model, "Tutorial");
    wait_for(&mut model, "the tutorial map", |m| m.size().x > 0);
    wait_for(&mut model, "idle", |m| !m.busy());
    dismiss_popups(&mut model);
    assert_eq!(model.size(), Pos { x: 40, y: 25 }, "hello declared 40x25");

    // A panel larger than the engine's default window: the engine grows its window to match.
    model.set_viewport(Pos { x: 60, y: 30 });
    wait_for(&mut model, "the engine window to grow", |m| {
        m.size() == Pos { x: 60, y: 30 }
    });

    // The far east column was outside the default 36x24 window; the engine now accepts a click there.
    let me = model.player();
    wait_for(&mut model, "idle", |m| !m.busy());
    assert!(model.travel(Pos { x: 59, y: me.y }));
    wait_for(&mut model, "the click to finish", |m| !m.busy());
    assert!(
        model.messages().iter().all(|m| !m.contains("rejected")),
        "{:?}",
        model.messages()
    );
}

#[test]
#[ignore = "requires BN_ENGINE; plays in a temp profile and never touches the real one"]
fn live_engine_saved_character_is_listed_on_the_load_tab_and_loads_after_relaunch() {
    let engine = std::env::var_os("BN_ENGINE").expect("set BN_ENGINE");
    let dir = std::env::temp_dir().join(format!("bn-saves-test-{}", std::process::id()));
    let profile = Profile {
        userdir: Some(dir.join("user")),
        configdir: Some(dir.join("config")),
        ..Profile::default()
    };
    for d in [&profile.userdir, &profile.configdir] {
        std::fs::create_dir_all(d.as_ref().unwrap()).unwrap();
    }
    let launch = || {
        let t = StdioTransport::spawn_with(engine.as_ref(), &profile).expect("engine starts");
        EngineModel::new(Box::new(t))
    };

    let mut model = launch();
    wait_for(&mut model, "subscription", |m| m.live());
    choose_label(&mut model, "New Game");
    choose_label(&mut model, "Play Now!");
    wait_for(&mut model, "the map", |m| m.size().x > 0);
    wait_for(&mut model, "idle", |m| !m.busy());
    dismiss_popups(&mut model);
    assert!(
        model.invoke_action("save"),
        "the native save action is offered"
    );
    wait_for(&mut model, "the save prompt", |m| m.menu().is_some());
    println!("prompt: {:?}", model.menu());
    choose_label(&mut model, "Yes");
    wait_for(&mut model, "the main menu again", |m| {
        m.menu()
            .is_some_and(|menu| menu.choices.iter().any(|c| c.label.contains("Load")))
    });
    drop(model);

    let mut model = launch();
    wait_for(&mut model, "subscription", |m| m.live());
    choose_label(&mut model, "Load");
    wait_for(&mut model, "the saved world", |m| {
        m.menu()
            .is_some_and(|menu| menu.choices.iter().any(|c| c.id.starts_with("load:")))
    });
    println!("load tab: {:?}", model.menu());
    let id = model
        .menu()
        .unwrap()
        .choices
        .iter()
        .find(|c| c.id.starts_with("load:"))
        .unwrap()
        .id
        .clone();
    wait_for(&mut model, "idle", |m| !m.busy());
    model.choose(&id);
    wait_for(&mut model, "the character list", |m| {
        m.menu()
            .is_some_and(|menu| menu.message.contains("Load character"))
    });
    println!("characters: {:?}", model.menu());
    let id = model.menu().unwrap().choices[0].id.clone();
    assert_eq!(id, "entry:0", "the saved character is the first entry");
    wait_for(&mut model, "idle", |m| !m.busy());
    model.choose(&id);
    wait_for(&mut model, "the loaded map", |m| m.size().x > 0);
    drop(model);
    std::fs::remove_dir_all(dir).unwrap();
}
