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

/// Clicks a button of the footer, which stays in view whatever the body scrolls; one that did not fit the footer is in
/// the menu of its `⋯`, the lowest one in the window.
fn click_footer(h: &mut H, label: &str) {
    if find(h, label).is_none() {
        let at = stable(h, label, |h| more_buttons(h).first().copied());
        click_at(h, at.center());
    }
    press(h, label);
}

/// The `⋯` buttons drawn, in the order the screen draws them: the footer's first, then those of tab bars.
fn more_buttons(h: &H) -> Vec<egui::Rect> {
    h.root()
        .children_recursive()
        .filter(|n| n.accesskit_node().label().as_deref() == Some("⋯"))
        .map(|n| n.rect())
        .collect()
}

/// The rectangle of the widget labelled exactly `label`, else the first whose label contains it.
fn find(h: &H, label: &str) -> Option<egui::Rect> {
    let labelled = |n: &egui_kittest::Node| {
        let a = n.accesskit_node();
        // What a person typed into a box is not a widget to aim at.
        (!matches!(
            a.role(),
            egui::accesskit::Role::TextInput | egui::accesskit::Role::TextRun
        ))
        .then(|| a.label().or_else(|| a.value()).map(|l| l.to_string()))
        .flatten()
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
    reveal_rect(h, label, |h| find(h, label));
}

/// `reveal` for any widget `rect_of` finds, named `what` when it never comes into view.
fn reveal_rect(h: &mut H, what: &str, rect_of: impl Fn(&H) -> Option<egui::Rect>) {
    let size = h.ctx.content_rect().size();
    let mut missing = 0;
    for _ in 0..120 {
        let Some(r) = rect_of(h) else {
            // Not drawn: it is scrolled out of the window, so look down the window, then up.
            wheel(
                h,
                if (missing / 30) % 2 == 0 {
                    -150.0
                } else {
                    150.0
                },
            );
            missing += 1;
            continue;
        };
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
    }
    panic!("{what} is never brought into view by the wheel");
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
    let in_bar = |h: &H| {
        h.root().children_recursive().any(|n| {
            n.accesskit_node().role() == egui::accesskit::Role::Tab
                && n.accesskit_node().label().as_deref() == Some(label)
        })
    };
    if !in_bar(h) {
        // A tab that did not fit the bar is in the menu of its `⋯`.
        open_more_tabs(h);
    }
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
    let path = crate::common::live::profile_dir().join(format!("mods/{name}.json"));
    click_beside(h, "New file");
    type_text(h, &path.to_string_lossy());
    click_beside(h, "Overmap terrain");
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
    click_beside(&mut h, "Symbol");
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

    // Furniture: the symbol `C` names a floor first, as the engine refuses a symbol without a terrain, then the first
    // catalog entry, and is drawn with the pencil on one square.
    click_tab(&mut h, "Terrain");
    let top = h.state().model.screens().last().unwrap();
    let catalog = serde_json::to_value(top.root.as_ref().unwrap().find("editor:catalog")).unwrap();
    let floor = catalog["rows"][0]["label"].as_str().unwrap().to_string();
    double_click(&mut h, &floor);
    assert_eq!(field(&h, "editor:mapping", "layer"), "terrain");
    click_beside(&mut h, "Symbol");
    replace_text(&mut h, "C");
    key(&mut h, egui::Key::Enter);
    click(&mut h, "Set mapping");
    click_tab(&mut h, "Furniture");
    let top = h.state().model.screens().last().unwrap();
    let catalog = serde_json::to_value(top.root.as_ref().unwrap().find("editor:catalog")).unwrap();
    let name = catalog["rows"][0]["label"].as_str().unwrap().to_string();
    double_click(&mut h, &name);
    assert_eq!(field(&h, "editor:mapping", "layer"), "furniture");
    let furniture = field(&h, "editor:mapping", "value");
    click_beside(&mut h, "Symbol");
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
    click_input(&mut h, "Search");
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
    click_input(&mut h, "Search");
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
    click_tab(&mut h, "Part");
    rest(&mut h);
    shot(&mut h, "veh-8-part-tab");
    click_beside(&mut h, "Fuel");
    replace_text(&mut h, "diesel");
    key(&mut h, egui::Key::Enter);
    rest(&mut h);
    shot(&mut h, "veh-9-fuel");

    // A headlight installed on the quarterpanel mount from the filtered part list, then shown on the Mount tab.
    click(&mut h, "frame, quarterpanel");
    click_tab(&mut h, "Install");
    click_input(&mut h, "Search");
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
    click_tab(&mut h, "Vehicle");
    rest(&mut h);
    shot(&mut h, "veh-15-vehicle-tab");
    click_beside(&mut h, "Name");
    replace_text(&mut h, "Walker Car");
    key(&mut h, egui::Key::Enter);
    rest(&mut h);
    shot(&mut h, "veh-16-renamed");

    // Saved as a mod file: the engine only writes under the game's data or a mod.
    let path = crate::common::live::profile_dir().join("mods/zz_vehicle.json");
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
    click_input(&mut h, "Search");
    type_text(&mut h, "armored_car");
    key(&mut h, egui::Key::Enter);
    double_click(&mut h, "Armored Car");
    wait_kind(&mut h, "vehicle_editor");
    rest(&mut h);
    shot(&mut h, "turret-1-opened");

    // The mount holding a turret, then the turret among its parts, then the Part tab's form.
    click(&mut h, "turret mount, mounted M249, heavy duty opaque door");
    double_click(&mut h, "mounted M249");
    click_tab(&mut h, "Part");
    rest(&mut h);
    shot(&mut h, "turret-2-part-tab-before");
    let form = "vehicle:part_form";
    assert_eq!(field(&h, form, "ammo"), "60");

    // The ammunition percentage, the ammunition types and the fewest and most rounds, each typed into its box.
    for (label, id, text) in [
        ("Ammo %", "ammo", "50"),
        ("Ammo types", "ammo_types", "223"),
        ("Rounds, fewest", "ammo_min", "10"),
        ("Rounds, most", "ammo_max", "20"),
    ] {
        click_beside(&mut h, label);
        replace_text(&mut h, text);
        key(&mut h, egui::Key::Enter);
        rest(&mut h);
        assert_eq!(field(&h, form, id), text, "{id}");
    }
    shot(&mut h, "turret-3-part-tab-after");

    // Saved as a mod file, where the turret's part entry carries what was typed.
    let path = crate::common::live::profile_dir().join("mods/zz_turret.json");
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

/// Opens the menu of the tabs that did not fit the tab bar: the last `⋯` drawn, not the footer's.
fn open_more_tabs(h: &mut H) {
    let at = click_stable(h, "⋯ of the tabs", |h| more_buttons(h).last().copied());
    click_at(h, at.center());
}

/// Clicks into the text box whose accessible label is exactly `label`, such as the search box above a list; of
/// several, the one drawn last, in the innermost pane.
fn click_input(h: &mut H, label: &str) {
    let rect_of = |h: &H| {
        h.root()
            .children_recursive()
            .filter(|n| {
                let a = n.accesskit_node();
                a.role() == egui::accesskit::Role::TextInput && a.label().as_deref() == Some(label)
            })
            .last()
            .map(|n| n.rect())
    };
    let at = click_stable(h, label, rect_of);
    click_at(h, at.center());
}

/// Clicks into the text box drawn right of the label `label`, on the label's row.
fn click_beside(h: &mut H, label: &str) {
    let rect_of = |h: &H| {
        let boxes = text_boxes(h);
        h.root()
            .children_recursive()
            .filter(|n| {
                let a = n.accesskit_node();
                a.role() != egui::accesskit::Role::TextInput
                    && a.label().or_else(|| a.value()).as_deref() == Some(label)
            })
            .map(|n| n.rect())
            .find(|l| {
                boxes
                    .iter()
                    .any(|b| b.y_range().contains(l.center().y) && b.left() > l.left())
            })
    };
    let at = click_stable(h, label, rect_of);
    click_at(h, egui::pos2(at.right() + 40.0, at.center().y));
}

/// Scrolls the rectangle `rect_of` finds into view, waits until it stopped moving and returns it.
fn click_stable(h: &mut H, what: &str, rect_of: impl Fn(&H) -> Option<egui::Rect>) -> egui::Rect {
    reveal_rect(h, what, &rect_of);
    stable(h, what, rect_of)
}

/// The rectangle `rect_of` finds once it stopped moving.
fn stable(h: &mut H, what: &str, rect_of: impl Fn(&H) -> Option<egui::Rect>) -> egui::Rect {
    let mut seen = None;
    while seen != rect_of(h) {
        seen = rect_of(h);
        h.run_steps(3);
    }
    seen.unwrap_or_else(|| panic!("no {what}: {:?}", labels(h)))
}

#[test]
#[ignore = "requires BN_ENGINE pointing at an engine binary"]
fn the_map_editor_creates_a_palette_maps_a_symbol_and_saves_it() {
    let sizes = [(1290.0, 830.0), (1280.0, 800.0)];
    let dir = std::env::temp_dir().join(format!("bn-walk-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let (mut h, _root) = start(&dir);
    h.set_size(egui::vec2(1290.0, 830.0));
    rest(&mut h);
    open_map_editor(&mut h);
    shot_at(&mut h, "pal-1-form", &sizes);
    // The kind select, opened by a click; its third option names the palette and relabels the id field.
    click(&mut h, "Mapgen");
    shot_at(&mut h, "pal-2-kinds", &sizes);
    click(&mut h, "Palette");
    assert!(find(&h, "Palette id").is_some(), "{:?}", labels(&h));
    let path = crate::common::live::profile_dir().join("mods/zz_palette.json");
    click_beside(&mut h, "New file");
    type_text(&mut h, &path.to_string_lossy());
    click_beside(&mut h, "Palette id");
    type_text(&mut h, "zz_walk_palette");
    shot_at(&mut h, "pal-3-form-filled", &sizes);
    click(&mut h, "Create");
    if find(&h, "Continue").is_some() {
        click(&mut h, "Continue");
    }
    wait_kind(&mut h, "editor");
    rest(&mut h);
    dump(&h, "the palette screen");
    shot_at(&mut h, "pal-4-created", &sizes);
    assert!(crate::live_editor::rows(&h, "editor:legend").contains(&"symbol:.".to_string()));
    let top = h.state().model.screens().last().unwrap();
    let root = top.root.as_ref().unwrap();
    assert!(root.find("editor:canvas").is_none() && root.find("editor:tools").is_none());
    assert_eq!(
        top.focus.as_ref().and_then(|f| f.component_id.as_deref()),
        Some("editor:legend")
    );

    // A terrain from the catalog becomes the palette's `#`.
    click(&mut h, "Terrain");
    shot_at(&mut h, "pal-5-catalog", &sizes);
    double_click(&mut h, "brick wall");
    assert_eq!(field(&h, "editor:mapping", "value"), "t_brick_wall");
    shot_at(&mut h, "pal-5b-picked", &sizes);
    click_beside(&mut h, "Symbol");
    replace_text(&mut h, "#");
    key(&mut h, egui::Key::Enter);
    click(&mut h, "Set mapping");
    shot_at(&mut h, "pal-6-mapped", &sizes);
    let legend = crate::live_editor::rows(&h, "editor:legend");
    assert!(legend.contains(&"symbol:#".to_string()), "{legend:?}");

    key_mod(&mut h, egui::Modifiers::CTRL, egui::Key::S);
    let saved = std::fs::read_to_string(&path).unwrap();
    assert!(
        saved.contains("zz_walk_palette") && saved.contains("t_brick_wall"),
        "{saved}"
    );
    assert!(
        document(&h).contains("\"Saved\"=\"Yes\""),
        "{}",
        document(&h)
    );
    for _ in 0..8 {
        wheel(&mut h, 150.0);
    }
    shot_at(&mut h, "pal-7-saved", &sizes);
}
