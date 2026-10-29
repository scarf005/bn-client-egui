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
    /// The canvas square the `square_at` squares count from.
    origin: (i32, i32),
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
            origin: (0, 0),
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
    fn paint(&mut self, x: i32, y: i32) {
        let at = self.square_at(x, y);
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
    r.click_field("Path");
    r.key_mod(egui::Modifiers::COMMAND, egui::Key::A);
    r.paste(&mods.join("demo_hut.json").to_string_lossy());
    r.click_field("Overmap terrain");
    r.key_mod(egui::Modifiers::COMMAND, egui::Key::A);
    r.type_slowly("demo_hut");
    r.click("Create");
    r.grab(HOLD_MS * 2);
    r.say("A path inside a mod folder makes the mod and the file");
    r.click_field("Path");
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

    r.say("Catalog: search a terrain and double-click it to take it as the brush; the legend gets its symbol");
    r.click_field("Catalog");
    r.type_slowly("brick wall");
    r.key(egui::Key::Enter);
    r.take("brick wall");
    println!("LEGEND {:?}", rows_of(&r.h, "editor:legend"));
    r.say("Room draws walls between two clicked corners");
    r.tab("Room");
    r.paint(3, 3);
    r.paint(14, 10);
    assert_eq!(
        canvas_glyphs(&r.h)[3]
            .chars()
            .nth(3)
            .map(String::from)
            .as_deref(),
        Some("b"),
        "the room did not draw"
    );
    r.say("Furniture the same way, from its own tab");
    r.tab("Furniture");
    r.click_field("Catalog");
    r.key_mod(egui::Modifiers::COMMAND, egui::Key::A);
    r.type_slowly("chair");
    r.key(egui::Key::Enter);
    let chair = rows_of(&r.h, "editor:catalog")
        .into_iter()
        .next()
        .expect("a chair in the furniture catalog");
    r.take(&chair);
    println!("LEGEND {:?}", rows_of(&r.h, "editor:legend"));
    r.say("Line draws a row of them along a drag");
    r.tab("Line");
    r.drag((5, 5), (9, 5), egui::PointerButton::Primary);
    r.say("Pencil places one square at a time");
    r.tab("Pencil");
    r.paint(5, 8);
    r.say("Ctrl+Z undoes the last stroke and the status line says so");
    r.key_mod(egui::Modifiers::CTRL, egui::Key::Z);
    assert!(engine_text(&r.h, "editor:status").contains("Undid"));
    r.say("Select marks the squares a placement is for");
    r.tab("Select");
    r.paint(6, 7);
    r.paint(8, 7);
    r.say("New placement adds an item group on the marked squares");
    r.click("New placement");
    r.click_input("Value");
    r.paste(r#"{"item": "kitchen", "chance": 100}"#);
    r.click("Add placement");
    r.say("Placed by this entry lists what the entry puts on the map");
    r.click("Placed by this entry");
    r.grab(HOLD_MS * 2);
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

impl Rec {
    /// Double-clicks the catalog row `row` until the engine says the brush is that.
    fn take(&mut self, row: &str) {
        for _ in 0..3 {
            self.double_click(row);
            if engine_text(&self.h, "editor:status").contains(&format!("Painting {row}")) {
                return;
            }
        }
        panic!("the brush did not become {row}");
    }

    /// Clicks the first text box `label` as it is drawn, without scrolling to it.
    fn click_input(&mut self, label: &str) {
        let rect_of = |h: &H| {
            h.root()
                .children_recursive()
                .find(|n| {
                    let a = n.accesskit_node();
                    a.role() == egui::accesskit::Role::TextInput
                        && a.label().as_deref() == Some(label)
                })
                .map(|n| n.rect())
        };
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
    r.origin = VEHICLE_ORIGIN;
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

    r.say("The parts palette has a tab for every category");
    for tab in [
        "Wheels & hubs",
        "Engines & motors",
        "Seats & controls",
        "Frames",
    ] {
        r.tab(tab);
    }
    r.say("The search box finds a part across the categories");
    r.click_field("Parts");
    r.type_slowly("door");
    r.grab(HOLD_MS * 2);
    r.key_mod(egui::Modifiers::COMMAND, egui::Key::A);
    r.key(egui::Key::Backspace);

    r.say("Rectangle drags a ring of frames around the first frame");
    r.pick("Frames", "frame");
    r.click("Rectangle");
    r.drag((-1, -1), (1, 1), egui::PointerButton::Primary);
    assert_eq!(r.parts_at(1, 1), 1);
    assert_eq!(r.parts_at(0, 0), 1, "the vehicle's first frame");
    r.say("Filled rectangle covers a 12 by 6 area with frames in one drag");
    r.click("Filled rectangle");
    r.drag((-6, -3), (5, 2), egui::PointerButton::Primary);
    assert_eq!(r.parts_at(-6, -3), 1);
    assert_eq!(r.parts_at(5, 2), 1);
    println!("HISTORY {:?}", rows_of(&r.h, "vehicle:history"));

    r.say("Mirror left and right on: the wheels are drawn on both sides at once");
    r.click("Mirror left and right");
    r.click("Pencil");
    r.pick("Wheels & hubs", "wheel hub assembly");
    r.click_square(-6, 2);
    r.pick("Wheels & hubs", "wheel");
    r.click_square(-6, 2);
    assert_eq!(
        rows_of(&r.h, "vehicle:history").last().map(String::as_str),
        Some("Install wheel ×2"),
        "the mirrored side got its wheel too"
    );
    r.click("Mirror left and right");

    r.say("Line draws a row of seats along a drag");
    r.pick("Seats & controls", "seat");
    r.click("Line");
    r.drag((-4, 0), (1, 0), egui::PointerButton::Primary);
    r.say("Pencil puts the controls down one square at a time");
    r.pick("Seats & controls", "controls");
    r.click("Pencil");
    r.click_square(2, 0);
    r.say("Fill covers every connected square of frames with the part in hand, here a headlight on each");
    r.pick("Lights", "headlight");
    r.click("Fill");
    r.click_square(5, -3);
    println!("HISTORY {:?}", rows_of(&r.h, "vehicle:history"));

    r.say("Eraser drags across squares and takes their top parts off");
    r.click("Eraser");
    r.drag((-3, 0), (-2, 0), egui::PointerButton::Primary);
    assert_eq!(r.parts_at(-3, 0), 1, "the seats are gone, the frames stay");

    r.say("Select a block of squares, then Edit > Copy");
    r.click("Select");
    r.drag((-1, 0), (1, 0), egui::PointerButton::Primary);
    r.menu("Edit", "Copy");
    r.say("The Paste tool outlines where the copy lands from the square under the pointer");
    r.click("Paste");
    r.move_to_square(-4, -2);
    r.grab(HOLD_MS * 2);
    r.click_square(-4, -2);
    r.say("Select the pasted squares and Edit > Cut takes them away");
    r.click("Select");
    r.drag((-4, -2), (-2, -2), egui::PointerButton::Primary);
    r.menu("Edit", "Cut");
    println!("HISTORY {:?}", rows_of(&r.h, "vehicle:history"));

    r.say("View > Turn view clockwise turns the canvas a quarter");
    r.menu("View", "Turn view clockwise");
    r.grab(HOLD_MS * 2);

    r.say("Edit > Undo steps back through the history; each stroke is one labelled step");
    r.menu("Edit", "Undo");
    r.menu("Edit", "Undo");
    r.grab(HOLD_MS * 2);
    r.say("Edit > Redo brings it back");
    r.menu("Edit", "Redo");
    r.grab(HOLD_MS * 2);

    r.say("Ctrl+S saves into the mod folder");
    r.key_mod(egui::Modifiers::CTRL, egui::Key::S);
    let saved = mods.join("my_mod/walk_van.json");
    let text =
        std::fs::read_to_string(&saved).unwrap_or_else(|e| panic!("{}: {e}", saved.display()));
    println!("SAVED {} bytes", text.len());
    assert!(text.contains("wheel"), "{text}");

    r.say("Esc leaves the editor");
    r.leave();
    r.finish();
}

/// The canvas square the vehicle's first frame is on: the middle of its grid.
const VEHICLE_ORIGIN: (i32, i32) = (20, 15);

impl Rec {
    /// The middle of the canvas square `dx`, `dy` squares from the vehicle's first frame: the squares are the lattice
    /// of equal square buttons, whose top left one is the grid's first.
    fn cell_at(&mut self, x: i32, y: i32) -> egui::Pos2 {
        let lattice = |h: &H| {
            let squares = h
                .root()
                .children_recursive()
                .filter(|n| n.accesskit_node().role() == egui::accesskit::Role::Button)
                .map(|n| n.rect())
                .filter(|r| (r.width() - r.height()).abs() < 1.0 && r.width() >= 14.0)
                .collect::<Vec<_>>();
            // The squares of the canvas are the most of its width: a tool's button may be square too.
            let side = squares.iter().map(|r| r.width()).max_by_key(|w| {
                squares
                    .iter()
                    .filter(|r| (r.width() - w).abs() < 1.0)
                    .count()
            })?;
            let lattice = squares.iter().filter(|r| (r.width() - side).abs() < 1.0);
            Some(egui::Rect::from_min_size(
                egui::pos2(
                    lattice.clone().map(|r| r.min.x).fold(f32::MAX, f32::min),
                    lattice.map(|r| r.min.y).fold(f32::MAX, f32::min),
                ),
                egui::vec2(side, side),
            ))
        };
        let first = self.stable("the canvas", lattice);
        first.center() + egui::vec2(x as f32 * first.width(), y as f32 * first.height())
    }

    fn square_at(&mut self, dx: i32, dy: i32) -> egui::Pos2 {
        self.cell_at(self.origin.0 + dx, self.origin.1 + dy)
    }

    fn click_square(&mut self, dx: i32, dy: i32) {
        let at = self.square_at(dx, dy);
        self.click_at(at);
    }

    fn move_to_square(&mut self, dx: i32, dy: i32) {
        let at = self.square_at(dx, dy);
        self.move_to(at);
        self.rest();
    }

    /// How many parts the canvas shows on the square `dx`, `dy` from the first frame.
    fn parts_at(&self, dx: i32, dy: i32) -> usize {
        let id = format!("square:{},{}", VEHICLE_ORIGIN.0 + dx, VEHICLE_ORIGIN.1 + dy);
        let top = self.h.state().model.screens().last().unwrap();
        let Some(bn_client_egui::protocol::Component::Grid(grid)) =
            top.root.as_ref().unwrap().find("vehicle:grid")
        else {
            panic!("no vehicle:grid");
        };
        grid.cells
            .iter()
            .find(|c| c.id.as_deref() == Some(&id))
            .map_or(0, |c| c.looks.len())
    }

    /// One drag of `by` from the square `from` to the square `to`, a frame at each step of the way.
    fn drag(&mut self, from: (i32, i32), to: (i32, i32), by: egui::PointerButton) {
        let (a, b) = (self.square_at(from.0, from.1), self.square_at(to.0, to.1));
        self.move_to(a);
        let button = |r: &mut Self, pressed| {
            r.h.event(egui::Event::PointerButton {
                pos: r.at,
                button: by,
                pressed,
                modifiers: egui::Modifiers::NONE,
            });
            r.h.run_steps(3);
        };
        button(self, true);
        self.grab(PRESS_MS);
        // One pointer sample per square crossed: a stroke that lingers on a square may act on it more than once.
        let steps = (from.0 - to.0).abs().max((from.1 - to.1).abs()).max(1);
        for step in 1..=steps {
            self.at = a + (b - a) * (step as f32 / steps as f32);
            self.h.hover_at(self.at);
            self.h.run_steps(2);
            self.grab(PRESS_MS);
        }
        button(self, false);
        self.done();
    }

    /// Opens the palette tab `tab`, scrolls its list until the part row `part` is whole, and picks it.
    fn pick(&mut self, tab: &str, part: &str) {
        self.tab(tab);
        // The list lies under the tab strip and over the page button, right of the canvas, whose squares and
        // whose mount's stack may bear the part's name.
        let named = |h: &H, role, label: &str| {
            h.root()
                .children_recursive()
                .filter(move |n| {
                    let a = n.accesskit_node();
                    a.role() == role && a.label().as_deref() == Some(label)
                })
                .map(|n| n.rect())
                .next()
        };
        let list = |h: &H| {
            Some(egui::Rect::from_min_max(
                egui::pos2(
                    named(h, egui::accesskit::Role::Tab, "Frames")?.left() - 10.0,
                    named(h, egui::accesskit::Role::Tab, "Other")?.bottom(),
                ),
                egui::pos2(
                    h.ctx.content_rect().right(),
                    named(h, egui::accesskit::Role::Button, "Parts page")?.top(),
                ),
            ))
        };
        let row = |h: &H| {
            let list = list(h)?;
            h.root()
                .children_recursive()
                .filter(|n| {
                    let a = n.accesskit_node();
                    a.role() == egui::accesskit::Role::Button && a.label().as_deref() == Some(part)
                })
                .map(|n| n.rect())
                .find(|r| list.contains_rect(*r))
        };
        // The wheel over the list takes it down, then up again, until the row is whole in it.
        for turn in 0..120 {
            self.h.run_steps(3);
            if row(&self.h).is_some() {
                break;
            }
            let Some(list) = list(&self.h) else {
                continue;
            };
            let dy = if turn / 30 % 2 == 0 { -60.0 } else { 60.0 };
            crate::live_editor_walk::wheel_at(&mut self.h, list.center(), dy);
        }
        let at = self.stable(part, row).center();
        self.click_at(at);
    }
}
