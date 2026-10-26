//! A player's walk through the map and vehicle editors against a real engine: every step is a mouse click on a
//! drawn widget or a key press, never an affordance invoked by id.
//!
//! `BN_ENGINE=<engine> SHOT_DIR=<dir> cargo test --release -p bn-client-egui --test it live_editor_walk:: --
//! --ignored --nocapture`
use crate::common::live::settle;
use crate::live_editor::{H, field, run_until, start, wait_kind};
use bn_client_egui::GameModel;
use eframe::egui;
use egui_kittest::kittest::NodeT;

/// The labels drawn now, one per line.
pub(crate) fn labels(h: &H) -> Vec<String> {
    h.root()
        .children_recursive()
        .filter_map(|n| {
            let a = n.accesskit_node();
            let l = a.label().or_else(|| a.value())?;
            (a.role() != egui::accesskit::Role::TextRun
                && a.role() != egui::accesskit::Role::Splitter
                && l.chars().count() > 1
                && !matches!(a.role(), egui::accesskit::Role::Group))
            .then(|| format!("{:?} {l}", a.role()))
        })
        .collect()
}

/// Pictures of the window at both sizes.
fn shot(h: &mut H, name: &str) {
    let Some(dir) = std::env::var_os("SHOT_DIR") else {
        return;
    };
    std::fs::create_dir_all(&dir).unwrap();
    for (w, ht) in [(1290.0, 830.0), (1920.0, 1080.0)] {
        h.set_size(egui::vec2(w, ht));
        settle(h);
        let image = h.render().expect("renders");
        image
            .save(std::path::Path::new(&dir).join(format!("{name}-{w}x{ht}.png")))
            .unwrap();
    }
    h.set_size(egui::vec2(1290.0, 830.0));
    settle(h);
}

/// The facts of the document panel as the engine sent them.
fn document(h: &H) -> String {
    let top = h.state().model.screens().last().unwrap();
    let panel = top.root.as_ref().unwrap().find("editor:document");
    let json = serde_json::to_value(panel).unwrap();
    json["stats"]
        .as_array()
        .map(|s| {
            s.iter()
                .map(|x| format!("{}={}", x["label"], x["value"]))
                .collect::<Vec<_>>()
                .join(" ")
        })
        .unwrap_or_default()
}

fn dump(h: &H, what: &str) {
    println!("--- {what}\n{}", labels(h).join("\n"));
}

/// Waits for the engine and the layout to rest.
fn rest(h: &mut H) {
    run_until(h, "idle", 120, |h| !h.state().model.busy());
    for _ in 0..20 {
        h.run_steps(1);
        std::thread::sleep(std::time::Duration::from_millis(30));
    }
    run_until(h, "idle", 120, |h| !h.state().model.busy());
}

/// Clicks the widget whose label contains `label`, as a person would, once it stopped moving.
fn click(h: &mut H, label: &str) {
    reveal(h, label);
    press(h, label);
}

/// Double-clicks the widget labelled `label`: a row of a list in a pane runs its action so.
fn double_click(h: &mut H, label: &str) {
    reveal(h, label);
    let mut seen = None;
    while seen != find(h, label) {
        seen = find(h, label);
        h.run_steps(3);
    }
    let at = find(h, label).unwrap().center();
    h.hover_at(at);
    h.run_steps(2);
    // Two clicks a frame or two apart, as a hand makes them.
    tap(h, at);
    tap(h, at);
    rest(h);
}

/// Clicks a button of the footer, which stays in view whatever the body scrolls.
fn click_footer(h: &mut H, label: &str) {
    press(h, label);
}

/// The rectangle of the widget labelled exactly `label`, else the first whose label contains it.
fn find(h: &H, label: &str) -> Option<egui::Rect> {
    let labelled = |n: &egui_kittest::Node| {
        let a = n.accesskit_node();
        a.label().or_else(|| a.value()).map(|l| l.to_string())
    };
    let nodes = h.root().children_recursive().collect::<Vec<_>>();
    let exact = nodes.iter().find(|n| labelled(n).as_deref() == Some(label));
    let part = nodes
        .iter()
        .find(|n| labelled(n).is_some_and(|l| l.contains(label)));
    exact.or(part).map(|n| n.rect())
}

fn press(h: &mut H, label: &str) {
    let mut seen = None;
    for _ in 0..100 {
        let rect = find(h, label).unwrap_or_else(|| panic!("no {label}"));
        if seen == Some(rect) {
            break;
        }
        seen = Some(rect);
        h.run_steps(2);
    }
    click_at(h, find(h, label).unwrap().center());
}

fn key(h: &mut H, key: egui::Key) {
    h.key_press(key);
    rest(h);
}

/// Scrolls the window with the wheel until the widget whose label contains `label` is drawn in view, between the
/// title bar and the footer buttons, as a player would before aiming at it.
fn reveal(h: &mut H, label: &str) {
    let size = h.ctx.content_rect().size();
    for _ in 0..80 {
        if let Some(r) = find(h, label) {
            if r.top() > 70.0 && r.bottom() < size.y - 50.0 && r.bottom() > 0.0 {
                return;
            }
            wheel(
                h,
                if r.center().y < size.y / 2.0 {
                    150.0
                } else {
                    -150.0
                },
            );
        } else {
            wheel(h, -150.0);
        }
    }
    panic!("{label} is never brought into view by the wheel");
}

/// Turns the wheel over the window's middle by `dy` points and lets the frames follow.
fn wheel(h: &mut H, dy: f32) {
    let size = h.ctx.content_rect().size();
    h.hover_at(egui::pos2(size.x * 0.4, size.y * 0.5));
    h.run_steps(2);
    h.event(egui::Event::MouseWheel {
        unit: egui::MouseWheelUnit::Point,
        delta: egui::vec2(0.0, dy),
        modifiers: egui::Modifiers::NONE,
        phase: egui::TouchPhase::Move,
    });
    h.run_steps(4);
}

fn key_mod(h: &mut H, mods: egui::Modifiers, key: egui::Key) {
    h.key_press_modifiers(mods, key);
    h.run_steps(3);
    rest(h);
}

/// Types `text` as a keyboard does: one text event, into whatever has the keyboard focus.
fn type_text(h: &mut H, text: &str) {
    h.event(egui::Event::Text(text.to_string()));
    h.run_steps(3);
    rest(h);
}

/// Scrolls until text box `at` of `text_boxes` is in view and clicks into it.
fn click_box(h: &mut H, at: usize) {
    let size = h.ctx.content_rect().size();
    for _ in 0..80 {
        let r = text_boxes(h)[at];
        if r.top() > 80.0 && r.bottom() < size.y - 50.0 {
            break;
        }
        wheel(
            h,
            if r.center().y < size.y / 2.0 {
                150.0
            } else {
                -150.0
            },
        );
    }
    h.run_steps(6);
    click_at(h, text_boxes(h)[at].center());
}

/// Replaces the text of the box with the keyboard focus, as Ctrl+A and typing do.
fn replace_text(h: &mut H, text: &str) {
    key_mod(h, egui::Modifiers::COMMAND, egui::Key::A);
    type_text(h, text);
}

/// The text boxes drawn, in the order the screen draws them.
fn text_boxes(h: &H) -> Vec<egui::Rect> {
    let mut boxes = h
        .root()
        .children_recursive()
        .filter(|n| n.accesskit_node().role() == egui::accesskit::Role::TextInput)
        .map(|n| n.rect())
        .collect::<Vec<_>>();
    boxes.sort_by(|a, b| {
        (a.top(), a.left())
            .partial_cmp(&(b.top(), b.left()))
            .unwrap()
    });
    boxes
}

/// The canvas of the editor on top, its glyphs row by row.
fn canvas_glyphs(h: &H) -> Vec<String> {
    crate::live_editor::glyphs(h, "editor:canvas")
}

/// The screen rectangle of canvas square (`x`, `y`), found among the drawn buttons in the order the grid draws them.
fn square(h: &H, x: usize, y: usize) -> egui::Rect {
    let glyphs = canvas_glyphs(h);
    let cols = glyphs[0].chars().count();
    let buttons = h
        .root()
        .children_recursive()
        .filter(|n| {
            let a = n.accesskit_node();
            a.role() == egui::accesskit::Role::Button
                && a.label()
                    .is_some_and(|l| l.chars().count() <= 1 && l != "⋯")
        })
        .collect::<Vec<_>>();
    let at = y * cols + x;
    let want = glyphs[y].chars().nth(x).unwrap().to_string();
    let found = &buttons[at];
    assert_eq!(
        found.accesskit_node().label().unwrap_or_default(),
        want,
        "square {x},{y}"
    );
    found.rect()
}

/// Presses and releases the primary button at `at`, as a click does.
fn click_at(h: &mut H, at: egui::Pos2) {
    tap(h, at);
    rest(h);
}

/// One press and release of the primary button at `at`, a frame or two long.
fn tap(h: &mut H, at: egui::Pos2) {
    h.hover_at(at);
    h.run_steps(2);
    for pressed in [true, false] {
        h.event(egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        });
    }
    h.run_steps(2);
}

/// The main menu's Editor tab, its map editor, and the mod list's answer, until the editor is up.
fn open_map_editor(h: &mut H) {
    click(h, "Editor");
    click(h, "Map editor");
    // The mod list: Escape leaves it with the mods as they are, and the engine asks to open the editor with them.
    key(h, egui::Key::Escape);
    click(h, "Yes");
    wait_kind(h, "editor");
}

/// Clicks the tab labelled `label` and waits until it is the selected one.
fn click_tab(h: &mut H, label: &str) {
    click(h, label);
    run_until(h, label, 30, |h| {
        h.root().children_recursive().any(|n| {
            let a = n.accesskit_node();
            a.role() == egui::accesskit::Role::Tab
                && a.label().as_deref() == Some(label)
                && a.is_selected() == Some(true)
        })
    });
    rest(h);
}

/// Whether a button is labelled exactly `label`: a row of a list, not a square that names it among its parts.
fn has_row(h: &H, label: &str) -> bool {
    labels(h).contains(&format!("Button {label}"))
}

/// Scrolls until square (`x`, `y`) of the canvas is in view and clicks it.
fn paint(h: &mut H, x: usize, y: usize) {
    let size = h.ctx.content_rect().size();
    for _ in 0..80 {
        let r = square(h, x, y);
        if r.top() > 80.0 && r.bottom() < size.y - 50.0 {
            break;
        }
        wheel(
            h,
            if r.center().y < size.y / 2.0 {
                150.0
            } else {
                -150.0
            },
        );
    }
    h.run_steps(6);
    click_at(h, square(h, x, y).center());
}

/// The glyph of square (`x`, `y`) of the canvas.
fn glyph(h: &H, x: usize, y: usize) -> Option<char> {
    canvas_glyphs(h)[y].chars().nth(x)
}

/// A new mapgen file in the profile's mods directory, made from the editor's form, and its canvas.
fn create_mapgen(h: &mut H, name: &str) -> std::path::PathBuf {
    let path =
        std::env::temp_dir().join(format!("bn-live-{}-0/mods/{name}.json", std::process::id()));
    click_box(h, 1);
    type_text(h, &path.to_string_lossy());
    click_box(h, 2);
    type_text(h, "field");
    click(h, "Create");
    if find(h, "Continue").is_some() {
        click(h, "Continue");
    }
    wait_kind(h, "editor");
    path
}

#[test]
#[ignore = "requires BN_ENGINE pointing at an engine binary"]
fn the_map_editor_paints_places_furniture_undoes_previews_saves_and_reopens() {
    let dir = std::env::temp_dir().join(format!("bn-walk-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let (mut h, _root) = start(&dir);
    h.set_size(egui::vec2(1290.0, 830.0));
    rest(&mut h);
    open_map_editor(&mut h);
    shot(&mut h, "map-1-editor");
    let path = create_mapgen(&mut h, "zz_walk");
    let blank = canvas_glyphs(&h);
    assert_eq!((blank.len(), blank[0].chars().count()), (24, 24));
    shot(&mut h, "map-2-created");

    // A terrain from the catalog becomes the legend's `#`, picked by a double click on its row.
    click(&mut h, "Terrain");
    double_click(&mut h, "brick wall");
    assert_eq!(field(&h, "editor:mapping", "value"), "t_brick_wall");
    click_box(&mut h, 2);
    replace_text(&mut h, "#");
    key(&mut h, egui::Key::Enter);
    click(&mut h, "Set mapping");
    assert!(crate::live_editor::rows(&h, "editor:legend").contains(&"symbol:#".to_string()));
    shot(&mut h, "map-3-mapped");
    double_click(&mut h, "\"t_brick_wall\"");
    assert!(
        document(&h).contains("\"Symbol\"=\"#\""),
        "{}",
        document(&h)
    );

    // A rectangle from two clicks, undone by the button, redone by Ctrl+Y, undone by Ctrl+Z.
    click(&mut h, "Rectangle");
    paint(&mut h, 3, 1);
    paint(&mut h, 9, 3);
    let walled = canvas_glyphs(&h);
    assert_eq!(
        walled[1].chars().skip(3).take(7).collect::<String>(),
        "#######"
    );
    assert_eq!(glyph(&h, 9, 3), Some('#'));
    shot(&mut h, "map-4-rectangle");
    click_footer(&mut h, "Undo");
    assert_eq!(canvas_glyphs(&h), blank);
    key_mod(&mut h, egui::Modifiers::CTRL, egui::Key::Y);
    assert_eq!(canvas_glyphs(&h), walled);
    key_mod(&mut h, egui::Modifiers::CTRL, egui::Key::Z);
    assert_eq!(canvas_glyphs(&h), blank);
    key_mod(&mut h, egui::Modifiers::CTRL, egui::Key::Y);

    // Furniture: the first catalog entry becomes the legend's `C`, drawn with the pencil on one square.
    click_tab(&mut h, "Furniture");
    let top = h.state().model.screens().last().unwrap();
    let catalog = serde_json::to_value(top.root.as_ref().unwrap().find("editor:catalog")).unwrap();
    let name = catalog["rows"][0]["label"].as_str().unwrap().to_string();
    double_click(&mut h, &name);
    assert_eq!(field(&h, "editor:mapping", "layer"), "furniture");
    let furniture = field(&h, "editor:mapping", "value");
    click_box(&mut h, 2);
    replace_text(&mut h, "C");
    key(&mut h, egui::Key::Enter);
    click(&mut h, "Set mapping");
    // The same symbol also names the floor under it: the entry has no `fill_ter`, so a symbol without
    // a terrain makes the entry invalid.
    click_tab(&mut h, "Terrain");
    let top = h.state().model.screens().last().unwrap();
    let catalog = serde_json::to_value(top.root.as_ref().unwrap().find("editor:catalog")).unwrap();
    let floor = catalog["rows"][0]["label"].as_str().unwrap().to_string();
    double_click(&mut h, &floor);
    assert_eq!(field(&h, "editor:mapping", "layer"), "terrain");
    click_box(&mut h, 2);
    replace_text(&mut h, "C");
    key(&mut h, egui::Key::Enter);
    click(&mut h, "Set mapping");
    double_click(&mut h, &format!("\"{furniture}\""));
    run_until(&mut h, "the symbol C", 20, |h| {
        document(h).contains("\"Symbol\"=\"C\"")
    });
    assert!(
        document(&h).contains("\"Symbol\"=\"C\""),
        "{}",
        document(&h)
    );
    click(&mut h, "Pencil");
    paint(&mut h, 15, 15);
    assert_eq!(glyph(&h, 15, 15), Some('C'));
    let edited = canvas_glyphs(&h);
    shot(&mut h, "map-5-furniture");

    // The game's own generation of the edited entry, from the overflow menu.
    click_footer(&mut h, "⋯");
    click_footer(&mut h, "Preview");
    shot(&mut h, "map-7-preview-asks");
    click_footer(&mut h, "OK");
    run_until(&mut h, "the preview", 60, |h| {
        let top = h.state().model.screens().last().unwrap();
        top.root.as_ref().unwrap().find("editor:preview").is_some()
    });
    shot(&mut h, "map-7-preview");
    dump(&h, "after preview");
    println!(
        "PREVIEW rows {:?}",
        crate::live_editor::glyphs(&h, "editor:preview").len()
    );

    // Saved with Ctrl+S, on disk with both mappings, and the editor left with Escape.
    key_mod(&mut h, egui::Modifiers::CTRL, egui::Key::S);
    assert!(
        document(&h).contains("\"Saved\"=\"Yes\""),
        "{}",
        document(&h)
    );
    let saved = std::fs::read_to_string(&path).unwrap();
    assert!(
        saved.contains("t_brick_wall") && saved.contains(&furniture),
        "{saved}"
    );
    shot(&mut h, "map-8-saved");
    key(&mut h, egui::Key::Escape);
    dump(&h, "after escape");
    shot(&mut h, "map-9-left");

    // Reopened from the file list by its name: the walls and the barrel are still there.
    click(&mut h, "Map Editor");
    wait_kind(&mut h, "editor");
    click_box(&mut h, 0);
    replace_text(&mut h, &path.to_string_lossy());
    key(&mut h, egui::Key::Enter);
    shot(&mut h, "map-10-filtered");
    wait_kind(&mut h, "editor");
    run_until(&mut h, "the reopened canvas", 30, |h| {
        canvas_glyphs(h).len() == 24
    });
    assert_eq!(canvas_glyphs(&h), edited);
    shot(&mut h, "map-11-reopened");
    let _ = dir;
}

/// Pictures of the window at each of `sizes`, back at 1290x830 after.
fn shot_at(h: &mut H, name: &str, sizes: &[(f32, f32)]) {
    let Some(dir) = std::env::var_os("SHOT_DIR") else {
        return;
    };
    std::fs::create_dir_all(&dir).unwrap();
    for &(w, ht) in sizes {
        h.set_size(egui::vec2(w, ht));
        settle(h);
        h.render()
            .expect("renders")
            .save(std::path::Path::new(&dir).join(format!("{name}-{w}x{ht}.png")))
            .unwrap();
    }
    h.set_size(egui::vec2(1290.0, 830.0));
    settle(h);
}

#[test]
#[ignore = "requires BN_ENGINE pointing at an engine binary"]
fn the_vehicle_editor_prototypes_list_keeps_its_id_column_clear_of_the_scroll_bar() {
    let dir = std::env::temp_dir().join(format!("bn-walk-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let (mut h, _root) = start(&dir);
    h.set_size(egui::vec2(1290.0, 830.0));
    rest(&mut h);
    click(&mut h, "Editor");
    click(&mut h, "Vehicle editor");
    key(&mut h, egui::Key::Escape);
    click(&mut h, "Yes");
    wait_kind(&mut h, "vehicle_editor");
    rest(&mut h);
    // Opened, the prototype takes the right of the window and the list is left a narrow pane.
    double_click(&mut h, "Offroad Car");
    wait_kind(&mut h, "vehicle_editor");
    rest(&mut h);
    shot_at(
        &mut h,
        "prototypes-beside-detail",
        &[(1290.0, 830.0), (1920.0, 1080.0)],
    );
    // No ID of the list reaches its scroll bar.
    let window = find(&h, "Vehicle Editor").expect("window");
    let shapes = &h.output().shapes;
    let bar = shapes
        .iter()
        .filter_map(|c| match &c.shape {
            egui::Shape::Rect(r) if r.rect.width() < 20.0 && r.rect.height() > 50.0 => Some(r.rect),
            _ => None,
        })
        .filter(|r| window.x_range().contains(r.left()))
        .map(|r| r.left())
        .fold(f32::MAX, f32::min);
    let ids = shapes
        .iter()
        .filter_map(|c| match &c.shape {
            egui::Shape::Text(t) if t.galley.text().ends_with("_car") => {
                Some(t.visual_bounding_rect().right())
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    assert!(
        !ids.is_empty() && ids.iter().all(|r| *r <= bar),
        "{ids:?} past the bar at {bar}"
    );
}

#[test]
#[ignore = "requires BN_ENGINE pointing at an engine binary"]
fn the_vehicle_editor_opens_a_prototype_installs_and_saves_a_mod_file() {
    let dir = std::env::temp_dir().join(format!("bn-walk-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let (mut h, _root) = start(&dir);
    h.set_size(egui::vec2(1290.0, 830.0));
    rest(&mut h);
    click(&mut h, "Editor");
    click(&mut h, "Vehicle editor");
    key(&mut h, egui::Key::Escape);
    click(&mut h, "Yes");
    wait_kind(&mut h, "vehicle_editor");
    rest(&mut h);
    shot(&mut h, "veh-1-editor");
    click_footer(&mut h, "Filter");
    rest(&mut h);
    shot(&mut h, "veh-2-filter-asks");
    type_text(&mut h, "car");
    shot(&mut h, "veh-3-typed");
    key(&mut h, egui::Key::Enter);
    rest(&mut h);
    shot(&mut h, "veh-3-filtered");
    double_click(&mut h, "Offroad Car");
    wait_kind(&mut h, "vehicle_editor");
    rest(&mut h);
    shot(&mut h, "veh-4-opened");
    // The mount holding the tank, by a click on its square.
    click(&mut h, "vehicle tank (60L)");
    rest(&mut h);
    shot(&mut h, "veh-5-mount");
    double_click(&mut h, "vehicle tank (60L)");
    rest(&mut h);
    shot(&mut h, "veh-6-part-picked");
    press(&mut h, "⋯");
    rest(&mut h);
    click(&mut h, "Part");
    rest(&mut h);
    shot(&mut h, "veh-8-part-tab");
    click_box(&mut h, 0);
    replace_text(&mut h, "diesel");
    key(&mut h, egui::Key::Enter);
    rest(&mut h);
    shot(&mut h, "veh-9-fuel");

    // A headlight installed on the quarterpanel mount from the filtered part list, then shown on the Mount tab.
    click(&mut h, "frame, quarterpanel");
    click_tab(&mut h, "Install");
    click_footer(&mut h, "Filter parts");
    type_text(&mut h, "headlight");
    key(&mut h, egui::Key::Enter);
    rest(&mut h);
    shot(&mut h, "veh-10-parts-filtered");
    double_click(&mut h, "headlight");
    rest(&mut h);
    shot(&mut h, "veh-11-installed");
    click_tab(&mut h, "Mount");
    shot(&mut h, "veh-12-mount-after");
    assert!(has_row(&h, "headlight"));

    // Removed from the stack by the footer's Remove and brought back by the footer's Undo; the engine binds no key to the vehicle editor's Undo.
    double_click(&mut h, "headlight");
    click_footer(&mut h, "Remove");
    rest(&mut h);
    shot(&mut h, "veh-13-removed");
    assert!(!has_row(&h, "headlight"), "{}", labels(&h).join("\n"));
    click_footer(&mut h, "Undo");
    rest(&mut h);
    assert!(has_row(&h, "headlight"));
    shot(&mut h, "veh-14-undone");

    // The vehicle's name on the Vehicle tab.
    press(&mut h, "⋯");
    click(&mut h, "Vehicle");
    rest(&mut h);
    shot(&mut h, "veh-15-vehicle-tab");
    click_box(&mut h, 0);
    replace_text(&mut h, "Walker Car");
    key(&mut h, egui::Key::Enter);
    rest(&mut h);
    shot(&mut h, "veh-16-renamed");

    // Saved as a mod file: the engine only writes under the game's data or a mod.
    let path = std::env::temp_dir().join(format!(
        "bn-live-{}-0/mods/zz_vehicle.json",
        std::process::id()
    ));
    click_footer(&mut h, "Save as");
    rest(&mut h);
    shot(&mut h, "veh-17-save-as-asks");
    type_text(&mut h, &path.to_string_lossy());
    key(&mut h, egui::Key::Enter);
    rest(&mut h);
    shot(&mut h, "veh-18-saved");

    // The edited design spawned beside the avatar, listed on the World tab, and looked at on the map.
    click_footer(&mut h, "Spawn");
    rest(&mut h);
    click_tab(&mut h, "World");
    shot(&mut h, "veh-19-world");
    key(&mut h, egui::Key::Escape);
    rest(&mut h);
    shot(&mut h, "veh-20-spawned-on-map");
    let saved = std::fs::read_to_string(&path).unwrap_or_default();
    assert!(
        saved.contains("Walker Car") && saved.contains("diesel"),
        "{saved}"
    );
}

#[test]
#[ignore = "requires BN_ENGINE pointing at an engine binary"]
fn the_vehicle_editor_sets_the_ammunition_of_a_turret_from_the_part_tab() {
    let dir = std::env::temp_dir().join(format!("bn-walk-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let (mut h, _root) = start(&dir);
    h.set_size(egui::vec2(1290.0, 830.0));
    rest(&mut h);
    click(&mut h, "Editor");
    click(&mut h, "Vehicle editor");
    key(&mut h, egui::Key::Escape);
    click(&mut h, "Yes");
    wait_kind(&mut h, "vehicle_editor");
    rest(&mut h);
    click_footer(&mut h, "Filter");
    type_text(&mut h, "armored_car");
    key(&mut h, egui::Key::Enter);
    double_click(&mut h, "Armored Car");
    wait_kind(&mut h, "vehicle_editor");
    rest(&mut h);
    shot(&mut h, "turret-1-opened");

    // The mount holding a turret, then the turret among its parts, then the Part tab's form.
    click(&mut h, "turret mount, mounted M249, heavy duty opaque door");
    double_click(&mut h, "mounted M249");
    press(&mut h, "⋯");
    click(&mut h, "Part");
    rest(&mut h);
    shot(&mut h, "turret-2-part-tab-before");
    let form = "vehicle:part_form";
    assert_eq!(field(&h, form, "ammo"), "60");

    // The ammunition percentage, the ammunition types and the fewest and most rounds, each typed into its box.
    for (at, id, text) in [
        (0, "ammo", "50"),
        (1, "ammo_types", "223"),
        (2, "ammo_min", "10"),
        (3, "ammo_max", "20"),
    ] {
        click_box(&mut h, at);
        replace_text(&mut h, text);
        key(&mut h, egui::Key::Enter);
        rest(&mut h);
        assert_eq!(field(&h, form, id), text, "{id}");
    }
    shot(&mut h, "turret-3-part-tab-after");

    // Saved as a mod file, where the turret's part entry carries what was typed.
    let path = std::env::temp_dir().join(format!(
        "bn-live-{}-0/mods/zz_turret.json",
        std::process::id()
    ));
    click_footer(&mut h, "Save as");
    type_text(&mut h, &path.to_string_lossy());
    key(&mut h, egui::Key::Enter);
    rest(&mut h);
    shot(&mut h, "turret-4-saved");
    let saved = std::fs::read_to_string(&path).unwrap_or_default();
    assert!(
        saved.contains(
            r#"{ "part": "m249", "ammo": 50, "ammo_types": [ "223" ], "ammo_qty": [ 10, 20 ] }"#
        ),
        "{saved}"
    );
}
