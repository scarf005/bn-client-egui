//! Writes PNGs of the windows and the map zoom to `$SHOT_DIR` for a person to look at.
//! Needs a Vulkan adapter, so it only runs when asked: `cargo test --test ui_shots_u1 -- --ignored`.
mod common;
use bn_client_egui::transport::{ScriptHandle, ScriptedTransport};
use bn_client_egui::{App, EngineModel};
use common::{fixture, subscribe_reply};
use eframe::egui;
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use serde_json::{Value, json};

type H<'a> = Harness<'a, App<EngineModel>>;

fn harness<'a>(settings: &std::path::Path) -> (H<'a>, ScriptHandle) {
    let mut reply = subscribe_reply();
    reply[0]["result"]["interaction"]["actions"]
        .as_array_mut()
        .unwrap()
        .extend([
            json!({"id":"zoom_in","name":"Zoom in","keys":["z"]}),
            json!({"id":"zoom_out","name":"Zoom out","keys":["Z"]}),
            json!({"id":"SCROLL_UP","name":"Scroll up","keys":["PPAGE"]}),
            json!({"id":"SCROLL_DOWN","name":"Scroll down","keys":["NPAGE"]}),
        ]);
    let (t, script) = ScriptedTransport::new();
    let t = t
        .on("bn.hello", vec![fixture("hello_result")])
        .on("bn.subscribe", reply.clone())
        .on("bn.subscribe", reply);
    let clock = bn_client_core::Clock::manual();
    let model = EngineModel::new(Box::new(t)).with_clock(clock.clone());
    let mut app = App::new(model).with_clock(clock);
    if std::env::var_os("U1_SETTINGS").is_some() {
        app = app.with_settings(settings.into());
    }
    let h = Harness::builder()
        .with_size(egui::vec2(1100.0, 700.0))
        .wgpu()
        .build_ui_state(|ui, app: &mut App<EngineModel>| app.show(ui), app);
    (h, script)
}

fn row(id: &str, label: &str, columns: Value) -> Value {
    json!({"id":id,"label":label,"enabled":true,"description":"","denial":"","selectable":true,
        "selected":false,"columns":columns})
}

fn open(h: &mut H, script: &ScriptHandle, context: &str, title: &str, rows: Vec<Value>) {
    h.run_steps(3);
    script.push(fixture("events_41_walk"));
    script.push(fixture("events_42_message"));
    let mut menu = fixture("events_43_menu");
    let change = &mut menu["params"]["events"][0]["changes"]["interaction"]["interaction"];
    change["context"] = context.into();
    change["title"] = title.into();
    let menu_actions = json!([{"id":"SCROLL_UP","name":"Scroll up","keys":["PPAGE"]},
        {"id":"SCROLL_DOWN","name":"Scroll down","keys":["NPAGE"]}]);
    menu["params"]["events"][0]["changes"]["interaction"]["actions"] = menu_actions;
    let change = &mut menu["params"]["events"][0]["changes"]["interaction"]["interaction"];
    change["choice_total"] = rows.len().into();
    change["choices"] = rows.into();
    script.push(menu);
    h.run_steps(5);
}

fn save(h: &mut H, name: &str) {
    let dir = std::path::PathBuf::from(std::env::var("SHOT_DIR").expect("SHOT_DIR"));
    std::fs::create_dir_all(&dir).unwrap();
    h.render().expect("renders").save(dir.join(name)).unwrap();
}

fn debug_rows() -> Vec<Value> {
    (0..14)
        .map(|i| row(&format!("e:{i}"), &format!("Debug entry {i}"), json!([])))
        .collect()
}

fn click(h: &mut H, at: egui::Pos2) {
    h.hover_at(at);
    h.run_steps(2);
    for pressed in [true, false] {
        h.event(egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        });
        h.run_steps(1);
    }
    h.run_steps(3);
}

fn drag(h: &mut H, from: egui::Pos2, to: egui::Pos2) {
    h.hover_at(from);
    h.run_steps(2);
    let button = |pos, pressed| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    h.event(button(from, true));
    h.run_steps(1);
    h.hover_at((from + to.to_vec2()) / 2.0);
    h.run_steps(1);
    h.hover_at(to);
    h.run_steps(2);
    h.event(button(to, false));
    h.run_steps(3);
}

fn rect(h: &H, role: egui::accesskit::Role, label: &str) -> egui::Rect {
    let found = h
        .query_all_by_label(label)
        .find(|n| n.accesskit_node().role() == role && n.accesskit_node().bounding_box().is_some());
    if found.is_none() {
        eprintln!("SHOT: no {role:?} labelled {label}");
    }
    found.map_or(egui::Rect::NOTHING, |n| n.rect())
}

#[test]
#[ignore = "needs a Vulkan adapter and SHOT_DIR"]
fn windows() {
    let file = std::env::temp_dir().join(format!("u1-shots-{}.json", std::process::id()));
    let (mut h, script) = harness(&file);
    open(&mut h, &script, "UILIST", "Debug", debug_rows());
    save(&mut h, "menu.png");
    // The Cancel button while a wheel step is in flight: no reply to the engine's action ever comes.
    let (mut w, script) = harness(&file);
    let long = (0..40)
        .map(|i| row(&format!("e:{i}"), &format!("Debug entry {i}"), json!([])))
        .collect();
    open(&mut w, &script, "UILIST", "Debug", long);
    w.hover_at(egui::pos2(500.0, 300.0));
    w.event(egui::Event::MouseWheel {
        unit: egui::MouseWheelUnit::Line,
        delta: egui::vec2(0.0, -1.0),
        modifiers: egui::Modifiers::NONE,
        phase: egui::TouchPhase::Move,
    });
    w.run_steps(2);
    save(&mut w, "cancel-during-wheel.png");

    let inventory = |i: usize, source: &str, category: &str, name: &str| {
        row(
            &format!("i:{i}"),
            name,
            json!([
                {"key":"source","label":"","value":source},
                {"key":"category","label":"","value":category},
                {"key":"name","label":"Name","value":name},
                {"key":"weight","label":"Weight","value":"1.2"},
            ]),
        )
    };
    let rows = vec![
        inventory(0, "inventory", "TOOLS", "hammer"),
        inventory(1, "inventory", "TOOLS", "flashlight"),
        inventory(2, "inventory", "FOOD", "water bottle"),
        inventory(3, "map_or_vehicle", "FOOD", "canned beans"),
        inventory(4, "worn_or_wielded", "ARMOR", "jeans"),
    ];
    let (mut i, script) = harness(&file);
    open(&mut i, &script, "INVENTORY", "Inventory", rows);
    save(&mut i, "inventory.png");

    // Collapsed from the title bar's arrow.
    let (mut c, script) = harness(&file);
    open(&mut c, &script, "UILIST", "Debug", debug_rows());
    let dialog = rect(&c, egui::accesskit::Role::Dialog, "Debug");
    let bar = rect(&c, egui::accesskit::Role::TitleBar, "Debug");
    click(&mut c, egui::pos2(dialog.left() + 14.0, bar.center().y));
    save(&mut c, "collapsed.png");

    // Dragged by its title and resized from its bottom edge.
    let (mut d, script) = harness(&file);
    open(&mut d, &script, "UILIST", "Debug", debug_rows());
    let bar = rect(&d, egui::accesskit::Role::TitleBar, "Debug");
    drag(
        &mut d,
        bar.center(),
        bar.center() + egui::vec2(-120.0, 70.0),
    );
    let dialog = rect(&d, egui::accesskit::Role::Dialog, "Debug");
    let edge = egui::pos2(dialog.center().x, dialog.bottom() + 2.0);
    drag(&mut d, edge, edge + egui::vec2(0.0, -140.0));
    save(&mut d, "dragged-and-resized.png");

    // Narrowed from its left edge.
    let narrow = std::env::temp_dir().join(format!("u1-shots-narrow-{}.json", std::process::id()));
    let (mut n, script) = harness(&narrow);
    open(&mut n, &script, "UILIST", "Debug", debug_rows());
    let dialog = rect(&n, egui::accesskit::Role::Dialog, "Debug");
    let edge = egui::pos2(dialog.left() - 2.0, dialog.center().y);
    drag(&mut n, edge, edge + egui::vec2(250.0, 0.0));
    save(&mut n, "menu-narrowed.png");
}

#[test]
#[ignore = "needs a Vulkan adapter and SHOT_DIR"]
fn zoom() {
    let file = std::env::temp_dir().join(format!("u1-zoom-{}.json", std::process::id()));
    let (mut h, _script) = harness(&file);
    h.run_steps(4);
    let step = |h: &mut H, key: &str, name: &str| {
        for _ in 0..2 {
            h.event(egui::Event::Text(key.into()));
            h.run_steps(3);
        }
        std::thread::sleep(std::time::Duration::from_millis(300));
        h.state().clock().advance(std::time::Duration::from_secs(1));
        h.run_steps(10);
        save(h, name);
    };
    save(&mut h, "zoom-0.png");
    step(&mut h, "z", "zoom-plus2.png");
    step(&mut h, "Z", "zoom-0-again.png");
    step(&mut h, "Z", "zoom-minus2.png");
}
