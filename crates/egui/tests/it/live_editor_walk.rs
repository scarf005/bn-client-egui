//! A player's walk through the map and vehicle editors against a real engine: every step is a mouse click on a
//! drawn widget or a key press, never an affordance invoked by id.
//!
//! `BN_ENGINE=<engine> SHOT_DIR=<dir> cargo test --release -p bn-client-egui --test it live_editor_walk:: --
//! --ignored --nocapture`
use crate::common::live::settle;
use crate::live_editor::{H, field, run_until, start, start_with, wait_kind};
use bn_client_egui::protocol::Component;
use bn_client_egui::{GameModel, MenuModel};
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

/// The row id of the legend's selected symbol, as `symbol:#`: the symbol the canvas draws with.
fn picked_symbol(h: &H) -> Option<String> {
    let top = h.state().model.screens().last().unwrap();
    let Some(Component::List(legend)) = top.root.as_ref().unwrap().find("editor:legend") else {
        return None;
    };
    legend
        .rows
        .iter()
        .find(|r| r.selected)
        .map(|r| r.id.clone())
}

/// The cell of the column `key` of the legend's row of `symbol`, such as the name of what it places on a layer: the
/// legend is drawn above the catalog, so a name both show is aimed at in the legend.
fn legend_cell(h: &H, symbol: &str, key: &str) -> String {
    let top = h.state().model.screens().last().unwrap();
    let Some(Component::List(legend)) = top.root.as_ref().unwrap().find("editor:legend") else {
        panic!("no legend");
    };
    let id = format!("symbol:{symbol}");
    legend
        .rows
        .iter()
        .find(|r| r.id == id)
        .and_then(|r| r.columns.iter().find(|c| c.key.as_deref() == Some(key)))
        .map(|c| c.value.shown())
        .unwrap_or_else(|| panic!("no {key} cell for {symbol}"))
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
pub(crate) fn click(h: &mut H, label: &str) {
    reveal(h, label);
    press(h, label);
}

/// The button labelled exactly `label`, the last one drawn, past a heading of the same name.
fn button(h: &H, label: &str) -> Option<egui::Rect> {
    h.root()
        .children_recursive()
        .filter(|n| {
            let a = n.accesskit_node();
            a.role() == egui::accesskit::Role::Button && a.label().as_deref() == Some(label)
        })
        .map(|n| n.rect())
        .last()
}

/// Clicks `button(label)` once it is in view and stopped moving.
fn click_button(h: &mut H, label: &str) {
    let at = click_stable(h, label, |h| button(h, label));
    click_at(h, at.center());
}

/// Double-clicks the widget labelled `label`: a row of a list in a pane runs its action so.
fn double_click(h: &mut H, label: &str) {
    let at = click_stable(h, label, |h| find(h, label)).center();
    h.hover_at(at);
    h.run_steps(2);
    // Two clicks a frame or two apart, as a hand makes them.
    tap(h, at);
    tap(h, at);
    rest(h);
}

/// Clicks a button of the footer, which stays in view whatever the body scrolls; one that did not fit the footer is in
/// the menu of its `⋯`, the lowest one in the window, and a grouped one is an item of a menu of the menu bar.
fn click_footer(h: &mut H, label: &str) {
    if button(h, label).is_none() {
        match more_buttons(h).first().copied() {
            Some(_) => {
                let at = stable(h, label, |h| more_buttons(h).first().copied());
                click_at(h, at.center());
            }
            None => open_menu_with(h, label),
        }
    }
    click_button(h, label);
}

/// Opens the menu of the menu bar that holds `label`, trying each menu in turn and shutting the others again.
fn open_menu_with(h: &mut H, label: &str) {
    let menus = ["File", "Edit", "View", "World"]
        .into_iter()
        .filter(|m| find(h, m).is_some())
        .collect::<Vec<_>>();
    for menu in menus {
        press(h, menu);
        if button(h, label).is_some() {
            return;
        }
        press(h, menu);
    }
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

pub(crate) fn key(h: &mut H, key: egui::Key) {
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
    // Over the right column's gutter the wheel scrolls the column; over its middle, the list there.
    let (bar, list) = (gutter(h), egui::pos2(size.x - 120.0, size.y * 0.5));
    // How many times it was not drawn so far.
    let mut misses = 0u32..;
    for _ in 0..200 {
        // A wheel turn scrolls on over a few frames: where it ends is what counts.
        let mut seen = None;
        while seen != rect_of(h) {
            seen = rect_of(h);
            h.run_steps(3);
        }
        let Some(r) = seen else {
            // Not drawn: it is scrolled out of the list in the column's middle or of the column, so look down that
            // list, then up it, then down and up the column.
            let phase = misses.next().unwrap_or_default() / 20 % 4;
            let down = phase % 2 == 0;
            let at = if phase < 2 { list } else { bar };
            wheel_at(h, at, if down { -150.0 } else { 150.0 });
            continue;
        };
        if in_view(h, r) {
            return;
        }
        let dy = if r.center().y < size.y / 2.0 {
            150.0
        } else {
            -150.0
        };
        wheel_at(h, bar, dy);
        // A column that scrolls no further leaves it where it was: its own list scrolls it into view.
        if rect_of(h) == Some(r) {
            wheel_at(h, r.center(), dy);
        }
    }
    let seen = rect_of(h);
    panic!(
        "{what} is never brought into view by the wheel; last at {seen:?} in {size:?} among {:#?}",
        labels(h)
    );
}

/// Whether the widget drawn at `r` is whole on screen, uncut by what clips it (the menu bar, the status line, the
/// end of its scroll area).
fn in_view(h: &H, r: egui::Rect) -> bool {
    let screen = h.ctx.content_rect();
    let clipped = h.ctx.viewport(|v| {
        v.prev_pass
            .widgets
            .layers()
            .flat_map(|(_, ws)| ws)
            .find(|w| w.rect == r)
            .map(|w| w.interact_rect)
    });
    clipped.is_some_and(|c| c.contains_rect(r.shrink(1.0))) && screen.contains_rect(r)
}

/// A point in the gutter of a workspace's right column, left of the parts it indents under their headings: the
/// wheel there scrolls the column, over the canvas it would zoom it, and over a list it would scroll only that list.
/// The window's right edge without a workspace.
fn gutter(h: &H) -> egui::Pos2 {
    let size = h.ctx.content_rect().size();
    let side = h
        .root()
        .children_recursive()
        .filter(|n| n.accesskit_node().label().as_deref() == Some("Resize workspace column"))
        .map(|n| n.rect().center().x)
        .fold(None, |most: Option<f32>, x| {
            Some(most.map_or(x, |m| m.max(x)))
        });
    egui::pos2(side.map_or(size.x - 12.0, |x| x + 10.0), size.y * 0.5)
}

/// Turns the wheel over the gutter of a workspace's right column, which holds the lists and forms a walk reaches
/// for, by `dy` points and lets the frames follow.
fn wheel(h: &mut H, dy: f32) {
    let at = gutter(h);
    wheel_at(h, at, dy);
}

/// Turns the wheel over `at` by `dy` points and lets the frames follow.
fn wheel_at(h: &mut H, at: egui::Pos2, dy: f32) {
    h.hover_at(at);
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
    // The squares are the run of buttons that spells the canvas row by row and lies on its grid; a legend's
    // symbols are buttons too, and spell a blank canvas as well one button earlier.
    let spelled = glyphs.concat().chars().collect::<Vec<_>>();
    let on_grid = |i: usize| {
        let rect = |k: usize| buttons[k].rect();
        let (first, next, under) = (rect(i), rect(i + 1), rect(i + cols));
        (next.left() - first.right()).abs() < 1.0 && (under.top() - first.bottom()).abs() < 1.0
    };
    let names = buttons
        .iter()
        .map(|b| {
            b.accesskit_node()
                .label()
                .unwrap_or_default()
                .chars()
                .next()
                .unwrap_or(' ')
        })
        .collect::<Vec<_>>();
    let start = (0..=names.len().saturating_sub(spelled.len()))
        .find(|&i| names[i..i + spelled.len()] == spelled[..] && on_grid(i))
        .unwrap_or_else(|| panic!("no run of buttons spells the canvas"));
    let at = start + y * cols + x;
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
    // A footer button can carry the tab's name, so the tab itself is the one aimed at when the bar shows it.
    let tab = |h: &H| {
        h.root()
            .children_recursive()
            .find(|n| {
                n.accesskit_node().role() == egui::accesskit::Role::Tab
                    && n.accesskit_node().label().as_deref() == Some(label)
            })
            .map(|n| n.rect())
    };
    match tab(h) {
        Some(_) => {
            let at = click_stable(h, label, tab);
            click_at(h, at.center());
        }
        None => click(h, label),
    }
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

/// Scrolls until square (`x`, `y`) of the canvas is in view and clicks it.
fn paint(h: &mut H, x: usize, y: usize) {
    scroll_to(h, x, y);
    click_at(h, square(h, x, y).center());
}

/// Scrolls with the wheel until square (`x`, `y`) of the canvas is in view.
fn scroll_to(h: &mut H, x: usize, y: usize) {
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
}

/// The glyph of square (`x`, `y`) of the canvas.
fn glyph(h: &H, x: usize, y: usize) -> Option<char> {
    canvas_glyphs(h)[y].chars().nth(x)
}

/// A new mapgen file in the profile's mods directory, made from the editor's form, and its canvas.
fn create_mapgen(h: &mut H, name: &str) -> std::path::PathBuf {
    let path = crate::common::live::profile_dir().join(format!("mods/{name}.json"));
    click_beside(h, "New file/Path");
    // The form keeps what the file made before it was given.
    replace_text(h, &path.to_string_lossy());
    click_beside(h, "Overmap terrain");
    replace_text(h, "field");
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
    let cell = legend_cell(&h, "#", "terrain");
    double_click(&mut h, &cell);
    assert_eq!(picked_symbol(&h).as_deref(), Some("symbol:#"));

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
    let cell = legend_cell(&h, "C", "furniture");
    double_click(&mut h, &cell);
    run_until(&mut h, "the symbol C", 20, |h| {
        picked_symbol(h).as_deref() == Some("symbol:C")
    });
    click(&mut h, "Pencil");
    paint(&mut h, 15, 15);
    // The engine answers a paint after the client went idle; wait for the square instead of racing the reply.
    run_until(&mut h, "the painted C", 60, |h| {
        glyph(h, 15, 15) == Some('C')
    });
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

    // Reopened from the main menu's recent files: the walls and the barrel are still there.
    click(&mut h, "Editor");
    shot(&mut h, "map-10-recent");
    click(&mut h, &path.to_string_lossy());
    // The mod list first, left with the mods as they are, as when the editor opened.
    wait_kind(&mut h, "world_creation");
    key(&mut h, egui::Key::Escape);
    click(&mut h, "Yes");
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
    pick_stacked(&mut h, "vehicle tank (60L)");
    rest(&mut h);
    shot(&mut h, "veh-6-part-picked");
    click_tab(&mut h, "Part settings");
    rest(&mut h);
    shot(&mut h, "veh-8-part-tab");
    click_beside(&mut h, "Fuel");
    replace_text(&mut h, "diesel");
    key(&mut h, egui::Key::Enter);
    rest(&mut h);
    shot(&mut h, "veh-9-fuel");

    // A headlight chosen from the filtered part list and installed by a click on the quarterpanel mount, then shown
    // on the Parts here tab.
    click_input(&mut h, "Parts/Search");
    type_text(&mut h, "headlight");
    key(&mut h, egui::Key::Enter);
    rest(&mut h);
    shot(&mut h, "veh-10-parts-filtered");
    double_click(&mut h, "headlight");
    click(&mut h, "frame, quarterpanel");
    rest(&mut h);
    shot(&mut h, "veh-11-installed");
    click(&mut h, "Select");
    click_tab(&mut h, "Parts here");
    shot(&mut h, "veh-12-mount-after");
    assert!(stacked(&h, "headlight"));

    // The installed part, picked on top of its mount, removed by the list's own Remove and brought back by the Edit
    // menu's Undo.
    click(&mut h, "Remove headlight");
    rest(&mut h);
    shot(&mut h, "veh-13-removed");
    assert!(!stacked(&h, "headlight"), "{}", labels(&h).join("\n"));
    click_footer(&mut h, "Undo");
    rest(&mut h);
    assert!(stacked(&h, "headlight"));
    shot(&mut h, "veh-14-undone");

    // The vehicle's name on the Vehicle tab.
    click_tab(&mut h, "Name and flags");
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

    // The edited design spawned beside the avatar, and looked at on the map the editor leaves for.
    click_footer(&mut h, "Spawn");
    run_until(&mut h, "the map", 60, |h| {
        !h.state().model.busy() && top(h).as_deref() != Some("vehicle_editor")
    });
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
    pick_stacked(&mut h, "mounted M249");
    click_tab(&mut h, "Part settings");
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
/// several, the one drawn last, in the innermost pane. `heading/label` names the first such box under the heading.
fn click_input(h: &mut H, label: &str) {
    let (heading, label) = label.rsplit_once('/').unwrap_or(("", label));
    let rect_of = |h: &H| {
        let boxes = h
            .root()
            .children_recursive()
            .skip_while(|n| {
                !heading.is_empty() && n.accesskit_node().label().as_deref() != Some(heading)
            })
            .filter(|n| {
                let a = n.accesskit_node();
                a.role() == egui::accesskit::Role::TextInput && a.label().as_deref() == Some(label)
            })
            .map(|n| n.rect())
            .collect::<Vec<_>>();
        if heading.is_empty() {
            boxes.last()
        } else {
            boxes.first()
        }
        .copied()
    };
    let at = click_stable(h, label, rect_of);
    click_at(h, at.center());
}

/// Clicks into the text box drawn right of the label `label`, on the label's row.
/// `heading/label` names the field `label` of the form under the heading `heading`, where several forms have one.
fn click_beside(h: &mut H, label: &str) {
    let (heading, label) = label.rsplit_once('/').unwrap_or(("", label));
    let rect_of = |h: &H| {
        let boxes = text_boxes(h);
        let named = |n: &egui_kittest::Node| {
            let a = n.accesskit_node();
            a.label().or_else(|| a.value()).map(|l| l.to_string())
        };
        h.root()
            .children_recursive()
            .skip_while(|n| !heading.is_empty() && named(n).as_deref() != Some(heading))
            .filter(|n| {
                let a = n.accesskit_node();
                a.role() != egui::accesskit::Role::TextInput
                    && a.label().or_else(|| a.value()).as_deref() == Some(label)
            })
            .map(|n| n.rect())
            .find(|l| {
                // A form's box sits next to its label; a status fact of the same name has none.
                boxes.iter().any(|b| {
                    b.y_range().contains(l.center().y)
                        && (l.right()..l.right() + 120.0).contains(&b.left())
                })
            })
    };
    let at = click_stable(h, label, rect_of);
    click_at(h, egui::pos2(at.right() + 40.0, at.center().y));
}

/// Scrolls the rectangle `rect_of` finds into view, waits until it stopped moving and returns it.
/// A column that moves on its own while it settles, as an engine reply relays it out, is revealed again.
fn click_stable(h: &mut H, what: &str, rect_of: impl Fn(&H) -> Option<egui::Rect>) -> egui::Rect {
    for _ in 0..10 {
        reveal_rect(h, what, &rect_of);
        let at = stable(h, what, &rect_of);
        if in_view(h, at) {
            return at;
        }
    }
    panic!("{what} moves out of view whenever it settles");
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
    click_beside(&mut h, "New file/Path");
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

/// A walk's engine profile holding the documents the editors opened before: a map file of the game's data and the
/// bicycle prototype, both with the core mod.
fn start_with_recent() -> (H<'static>, String) {
    let dir = std::env::temp_dir().join(format!("bn-walk-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let root = crate::common::live::root();
    let file = root.join("data/json/mapgen/house/house01.json");
    assert!(file.is_file(), "{}", file.display());
    let file = file.to_string_lossy().replace('\\', "/");
    let recent = serde_json::json!([
        { "editor": "map", "name": file, "mods": ["bn"] },
        { "editor": "vehicle", "name": "bicycle", "mods": ["bn"] },
    ]);
    let (mut h, _root) = start_with(&dir, &[("editor_recent.json", recent.to_string())]);
    h.set_size(egui::vec2(1290.0, 830.0));
    rest(&mut h);
    (h, file)
}

/// The Editor tab opened, with each of `rows` drawn whole inside the window and not cut short, and its description
/// drawn once the arrow keys moved the selection onto it, as the menu shows the selected entry's.
fn editor_tab_lists(h: &mut H, rows: &[(&str, &str)]) {
    click(h, "Editor");
    rest(h);
    shot(h, "recent-1-editor-tab");
    for (label, _) in rows {
        for (w, ht) in [(1290.0, 830.0), (1920.0, 1080.0)] {
            h.set_size(egui::vec2(w, ht));
            settle(h);
            reveal(h, label);
            let window = h.ctx.content_rect();
            let row = find(h, label).unwrap_or_else(|| panic!("no row {label}: {:?}", labels(h)));
            assert!(
                window.contains_rect(row),
                "{label} {row:?} outside {window:?}"
            );
            // A label cut short is drawn with an ellipsis in place of its end.
            let cut = h.output().shapes.iter().any(|c| match &c.shape {
                egui::Shape::Text(t) => {
                    let text = t.galley.text().trim_end_matches('\u{2026}').to_string();
                    t.galley.text().ends_with('\u{2026}') && label.contains(&text)
                }
                _ => false,
            });
            assert!(!cut, "{label} is drawn cut at {w}x{ht}");
        }
        h.set_size(egui::vec2(1290.0, 830.0));
        settle(h);
    }
    // Map editor, Vehicle editor, Load save, then the recent documents.
    for _ in 0..2 {
        key(h, egui::Key::ArrowDown);
    }
    for (i, (label, description)) in rows.iter().enumerate() {
        key(h, egui::Key::ArrowDown);
        shot(h, &format!("recent-1b-selected-{i}"));
        assert!(
            find(h, description).is_some(),
            "no {description} on {label}: {:?}",
            labels(h)
        );
    }
    dump(h, "the editor tab");
}

#[test]
#[ignore = "requires BN_ENGINE pointing at an engine binary"]
fn the_main_menu_reopens_a_recent_map_file_in_the_map_editor() {
    let (mut h, file) = start_with_recent();
    editor_tab_lists(
        &mut h,
        &[
            (&file, "Open it in the map editor with"),
            ("bicycle", "Open it in the vehicle editor with"),
        ],
    );
    click(&mut h, &file);
    rest(&mut h);
    shot(&mut h, "recent-2-map-mods-asks");
    // The mod list: Escape leaves it with the mods the document was opened with, and the engine asks to open.
    key(&mut h, egui::Key::Escape);
    shot(&mut h, "recent-3-map-confirm");
    click(&mut h, "Yes");
    wait_kind(&mut h, "editor");
    rest(&mut h);
    shot(&mut h, "recent-4-map-opened");
    dump(&h, "the map editor");
    // The status line names the file by its path under the game's folder.
    let opened = labels(&h);
    assert!(
        opened
            .iter()
            .filter_map(|l| l.split_once(' ').map(|(_, text)| text))
            .any(|l| l.ends_with("house01.json") && file.ends_with(l)),
        "{opened:?}"
    );
    assert!(canvas_glyphs(&h).len() > 1, "no canvas of the opened file");
}

#[test]
#[ignore = "requires BN_ENGINE pointing at an engine binary"]
fn the_main_menu_reopens_a_recent_vehicle_in_the_vehicle_editor() {
    let (mut h, file) = start_with_recent();
    editor_tab_lists(
        &mut h,
        &[
            (&file, "Open it in the map editor with"),
            ("bicycle", "Open it in the vehicle editor with"),
        ],
    );
    click(&mut h, "bicycle");
    rest(&mut h);
    shot(&mut h, "recent-2-vehicle-mods-asks");
    key(&mut h, egui::Key::Escape);
    click(&mut h, "Yes");
    wait_kind(&mut h, "vehicle_editor");
    rest(&mut h);
    shot(&mut h, "recent-3-vehicle-opened");
    dump(&h, "the vehicle editor");
    let opened = labels(&h).join("\n");
    assert!(opened.contains("Bicycle"), "{opened}");
}

#[test]
#[ignore = "requires BN_ENGINE pointing at an engine binary"]
fn the_map_editor_exports_two_squares_stamps_pasted_mapgen_and_takes_it_back() {
    let sizes = [(1280.0, 800.0)];
    let dir = std::env::temp_dir().join(format!("bn-walk-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let (mut h, _root) = start(&dir);
    h.set_size(egui::vec2(1280.0, 800.0));
    rest(&mut h);
    open_map_editor(&mut h);
    shot_at(&mut h, "imp-1-editor", &sizes);
    dump(&h, "the map editor");
    let unstamp_enabled = |h: &H| {
        h.state()
            .model
            .screens()
            .last()
            .and_then(|s| s.affordances.iter().find(|a| a.id == "editor:unstamp"))
            .map(|a| a.enabled)
    };
    assert_eq!(unstamp_enabled(&h), Some(false));

    // The export is two squares wide: the count prompt takes its digit and OK.
    click_footer(&mut h, "Export width");
    shot_at(&mut h, "imp-2-width-asks", &sizes);
    replace_text(&mut h, "2");
    shot_at(&mut h, "imp-3-width-typed", &sizes);
    click(&mut h, "OK");
    click_footer(&mut h, "Export width");
    shot_at(&mut h, "imp-4-width-kept", &sizes);
    click(&mut h, "Cancel");

    // Exported to a new mod file, whose first entry names the avatar's overmap terrain.
    let path = crate::common::live::profile_dir().join("mods/zz_export.json");
    click_footer(&mut h, "Export mapgen here");
    type_text(&mut h, &path.to_string_lossy());
    shot_at(&mut h, "imp-5-path", &sizes);
    key(&mut h, egui::Key::Enter);
    shot_at(&mut h, "imp-6-exported", &sizes);
    let exported = std::fs::read_to_string(&path).expect("the export file");
    let json: serde_json::Value = serde_json::from_str(&exported).expect("json");
    assert!(
        json[0]["om_terrain"][0][0].is_string(),
        "no om_terrain: {exported}"
    );
    assert_eq!(json[0]["om_terrain"][0].as_array().map(Vec::len), Some(2));

    // The export file's own text, an array of entries, pasted into the import box and stamped by its button.
    let box_text = |h: &H| {
        h.root()
            .children_recursive()
            .filter(|n| n.accesskit_node().role() == egui::accesskit::Role::TextInput)
            .filter_map(|n| n.accesskit_node().value().map(|v| v.to_string()))
            .find(|v| v.contains("om_terrain"))
    };
    click_beside(&mut h, "Mapgen to stamp");
    h.event(egui::Event::Paste(exported.clone()));
    h.run_steps(3);
    rest(&mut h);
    shot_at(&mut h, "imp2-1-pasted", &sizes);
    // Leaving the box only stores the text: nothing is stamped yet.
    let title = find(&h, "Map Editor").expect("the window title");
    click_at(&mut h, title.center());
    rest(&mut h);
    assert_eq!(unstamp_enabled(&h), Some(false), "a blur stamped");
    assert!(find(&h, "Stamped.").is_none());
    click(&mut h, "Stamp text");
    rest(&mut h);
    shot_at(&mut h, "imp2-2-stamped", &sizes);
    assert!(find(&h, "Stamped.").is_some(), "{:?}", labels(&h));
    assert_eq!(unstamp_enabled(&h), Some(true), "{:?}", labels(&h));
    assert!(box_text(&h).is_some(), "the box shows the stored text");

    click_footer(&mut h, "Take back stamp");
    rest(&mut h);
    shot_at(&mut h, "imp2-3-taken-back", &sizes);
    assert_eq!(unstamp_enabled(&h), Some(false));
    // The text stays stored; a later blur does not stamp it again.
    assert!(box_text(&h).is_some(), "the box keeps the stored text");
    click_beside(&mut h, "Mapgen to stamp");
    let title = find(&h, "Map Editor").expect("the window title");
    click_at(&mut h, title.center());
    rest(&mut h);
    shot_at(&mut h, "imp2-4-blur-after", &sizes);
    assert_eq!(
        unstamp_enabled(&h),
        Some(false),
        "a later blur stamped again"
    );
}

#[test]
#[ignore = "requires BN_ENGINE pointing at an engine binary"]
fn the_map_editor_lets_turns_pass_and_opens_again_by_itself() {
    let sizes = [(1280.0, 800.0)];
    let dir = std::env::temp_dir().join(format!("bn-walk-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let (mut h, _root) = start(&dir);
    h.set_size(egui::vec2(1280.0, 800.0));
    rest(&mut h);
    open_map_editor(&mut h);
    let before = document(&h);
    shot_at(&mut h, "adv-1-editor", &sizes);

    click_footer(&mut h, "Let turns pass");
    shot_at(&mut h, "adv-2-count-asks", &sizes);
    replace_text(&mut h, "5");
    shot_at(&mut h, "adv-3-count-typed", &sizes);
    click(&mut h, "OK");
    // The editor closes, the avatar waits, and the editor comes back with its status.
    run_until(&mut h, "the editor reopened with its status", 600, |h| {
        find(h, "5 turns passed.").is_some()
    });
    rest(&mut h);
    shot_at(&mut h, "adv-4-passed", &sizes);
    dump(&h, "after letting turns pass");
    assert_eq!(document(&h), before, "the same document is back");
}

/// The squares the canvas marks faded, and its block size.
type Faded = (Vec<(u32, u32)>, Option<(u32, u32)>);

/// The squares the canvas on top marks faded, as (x, y), and its block size.
fn faded_squares(h: &H) -> Faded {
    let top = h.state().model.screens().last().unwrap();
    let Some(bn_client_egui::protocol::Component::Grid(grid)) =
        top.root.as_ref().unwrap().find("editor:canvas")
    else {
        panic!("no canvas");
    };
    let faded = grid
        .cells
        .iter()
        .filter(|c| c.faded == Some(true))
        .map(|c| (c.x, c.y))
        .collect();
    (faded, grid.block.map(|b| (b.x, b.y)))
}

/// A mod file of two overmap squares side by side and the roof of the first, open air on its west half, opened from
/// the recent documents as a player reopens a file.
#[test]
#[ignore = "requires BN_ENGINE pointing at an engine binary"]
fn the_map_editor_outlines_overmap_squares_and_shows_the_level_below_faded() {
    let sizes = [(1920.0, 1080.0)];
    let dir = std::env::temp_dir().join(format!("bn-walk-{}", std::process::id()));
    // The engine this test spawns opens files from its profile's user mods, and the recent documents name this one
    // before the engine runs.
    let profile = crate::common::live::reserve_profile();
    let moddir = profile.join("mods").join("walk_levels");
    std::fs::create_dir_all(&moddir).unwrap();
    std::fs::create_dir_all(&dir).unwrap();
    let ground = vec![".".repeat(48); 24];
    let roof = vec![format!("{}{}", " ".repeat(12), "r".repeat(12)); 24];
    let entries = serde_json::json!([
        { "type": "mapgen", "method": "json", "om_terrain": [["house_01", "house_04"]],
          "object": { "rows": ground, "terrain": { ".": "t_floor" } } },
        { "type": "mapgen", "method": "json", "om_terrain": "house_01_roof",
          "object": { "rows": roof, "terrain": { " ": "t_open_air", "r": "t_flat_roof" } } },
    ]);
    let file = moddir.join("levels.json");
    std::fs::write(&file, entries.to_string()).unwrap();
    let file = file.to_string_lossy().to_string();
    let recent = serde_json::json!([{ "editor": "map", "name": file, "mods": ["bn"] }]);
    let (mut h, _root) = start_with(&dir, &[("editor_recent.json", recent.to_string())]);
    assert_eq!(crate::common::live::profile_dir(), profile);
    h.set_size(egui::vec2(1920.0, 1080.0));
    rest(&mut h);
    click(&mut h, "Editor");
    rest(&mut h);
    click(&mut h, &file);
    rest(&mut h);
    key(&mut h, egui::Key::Escape);
    click(&mut h, "Yes");
    wait_kind(&mut h, "editor");
    rest(&mut h);
    shot_at(&mut h, "lv-1-ground", &sizes);
    dump(&h, "ground level");
    let (faded, block) = faded_squares(&h);
    assert_eq!(block, Some((24, 24)));
    assert!(faded.is_empty(), "the ground level shows nothing faded");
    assert_eq!(canvas_glyphs(&h)[0].chars().count(), 48);

    type_text(&mut h, "<");
    shot_at(&mut h, "lv-2-roof", &sizes);
    dump(&h, "roof level");
    assert!(document(&h).contains("\"Level\"=\"1\""), "{}", document(&h));
    let (faded, _) = faded_squares(&h);
    assert!(!faded.is_empty(), "the open air shows the floor below");
    assert!(faded.iter().all(|&(x, _)| x < 12), "{faded:?}");

    // A faded square takes a paint like any other.
    // The roof's own symbol is picked off a roof square with the eyedropper key, then drawn with the pencil key.
    type_text(&mut h, "i");
    paint(&mut h, 14, 3);
    type_text(&mut h, "b");
    assert_eq!(picked_symbol(&h).as_deref(), Some("symbol:r"));
    shot_at(&mut h, "lv-2b-roof-symbol-picked", &sizes);
    let before = canvas_glyphs(&h);
    paint(&mut h, 3, 3);
    // The engine answers a paint after the client went idle.
    run_until(&mut h, "the painted roof", 60, |h| {
        canvas_glyphs(h) != before
    });
    shot_at(&mut h, "lv-3-painted-on-faded", &sizes);
    assert_ne!(canvas_glyphs(&h), before, "the paint changed no square");
    assert_eq!(canvas_glyphs(&h)[3].chars().nth(3), Some('r'));

    type_text(&mut h, ">");
    shot_at(&mut h, "lv-4-back-down", &sizes);
    assert!(document(&h).contains("\"Level\"=\"0\""), "{}", document(&h));
    assert!(faded_squares(&h).0.is_empty());
}

/// A mod file of two mapgens that stand side by side in the `urban_1_house` city building, opened as a player
/// reopens a file; a click on the faded square beside the canvas puts that entry on it.
#[test]
#[ignore = "requires BN_ENGINE pointing at an engine binary"]
fn the_map_editor_shows_the_squares_beside_the_entry_and_a_click_on_one_opens_it() {
    let sizes = [(1920.0, 1080.0)];
    let dir = std::env::temp_dir().join(format!("bn-walk-{}", std::process::id()));
    let profile = crate::common::live::reserve_profile();
    let moddir = profile.join("mods").join("walk_neighbours");
    std::fs::create_dir_all(&moddir).unwrap();
    std::fs::create_dir_all(&dir).unwrap();
    let entry = |terrain: &str, symbol: &str, id: &str| {
        serde_json::json!({ "type": "mapgen", "method": "json", "om_terrain": terrain,
            "object": { "rows": vec![symbol.repeat(24); 24], "terrain": { symbol: id } } })
    };
    let entries = serde_json::json!([
        entry("urban_1_1", ".", "t_floor"),
        entry("urban_1_2", "#", "t_wall")
    ]);
    let file = moddir.join("neighbours.json");
    std::fs::write(&file, entries.to_string()).unwrap();
    let file = file.to_string_lossy().to_string();
    let recent = serde_json::json!([{ "editor": "map", "name": file, "mods": ["bn"] }]);
    let (mut h, _root) = start_with(&dir, &[("editor_recent.json", recent.to_string())]);
    assert_eq!(crate::common::live::profile_dir(), profile);
    h.set_size(egui::vec2(1920.0, 1080.0));
    rest(&mut h);
    click(&mut h, "Editor");
    rest(&mut h);
    click(&mut h, &file);
    rest(&mut h);
    key(&mut h, egui::Key::Escape);
    click(&mut h, "Yes");
    wait_kind(&mut h, "editor");
    rest(&mut h);
    shot_at(&mut h, "nb-1-first-entry", &sizes);
    dump(&h, "first entry");
    let (faded, block) = faded_squares(&h);
    assert_eq!(block, Some((24, 24)));
    assert!(!faded.is_empty(), "the square east of the entry is shown");
    assert!(faded.iter().all(|&(x, _)| x >= 24), "{faded:?}");
    let rows = canvas_glyphs(&h);
    assert_eq!(rows.len(), 24);
    assert!(rows[0].starts_with(&".".repeat(24)), "{}", rows[0]);
    assert!(rows[0].ends_with('#'), "{}", rows[0]);

    // A click on a faded square, as a player makes it, opens that entry.
    // A square of the sixth row: one a window of this height shows inside the canvas's own scroll area.
    let (x, y) = *faded.iter().find(|&&(x, y)| y == 6 && x > 24).unwrap();
    paint(&mut h, x as usize, y as usize);
    shot_at(&mut h, "nb-2-second-entry", &sizes);
    dump(&h, "second entry");
    let (faded, _) = faded_squares(&h);
    assert!(!faded.is_empty(), "the square west of the entry is shown");
    assert!(faded.iter().all(|&(x, _)| x < 24), "{faded:?}");
    let rows = canvas_glyphs(&h);
    assert!(rows[0].ends_with(&"#".repeat(24)), "{}", rows[0]);
    assert!(rows[0].trim_start().starts_with("....#"), "{}", rows[0]);
}

/// The regions the canvas on top outlines: id, (west, north), (east, south) and whether it is the picked one.
type Regions = Vec<(String, (i32, i32), (i32, i32), bool)>;

fn canvas_regions(h: &H) -> Regions {
    let top = h.state().model.screens().last().unwrap();
    let Some(bn_client_egui::protocol::Component::Grid(grid)) =
        top.root.as_ref().unwrap().find("editor:canvas")
    else {
        panic!("no canvas");
    };
    grid.regions
        .iter()
        .map(|r| {
            (
                r.id.clone(),
                (r.from.x, r.from.y),
                (r.to.x, r.to.y),
                r.selected == Some(true),
            )
        })
        .collect()
}

/// Presses on square `from`, moves the pointer through `via` and releases on the last one, a frame or two apart, as
/// a hand drags; the engine's answer is waited for.
fn drag_squares(
    h: &mut H,
    shot: &str,
    from: (usize, usize),
    via: &[(usize, usize)],
    grip: egui::Vec2,
) {
    let end = via.last().copied().unwrap_or(from);
    scroll_to(h, (from.0 + end.0) / 2, (from.1 + end.1) / 2);
    let at = square(h, from.0, from.1).center() + grip;
    h.hover_at(at);
    h.run_steps(2);
    let button = |h: &mut H, pos: egui::Pos2, pressed: bool| {
        h.event(egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        });
        h.run_steps(2);
    };
    button(h, at, true);
    let mut last = at;
    for &(x, y) in via {
        last = square(h, x, y).center();
        h.hover_at(last);
        h.run_steps(3);
    }
    // The picture is taken at the window's size, which a resize would change under the drag.
    if let Some(dir) = std::env::var_os("SHOT_DIR") {
        std::fs::create_dir_all(&dir).unwrap();
        let size = h.ctx.content_rect().size();
        let path = std::path::Path::new(&dir).join(format!("{shot}-{}x{}.png", size.x, size.y));
        h.render().expect("renders").save(path).unwrap();
    }
    button(h, last, false);
    rest(h);
}

/// A mod mapgen of a monster and an item placement, opened as a player reopens a file; the Placements tool drags the
/// monster whole and stretches the items by their corner.
#[test]
#[ignore = "requires BN_ENGINE pointing at an engine binary"]
fn the_map_editor_drags_a_placement_whole_and_by_its_corner() {
    let sizes = [(1290.0, 830.0)];
    let dir = std::env::temp_dir().join(format!("bn-walk-{}", std::process::id()));
    let profile = crate::common::live::reserve_profile();
    let moddir = profile.join("mods").join("walk_regions");
    std::fs::create_dir_all(&moddir).unwrap();
    std::fs::create_dir_all(&dir).unwrap();
    let entries = serde_json::json!([
        { "type": "mapgen", "method": "json", "om_terrain": "house_01",
          "object": { "rows": vec![".".repeat(24); 24], "terrain": { ".": "t_floor" },
            "place_monster": [{ "monster": "mon_zombie", "x": [2, 4], "y": [3, 4] }],
            "place_items": [{ "item": "bedroom", "x": [10, 14], "y": [10, 13], "chance": 100 }] } },
    ]);
    let file = moddir.join("regions.json");
    std::fs::write(&file, entries.to_string()).unwrap();
    let file = file.to_string_lossy().to_string();
    let recent = serde_json::json!([{ "editor": "map", "name": file, "mods": ["bn"] }]);
    let (mut h, _root) = start_with(&dir, &[("editor_recent.json", recent.to_string())]);
    assert_eq!(crate::common::live::profile_dir(), profile);
    h.set_size(egui::vec2(1290.0, 830.0));
    rest(&mut h);
    click(&mut h, "Editor");
    rest(&mut h);
    click(&mut h, &file);
    rest(&mut h);
    key(&mut h, egui::Key::Escape);
    click(&mut h, "Yes");
    wait_kind(&mut h, "editor");
    rest(&mut h);
    shot_at(&mut h, "pr-1-opened", &sizes);
    let before = canvas_regions(&h);
    println!("REGIONS before {before:?}");
    assert_eq!(before.len(), 2, "{before:?}");

    click_tab(&mut h, "Placements");
    rest(&mut h);
    shot_at(&mut h, "pr-2-placements-tool", &sizes);

    // The monster placement dragged whole, three squares east and two south.
    let monster = before
        .iter()
        .find(|r| r.0.contains("place_monster"))
        .unwrap();
    let (mx, my) = (monster.1.0 as usize + 1, monster.1.1 as usize);
    drag_squares(
        &mut h,
        "pr-3-preview-move",
        (mx, my),
        &[(mx + 1, my + 1), (mx + 3, my + 2)],
        egui::Vec2::ZERO,
    );
    let moved = canvas_regions(&h);
    println!("REGIONS after move {moved:?}");
    let now = moved.iter().find(|r| r.0 == monster.0).unwrap();
    assert_eq!(
        (now.1, now.2),
        (
            (monster.1.0 + 3, monster.1.1 + 2),
            (monster.2.0 + 3, monster.2.1 + 2)
        )
    );
    shot_at(&mut h, "pr-5-moved", &sizes);

    // The item placement stretched by its bottom-right corner, two squares east and south.
    let items = before.iter().find(|r| r.0.contains("place_items")).unwrap();
    let corner = (items.2.0 as usize, items.2.1 as usize);
    drag_squares(
        &mut h,
        "pr-4-preview-stretch",
        corner,
        &[(corner.0 + 1, corner.1 + 1), (corner.0 + 2, corner.1 + 2)],
        // The outer half of the corner square: a press in its middle moves the whole placement.
        egui::vec2(6.0, 6.0),
    );
    let stretched = canvas_regions(&h);
    println!("REGIONS after stretch {stretched:?}");
    let now = stretched.iter().find(|r| r.0 == items.0).unwrap();
    assert_eq!((now.1, now.2), (items.1, (items.2.0 + 2, items.2.1 + 2)));
    shot_at(&mut h, "pr-6-stretched", &sizes);

    // The engine's placements list and file hold the new ranges.
    dump(&h, "after the drags");
    key_mod(&mut h, egui::Modifiers::CTRL, egui::Key::S);
    let saved = std::fs::read_to_string(&file).unwrap();
    println!("SAVED {saved}");
    let saved = serde_json::from_str::<serde_json::Value>(&saved).unwrap();
    let object = &saved[0]["object"];
    assert_eq!(object["place_monster"][0]["x"], serde_json::json!([5, 7]));
    assert_eq!(object["place_monster"][0]["y"], serde_json::json!([5, 6]));
    assert_eq!(object["place_items"][0]["x"], serde_json::json!([10, 16]));
    assert_eq!(object["place_items"][0]["y"], serde_json::json!([10, 15]));
}

/// The label of the selected tab of the bar that has a tab labelled `of`.
fn selected_tool(h: &H) -> Option<String> {
    h.root().children_recursive().find_map(|n| {
        let a = n.accesskit_node();
        (a.role() == egui::accesskit::Role::Tab && a.is_selected() == Some(true))
            .then(|| a.label().map(|l| l.to_string()))
            .flatten()
            .filter(|l| ["Pencil", "Line", "Rectangle", "Select", "Paste"].contains(&l.as_str()))
    })
}

/// A mapgen opened as a player reopens a file; the tool keys choose the tools, and z turns the copied region.
#[test]
#[ignore = "requires BN_ENGINE pointing at an engine binary"]
fn the_map_editor_tool_keys_choose_tools_and_z_turns_the_clipboard() {
    let sizes = [(1290.0, 830.0)];
    let dir = std::env::temp_dir().join(format!("bn-walk-{}", std::process::id()));
    let profile = crate::common::live::reserve_profile();
    let moddir = profile.join("mods").join("walk_keys");
    std::fs::create_dir_all(&moddir).unwrap();
    std::fs::create_dir_all(&dir).unwrap();
    let entries = serde_json::json!([
        { "type": "mapgen", "method": "json", "om_terrain": "house_01",
          "object": { "rows": vec![".".repeat(24); 24], "terrain": { ".": "t_floor" } } },
    ]);
    let file = moddir.join("keys.json");
    std::fs::write(&file, entries.to_string()).unwrap();
    let file = file.to_string_lossy().to_string();
    let recent = serde_json::json!([{ "editor": "map", "name": file, "mods": ["bn"] }]);
    let (mut h, _root) = start_with(&dir, &[("editor_recent.json", recent.to_string())]);
    h.set_size(egui::vec2(1290.0, 830.0));
    rest(&mut h);
    click(&mut h, "Editor");
    rest(&mut h);
    click(&mut h, &file);
    rest(&mut h);
    key(&mut h, egui::Key::Escape);
    click(&mut h, "Yes");
    wait_kind(&mut h, "editor");
    rest(&mut h);
    assert_eq!(selected_tool(&h).as_deref(), Some("Pencil"));
    shot_at(&mut h, "tk-1-opened", &sizes);

    for (typed, tool) in [("l", "Line"), ("r", "Rectangle"), ("s", "Select")] {
        type_text(&mut h, typed);
        assert_eq!(selected_tool(&h).as_deref(), Some(tool), "after {typed}");
        shot_at(&mut h, &format!("tk-2-{typed}"), &sizes);
    }

    // A 3x2 region selected by its two corners and copied; z turns it to 2x3.
    paint(&mut h, 2, 2);
    paint(&mut h, 4, 3);
    shot_at(&mut h, "tk-3-select", &sizes);
    key_mod(&mut h, egui::Modifiers::CTRL, egui::Key::C);
    println!("DOC after copy: {}", document(&h));
    assert!(
        document(&h).contains("\"Clipboard\"=\"3x2\""),
        "{}",
        document(&h)
    );
    shot_at(&mut h, "tk-4-copied", &sizes);
    type_text(&mut h, "z");
    println!("DOC after z: {}", document(&h));
    assert!(
        document(&h).contains("\"Clipboard\"=\"2x3\""),
        "{}",
        document(&h)
    );
    shot_at(&mut h, "tk-5-rotated", &sizes);
}

/// The canvas on top as (cell id, label, glyph), in the order the engine sent them.
fn canvas_cells(h: &H) -> Vec<(String, String, String)> {
    let top = h.state().model.screens().last().unwrap();
    let Some(bn_client_egui::protocol::Component::Grid(grid)) =
        top.root.as_ref().unwrap().find("editor:canvas")
    else {
        panic!("no canvas");
    };
    grid.cells
        .iter()
        .map(|c| {
            (
                c.id.clone().unwrap_or_default(),
                c.label.as_ref().map(|l| l.shown()).unwrap_or_default(),
                c.glyph.clone().unwrap_or_default(),
            )
        })
        .collect()
}

/// A mod file with one overmap special: a house and a field on the ground, the house's roof above it; opened as a
/// player reopens a file. `<` walks to the roof level and a click on a roof square opens its mapgen file.
#[test]
#[ignore = "requires BN_ENGINE pointing at an engine binary"]
fn the_map_editor_shows_an_overmap_special_by_level_and_a_click_on_a_square_opens_its_mapgen() {
    let sizes = [(1920.0, 1080.0)];
    let dir = std::env::temp_dir().join(format!("bn-walk-{}", std::process::id()));
    let profile = crate::common::live::reserve_profile();
    let moddir = profile.join("mods").join("walk_special");
    std::fs::create_dir_all(&moddir).unwrap();
    std::fs::create_dir_all(&dir).unwrap();
    let entries = serde_json::json!([{
        "type": "overmap_special", "id": "walk_special", "subtype": "fixed",
        "locations": ["land"], "city_distance": [0, 20], "city_sizes": [1, 20], "occurrences": [0, 1],
        "overmaps": [
            { "point": [0, 0, 0], "overmap": "house_01_north" },
            { "point": [1, 0, 0], "overmap": "field" },
            { "point": [0, 0, 1], "overmap": "house_01_roof_north" }
        ]
    }]);
    let file = moddir.join("special.json");
    std::fs::write(&file, entries.to_string()).unwrap();
    let file = file.to_string_lossy().to_string();
    let recent = serde_json::json!([{ "editor": "map", "name": file, "mods": ["bn"] }]);
    let (mut h, _root) = start_with(&dir, &[("editor_recent.json", recent.to_string())]);
    assert_eq!(crate::common::live::profile_dir(), profile);
    h.set_size(egui::vec2(1920.0, 1080.0));
    rest(&mut h);
    click(&mut h, "Editor");
    rest(&mut h);
    click(&mut h, &file);
    rest(&mut h);
    key(&mut h, egui::Key::Escape);
    click(&mut h, "Yes");
    wait_kind(&mut h, "editor");
    rest(&mut h);
    shot_at(&mut h, "sp-1-ground", &sizes);
    dump(&h, "ground level");
    let cells = canvas_cells(&h);
    println!("ground cells {cells:?}");
    assert_eq!(
        cells.iter().map(|c| c.0.as_str()).collect::<Vec<_>>(),
        ["square:0,0", "square:1,0"]
    );
    assert!(cells.iter().all(|c| !c.1.is_empty() && !c.2.is_empty()));
    assert!(document(&h).contains("\"Level\"=\"0\""), "{}", document(&h));

    type_text(&mut h, "<");
    shot_at(&mut h, "sp-2-roof", &sizes);
    dump(&h, "roof level");
    assert!(document(&h).contains("\"Level\"=\"1\""), "{}", document(&h));
    let roof = canvas_cells(&h);
    println!("roof cells {roof:?}");
    assert_eq!(roof.len(), 1);
    assert_eq!(roof[0].0, "square:0,0");

    click(&mut h, &roof[0].1);
    wait_kind(&mut h, "editor");
    rest(&mut h);
    shot_at(&mut h, "sp-3-opened", &sizes);
    dump(&h, "opened mapgen");
    let path = field(&h, "editor:file", "path");
    assert!(path.ends_with("house01.json"), "{path}");
}

/// The grid cells of the canvas on top as (x, y, id, label, glyph).
type Cells = Vec<(u32, u32, String, String, String)>;

fn canvas_grid(h: &H) -> (u32, u32, Cells) {
    let top = h.state().model.screens().last().unwrap();
    let Some(bn_client_egui::protocol::Component::Grid(grid)) =
        top.root.as_ref().unwrap().find("editor:canvas")
    else {
        panic!("no canvas");
    };
    let cells = grid
        .cells
        .iter()
        .map(|c| {
            (
                c.x,
                c.y,
                c.id.clone().unwrap_or_default(),
                c.label.as_ref().map(|l| l.shown()).unwrap_or_default(),
                c.glyph.clone().unwrap_or_default(),
            )
        })
        .collect();
    (grid.cols, grid.rows, cells)
}

/// Where the canvas square (`x`, `y`) is drawn, from the square of the cell `anchor` (its grid x, y and label).
fn square_from(h: &H, anchor: (u32, u32, &str), x: u32, y: u32) -> egui::Rect {
    let at = find(h, anchor.2).unwrap_or_else(|| panic!("no anchor {}: {:?}", anchor.2, labels(h)));
    at.translate(egui::vec2(
        (x as f32 - anchor.0 as f32) * at.width(),
        (y as f32 - anchor.1 as f32) * at.height(),
    ))
}

/// The ids of the tabs of the bar that has the tab `of`, and the label of the selected one.
fn tabs_of(h: &H, of: &str) -> (Vec<String>, Option<String>) {
    let tabs = h
        .root()
        .children_recursive()
        .filter(|n| n.accesskit_node().role() == egui::accesskit::Role::Tab)
        .map(|n| {
            (
                n.accesskit_node().label().unwrap_or_default().to_string(),
                n.accesskit_node().is_selected() == Some(true),
            )
        })
        .collect::<Vec<_>>();
    assert!(tabs.iter().any(|t| t.0 == of), "no tab {of}: {tabs:?}");
    let selected = tabs.iter().find(|t| t.1).map(|t| t.0.clone());
    (tabs.into_iter().map(|t| t.0).collect(), selected)
}

/// A mod file with one overmap special: a house and a field. Point draws a forest from the catalog on a square of
/// the room around the special and on one beside it with a drag, one undo takes the drag back, erase removes a
/// square, and the saved file holds the result.
#[test]
#[ignore = "requires BN_ENGINE pointing at an engine binary"]
fn the_map_editor_draws_and_erases_the_squares_of_an_overmap_special_and_saves_them() {
    let sizes = [(1920.0, 1080.0)];
    let dir = std::env::temp_dir().join(format!("bn-walk-{}", std::process::id()));
    let profile = crate::common::live::reserve_profile();
    let moddir = profile.join("mods").join("walk_draw");
    std::fs::create_dir_all(&moddir).unwrap();
    std::fs::create_dir_all(&dir).unwrap();
    let entries = serde_json::json!([{
        "type": "overmap_special", "id": "walk_draw", "subtype": "fixed",
        "locations": ["land"], "city_distance": [0, 20], "city_sizes": [1, 20], "occurrences": [0, 1],
        "overmaps": [
            { "point": [0, 0, 0], "overmap": "house_01_north" },
            { "point": [1, 0, 0], "overmap": "field" }
        ]
    }]);
    let file = moddir.join("draw.json");
    std::fs::write(&file, entries.to_string()).unwrap();
    let path = file.to_string_lossy().to_string();
    let recent = serde_json::json!([{ "editor": "map", "name": path, "mods": ["bn"] }]);
    let (mut h, _root) = start_with(&dir, &[("editor_recent.json", recent.to_string())]);
    h.set_size(egui::vec2(1920.0, 1080.0));
    rest(&mut h);
    click(&mut h, "Editor");
    rest(&mut h);
    click(&mut h, &path);
    rest(&mut h);
    key(&mut h, egui::Key::Escape);
    click(&mut h, "Yes");
    wait_kind(&mut h, "editor");
    rest(&mut h);
    shot_at(&mut h, "sd-1-opened", &sizes);
    dump(&h, "opened");
    println!("grid {:?}", canvas_grid(&h));
    assert_eq!(tabs_of(&h, "Open mapgen").1.as_deref(), Some("Open mapgen"));
    type_text(&mut h, "b");
    rest(&mut h);
    assert_eq!(tabs_of(&h, "Pencil").1.as_deref(), Some("Pencil"));
    let (cols, rows, cells) = canvas_grid(&h);
    assert_eq!((cols, rows, cells.len()), (4, 3, 12), "{cells:?}");
    assert_eq!(
        cells
            .iter()
            .filter(|c| c.3.is_empty() && c.4 == " ")
            .count(),
        10
    );
    shot_at(&mut h, "sd-2-point", &sizes);

    // The catalog, filtered to one terrain, picks it for the pencil.
    let first_search = |h: &H| {
        h.root()
            .children_recursive()
            .find(|n| {
                let a = n.accesskit_node();
                a.role() == egui::accesskit::Role::TextInput
                    && a.label().as_deref() == Some("Search")
            })
            .map(|n| n.rect())
    };
    let at = click_stable(&mut h, "catalog search", first_search);
    click_at(&mut h, at.center());
    type_text(&mut h, "forest_thick");
    rest(&mut h);
    shot_at(&mut h, "sd-3-catalog", &sizes);
    dump(&h, "catalog filtered");
    double_click(&mut h, "forest_thick");
    println!("DOC after pick: {}", document(&h));
    assert!(document(&h).contains("forest_thick"), "{}", document(&h));
    shot_at(&mut h, "sd-4-picked", &sizes);

    // A drag from the field into the room beside it, one stroke.
    let anchor_of = |h: &H| {
        let (_, _, cells) = canvas_grid(h);
        let house = cells.iter().find(|c| c.3 == "house").expect("a house");
        (house.0, house.1)
    };
    let at = |h: &H, x: u32, y: u32| {
        let (ax, ay) = anchor_of(h);
        square_from(h, (ax, ay, "house"), x, y).center()
    };
    let (hx, hy) = anchor_of(&h);
    let (from, to) = (at(&h, hx + 1, hy), at(&h, hx + 2, hy));
    let before = canvas_grid(&h);
    h.hover_at(from);
    h.run_steps(2);
    let button = |h: &mut H, pos: egui::Pos2, pressed: bool| {
        h.event(egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        });
        h.run_steps(2);
    };
    button(&mut h, from, true);
    h.hover_at(to);
    h.run_steps(3);
    button(&mut h, to, false);
    rest(&mut h);
    shot_at(&mut h, "sd-5-dragged", &sizes);
    let (_, _, drawn) = canvas_grid(&h);
    println!("after drag {drawn:?}");
    let forests = |cells: &[(u32, u32, String, String, String)]| {
        cells
            .iter()
            .filter(|c| c.2 == "square:1,0" || c.2 == "square:2,0")
            .map(|c| c.3.clone())
            .collect::<Vec<_>>()
    };
    assert!(
        forests(&drawn)
            .iter()
            .all(|l| !l.is_empty() && l != "field"),
        "{drawn:?}"
    );

    // One undo takes both squares back.
    click_footer(&mut h, "Undo");
    rest(&mut h);
    shot_at(&mut h, "sd-6-undone", &sizes);
    let (_, _, undone) = canvas_grid(&h);
    assert_eq!(forests(&undone), forests(&before.2), "{undone:?}");

    // The drag again, then the eraser on the house.
    let (hx, hy) = anchor_of(&h);
    let (from, to) = (at(&h, hx + 1, hy), at(&h, hx + 2, hy));
    h.hover_at(from);
    h.run_steps(2);
    button(&mut h, from, true);
    h.hover_at(to);
    h.run_steps(3);
    button(&mut h, to, false);
    rest(&mut h);
    type_text(&mut h, "e");
    rest(&mut h);
    assert_eq!(tabs_of(&h, "Eraser").1.as_deref(), Some("Eraser"));
    let (hx, hy) = anchor_of(&h);
    let house = at(&h, hx, hy);
    click_at(&mut h, house);
    shot_at(&mut h, "sd-7-erased", &sizes);
    let (_, _, erased) = canvas_grid(&h);
    println!("after erase {erased:?}");
    assert!(erased.iter().all(|c| c.3 != "house"), "{erased:?}");

    // Saved, the file holds the forest squares and not the house.
    key_mod(&mut h, egui::Modifiers::CTRL, egui::Key::S);
    rest(&mut h);
    shot_at(&mut h, "sd-8-saved", &sizes);
    assert!(
        document(&h).contains("\"Saved\"=\"Yes\""),
        "{}",
        document(&h)
    );
    let saved = std::fs::read_to_string(&file).unwrap();
    println!("saved {saved}");
    let saved = serde_json::from_str::<serde_json::Value>(&saved).unwrap();
    let overmaps = saved[0]["overmaps"].as_array().unwrap();
    assert!(
        overmaps.iter().all(|o| o["overmap"] != "house_01_north"),
        "{overmaps:?}"
    );
    assert_eq!(
        overmaps
            .iter()
            .filter(|o| o["overmap"] == "forest_thick")
            .count(),
        2,
        "{overmaps:?}"
    );
}

/// Whether the editor screen on top offers the affordance `id` and has it enabled.
fn enabled(h: &H, id: &str) -> Option<bool> {
    let top = h.state().model.screens().last()?;
    top.affordances
        .iter()
        .find(|a| a.id == id)
        .map(|a| a.enabled)
}

/// A mod file with one overmap special, opened as a player reopens a file. "Place in world" puts it on land near the
/// avatar and enables "Test", which leaves the editor for the map with the avatar moved to the special's origin.
#[test]
#[ignore = "requires BN_ENGINE pointing at an engine binary"]
fn the_map_editor_places_an_overmap_special_in_the_world_and_tests_it_there() {
    let sizes = [(1920.0, 1080.0)];
    let dir = std::env::temp_dir().join(format!("bn-walk-{}", std::process::id()));
    let profile = crate::common::live::reserve_profile();
    let moddir = profile.join("mods").join("walk_place");
    std::fs::create_dir_all(&moddir).unwrap();
    std::fs::create_dir_all(&dir).unwrap();
    let entries = serde_json::json!([{
        "type": "overmap_special", "id": "walk_place", "subtype": "fixed",
        "locations": ["land"], "city_distance": [0, 20], "city_sizes": [1, 20], "occurrences": [0, 1],
        "overmaps": [
            { "point": [0, 0, 0], "overmap": "house_01_north" },
            { "point": [1, 0, 0], "overmap": "field" }
        ]
    }]);
    let file = moddir.join("place.json");
    std::fs::write(&file, entries.to_string()).unwrap();
    let path = file.to_string_lossy().to_string();
    let recent = serde_json::json!([{ "editor": "map", "name": path, "mods": ["bn"] }]);
    let (mut h, _root) = start_with(&dir, &[("editor_recent.json", recent.to_string())]);
    h.set_size(egui::vec2(1920.0, 1080.0));
    rest(&mut h);
    let before = h.state().model.origin();
    println!("view origin before {before:?}");

    click(&mut h, "Editor");
    rest(&mut h);
    click(&mut h, &path);
    rest(&mut h);
    key(&mut h, egui::Key::Escape);
    click(&mut h, "Yes");
    wait_kind(&mut h, "editor");
    rest(&mut h);
    shot_at(&mut h, "ep-1-opened", &sizes);
    dump(&h, "opened");
    assert_eq!(enabled(&h, "editor:place_special"), Some(true));
    assert_eq!(enabled(&h, "editor:test"), Some(false));

    click_footer(&mut h, "Place in world");
    rest(&mut h);
    shot_at(&mut h, "ep-2-placed", &sizes);
    dump(&h, "placed");
    assert_eq!(enabled(&h, "editor:test"), Some(true));

    click_footer(&mut h, "Test");
    run_until(&mut h, "the map", 60, |h| {
        !h.state().model.busy()
            && h.state()
                .model
                .screens()
                .last()
                .is_none_or(|s| s.kind.as_deref() != Some("editor"))
    });
    rest(&mut h);
    shot_at(&mut h, "ep-3-tested", &sizes);
    let after = h.state().model.origin();
    println!("view origin after {after:?}");

    assert_ne!(after, before);
}

/// A wall symbol `#` mapped to the catalog's brick wall in the open mapgen, as the first walk maps it.
fn map_wall(h: &mut H) {
    click(h, "Terrain");
    double_click(h, "brick wall");
    click_beside(h, "Symbol");
    replace_text(h, "#");
    key(h, egui::Key::Enter);
    click(h, "Set mapping");
    // The legend row picks the symbol the canvas draws with.
    let cell = legend_cell(h, "#", "terrain");
    double_click(h, &cell);
    assert_eq!(picked_symbol(h).as_deref(), Some("symbol:#"));
}

/// A region copied from one mapgen file and cut from another is pasted into a third, which maps nothing of it.
#[test]
#[ignore = "requires BN_ENGINE pointing at an engine binary"]
fn the_map_editor_copies_and_cuts_between_two_open_documents_and_pastes() {
    let sizes = [(1280.0, 800.0), (1920.0, 1080.0)];
    let dir = std::env::temp_dir().join(format!("bn-walk-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let (mut h, _root) = start(&dir);
    // A window tall enough for the canvas to show the squares the walk aims at.
    h.set_size(egui::vec2(1920.0, 1080.0));
    rest(&mut h);
    open_map_editor(&mut h);

    // Document A: a wall bar, saved, with a 3x2 region of it copied.
    let a = create_mapgen(&mut h, "zz_xdoc_a");
    map_wall(&mut h);
    click(&mut h, "Rectangle");
    paint(&mut h, 3, 1);
    paint(&mut h, 9, 2);
    type_text(&mut h, "s");
    paint(&mut h, 2, 1);
    paint(&mut h, 4, 2);
    key_mod(&mut h, egui::Modifiers::CTRL, egui::Key::C);
    assert!(
        document(&h).contains("\"Clipboard\"=\"3x2\""),
        "{}",
        document(&h)
    );
    key_mod(&mut h, egui::Modifiers::CTRL, egui::Key::S);
    assert!(
        document(&h).contains("\"Saved\"=\"Yes\""),
        "{}",
        document(&h)
    );
    shot_at(&mut h, "xdoc-1-a-copied", &sizes);

    // Document B is another file, made in the same editor: the clipboard stays, and B maps no `#` of its own.
    let b = create_mapgen(&mut h, "zz_xdoc_b");
    assert_ne!(a, b);
    dump(&h, "B created");
    shot_at(&mut h, "xdoc-2-b-opened", &sizes);
    assert!(
        !crate::live_editor::rows(&h, "editor:legend").contains(&"symbol:#".to_string()),
        "B must start without the wall"
    );
    assert!(
        document(&h).contains("\"Clipboard\"=\"3x2\""),
        "{}",
        document(&h)
    );
    shot_at(&mut h, "xdoc-2-b-opened", &sizes);

    // Pasted with the paste tool on one square, high on the canvas where the window shows it: the wall squares arrive with their mapping.
    type_text(&mut h, "v");
    assert_eq!(selected_tool(&h).as_deref(), Some("Paste"));
    paint(&mut h, 10, 4);
    // The engine answers a paint after the client went idle.
    run_until(&mut h, "the paste", 60, |h| glyph(h, 11, 4) == Some('#'));
    shot_at(&mut h, "xdoc-3-b-pasted", &sizes);
    // The copied region is `.##` over `.##`: its top-left square lands on the one pasted on.
    for (x, y) in [(11, 4), (12, 4), (11, 5), (12, 5)] {
        assert_eq!(glyph(&h, x, y), Some('#'), "pasted square {x},{y}");
    }
    assert_ne!(glyph(&h, 10, 4), Some('#'));
    assert_ne!(glyph(&h, 13, 4), Some('#'));
    assert!(
        crate::live_editor::rows(&h, "editor:legend").contains(&"symbol:#".to_string()),
        "the paste brings the mapping"
    );
    let pasted = canvas_glyphs(&h);

    // One undo takes the whole paste back; redo brings it again.
    key_mod(&mut h, egui::Modifiers::CTRL, egui::Key::Z);
    assert_ne!(glyph(&h, 11, 4), Some('#'));
    key_mod(&mut h, egui::Modifiers::CTRL, egui::Key::Y);
    assert_eq!(canvas_glyphs(&h), pasted);

    // Cut from B: the region leaves B and the cut region is on the clipboard.
    type_text(&mut h, "s");
    paint(&mut h, 11, 4);
    paint(&mut h, 12, 5);
    key_mod(&mut h, egui::Modifiers::CTRL, egui::Key::X);
    run_until(&mut h, "the cut", 60, |h| glyph(h, 11, 4) != Some('#'));
    shot_at(&mut h, "xdoc-4-b-cut", &sizes);
    assert_ne!(glyph(&h, 11, 4), Some('#'), "cut leaves the square");
    assert!(
        canvas_glyphs(&h).iter().all(|r| !r.contains('#')),
        "{:?}",
        canvas_glyphs(&h)
    );
    assert!(
        document(&h).contains("\"Clipboard\"=\"2x2\""),
        "{}",
        document(&h)
    );
    key_mod(&mut h, egui::Modifiers::CTRL, egui::Key::S);
    let saved = std::fs::read_to_string(&b).unwrap();
    assert!(saved.contains("t_brick_wall"), "{saved}");
}

/// A nested chunk made from the form, and a mapgen whose own legend moves into a new palette.
#[test]
#[ignore = "requires BN_ENGINE pointing at an engine binary"]
fn the_map_editor_makes_a_nested_chunk_and_moves_a_legend_into_a_palette() {
    let sizes = [(1280.0, 800.0), (1920.0, 1080.0)];
    let dir = std::env::temp_dir().join(format!("bn-walk-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let (mut h, _root) = start(&dir);
    h.set_size(egui::vec2(1280.0, 800.0));
    rest(&mut h);
    open_map_editor(&mut h);

    // The form's kind select names a nested chunk, relabels the id field and sizes the chunk.
    click(&mut h, "Mapgen");
    click(&mut h, "Nested chunk");
    assert!(find(&h, "Chunk id").is_some(), "{:?}", labels(&h));
    let nested = crate::common::live::profile_dir().join("mods/zz_nested.json");
    click_beside(&mut h, "New file/Path");
    replace_text(&mut h, &nested.to_string_lossy());
    click_beside(&mut h, "Chunk id");
    replace_text(&mut h, "zz_walk_chunk");
    click_beside(&mut h, "Chunk size");
    replace_text(&mut h, "6");
    shot_at(&mut h, "nc-1-form", &sizes);
    click(&mut h, "Create");
    if find(&h, "Continue").is_some() {
        click(&mut h, "Continue");
    }
    wait_kind(&mut h, "editor");
    rest(&mut h);
    shot_at(&mut h, "nc-2-created", &sizes);
    let chunk = canvas_glyphs(&h);
    assert_eq!((chunk.len(), chunk[0].chars().count()), (6, 6));
    let saved = std::fs::read_to_string(&nested).unwrap();
    assert!(
        saved.contains("zz_walk_chunk") && saved.contains("nested"),
        "{saved}"
    );

    // A mapgen with a wall symbol of its own, whose legend moves into a palette the mapgen then names.
    click(&mut h, "Nested chunk");
    click(&mut h, "Mapgen");
    let path = create_mapgen(&mut h, "zz_extract");
    map_wall(&mut h);
    shot_at(&mut h, "nc-3-legend", &sizes);
    click_footer(&mut h, "Extract legend to palette");
    shot_at(&mut h, "nc-4-extract-asks", &sizes);
    type_text(&mut h, "zz_walk_extracted");
    key(&mut h, egui::Key::Enter);
    rest(&mut h);
    shot_at(&mut h, "nc-5-extracted", &sizes);
    key_mod(&mut h, egui::Modifiers::CTRL, egui::Key::S);
    let saved = std::fs::read_to_string(&path).unwrap();
    let json = serde_json::from_str::<serde_json::Value>(&saved).unwrap();
    assert!(
        saved.contains("\"palettes\"") && saved.contains("zz_walk_extracted"),
        "{saved}"
    );
    let palette = json
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["type"] == "palette")
        .unwrap_or_else(|| panic!("no palette entry: {saved}"));
    assert_eq!(palette["id"], "zz_walk_extracted");
    assert_eq!(palette["terrain"]["#"], "t_brick_wall");
    assert!(
        json[0]["object"]
            .get("terrain")
            .is_none_or(|t| t.get("#").is_none()),
        "{saved}"
    );

    // One undo takes the whole move back.
    key_mod(&mut h, egui::Modifiers::CTRL, egui::Key::Z);
    assert!(
        crate::live_editor::rows(&h, "editor:legend").contains(&"symbol:#".to_string()),
        "the wall is still mapped after the undo"
    );
}

/// The in-game path: the toolbar's Map Editor answers without the Debug Editor trait, and opens with it.
#[test]
#[ignore = "requires BN_ENGINE pointing at an engine binary"]
fn the_toolbar_offers_the_map_editor_in_game_only_with_the_debug_trait() {
    use crate::live_editor::{invoke, row, text};
    use bn_client_egui::protocol::{InvokeArgs, ScreenRole};
    let sizes = [(1280.0, 800.0), (1920.0, 1080.0)];
    let dir = std::env::temp_dir().join(format!("bn-walk-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let (mut h, _root) = start(&dir);
    h.set_size(egui::vec2(1280.0, 800.0));
    rest(&mut h);
    invoke(&mut h, "menu:tab", row("new_game"));
    invoke(&mut h, "menu:entry", row("new_game:play_now_default"));
    run_until(&mut h, "the map", 900, |h| h.state().model.size().x > 0);
    run_until(&mut h, "the settled game", 300, |h| {
        h.state().model.menu().is_none() && !h.state().model.busy()
    });
    crate::common::live::dismiss_popups(&mut h);
    settle(&mut h);
    shot_at(&mut h, "ig-1-map", &sizes);

    // Without the trait the toolbar's button says so.
    click(&mut h, "Map Editor");
    shot_at(&mut h, "ig-2-denied", &sizes);
    assert!(
        find(&h, "You need the Debug Editor trait.").is_some(),
        "{:?}",
        labels(&h)
    );
    click(&mut h, "OK");

    // The trait is given through the Lua console; then the same button opens the editor, and Escape returns.
    assert!(h.state_mut().model.invoke_action("lua_console"));
    wait_kind(&mut h, "lua_console");
    invoke(&mut h, "EDIT", InvokeArgs::default());
    wait_kind(&mut h, "text_editor");
    invoke(
        &mut h,
        "editor:set",
        text(r#"gapi.get_avatar():set_mutation(MutationBranchId.new("DEBUG_EDITOR"))"#),
    );
    invoke(&mut h, "TEXT.CONFIRM", InvokeArgs::default());
    wait_kind(&mut h, "lua_console");
    invoke(&mut h, "QUIT", InvokeArgs::default());
    run_until(&mut h, "the map", 60, |h| {
        h.state()
            .model
            .screens()
            .last()
            .is_some_and(|s| s.role == ScreenRole::Map)
    });
    click(&mut h, "Map Editor");
    wait_kind(&mut h, "editor");
    rest(&mut h);
    shot_at(&mut h, "ig-3-editor", &sizes);

    // The mapgen of the square the avatar stands on opens from the footer.
    click_footer(&mut h, "Open mapgen here");
    rest(&mut h);
    shot_at(&mut h, "ig-4-here", &sizes);
    dump(&h, "mapgen here");
    key(&mut h, egui::Key::Escape);
    rest(&mut h);
    shot_at(&mut h, "ig-5-back-on-the-map", &sizes);
    assert!(
        h.state()
            .model
            .screens()
            .last()
            .is_some_and(|s| s.role == ScreenRole::Map),
        "Escape did not return to the map"
    );
}

/// The cells of the grid `id` on top: their squares and glyphs.
fn grid_cells(h: &H, id: &str) -> Vec<((u32, u32), String)> {
    let top = h.state().model.screens().last().unwrap();
    let Some(bn_client_egui::protocol::Component::Grid(grid)) = top.root.as_ref().unwrap().find(id)
    else {
        panic!("no {id}");
    };
    grid.cells
        .iter()
        .map(|c| ((c.x, c.y), c.glyph.clone().unwrap_or_default()))
        .collect()
}

/// Clicks the square of the vehicle editor's grid that holds a part named `part`, among others.
fn click_square(h: &mut H, part: &str) {
    let square = |h: &H| {
        h.root()
            .children_recursive()
            .find(|n| {
                let a = n.accesskit_node();
                a.role() == egui::accesskit::Role::Button
                    && a.label()
                        .is_some_and(|l| l.split(", ").any(|p| p == part) && l.contains(", "))
            })
            .map(|n| n.rect())
    };
    let at = click_stable(h, part, square);
    click_at(h, at.center());
    rest(h);
}

/// Double-clicks the row of the vehicle editor's stack named `part`, not a square that names it among its parts.
fn pick_stacked(h: &mut H, part: &str) {
    let row = |h: &H| {
        h.root()
            .children_recursive()
            .find(|n| n.accesskit_node().label().as_deref() == Some(part))
            .map(|n| n.rect())
    };
    let at = click_stable(h, part, row).center();
    h.hover_at(at);
    h.run_steps(2);
    tap(h, at);
    tap(h, at);
    rest(h);
}

/// Whether the vehicle editor's stack of parts on the picked mount has a part named `label`.
fn stacked(h: &H, label: &str) -> bool {
    let top = h.state().model.screens().last().unwrap();
    let Some(bn_client_egui::protocol::Component::List(stack)) =
        top.root.as_ref().unwrap().find("vehicle:stack")
    else {
        return false;
    };
    stack.rows.iter().any(|r| r.label.shown() == label)
}

/// The text of the vehicle editor's notice, empty when it shows none.
fn vehicle_notice(h: &H) -> String {
    let top = h.state().model.screens().last().unwrap();
    serde_json::to_string(&top.root.as_ref().unwrap().find("vehicle:notice")).unwrap()
}

#[test]
#[ignore = "requires BN_ENGINE pointing at an engine binary"]
fn the_vehicle_editor_turns_a_preview_reloads_a_file_and_asks_before_leaving() {
    let sizes = [(1920.0, 1080.0), (1280.0, 800.0)];
    let dir = std::env::temp_dir().join(format!("bn-walk-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let (mut h, _root) = start(&dir);
    h.set_size(egui::vec2(1920.0, 1080.0));
    rest(&mut h);
    click(&mut h, "Editor");
    click(&mut h, "Vehicle editor");
    key(&mut h, egui::Key::Escape);
    click(&mut h, "Yes");
    wait_kind(&mut h, "vehicle_editor");
    rest(&mut h);
    click_input(&mut h, "Search");
    type_text(&mut h, "bicycle");
    key(&mut h, egui::Key::Enter);
    double_click(&mut h, "Bicycle");
    wait_kind(&mut h, "vehicle_editor");
    rest(&mut h);
    let path = crate::common::live::profile_dir().join("mods/zz_veh_reload.json");
    click_footer(&mut h, "Save as");
    type_text(&mut h, &path.to_string_lossy());
    key(&mut h, egui::Key::Enter);
    rest(&mut h);
    shot_at(&mut h, "vr-1-saved", &sizes);
    let text = std::fs::read_to_string(&path).unwrap();
    println!("FILE {text}");

    // The Spawn tab turns the design: the cells of the turned grid differ between facings 0 and 90.
    click_tab(&mut h, "Test spawn");
    rest(&mut h);
    shot_at(&mut h, "vr-2-spawn-tab", &sizes);
    dump(&h, "spawn tab");
    let north = grid_cells(&h, "vehicle:turned");
    assert!(!north.is_empty(), "no turned preview");
    click_beside(&mut h, "Facing");
    replace_text(&mut h, "90");
    key(&mut h, egui::Key::Enter);
    rest(&mut h);
    shot_at(&mut h, "vr-3-spawn-turned", &sizes);
    let east = grid_cells(&h, "vehicle:turned");
    println!("TURNED {north:?} -> {east:?}");
    assert_ne!(north, east, "the preview did not turn");
    // A facing past the last degree is refused in words naming the field's bounds, and the facing stays.
    click_beside(&mut h, "Facing");
    replace_text(&mut h, "400");
    key(&mut h, egui::Key::Enter);
    rest(&mut h);
    shot_at(&mut h, "vr-3b-facing-refused", &sizes);
    assert!(
        labels(&h).iter().any(|l| l.contains("from 0 to 359")),
        "no refusal shown"
    );
    assert_eq!(grid_cells(&h, "vehicle:turned"), east);

    // A clean document follows its file when a text editor changes it: the name on disk is the one shown.
    let on_disk =
        |name: &str| text.replacen("\"name\": \"Bicycle\"", &format!("\"name\": \"{name}\""), 1);
    assert_ne!(on_disk("Disk Bike"), text, "no name to change in {text}");
    std::fs::write(&path, on_disk("Disk Bike")).unwrap();
    click_tab(&mut h, "Parts here");
    rest(&mut h);
    shot_at(&mut h, "vr-4-reloaded", &sizes);
    dump(&h, "after the file changed");
    assert!(find(&h, "Disk Bike").is_some(), "{:?}", labels(&h));
    assert!(
        vehicle_notice(&h).contains("disk") || vehicle_notice(&h).contains("changed"),
        "{}",
        vehicle_notice(&h)
    );

    // An edited document keeps its edit, says so, and refuses to save over the other file.
    click_tab(&mut h, "Name and flags");
    click_beside(&mut h, "Name");
    replace_text(&mut h, "Mine");
    key(&mut h, egui::Key::Enter);
    rest(&mut h);
    std::fs::write(&path, on_disk("Disk Bike Again")).unwrap();
    click_tab(&mut h, "Parts here");
    rest(&mut h);
    shot_at(&mut h, "vr-5-kept-edit", &sizes);
    assert!(find(&h, "Mine").is_some(), "{:?}", labels(&h));
    assert!(
        find(&h, "Disk Bike Again").is_none(),
        "an edited document was overwritten"
    );
    println!("NOTICE {}", vehicle_notice(&h));
    key_mod(&mut h, egui::Modifiers::CTRL, egui::Key::S);
    shot_at(&mut h, "vr-6-save-refused", &sizes);
    assert!(
        !std::fs::read_to_string(&path).unwrap().contains("Mine"),
        "the save overwrote a file changed on disk"
    );

    // Leaving with the edit unsaved asks; Stay keeps the editor, Leave without saving leaves it.
    key(&mut h, egui::Key::Escape);
    shot_at(&mut h, "vr-7-leave-asks", &sizes);
    dump(&h, "leave prompt");
    for choice in ["Save and leave", "Leave without saving", "Stay"] {
        assert!(find(&h, choice).is_some(), "no {choice}: {:?}", labels(&h));
    }
    click(&mut h, "Stay");
    assert_eq!(
        h.state()
            .model
            .screens()
            .last()
            .and_then(|s| s.kind.clone())
            .as_deref(),
        Some("vehicle_editor")
    );
    key(&mut h, egui::Key::Escape);
    click(&mut h, "Leave without saving");
    rest(&mut h);
    shot_at(&mut h, "vr-8-left", &sizes);
    assert_ne!(
        h.state()
            .model
            .screens()
            .last()
            .and_then(|s| s.kind.clone())
            .as_deref(),
        Some("vehicle_editor"),
        "the editor stayed after Leave without saving"
    );
}

/// A bicycle spawned from the vehicle editor stands in the world, and the map editor's export names its prototype.
#[test]
#[ignore = "requires BN_ENGINE pointing at an engine binary"]
fn the_map_editor_exports_a_spawned_vehicle_as_a_placement() {
    let sizes = [(1280.0, 800.0), (1920.0, 1080.0)];
    let dir = std::env::temp_dir().join(format!("bn-walk-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let (mut h, _root) = start(&dir);
    h.set_size(egui::vec2(1280.0, 800.0));
    rest(&mut h);
    open_map_editor(&mut h);
    click_footer(&mut h, "Vehicle editor");
    wait_kind(&mut h, "vehicle_editor");
    rest(&mut h);
    click_input(&mut h, "Search");
    type_text(&mut h, "bicycle");
    key(&mut h, egui::Key::Enter);
    double_click(&mut h, "Bicycle");
    wait_kind(&mut h, "vehicle_editor");
    rest(&mut h);
    // Spawn leaves for the map, where the map editor's action opens it again.
    click_footer(&mut h, "Spawn");
    run_until(&mut h, "the map", 120, |h| {
        h.state()
            .model
            .screens()
            .last()
            .is_some_and(|s| s.role == bn_client_egui::protocol::ScreenRole::Map)
    });
    rest(&mut h);
    shot_at(&mut h, "xv-1-spawned", &sizes);
    assert!(h.state_mut().model.invoke_action("editor"));
    wait_kind(&mut h, "editor");
    rest(&mut h);

    // Two squares wide and high around the avatar's, so the bicycle east of it is inside whatever its square.
    for (label, count) in [("Export width", "2"), ("Export height", "2")] {
        click_footer(&mut h, label);
        replace_text(&mut h, count);
        click(&mut h, "OK");
    }
    let path = crate::common::live::profile_dir().join("mods/zz_export_vehicle.json");
    click_footer(&mut h, "Export mapgen here");
    type_text(&mut h, &path.to_string_lossy());
    key(&mut h, egui::Key::Enter);
    rest(&mut h);
    shot_at(&mut h, "xv-2-exported", &sizes);
    let exported = std::fs::read_to_string(&path).expect("the export file");
    assert!(
        exported.contains("place_vehicles") && exported.contains("\"bicycle\""),
        "{exported}"
    );
}

/// The label of the selected tab among the drawing tools.
fn tool_tab(h: &H) -> Option<String> {
    let names = [
        "Pencil",
        "Line",
        "Rectangle",
        "Filled rectangle",
        "Ellipse",
        "Room",
        "Fill",
        "Eyedropper",
        "Select",
        "Paste",
        "Placements",
    ];
    h.root().children_recursive().find_map(|n| {
        let a = n.accesskit_node();
        (a.role() == egui::accesskit::Role::Tab && a.is_selected() == Some(true))
            .then(|| a.label().map(|l| l.to_string()))
            .flatten()
            .filter(|l| names.contains(&l.as_str()))
    })
}

/// Every drawing tool has its key, and undo, redo and save run from the keyboard on a document painted by the mouse.
#[test]
#[ignore = "requires BN_ENGINE pointing at an engine binary"]
fn the_map_editor_keys_pick_every_tool_and_undo_redo_and_save() {
    let sizes = [(1280.0, 800.0), (1920.0, 1080.0)];
    let dir = std::env::temp_dir().join(format!("bn-walk-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let (mut h, _root) = start(&dir);
    h.set_size(egui::vec2(1920.0, 1080.0));
    rest(&mut h);
    open_map_editor(&mut h);
    let path = create_mapgen(&mut h, "zz_keys");
    for (typed, tool) in [
        ("b", "Pencil"),
        ("l", "Line"),
        ("r", "Rectangle"),
        ("u", "Filled rectangle"),
        ("o", "Ellipse"),
        ("m", "Room"),
        ("f", "Fill"),
        ("i", "Eyedropper"),
        ("s", "Select"),
        ("v", "Paste"),
        ("p", "Placements"),
    ] {
        type_text(&mut h, typed);
        assert_eq!(tool_tab(&h).as_deref(), Some(tool), "after {typed}");
    }
    shot_at(&mut h, "kb-1-tools", &sizes);
    // The pictures leave the window at their last size's neighbour; the walk goes on in the tall one.
    h.set_size(egui::vec2(1920.0, 1080.0));
    settle(&mut h);

    // A wall rectangle by the keys of the tools and two clicks; undo, redo and save by their keys.
    map_wall(&mut h);
    type_text(&mut h, "r");
    assert_eq!(
        tool_tab(&h).as_deref(),
        Some("Rectangle"),
        "{:?}",
        labels(&h)
    );
    let blank = canvas_glyphs(&h);
    paint(&mut h, 3, 2);
    paint(&mut h, 8, 5);
    // The engine answers a paint after the client went idle.
    run_until(&mut h, "the rectangle", 60, |h| canvas_glyphs(h) != blank);
    let walled = canvas_glyphs(&h);
    key_mod(&mut h, egui::Modifiers::CTRL, egui::Key::Z);
    assert_eq!(canvas_glyphs(&h), blank);
    key_mod(&mut h, egui::Modifiers::CTRL, egui::Key::Y);
    assert_eq!(canvas_glyphs(&h), walled);
    assert!(
        document(&h).contains("\"Saved\"=\"No\""),
        "{}",
        document(&h)
    );
    key_mod(&mut h, egui::Modifiers::CTRL, egui::Key::S);
    assert!(
        document(&h).contains("\"Saved\"=\"Yes\""),
        "{}",
        document(&h)
    );
    shot_at(&mut h, "kb-2-saved", &sizes);
    assert!(
        std::fs::read_to_string(&path)
            .unwrap()
            .contains("t_brick_wall")
    );

    // Apply, Stamp, Test and Preview carry no key: their affordances are offered but name none.
    let top = h.state().model.screens().last().unwrap();
    let unbound = [
        "editor:apply",
        "editor:stamp",
        "editor:test",
        "editor:preview",
    ]
    .into_iter()
    .filter(|id| {
        top.affordances
            .iter()
            .any(|a| a.id == *id && a.keys.is_empty())
    })
    .collect::<Vec<_>>();
    println!("UNBOUND {unbound:?}");
}

/// A new overmap special from the form, a connection drawn to the square beside it, and the saved file.
#[test]
#[ignore = "requires BN_ENGINE pointing at an engine binary"]
fn the_map_editor_makes_a_new_overmap_special_and_connects_it() {
    let sizes = [(1920.0, 1080.0), (1280.0, 800.0)];
    let dir = std::env::temp_dir().join(format!("bn-walk-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let (mut h, _root) = start(&dir);
    h.set_size(egui::vec2(1920.0, 1080.0));
    rest(&mut h);
    open_map_editor(&mut h);
    click(&mut h, "Mapgen");
    click(&mut h, "Overmap special");
    let path = crate::common::live::profile_dir().join("mods/zz_new_special.json");
    click_beside(&mut h, "New file/Path");
    replace_text(&mut h, &path.to_string_lossy());
    click_beside(&mut h, "Special id");
    replace_text(&mut h, "zz_walk_new_special");
    click(&mut h, "Create");
    shot_at(&mut h, "ns-1-created-or-asks", &sizes);
    if find(&h, "Continue").is_some() {
        click(&mut h, "Continue");
    }
    wait_kind(&mut h, "editor");
    rest(&mut h);
    shot_at(&mut h, "ns-2-created", &sizes);
    let (_, _, cells) = canvas_grid(&h);
    let own = cells
        .iter()
        .find(|c| !c.3.is_empty())
        .expect("the special's own square")
        .clone();
    println!("OWN {own:?}");

    // The connection tool, and a click on the square east of the special.
    type_text(&mut h, "c");
    assert_eq!(tabs_of(&h, "Connect").1.as_deref(), Some("Connect"));
    let beside = square_from(&h, (own.0, own.1, &own.3), own.0 + 1, own.1).center();
    click_at(&mut h, beside);
    shot_at(&mut h, "ns-3-connected", &sizes);
    let (_, _, after) = canvas_grid(&h);
    println!("BEFORE {cells:?}\nAFTER {after:?}");
    assert!(
        after.iter().any(|c| c.3.contains("connection")),
        "{after:?}"
    );

    key_mod(&mut h, egui::Modifiers::CTRL, egui::Key::S);
    let saved = std::fs::read_to_string(&path).unwrap();
    assert!(
        saved.contains("zz_walk_new_special") && saved.contains("connections"),
        "{saved}"
    );
}

/// A placement made from a marked rectangle and the form, outlined on the canvas, and removed again.
#[test]
#[ignore = "requires BN_ENGINE pointing at an engine binary"]
fn the_map_editor_adds_a_placement_from_a_marked_rectangle_and_removes_it() {
    let sizes = [(1920.0, 1080.0)];
    let dir = std::env::temp_dir().join(format!("bn-walk-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let (mut h, _root) = start(&dir);
    h.set_size(egui::vec2(1920.0, 1080.0));
    rest(&mut h);
    open_map_editor(&mut h);
    let path = create_mapgen(&mut h, "zz_placement");
    type_text(&mut h, "s");
    paint(&mut h, 4, 3);
    paint(&mut h, 7, 5);
    assert_eq!(field(&h, "editor:place", "x"), "4-7");
    assert_eq!(field(&h, "editor:place", "y"), "3-5");
    shot_at(&mut h, "pl-1-marked", &sizes);
    h.set_size(egui::vec2(1920.0, 1080.0));
    settle(&mut h);

    // An item placement: the form's default kind, its other members as JSON; its button is named as its heading.
    click_beside(&mut h, "Add placement/Value");
    type_text(&mut h, r#"{"item": "bedroom", "chance": 100}"#);
    click_button(&mut h, "Add placement");
    rest(&mut h);
    shot_at(&mut h, "pl-2-added", &sizes);
    h.set_size(egui::vec2(1920.0, 1080.0));
    settle(&mut h);
    let rows = crate::live_editor::rows(&h, "editor:placements");
    assert_eq!(rows, ["placement:place_items:0"]);
    key_mod(&mut h, egui::Modifiers::CTRL, egui::Key::S);
    let saved = std::fs::read_to_string(&path).unwrap();
    assert!(
        saved.contains("place_items") && saved.contains("bedroom"),
        "{saved}"
    );

    // The footer's Delete removes the picked placement with its array.
    click_footer(&mut h, "Delete placement");
    rest(&mut h);
    assert!(crate::live_editor::rows(&h, "editor:placements").is_empty());
}

/// A mapgen drawn in the editor, generated into the world over the avatar's square, and the avatar put there.
#[test]
#[ignore = "requires BN_ENGINE pointing at an engine binary"]
fn the_map_editor_stamps_the_document_into_the_world_and_tests_it_there() {
    use bn_client_egui::protocol::ScreenRole;
    let sizes = [(1920.0, 1080.0), (1280.0, 800.0)];
    let dir = std::env::temp_dir().join(format!("bn-walk-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let (mut h, _root) = start(&dir);
    h.set_size(egui::vec2(1920.0, 1080.0));
    rest(&mut h);
    open_map_editor(&mut h);
    create_mapgen(&mut h, "zz_stamp");
    map_wall(&mut h);
    click(&mut h, "Filled rectangle");
    paint(&mut h, 2, 2);
    paint(&mut h, 20, 20);
    run_until(&mut h, "the wall", 60, |h| glyph(h, 10, 10) == Some('#'));
    assert_eq!(enabled(&h, "editor:unstamp"), Some(false));
    // Test stamps a mapgen it is on by itself.
    assert_eq!(enabled(&h, "editor:test"), Some(true));
    click_footer(&mut h, "Stamp");
    rest(&mut h);
    shot_at(&mut h, "st-1-stamped", &sizes);
    assert_eq!(enabled(&h, "editor:unstamp"), Some(true));
    assert_eq!(enabled(&h, "editor:test"), Some(true));
    let canvas = canvas_glyphs(&h);

    // Test leaves the editor on the map, with the avatar in the stamped square; the editor opens again as it was.
    click_footer(&mut h, "Test");
    run_until(&mut h, "the map", 120, |h| {
        h.state()
            .model
            .screens()
            .last()
            .is_some_and(|s| s.role == ScreenRole::Map)
    });
    rest(&mut h);
    shot_at(&mut h, "st-2-tested", &sizes);
    assert!(h.state_mut().model.invoke_action("editor"));
    wait_kind(&mut h, "editor");
    assert_eq!(canvas_glyphs(&h), canvas);
    shot_at(&mut h, "st-3-editor-again", &sizes);
}

/// Pictures of the window at the laptop and desktop sizes, back at 1290x830 after.
fn shot_both(h: &mut H, name: &str) {
    shot_at(h, name, &[(1280.0, 800.0), (1920.0, 1080.0)]);
}

/// Opens the menu `group` of the menu bar and clicks its item `item`: the one under the menu's button, as a tab
/// may read the same.
fn menu(h: &mut H, group: &str, item: &str) {
    press(h, group);
    h.run_steps(4);
    let under = find(h, group).unwrap().left_bottom();
    let at = h
        .root()
        .children_recursive()
        .filter(|n| n.accesskit_node().label().as_deref() == Some(item))
        .map(|n| n.rect())
        .filter(|r| r.top() >= under.y - 1.0)
        .min_by(|a, b| {
            (a.left_top() - under)
                .length()
                .total_cmp(&(b.left_top() - under).length())
        })
        .unwrap_or_else(|| panic!("no {item} in {group}"));
    click_at(h, at.center());
    rest(h);
}

/// The top screen's kind.
fn top(h: &H) -> Option<String> {
    h.state()
        .model
        .screens()
        .last()
        .and_then(|s| s.kind.clone())
}

#[test]
#[ignore = "requires BN_ENGINE pointing at an engine binary"]
fn a_modder_opens_draws_undoes_saves_tests_and_leaves_the_map_editor_workspace() {
    let dir = std::env::temp_dir().join(format!("bn-walk-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let (mut h, _root) = start(&dir);
    h.set_size(egui::vec2(1290.0, 830.0));
    rest(&mut h);
    open_map_editor(&mut h);
    rest(&mut h);
    shot_both(&mut h, "ws-map-01-start");
    dump(&h, "the start page");

    // A mapgen file of the game found by the browser's search and opened by a double click on its row.
    click_input(&mut h, "Search");
    type_text(&mut h, "house/2story01");
    key(&mut h, egui::Key::Enter);
    rest(&mut h);
    shot_both(&mut h, "ws-map-02-browsed");
    double_click(&mut h, "2Story01.json");
    wait_kind(&mut h, "editor");
    run_until(&mut h, "the opened canvas", 60, |h| {
        canvas_glyphs(h).len() == 24
    });
    rest(&mut h);
    shot_both(&mut h, "ws-map-03-opened");
    dump(&h, "the opened file");

    // A symbol of the legend picked by a click on its row.
    let top_screen = h.state().model.screens().last().unwrap();
    let legend =
        serde_json::to_value(top_screen.root.as_ref().unwrap().find("editor:legend")).unwrap();
    let wall = legend["rows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["id"] == "symbol:#")
        .map(|r| r["label"].as_str().unwrap().to_string())
        .expect("a # in the legend");
    // The legend is right of the canvas, whose squares read as their symbols too.
    let row = h
        .root()
        .children_recursive()
        .filter(|n| n.accesskit_node().label().as_deref() == Some(wall.as_str()))
        .map(|n| n.rect())
        .max_by(|a, b| a.left().total_cmp(&b.left()))
        .expect("the legend's # row");
    tap(&mut h, row.center());
    tap(&mut h, row.center());
    rest(&mut h);
    run_until(&mut h, "the symbol #", 20, |h| {
        picked_symbol(h).as_deref() == Some("symbol:#")
    });
    shot_both(&mut h, "ws-map-04-symbol");

    // The pencil on one square, then a rectangle from two clicks.
    let opened = canvas_glyphs(&h);
    click_tab(&mut h, "Pencil");
    // Squares in the canvas's first view, which the history under it would hide lower down.
    paint(&mut h, 2, 10);
    run_until(&mut h, "the pencil square", 30, |h| {
        glyph(h, 2, 10) == Some('#')
    });
    click_tab(&mut h, "Rectangle");
    paint(&mut h, 4, 9);
    paint(&mut h, 8, 12);
    run_until(&mut h, "the rectangle", 30, |h| {
        glyph(h, 8, 12) == Some('#')
    });
    shot_both(&mut h, "ws-map-05-drawn");
    let drawn = canvas_glyphs(&h);

    // Undone from the Edit menu.
    press(&mut h, "Edit");
    h.run_steps(4);
    shot_both(&mut h, "ws-map-06-edit-menu");
    press(&mut h, "Undo");
    rest(&mut h);
    assert_ne!(canvas_glyphs(&h), drawn);
    assert_eq!(glyph(&h, 2, 10), Some('#'));
    shot_both(&mut h, "ws-map-07-undone");

    // What can spawn on squares marked by the select tool, from the View menu: the list comes into view.
    click_tab(&mut h, "Select");
    paint(&mut h, 4, 9);
    paint(&mut h, 8, 12);
    menu(&mut h, "View", "What can spawn here");
    rest(&mut h);
    let top_screen = h.state().model.screens().last().unwrap();
    let Some(Component::List(loot)) = top_screen.root.as_ref().unwrap().find("editor:loot") else {
        panic!("no loot list: {:#?}", labels(&h));
    };
    let first = loot.rows.first().expect("a kind of item").label.shown();
    let at = find(&h, &first).expect("the first kind drawn");
    assert!(in_view(&h, at), "{first} at {at:?}");
    shot_both(&mut h, "ws-map-07b-loot");

    // Saved as a mod file from the File menu, which asks for the path as the footer would.
    let path = crate::common::live::profile_dir().join("mods/zz_ws_house.json");
    menu(&mut h, "File", "Save as");
    shot_both(&mut h, "ws-map-08-save-as-asks");
    replace_text(&mut h, &path.to_string_lossy());
    key(&mut h, egui::Key::Enter);
    rest(&mut h);
    shot_both(&mut h, "ws-map-09-saved");
    assert!(path.is_file(), "no {}", path.display());
    let _ = opened;

    // Tested on the map, and back by the map's editor action.
    menu(&mut h, "World", "Test");
    run_until(&mut h, "the map", 60, |h| {
        !h.state().model.busy() && top(h).as_deref() != Some("editor")
    });
    rest(&mut h);
    shot_both(&mut h, "ws-map-10-tested");
    click(&mut h, "Map Editor");
    wait_kind(&mut h, "editor");
    rest(&mut h);
    shot_both(&mut h, "ws-map-11-back");

    // Left: the world ends and the main menu is back.
    key(&mut h, egui::Key::Escape);
    rest(&mut h);
    shot_both(&mut h, "ws-map-12-leaving");
    if find(&h, "Leave without saving").is_some() {
        click(&mut h, "Leave without saving");
    }
    run_until(&mut h, "the main menu", 300, |h| {
        h.state().model.menu().is_some() && !h.state().model.busy()
    });
    rest(&mut h);
    shot_both(&mut h, "ws-map-13-main-menu");
}

#[test]
#[ignore = "requires BN_ENGINE pointing at an engine binary"]
fn a_modder_opens_a_prototype_installs_a_part_spawns_and_leaves_the_vehicle_editor_workspace() {
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
    shot_both(&mut h, "ws-veh-01-start");
    dump(&h, "the vehicle editor");

    click_input(&mut h, "Search");
    type_text(&mut h, "bicycle");
    key(&mut h, egui::Key::Enter);
    rest(&mut h);
    shot_both(&mut h, "ws-veh-02-filtered");
    double_click(&mut h, "Bicycle");
    wait_kind(&mut h, "vehicle_editor");
    rest(&mut h);
    shot_both(&mut h, "ws-veh-03-opened");
    dump(&h, "the opened prototype");

    // A mount picked by a click on its square, and a headlight chosen from the part list and installed there by a
    // click on that square.
    click_square(&mut h, "67.5cm bicycle wheel");
    rest(&mut h);
    shot_both(&mut h, "ws-veh-04-mount");
    click_input(&mut h, "Parts/Search");
    type_text(&mut h, "headlight");
    key(&mut h, egui::Key::Enter);
    rest(&mut h);
    shot_both(&mut h, "ws-veh-05-parts");
    double_click(&mut h, "headlight");
    click_square(&mut h, "67.5cm bicycle wheel");
    rest(&mut h);
    shot_both(&mut h, "ws-veh-06-installed");
    dump(&h, "after the install");
    assert!(stacked(&h, "headlight"), "{}", labels(&h).join("\n"));

    // Every tab of the design is reached and read in the right column.
    for tab in ["Items here", "Raw JSON", "Parts here"] {
        click_tab(&mut h, tab);
        shot_both(
            &mut h,
            &format!("ws-veh-06-{}", tab.replace(' ', "-").to_lowercase()),
        );
    }

    // Spawned beside the avatar from the World menu: the map shows it.
    menu(&mut h, "World", "Spawn");
    run_until(&mut h, "the map", 60, |h| {
        !h.state().model.busy() && top(h).as_deref() != Some("vehicle_editor")
    });
    rest(&mut h);
    shot_both(&mut h, "ws-veh-07-spawned");

    // Back in the map editor by the map's editor action, and left for the main menu.
    click(&mut h, "Map Editor");
    wait_kind(&mut h, "editor");
    rest(&mut h);
    shot_both(&mut h, "ws-veh-08-editor-again");
    key(&mut h, egui::Key::Escape);
    rest(&mut h);
    if find(&h, "Leave without saving").is_some() {
        click(&mut h, "Leave without saving");
    }
    run_until(&mut h, "the main menu", 300, |h| {
        h.state().model.menu().is_some() && !h.state().model.busy()
    });
    rest(&mut h);
    shot_both(&mut h, "ws-veh-09-main-menu");
}
