//! Needs a real engine build: `BN_ENGINE=/path/to/cataclysm-bn-tiles cargo test --test live_engine -- --ignored`
mod common;
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
    common::start_tutorial(&mut model);
    wait_for(&mut model, "the tutorial map", |m| m.size().x > 0);

    // The view follows the avatar, so only the absolute square tells that it moved.
    let square = |m: &EngineModel| (m.player().x + m.origin().x, m.player().y + m.origin().y);
    let start = square(&model);
    assert!(common::step(&mut model, 1, 0), "movement was submitted");
    wait_for(&mut model, "one step east", |m| square(m) != start);
    assert_eq!(square(&model), (start.0 + 1, start.1));
}

#[test]
#[ignore = "requires BN_ENGINE pointing at an engine binary"]
fn live_engine_non_movement_action_opens_a_menu_that_can_be_cancelled() {
    let engine = std::env::var_os("BN_ENGINE").expect("set BN_ENGINE");
    let transport = StdioTransport::spawn_fresh(engine.as_ref()).expect("engine starts");
    let mut model = EngineModel::new(Box::new(transport));

    wait_for(&mut model, "subscription", |m| m.live());
    common::start_tutorial(&mut model);
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
    let inventory_open = |m: &EngineModel| {
        m.screens()
            .last()
            .is_some_and(|s| s.context.as_deref() == Some("INVENTORY"))
    };
    wait_for(&mut model, "the inventory screen", inventory_open);
    wait_for(&mut model, "the command to finish", |m| !m.busy());
    model
        .invoke_affordance("QUIT", Default::default())
        .expect("the inventory offers its cancel");
    wait_for(&mut model, "the inventory to close", |m| !inventory_open(m));
}

#[test]
#[ignore = "requires BN_ENGINE built with bn.loading support"]
fn live_engine_shows_a_loading_screen_while_the_tutorial_loads() {
    let engine = std::env::var_os("BN_ENGINE").expect("set BN_ENGINE");
    let transport = StdioTransport::spawn_fresh(engine.as_ref()).expect("engine starts");
    let mut model = EngineModel::new(Box::new(transport));
    wait_for(&mut model, "subscription", |m| m.live());
    common::main_menu_tab(&mut model, "new_game");
    let mut seen = vec![];
    // The command finishes only after loading.
    common::submit_main_menu(&mut model, "menu:entry", "new_game:tutorial");
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
    common::start_tutorial(&mut model);
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
            let id = model.cell(p).id;
            !id.is_empty() && !id.contains("wall")
        })
        .collect::<Vec<_>>();
    candidates.sort_by_key(|&p| std::cmp::Reverse(reach(p)));
    assert!(!candidates.is_empty(), "no known square in view");
    // Prefer a square the straight walk cannot reach: its route goes around walls or through a door.
    let mut best: Option<(Pos, i32)> = None;
    for at in candidates.into_iter().take(60) {
        wait_for(&mut model, "idle", |m| !m.busy());
        model.travel(at);
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
        model.travel(at);
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
    common::start_tutorial(&mut model);
    wait_for(&mut model, "the tutorial map", |m| m.size().x > 0);
    wait_for(&mut model, "idle", |m| !m.busy());
    dismiss_popups(&mut model);

    let step = |model: &mut EngineModel, dy: i32| {
        wait_for(model, "idle", |m| !m.busy());
        common::step(model, 0, dy);
        wait_for(model, "the step to finish", |m| !m.busy());
        dismiss_popups(model);
    };
    let racks = |model: &EngineModel| {
        let (w, h) = (model.size().x, model.size().y);
        (0..h)
            .flat_map(|y| (0..w).map(move |x| Pos { x, y }))
            .filter(|&p| model.cell(p).id == "f_rack")
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
    common::start_tutorial(&mut model);
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
    model.travel(Pos { x: 59, y: me.y });
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
    common::main_menu_tab(&mut model, "new_game");
    common::main_menu_entry(&mut model, "new_game:play_now");
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
        common::main_menu_tabs(m).contains(&"load".into())
    });
    drop(model);

    let mut model = launch();
    wait_for(&mut model, "subscription", |m| m.live());
    common::main_menu_tab(&mut model, "load");
    wait_for(&mut model, "the saved world", |m| {
        common::main_menu_entries(m)
            .iter()
            .any(|id| id.starts_with("load:"))
    });
    println!("load tab: {:?}", common::main_menu_entries(&model));
    let id = common::main_menu_entries(&model)
        .into_iter()
        .find(|id| id.starts_with("load:"))
        .unwrap();
    common::main_menu_entry(&mut model, &id);
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

#[test]
#[ignore = "requires BN_ENGINE built with bn.interrupt"]
fn live_engine_interrupts_a_long_wait_and_the_stop_question_is_a_menu() {
    let engine = std::env::var_os("BN_ENGINE").expect("set BN_ENGINE");
    let transport = StdioTransport::spawn_fresh(engine.as_ref()).expect("engine starts");
    let mut model = EngineModel::new(Box::new(transport));
    wait_for(&mut model, "subscription", |m| m.live());
    common::start_tutorial(&mut model);
    wait_for(&mut model, "the tutorial map", |m| m.size().x > 0);
    wait_for(&mut model, "idle", |m| !m.busy());
    dismiss_popups(&mut model);

    // A short wait first lets the tutorial's lessons pop up and be closed.
    assert!(model.invoke_action("wait"));
    choose_label(&mut model, "5 minutes");
    dismiss_popups(&mut model);

    assert!(model.invoke_action("wait"));
    wait_for(&mut model, "the wait menu", |m| m.menu().is_some());
    wait_for(&mut model, "the command to finish", |m| !m.busy());
    let id = model
        .menu()
        .unwrap()
        .choices
        .iter()
        .find(|c| c.label.starts_with("6 hours"))
        .expect("6 hours")
        .id
        .clone();
    model.choose(&id);
    assert!(model.busy());
    std::thread::sleep(Duration::from_secs(1));
    assert!(model.interrupt(), "a command is running");
    wait_for(&mut model, "the stop question", |m| {
        m.menu()
            .is_some_and(|menu| menu.choices.iter().any(|c| c.label == "Yes"))
    });
    wait_for(&mut model, "the command to finish", |m| !m.busy());
    choose_label(&mut model, "Yes");
    wait_for(&mut model, "the menu to close", |m| m.menu().is_none());
    assert!(
        !model.messages().iter().any(|l| l.contains("Engine error")),
        "{:?}",
        model.messages()
    );
}

#[test]
#[ignore = "requires BN_ENGINE that reports the avatar sidebar"]
fn live_engine_reports_the_native_sidebar_of_the_tutorial_avatar() {
    let engine = std::env::var_os("BN_ENGINE").expect("set BN_ENGINE");
    let transport = StdioTransport::spawn_fresh(engine.as_ref()).expect("engine starts");
    let mut model = EngineModel::new(Box::new(transport));
    wait_for(&mut model, "subscription", |m| m.live());
    common::start_tutorial(&mut model);
    wait_for(&mut model, "the sidebar", |m| !m.sidebar().is_empty());
    let rows: Vec<String> = model
        .sidebar()
        .iter()
        .map(|l| {
            l.iter()
                .map(|s| s.text.as_str())
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect();
    println!("{}", rows.join("\n"));
    assert!(rows.iter().any(|r| r.starts_with("HEAD")), "{rows:?}");
    assert!(rows.iter().any(|r| r.starts_with("Location:")), "{rows:?}");
    assert!(rows.iter().any(|r| r.starts_with("Weapon")), "{rows:?}");
}

#[test]
#[ignore = "requires BN_ENGINE that reports the sidebar layout and the compass"]
fn live_engine_reports_the_sidebar_layout_and_the_compass_of_the_tutorial_avatar() {
    let engine = std::env::var_os("BN_ENGINE").expect("set BN_ENGINE");
    let transport = StdioTransport::spawn_fresh(engine.as_ref()).expect("engine starts");
    let mut model = EngineModel::new(Box::new(transport));
    wait_for(&mut model, "subscription", |m| m.live());
    common::start_tutorial(&mut model);
    wait_for(&mut model, "the sidebar layout", |m| {
        m.sidebar_layout().is_some()
    });
    let layout = model.sidebar_layout().expect("layout");
    for panel in &layout.panels {
        println!("{} {:?} {:?}", panel.label, panel.height, panel.body);
    }
    println!("side {:?} width {}", layout.side, layout.width);
    assert!(layout.width > 0 && !layout.panels.is_empty(), "{layout:?}");
}

#[test]
#[ignore = "requires BN_ENGINE pointing at an engine binary"]
fn live_engine_killed_mid_game_shows_the_failure_and_restart_brings_the_menu_back() {
    let engine = std::path::PathBuf::from(std::env::var_os("BN_ENGINE").expect("set BN_ENGINE"));
    let transport = StdioTransport::spawn_fresh(&engine).expect("engine starts");
    let pid = transport.pid();
    let again = engine.clone();
    let mut model = EngineModel::new(Box::new(transport)).with_respawn(Box::new(move || {
        StdioTransport::spawn_fresh(&again)
            .map(|t| Box::new(t) as Box<dyn bn_client_core::transport::Transport>)
    }));
    wait_for(&mut model, "subscription", |m| m.live());
    common::start_tutorial(&mut model);
    wait_for(&mut model, "the tutorial map", |m| m.size().x > 0);
    assert!(model.failure().is_none());

    let killed = std::process::Command::new("kill")
        .args(["-9", &pid.to_string()])
        .status()
        .expect("kill runs");
    assert!(killed.success());
    wait_for(&mut model, "the failure", |m| m.failure().is_some());
    let failure = model.failure().unwrap();
    assert_eq!(failure.kind, bn_client_core::FailureKind::Exited);
    println!("failure: {failure:?}");

    assert!(model.restart(), "the engine starts again");
    wait_for(&mut model, "the new session", |m| m.live());
    assert!(model.failure().is_none());
    wait_for(&mut model, "the main menu", |m| {
        common::main_menu_tabs(m).contains(&"new_game".into())
    });
}

/// A noise interrupts a wait with "Stop waiting?"; answering No lets it go on. True when it was asked.
fn keep_waiting(model: &mut EngineModel) -> bool {
    let no = model
        .menu()
        .filter(|m| m.message.contains("Stop waiting"))
        .and_then(|m| m.choices.into_iter().find(|c| c.label == "No"));
    no.is_some_and(|choice| {
        model.choose(&choice.id);
        true
    })
}

#[test]
#[ignore = "requires BN_ENGINE with bn.progress heartbeats"]
fn live_engine_long_wait_is_never_reported_as_stalled() {
    let engine = std::env::var_os("BN_ENGINE").expect("set BN_ENGINE");
    let transport = StdioTransport::spawn_fresh(engine.as_ref()).expect("engine starts");
    let mut model = EngineModel::new(Box::new(transport));
    wait_for(&mut model, "subscription", |m| m.live());
    common::start_tutorial(&mut model);
    wait_for(&mut model, "the tutorial map", |m| m.size().x > 0);
    wait_for(&mut model, "idle", |m| !m.busy());
    dismiss_popups(&mut model);
    assert!(model.invoke_action("wait"));
    choose_label(&mut model, "5 minutes");
    dismiss_popups(&mut model);

    assert!(model.invoke_action("wait"));
    wait_for(&mut model, "the wait menu", |m| m.menu().is_some());
    wait_for(&mut model, "the command to finish", |m| !m.busy());
    // An unloaded engine plays six hours in about a second, so ask for 200 days, the most the engine's
    // int move count holds: the wait has to outlast the stall limit whatever the machine.
    let id = model
        .menu()
        .unwrap()
        .choices
        .iter()
        .find(|c| c.label == "Custom input")
        .expect("Custom input")
        .id
        .clone();
    model.choose(&id);
    wait_for(&mut model, "the duration prompt", |m| {
        m.menu().is_some_and(|menu| menu.field.is_some())
    });
    wait_for(&mut model, "the command to finish", |m| !m.busy());
    let field = model.menu().unwrap().field.unwrap().id;
    // A stall limit far below the wait: only the heartbeats keep the model from reporting one.
    model.set_stall(Duration::from_secs(5));
    model.fill(&field, "200d", true);
    assert!(model.busy());
    let started = std::time::Instant::now();
    while started.elapsed() < Duration::from_secs(20) && (model.busy() || keep_waiting(&mut model))
    {
        model.poll();
        assert!(
            model.failure().is_none(),
            "{:?} after {:?}",
            model.failure(),
            started.elapsed()
        );
        std::thread::sleep(Duration::from_millis(100));
    }
    // The tutorial ends a long wait with a lesson pop-up when the day turns dark, so the wait may
    // end by itself; it must at least outlast the stall limit.
    assert!(
        started.elapsed() > Duration::from_secs(5),
        "the wait ended after {:?}: {:?} menu {:?} messages {:?}",
        started.elapsed(),
        model.command(),
        model.menu().map(|m| (m.title, m.message)),
        model.messages()
    );
    if model.busy() {
        assert!(model.interrupt());
        wait_for(&mut model, "the stop question", |m| {
            m.menu()
                .is_some_and(|menu| menu.choices.iter().any(|c| c.label == "Yes"))
        });
        wait_for(&mut model, "the command to finish", |m| !m.busy());
        model.set_stall(Duration::from_secs(120));
        choose_label(&mut model, "Yes");
    }
    model.set_stall(Duration::from_secs(120));
    dismiss_popups(&mut model);
    assert!(model.failure().is_none());
}

#[test]
#[ignore = "requires BN_ENGINE with the structured overmap"]
fn live_engine_opens_the_overmap_moves_its_cursor_and_closes_it() {
    let engine = std::env::var_os("BN_ENGINE").expect("set BN_ENGINE");
    let transport = StdioTransport::spawn_fresh(engine.as_ref()).expect("engine starts");
    let mut model = EngineModel::connect(Box::new(transport), true);
    wait_for(&mut model, "subscription", |m| m.live());
    common::start_tutorial(&mut model);
    wait_for(&mut model, "the tutorial map", |m| m.size().x > 0);
    wait_for(&mut model, "idle", |m| !m.busy());
    dismiss_popups(&mut model);

    assert!(model.invoke_action("map"), "the overmap action is offered");
    wait_for(&mut model, "the overmap", |m| m.overmap().is_some());
    wait_for(&mut model, "idle", |m| !m.busy());
    let first = model.overmap().unwrap();
    println!(
        "overmap {}x{} cursor {:?} legend {:?}",
        first.cols,
        first.rows,
        first.cursor,
        first.legend.iter().take(3).collect::<Vec<_>>()
    );
    assert!(first.cols > 10 && first.rows > 5);
    assert_eq!(first.cursor, first.player, "it opens on the player");
    assert!(first.legend.iter().any(|l| !l.trim().is_empty()));
    // The client said it draws tiles: every square names a tile, the one under the player a known one.
    assert_eq!(first.tiles.len(), (first.cols * first.rows) as usize);
    let under = first.tile(first.cols / 2, first.rows / 2).unwrap();
    println!("tile under the player: {under:?}");
    assert!(!under.id.is_empty() && under.id != "unknown_terrain");

    let to = (first.cursor.0 + 3, first.cursor.1 - 2, first.cursor.2);
    model.overmap_cursor_to(to);
    wait_for(&mut model, "the cursor to move", |m| {
        m.overmap().is_some_and(|o| o.cursor == to)
    });
    wait_for(&mut model, "idle", |m| !m.busy());
    let moved = model.overmap().unwrap();
    assert_eq!(
        moved.origin.0,
        first.origin.0 + 3,
        "the view follows the cursor"
    );
    assert_eq!(moved.player, first.player);

    model.close_overmap();
    wait_for(&mut model, "the overmap to close", |m| {
        m.overmap().is_none()
    });
    wait_for(&mut model, "idle", |m| !m.busy());
    assert!(model.failure().is_none());
}

#[test]
#[ignore = "requires BN_ENGINE with the structured overmap"]
fn live_engine_overmap_gains_and_loses_tile_ids_when_the_client_switches() {
    let engine = std::env::var_os("BN_ENGINE").expect("set BN_ENGINE");
    let transport = StdioTransport::spawn_fresh(engine.as_ref()).expect("engine starts");
    let mut model = EngineModel::connect(Box::new(transport), false);
    wait_for(&mut model, "subscription", |m| m.live());
    common::start_tutorial(&mut model);
    wait_for(&mut model, "the tutorial map", |m| m.size().x > 0);
    wait_for(&mut model, "idle", |m| !m.busy());
    dismiss_popups(&mut model);

    assert!(model.invoke_action("map"));
    wait_for(&mut model, "the overmap", |m| m.overmap().is_some());
    wait_for(&mut model, "idle", |m| !m.busy());
    assert!(
        model.overmap().unwrap().tiles.is_empty(),
        "ASCII client: no tile ids"
    );

    // The player picks Tiles while the overmap is open: the open screen is published again with ids.
    model.set_tiles(true);
    wait_for(&mut model, "tile ids", |m| {
        m.overmap().is_some_and(|o| !o.tiles.is_empty())
    });
    let tiled = model.overmap().unwrap();
    assert_eq!(tiled.tiles.len(), (tiled.cols * tiled.rows) as usize);

    model.set_tiles(false);
    wait_for(&mut model, "no tile ids", |m| {
        m.overmap().is_some_and(|o| o.tiles.is_empty())
    });
    model.close_overmap();
    wait_for(&mut model, "the overmap to close", |m| {
        m.overmap().is_none()
    });
    assert!(model.failure().is_none());
}

#[test]
#[ignore = "requires BN_ENGINE pointing at an engine binary"]
fn live_engine_says_hello_with_0_2_and_shows_the_main_menu() {
    let engine = std::env::var_os("BN_ENGINE").expect("set BN_ENGINE");
    let transport = StdioTransport::spawn_fresh(engine.as_ref()).expect("engine starts");
    let mut model = EngineModel::new(Box::new(transport));

    wait_for(&mut model, "subscription", |m| m.live());
    wait_for(&mut model, "the main menu", |m| {
        !common::main_menu_tabs(m).is_empty()
    });
    assert!(model.failure().is_none(), "{:?}", model.failure());
    println!(
        "tabs: {:?}; screens: {:?}",
        common::main_menu_tabs(&model),
        model
            .screens()
            .iter()
            .map(|s| (s.role, s.context.clone()))
            .collect::<Vec<_>>()
    );
    assert!(common::main_menu_tabs(&model).contains(&"new_game".into()));
}

#[test]
#[ignore = "requires BN_ENGINE pointing at an engine binary"]
fn live_engine_debug_menu_filter_matches_tele_in_any_case() {
    let engine = std::env::var_os("BN_ENGINE").expect("set BN_ENGINE");
    let transport = StdioTransport::spawn_fresh(engine.as_ref()).expect("engine starts");
    let mut model = EngineModel::new(Box::new(transport));
    wait_for(&mut model, "subscription", |m| m.live());
    common::start_tutorial(&mut model);
    wait_for(&mut model, "the tutorial map", |m| m.size().x > 0);
    wait_for(&mut model, "idle", |m| !m.busy());
    dismiss_popups(&mut model);
    assert!(model.invoke_action("debug"), "the debug action is offered");
    wait_for(&mut model, "the debug menu", |m| m.menu().is_some());
    wait_for(&mut model, "idle", |m| !m.busy());
    for text in ["tele", "Tele", "TELE"] {
        assert!(model.invoke_action("FILTER"), "the list offers its filter");
        wait_for(&mut model, "the filter prompt", |m| {
            !m.busy() && m.menu().is_some_and(|menu| menu.field.is_some())
        });
        let field = model.menu().unwrap().field.clone().unwrap();
        model.fill(&field.id, text, true);
        wait_for(&mut model, "the filtered list", |m| {
            !m.busy() && m.menu().is_some_and(|menu| menu.field.is_none())
        });
        let menu = model.menu().unwrap();
        let labels = menu
            .choices
            .iter()
            .map(|c| c.label.as_str())
            .collect::<Vec<_>>();
        assert!(
            labels.iter().all(|l| l.contains("Teleport")),
            "{text}: {labels:?}"
        );
        assert!(!labels.is_empty(), "{text}: nothing matched");
    }
}
