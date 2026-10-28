//! Two usage demos of the editors for a video: a modder's walk through the map editor and the vehicle editor against
//! a real engine, every step a mouse move and click on a drawn widget or a key press. After each input step the frame
//! is rendered at 1920x1080 with a pointer drawn on it (the headless render has none) into `$DEMO_DIR/<demo>/frames/`,
//! with `captions.tsv` (first frame, caption) and `timing.tsv` (frame, milliseconds shown) beside it;
//! `tools/demo_video.ts` turns a demo directory into an mp4.
//!
//! `BN_ENGINE=<engine> DEMO_DIR=<dir> cargo test --release -p bn-client-egui --test it demo_editor_video:: --
//! --ignored --nocapture --test-threads=1`
use crate::live_editor::{H, run_until, start, wait_kind};
use bn_client_egui::{GameModel, MenuModel};
use eframe::egui;
use egui_kittest::kittest::NodeT;
use std::fmt::Write as _;
use std::path::PathBuf;

/// How long the frame after a finished step stays on screen, and the frames of a moving pointer and of a press.
const HOLD_MS: u32 = 1500;
const MOVE_MS: u32 = 40;
const PRESS_MS: u32 = 160;
const TYPE_MS: u32 = 90;
/// The frames a pointer takes to travel to its target.
const TRAVEL: usize = 10;

/// A harness whose every input step leaves a frame behind.
struct Rec {
    h: H<'static>,
    dir: PathBuf,
    frame: usize,
    at: egui::Pos2,
    captions: String,
    timing: String,
}

impl Rec {
    /// The client on a real engine's main menu in a 1920x1080 window, recording the demo `name`.
    fn new(name: &str) -> Self {
        let out = PathBuf::from(std::env::var_os("DEMO_DIR").expect("set DEMO_DIR")).join(name);
        let frames = out.join("frames");
        if frames.exists() {
            std::fs::remove_dir_all(&frames).unwrap();
        }
        std::fs::create_dir_all(&frames).unwrap();
        let scratch = std::env::temp_dir().join(format!("bn-demo-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&scratch).unwrap();
        let (mut h, _root) = start(&scratch);
        h.set_size(egui::vec2(1920.0, 1080.0));
        let mut rec = Self {
            h,
            dir: out,
            frame: 0,
            at: egui::pos2(960.0, 540.0),
            captions: String::new(),
            timing: String::new(),
        };
        rec.rest();
        rec.grab(HOLD_MS);
        rec
    }

    /// Writes the captions and timing beside the frames.
    fn finish(self) {
        std::fs::write(self.dir.join("captions.tsv"), self.captions).unwrap();
        std::fs::write(self.dir.join("timing.tsv"), self.timing).unwrap();
        println!("{} frames in {}", self.frame, self.dir.display());
    }

    /// The caption of the frames from the next one on.
    fn say(&mut self, text: &str) {
        println!("{:05} {text}", self.frame);
        writeln!(self.captions, "{}\t{text}", self.frame).unwrap();
    }

    /// Renders the window with the pointer on it as the next frame, shown for `ms`.
    fn grab(&mut self, ms: u32) {
        let mut image = self.h.render().expect("renders");
        draw_pointer(&mut image, self.at);
        let name = format!("{:05}.png", self.frame);
        image.save(self.dir.join("frames").join(name)).unwrap();
        writeln!(self.timing, "{}\t{ms}", self.frame).unwrap();
        self.frame += 1;
    }

    /// Waits for the engine and the layout to rest, without a frame.
    fn rest(&mut self) {
        run_until(&mut self.h, "idle", 120, |h| !h.state().model.busy());
        for _ in 0..20 {
            self.h.run_steps(1);
            std::thread::sleep(std::time::Duration::from_millis(30));
        }
        run_until(&mut self.h, "idle", 120, |h| !h.state().model.busy());
    }

    /// A finished step: the engine answered, and the frame stays.
    fn done(&mut self) {
        self.rest();
        self.grab(HOLD_MS);
    }

    /// Moves the pointer to `to` over a few frames.
    fn move_to(&mut self, to: egui::Pos2) {
        let from = self.at;
        for i in 1..=TRAVEL {
            let t = i as f32 / TRAVEL as f32;
            // Slow at both ends, as a hand is.
            let t = t * t * (3.0 - 2.0 * t);
            self.at = from + (to - from) * t;
            self.h.hover_at(self.at);
            self.h.run_steps(1);
            self.grab(MOVE_MS);
        }
    }

    /// One press and release of the primary button where the pointer is.
    fn tap(&mut self) {
        for pressed in [true, false] {
            self.h.event(egui::Event::PointerButton {
                pos: self.at,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            });
            self.h.run_steps(2);
            if pressed {
                self.grab(PRESS_MS);
            }
        }
    }

    fn click_at(&mut self, at: egui::Pos2) {
        self.move_to(at);
        self.tap();
        self.done();
    }

    /// Clicks the widget labelled `label` once it stopped moving.
    fn click(&mut self, label: &str) {
        crate::live_editor_walk::reveal_rect(&mut self.h, label, |h| find(h, label));
        let at = self.stable(label, |h| find(h, label)).center();
        self.click_at(at);
    }

    /// Double-clicks the widget labelled `label`.
    fn double_click(&mut self, label: &str) {
        crate::live_editor_walk::reveal_rect(&mut self.h, label, |h| find(h, label));
        let at = self.stable(label, |h| find(h, label)).center();
        self.move_to(at);
        self.tap();
        self.tap();
        self.done();
    }

    /// Clicks the menu `group` of the menu bar and its item `item`.
    fn menu(&mut self, group: &str, item: &str) {
        self.click(group);
        // The menu's items are drawn after the rest, so the last button of that name is the item, not a tab.
        let at = self.stable(item, |h| last_button(h, item)).center();
        self.click_at(at);
    }

    /// The rectangle `rect_of` finds once it stopped moving; panics with what is drawn when there is none.
    fn stable(&mut self, what: &str, rect_of: impl Fn(&H) -> Option<egui::Rect>) -> egui::Rect {
        let mut seen = None;
        for _ in 0..200 {
            let now = rect_of(&self.h);
            if let Some(found) = now.filter(|_| now == seen) {
                return found;
            }
            seen = now;
            self.h.run_steps(3);
        }
        self.grab(HOLD_MS);
        panic!(
            "no {what} (frame {}): {:?}",
            self.frame - 1,
            labels(&self.h)
        );
    }

    /// Types `text` a letter at a time into the box with the keyboard focus.
    fn type_slowly(&mut self, text: &str) {
        for c in text.chars() {
            self.h.event(egui::Event::Text(c.to_string()));
            self.h.run_steps(2);
            self.grab(TYPE_MS);
        }
    }

    /// Types `text` in one go, for a long path.
    fn paste(&mut self, text: &str) {
        self.h.event(egui::Event::Text(text.to_string()));
        self.h.run_steps(3);
        self.done();
    }

    fn key(&mut self, key: egui::Key) {
        self.h.key_press(key);
        self.done();
    }

    fn key_mod(&mut self, mods: egui::Modifiers, key: egui::Key) {
        self.h.key_press_modifiers(mods, key);
        self.h.run_steps(3);
        self.done();
    }

    /// Clicks the square (`x`, `y`) of the canvas.
    fn paint(&mut self, x: usize, y: usize) {
        let at = self
            .stable("a canvas square", |h| Some(square(h, x, y)))
            .center();
        self.click_at(at);
    }

    /// Waits until the screen on top is of `kind`.
    fn wait_kind(&mut self, kind: &str) {
        wait_kind(&mut self.h, kind);
        self.done();
    }

    /// Opens the editor from the main menu: the Editor tab, the editor's entry, the mod list left as it is.
    fn open_editor(&mut self, entry: &str, kind: &str) {
        self.say("Main menu: open the Editor tab");
        self.click("Editor");
        self.say(&format!("Choose {entry}"));
        self.click(entry);
        self.say("The mod list asks which mod holds your files; Esc confirms it");
        self.grab(HOLD_MS * 2);
        self.key(egui::Key::Escape);
        self.click("Yes");
        self.wait_kind(kind);
    }

    /// Leaves the editor with Escape, leaving without saving when asked, and waits for the main menu.
    fn leave(&mut self) {
        self.key(egui::Key::Escape);
        if find(&self.h, "Leave without saving").is_some() {
            self.click("Leave without saving");
        }
        run_until(&mut self.h, "the main menu", 300, |h| {
            h.state().model.menu().is_some() && !h.state().model.busy()
        });
        self.done();
    }
}

/// Whether (`x`, `y`) is inside the pointer arrow whose tip is the origin, in a 12x20 box.
fn in_arrow(x: f32, y: f32) -> bool {
    const ARROW: [(f32, f32); 7] = [
        (0.0, 0.0),
        (0.0, 17.0),
        (4.0, 13.0),
        (7.5, 20.0),
        (10.0, 19.0),
        (6.5, 12.0),
        (12.0, 12.0),
    ];
    let mut inside = false;
    for (i, a) in ARROW.iter().enumerate() {
        let b = ARROW[(i + 1) % ARROW.len()];
        if (a.1 > y) != (b.1 > y) && x < (b.0 - a.0) * (y - a.1) / (b.1 - a.1) + a.0 {
            inside = !inside;
        }
    }
    inside
}

/// A white arrow with a black outline, its tip at `at`, drawn onto the picture.
fn draw_pointer(image: &mut image::RgbaImage, at: egui::Pos2) {
    let scale = 1.6;
    let (w, h) = (image.width() as i32, image.height() as i32);
    for dy in -3..(22.0 * scale) as i32 + 3 {
        for dx in -3..(14.0 * scale) as i32 + 3 {
            let (px, py) = (at.x as i32 + dx, at.y as i32 + dy);
            if px < 0 || py < 0 || px >= w || py >= h {
                continue;
            }
            let (x, y) = (dx as f32 / scale, dy as f32 / scale);
            let colour = if in_arrow(x, y) {
                [255, 255, 255, 255]
            } else if [(-1.0, 0.0), (1.0, 0.0), (0.0, -1.0), (0.0, 1.0)]
                .iter()
                .any(|(ox, oy)| in_arrow(x + ox * 1.4, y + oy * 1.4))
            {
                [0, 0, 0, 255]
            } else {
                continue;
            };
            image.get_pixel_mut(px as u32, py as u32).0 = colour;
        }
    }
}

/// The labels drawn now, one per line.
fn labels(h: &H) -> Vec<String> {
    crate::live_editor_walk::labels(h)
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

/// The rectangle of the last button labelled exactly `label`.
fn last_button(h: &H, label: &str) -> Option<egui::Rect> {
    h.root()
        .children_recursive()
        .filter(|n| {
            let a = n.accesskit_node();
            a.role() == egui::accesskit::Role::Button && a.label().as_deref() == Some(label)
        })
        .last()
        .map(|n| n.rect())
}

/// The canvas of the editor on top, its glyphs row by row.
fn canvas_glyphs(h: &H) -> Vec<String> {
    crate::live_editor::glyphs(h, "editor:canvas")
}

/// The screen rectangle of canvas square (`x`, `y`): the buttons that spell the canvas row by row, in the order the
/// grid draws them.
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
    let spelled = glyphs.concat().chars().collect::<Vec<_>>();
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
        .find(|&i| names[i..i + spelled.len()] == spelled[..])
        .unwrap_or_else(|| panic!("no run of buttons spells the canvas"));
    buttons[start + y * cols + x].rect()
}

/// The symbol of the legend's selected row.
fn selected_symbol(h: &H) -> String {
    let top = h.state().model.screens().last().unwrap();
    let legend = serde_json::to_value(top.root.as_ref().unwrap().find("editor:legend")).unwrap();
    legend["rows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["selected"] == true)
        .and_then(|r| r["label"].as_str().map(str::to_string))
        .expect("a selected legend row")
}

/// The text of the component `id` of the screen on top, as the engine sent it.
fn engine_text(h: &H, id: &str) -> String {
    let top = h.state().model.screens().last().unwrap();
    serde_json::to_string(&top.root.as_ref().and_then(|r| r.find(id))).unwrap()
}

/// The labels of the rows of the list `id` of the screen on top.
fn rows_of(h: &H, id: &str) -> Vec<String> {
    let top = h.state().model.screens().last().unwrap();
    match top.root.as_ref().and_then(|r| r.find(id)) {
        Some(bn_client_egui::protocol::Component::List(list)) => {
            list.rows.iter().map(|r| r.label.shown()).collect()
        }
        _ => vec![],
    }
}

impl Rec {
    /// Clicks the last button labelled `label`, such as a list's row over a heading of the same name.
    fn click_button(&mut self, label: &str) {
        let at = self.stable(label, |h| last_button(h, label)).center();
        self.click_at(at);
    }

    /// Clicks the tab labelled `label`.
    fn tab(&mut self, label: &str) {
        let at = self
            .stable(label, |h| {
                h.root()
                    .children_recursive()
                    .find(|n| {
                        let a = n.accesskit_node();
                        a.role() == egui::accesskit::Role::Tab
                            && a.label().as_deref() == Some(label)
                    })
                    .map(|n| n.rect())
            })
            .center();
        self.click_at(at);
    }
}

#[test]
#[ignore = "requires BN_ENGINE pointing at an engine binary"]
fn map_editor_demo() {
    let mut r = Rec::new("map-editor");
    let mods = crate::common::live::profile_dir().join("mods");
    r.open_editor("Map editor", "editor");

    r.say("A new mapgen file: fields marked * are required");
    r.grab(HOLD_MS * 2);
    r.say("A path outside a mod folder is refused with the reason");
    r.click_field("New mapgen file/Path");
    r.key_mod(egui::Modifiers::COMMAND, egui::Key::A);
    r.paste(&mods.join("demo_hut.json").to_string_lossy());
    r.click_field("New mapgen file/Overmap terrain");
    r.key_mod(egui::Modifiers::COMMAND, egui::Key::A);
    r.type_slowly("field");
    r.click("Create");
    r.grab(HOLD_MS * 2);
    r.say("A path inside a mod folder makes the mod and the file");
    r.click_field("New mapgen file/Path");
    r.key_mod(egui::Modifiers::COMMAND, egui::Key::A);
    let path = mods.join("my_mod/demo_hut.json");
    r.paste(&path.to_string_lossy());
    r.click("Create");
    if find(&r.h, "Continue").is_some() {
        r.click("Continue");
    }
    r.wait_kind("editor");
    assert!(path.is_file(), "no {}", path.display());
    assert!(mods.join("my_mod/modinfo.json").is_file(), "no modinfo");
    r.say("With a document open, the start page's parts fold");
    r.grab(HOLD_MS * 2);

    r.say("Catalog: pick a terrain, give it a symbol, Set mapping adds it to the legend");
    r.click_field("Catalog");
    r.type_slowly("brick wall");
    r.key(egui::Key::Enter);
    r.double_click("brick wall");
    r.click_field("Mapping/Symbol");
    r.key_mod(egui::Modifiers::COMMAND, egui::Key::A);
    r.type_slowly("#");
    r.key(egui::Key::Enter);
    r.click("Set mapping");
    r.say("Furniture the same way, on its own layer");
    r.tab("Furniture");
    r.click_field("Catalog");
    r.key_mod(egui::Modifiers::COMMAND, egui::Key::A);
    r.type_slowly("chair");
    r.key(egui::Key::Enter);
    let chair = rows_of(&r.h, "editor:catalog")
        .into_iter()
        .next()
        .expect("a chair in the furniture catalog");
    r.double_click(&chair);
    r.click_field("Mapping/Symbol");
    r.key_mod(egui::Modifiers::COMMAND, egui::Key::A);
    r.type_slowly("c");
    r.key(egui::Key::Enter);
    println!("MAPPING {}", engine_text(&r.h, "editor:mapping"));
    r.click("Set mapping");
    println!("LEGEND {:?}", rows_of(&r.h, "editor:legend"));
    r.grab(HOLD_MS * 2);

    r.say("Pick the wall symbol in the legend, then draw a room with two clicks");
    let wall = legend_row("symbol:#", &r.h);
    r.double_click(&wall);
    println!("SELECTED {}", selected_symbol(&r.h));
    r.tab("Room");
    r.paint(3, 3);
    r.paint(14, 10);
    assert_eq!(
        canvas_glyphs(&r.h)[3].chars().nth(3).map(String::from),
        Some("#".into()),
        "the room did not draw"
    );
    r.say("Pick the furniture symbol and place it with the pencil");
    let furniture = legend_row("symbol:c", &r.h);
    r.double_click(&furniture);
    r.tab("Pencil");
    r.paint(5, 5);
    r.paint(6, 5);
    r.paint(7, 5);
    r.say("Ctrl+Z undoes the last stroke and the status line says so");
    r.key_mod(egui::Modifiers::CTRL, egui::Key::Z);
    assert!(engine_text(&r.h, "editor:status").contains("Undid"));
    r.say("Ctrl+S saves into the mod folder");
    r.key_mod(egui::Modifiers::CTRL, egui::Key::S);
    println!("SAVED {}", std::fs::read_to_string(&path).unwrap());

    r.say("World > Test stamps the mapgen over the player's square and moves the player there");
    r.menu("World", "Test");
    run_until(&mut r.h, "the map", 120, |h| {
        !h.state().model.busy()
            && h.state()
                .model
                .screens()
                .last()
                .is_some_and(|s| s.kind.as_deref() != Some("editor"))
    });
    r.grab(HOLD_MS * 3);
    r.done();
    r.say("Back in the editor through the map's Map Editor action");
    r.click("Map Editor");
    r.wait_kind("editor");

    r.say("Export writes the squares around the player as a mapgen file in a mod");
    r.menu_any("Export mapgen here");
    r.grab(HOLD_MS * 2);
    r.key_mod(egui::Modifiers::COMMAND, egui::Key::A);
    let exported = mods.join("my_mod/demo_hut_export.json");
    r.paste(&exported.to_string_lossy());
    r.key(egui::Key::Enter);
    assert!(exported.is_file(), "no {}", exported.display());
    r.grab(HOLD_MS * 2);

    r.say("Esc leaves the editor");
    r.leave();
    r.finish();
}

/// What the legend row `id` shows to click on: its terrain, else its furniture.
fn legend_row(id: &str, h: &H) -> String {
    let top = h.state().model.screens().last().unwrap();
    let legend = serde_json::to_value(top.root.as_ref().unwrap().find("editor:legend")).unwrap();
    let row = legend["rows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["id"] == id)
        .unwrap_or_else(|| panic!("no {id} in the legend: {legend}"));
    ["terrain", "furniture"]
        .into_iter()
        .filter_map(|key| {
            row["columns"]
                .as_array()?
                .iter()
                .find(|c| c["key"] == key)?["value"]
                .as_str()
                .filter(|v| !v.is_empty())
                .map(str::to_string)
        })
        .next()
        .unwrap_or_else(|| panic!("nothing to click on in {row}"))
}

impl Rec {
    /// Clicks the select labelled `label`.
    fn click_combo(&mut self, label: &str) {
        let rect_of = |h: &H| {
            h.root()
                .children_recursive()
                .find(|n| {
                    let a = n.accesskit_node();
                    a.role() == egui::accesskit::Role::ComboBox
                        && a.label().as_deref() == Some(label)
                })
                .map(|n| n.rect())
        };
        crate::live_editor_walk::reveal_rect(&mut self.h, label, rect_of);
        let at = self.stable(label, rect_of).center();
        self.click_at(at);
    }

    /// Clicks the text box `label`, as `heading/label` after the heading that names its form.
    fn click_field(&mut self, label: &str) {
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
                    a.role() == egui::accesskit::Role::TextInput
                        && a.label().as_deref() == Some(label)
                })
                .map(|n| n.rect())
                .collect::<Vec<_>>();
            boxes.first().copied()
        };
        crate::live_editor_walk::reveal_rect(&mut self.h, label, rect_of);
        let at = self.stable(label, rect_of).center();
        self.click_at(at);
    }

    /// Clicks the item `item` of whichever menu of the menu bar holds it.
    fn menu_any(&mut self, item: &str) {
        for group in ["File", "Edit", "View", "World"] {
            if find(&self.h, group).is_none() {
                continue;
            }
            self.click(group);
            if last_button(&self.h, item).is_some() {
                let at = self.stable(item, |h| last_button(h, item)).center();
                return self.click_at(at);
            }
            self.click(group);
        }
        panic!("no menu holds {item}: {:?}", labels(&self.h));
    }
}

#[test]
#[ignore = "requires BN_ENGINE pointing at an engine binary"]
fn vehicle_editor_demo() {
    let mut r = Rec::new("vehicle-editor");
    let mods = crate::common::live::profile_dir().join("mods");
    r.open_editor("Vehicle editor", "vehicle_editor");

    r.say("File > New vehicle asks for a name");
    r.menu("File", "New vehicle");
    r.grab(HOLD_MS);
    r.key_mod(egui::Modifiers::COMMAND, egui::Key::A);
    r.type_slowly("walk_van");
    r.key(egui::Key::Enter);
    if find(&r.h, "Continue").is_some() {
        r.click("Continue");
    }
    run_until(&mut r.h, "the new vehicle", 60, |h| {
        !rows_of(h, "vehicle:history").is_empty()
    });
    r.done();

    r.say("The parts palette has category tabs; Frames: pick a frame, click the squares beside the first one");
    r.pick("Frames", "frame");
    r.install(&["mount:1,0", "mount:-1,0"]);
    r.say("A wheel needs a hub on its mount, so this install is refused");
    r.pick("Wheels & hubs", "wheel");
    r.install(&["mount:-1,0"]);
    r.say("Install the wheel hub assembly first, then the wheel");
    r.pick("Wheels & hubs", "wheel hub assembly");
    r.install(&["mount:-1,0"]);
    r.pick("Wheels & hubs", "wheel");
    r.install(&["mount:-1,0"]);
    r.say("Seats & controls: a seat and controls on the middle square");
    r.pick("Seats & controls", "seat");
    r.install(&["mount:0,0"]);
    r.pick("Seats & controls", "controls");
    r.install(&["mount:0,0"]);
    r.say("Engines & motors: an engine on a frame below");
    r.pick("Frames", "frame");
    r.install(&["mount:0,1"]);
    r.pick("Engines & motors", "1-cylinder engine");
    r.install(&["mount:0,1"]);
    r.say("Fuel & batteries: a tank on a frame above");
    r.pick("Frames", "frame");
    r.install(&["mount:0,-1"]);
    r.pick("Fuel & batteries", "vehicle tank (60L)");
    r.install(&["mount:0,-1"]);
    println!("HISTORY {:?}", rows_of(&r.h, "vehicle:history"));

    r.say("Select tool, then the tank's square and its part there; the part has a Fuel select");
    r.click("Select");
    r.mount("mount:0,-1");
    r.click_button("vehicle tank (60L)");
    r.click_combo("Fuel");
    r.grab(HOLD_MS);
    r.say("The select has a filter: type to narrow the fuels, Enter picks the first");
    r.type_slowly("gaso");
    r.key(egui::Key::Enter);
    r.grab(HOLD_MS * 2);

    r.say("A right click on a square removes its top part");
    r.right_click_square("mount:-1,0");
    println!("HISTORY {:?}", rows_of(&r.h, "vehicle:history"));

    r.say("Ctrl+S saves into the mod folder");
    r.key_mod(egui::Modifiers::CTRL, egui::Key::S);
    let saved = mods.join("my_mod/walk_van.json");
    let text =
        std::fs::read_to_string(&saved).unwrap_or_else(|e| panic!("{}: {e}", saved.display()));
    println!("SAVED {text}");
    assert!(text.contains("gasoline"), "{text}");

    r.say("World > Spawn puts the vehicle on the game map; the editor stays open");
    r.menu("World", "Spawn");
    r.grab(HOLD_MS * 3);
    assert_eq!(
        r.h.state()
            .model
            .screens()
            .last()
            .and_then(|s| s.kind.clone())
            .as_deref(),
        Some("vehicle_editor")
    );
    println!("NOTICE {}", engine_text(&r.h, "vehicle:notice"));

    r.say("Esc leaves the editor");
    r.leave();
    r.finish();
}

/// The square of the vehicle editor's grid for the mount `id`: the run of buttons named as the grid's cells are,
/// in the order the grid draws them.
fn mount_square(h: &H, id: &str) -> Option<egui::Rect> {
    let top = h.state().model.screens().last()?;
    let Some(bn_client_egui::protocol::Component::Grid(grid)) =
        top.root.as_ref()?.find("vehicle:grid")
    else {
        return None;
    };
    let names = grid
        .cells
        .iter()
        .map(|c| match c.label.as_ref().map(|l| l.shown()) {
            Some(l) if !l.is_empty() => l,
            _ => c.glyph.clone().unwrap_or_default(),
        })
        .map(|n| n.trim().to_string())
        .collect::<Vec<_>>();
    let at = grid
        .cells
        .iter()
        .position(|c| c.id.as_deref() == Some(id))?;
    let buttons = h
        .root()
        .children_recursive()
        .filter(|n| n.accesskit_node().role() == egui::accesskit::Role::Button)
        .map(|n| {
            (
                n.accesskit_node()
                    .label()
                    .unwrap_or_default()
                    .trim()
                    .to_string(),
                n.rect(),
            )
        })
        .collect::<Vec<_>>();
    (0..=buttons.len().checked_sub(names.len())?)
        .find(|&i| {
            buttons[i..i + names.len()]
                .iter()
                .map(|b| &b.0)
                .eq(names.iter())
        })
        .map(|i| buttons[i + at].1)
}

impl Rec {
    /// Clicks the grid square of the mount `id`.
    fn mount(&mut self, id: &str) {
        let at = self.stable(id, |h| mount_square(h, id)).center();
        self.click_at(at);
    }

    /// Opens the palette tab `tab` and picks the part row `part` with a double click.
    fn pick(&mut self, tab: &str, part: &str) {
        self.tab(tab);
        // The palette's rows lie under its tab strip, over the heads of the sections under it, right of the
        // canvas, whose squares and whose mount's stack may bear the part's name.
        let row = |h: &H| {
            let named = |role, label: &str| {
                h.root()
                    .children_recursive()
                    .filter(move |n| {
                        let a = n.accesskit_node();
                        a.role() == role && a.label().as_deref() == Some(label)
                    })
                    .map(|n| n.rect())
                    .next()
            };
            let (top, bottom, left) = (
                named(egui::accesskit::Role::Tab, "Other")?.bottom(),
                named(egui::accesskit::Role::Button, "Vehicles")?.top(),
                named(egui::accesskit::Role::Tab, "Frames")?.left() - 10.0,
            );
            h.root()
                .children_recursive()
                .filter(|n| {
                    let a = n.accesskit_node();
                    a.role() == egui::accesskit::Role::Button && a.label().as_deref() == Some(part)
                })
                .map(|n| n.rect())
                .find(|r| r.left() >= left && r.top() > top && r.bottom() < bottom + 1.0)
        };
        crate::live_editor_walk::reveal_rect(&mut self.h, part, row);
        let at = self.stable(part, row).center();
        self.move_to(at);
        self.tap();
        self.tap();
        self.done();
    }

    /// Clicks each of `mounts` to install the picked part there.
    fn install(&mut self, mounts: &[&str]) {
        for id in mounts {
            self.mount(id);
        }
        println!("INSTALLED: {}", engine_text(&self.h, "vehicle:notice"));
    }

    /// A right click on the grid square of the mount `id`.
    fn right_click_square(&mut self, id: &str) {
        let at = self.stable(id, |h| mount_square(h, id)).center();
        self.move_to(at);
        for pressed in [true, false] {
            self.h.event(egui::Event::PointerButton {
                pos: self.at,
                button: egui::PointerButton::Secondary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            });
            self.h.run_steps(2);
            if pressed {
                self.grab(PRESS_MS);
            }
        }
        self.done();
    }
}
