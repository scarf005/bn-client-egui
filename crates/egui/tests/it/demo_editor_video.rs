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
        let at = self.stable(label, |h| find(h, label)).center();
        self.click_at(at);
    }

    /// Double-clicks the widget labelled `label`.
    fn double_click(&mut self, label: &str) {
        let at = self.stable(label, |h| find(h, label)).center();
        self.move_to(at);
        self.tap();
        self.tap();
        self.done();
    }

    /// Clicks the first text box labelled `label`, such as a list's search box: the palette's over the outline's.
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

    /// The wheel turned `dy` points over `at` in a few notches.
    fn wheel(&mut self, at: egui::Pos2, dy: f32) {
        self.move_to(at);
        for _ in 0..6 {
            self.h.event(egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: egui::vec2(0.0, dy / 6.0),
                modifiers: egui::Modifiers::NONE,
                phase: egui::TouchPhase::Move,
            });
            self.h.run_steps(3);
            self.grab(MOVE_MS * 2);
        }
        self.done();
    }

    /// Drags with the middle button from where the pointer is by `by`.
    fn middle_drag(&mut self, by: egui::Vec2) {
        let from = self.at;
        let button = |pos, pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Middle,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        let press = button(from, true);
        self.h.event(press);
        self.h.run_steps(2);
        for i in 1..=TRAVEL {
            self.at = from + by * (i as f32 / TRAVEL as f32);
            self.h.hover_at(self.at);
            self.h.run_steps(2);
            self.grab(MOVE_MS);
        }
        let release = button(self.at, false);
        self.h.event(release);
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
        self.say("메인 메뉴에서 에디터 탭을 연다");
        self.click("Editor");
        self.say(&format!("{entry}를 고른다"));
        self.click(entry);
        self.say("모드 목록은 그대로 두고 확인한다");
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

/// The text of the first cell of the legend's row of the symbol `#`, else of the first symbol with a terrain: what a
/// click on that row aims at, as the symbol column itself can be scrolled out of the pane.
fn legend_cell(h: &H) -> String {
    let top = h.state().model.screens().last().unwrap();
    let legend = serde_json::to_value(top.root.as_ref().unwrap().find("editor:legend")).unwrap();
    let rows = legend["rows"].as_array().unwrap();
    let cell = |r: &serde_json::Value| r["columns"][0]["value"].as_str().map(str::to_string);
    let has_terrain = |r: &&serde_json::Value| r["columns"][0]["key"] == "terrain";
    rows.iter()
        .find(|r| r["id"] == "symbol:#" && has_terrain(r))
        .or_else(|| rows.iter().find(has_terrain))
        .and_then(cell)
        .expect("a terrain in the legend")
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

fn top_kind(h: &H) -> Option<String> {
    h.state()
        .model
        .screens()
        .last()
        .and_then(|s| s.kind.clone())
}

#[test]
#[ignore = "requires BN_ENGINE pointing at an engine binary"]
fn map_editor_demo() {
    let mut r = Rec::new("map-editor");
    r.open_editor("Map editor", "editor");

    r.say("시작 화면의 파일 목록에서 맵젠 파일을 찾는다");
    r.click_input("Search");
    r.type_slowly("house/2story01");
    r.key(egui::Key::Enter);
    r.say("파일을 더블클릭해 연다");
    r.double_click("2Story01.json");
    r.wait_kind("editor");
    run_until(&mut r.h, "the opened canvas", 60, |h| {
        canvas_glyphs(h).len() == 24
    });
    r.done();

    let centre = r
        .stable("a canvas square", |h| Some(square(h, 12, 12)))
        .center();
    r.say("범례에서 벽 기호를 고른다");
    let cell = legend_cell(&r.h);
    r.double_click(&cell);
    let symbol = selected_symbol(&r.h);

    r.say("연필 도구로 한 칸을 칠한다");
    r.click("Pencil");
    r.paint(2, 13);
    run_until(&mut r.h, "the pencil square", 30, |h| {
        canvas_glyphs(h)[13]
            .chars()
            .nth(2)
            .map(String::from)
            .as_deref()
            == Some(symbol.as_str())
    });
    r.say("사각형 도구로 두 칸을 눌러 방을 그린다");
    r.click("Rectangle");
    r.paint(4, 15);
    r.paint(9, 18);
    let drawn = canvas_glyphs(&r.h);
    assert_eq!(
        drawn[18].chars().nth(9).map(String::from),
        Some(symbol.clone())
    );

    r.say("편집 메뉴에서 되돌린다");
    r.menu("Edit", "Undo");
    assert_ne!(canvas_glyphs(&r.h), drawn, "the undo changed nothing");
    r.say("Ctrl+Y로 다시 실행한다");
    r.key_mod(egui::Modifiers::CTRL, egui::Key::Y);
    assert_eq!(canvas_glyphs(&r.h), drawn, "the redo changed nothing");

    r.say("휠로 캔버스를 확대한다");
    r.wheel(centre, 240.0);
    r.say("가운데 버튼으로 끌어 캔버스를 옮긴다");
    r.middle_drag(egui::vec2(-160.0, 90.0));
    r.say("휠로 다시 전체가 보이게 줄인다");
    let at = r.at;
    r.wheel(at, -2000.0);

    r.say("파일 메뉴에서 모드 폴더에 다른 이름으로 저장한다");
    let path = crate::common::live::profile_dir().join("mods/zz_demo_house.json");
    r.menu("File", "Save as");
    r.key_mod(egui::Modifiers::COMMAND, egui::Key::A);
    r.paste(&path.to_string_lossy());
    r.key(egui::Key::Enter);
    assert!(path.is_file(), "no {}", path.display());

    r.say("월드 메뉴의 Test로 게임 안에서 확인한다");
    r.menu("World", "Test");
    run_until(&mut r.h, "the map", 120, |h| {
        !h.state().model.busy() && top_kind(h).as_deref() != Some("editor")
    });
    r.done();
    r.say("맵의 Map Editor 항목으로 에디터에 돌아온다");
    r.click("Map Editor");
    r.wait_kind("editor");

    r.say("Esc로 에디터를 나가 메인 메뉴로 돌아온다");
    r.leave();
    r.finish();
}

#[test]
#[ignore = "requires BN_ENGINE pointing at an engine binary"]
fn vehicle_editor_demo() {
    let mut r = Rec::new("vehicle-editor");
    r.open_editor("Vehicle editor", "vehicle_editor");

    r.say("목록을 검색해 원형 차량을 찾는다");
    r.click_input("Search");
    r.type_slowly("bicycle");
    r.key(egui::Key::Enter);
    r.say("차량을 더블클릭해 연다");
    r.double_click("Bicycle");
    r.wait_kind("vehicle_editor");

    r.say("장착 칸을 눌러 고른다");
    r.click("bicycle wheel");
    r.say("부품 팔레트를 검색한다");
    r.click_input("Search");
    r.type_slowly("headlight");
    r.key(egui::Key::Enter);
    r.say("부품을 더블클릭해 설치한다");
    r.double_click("headlight");
    r.say("편집 메뉴에서 되돌린다");
    r.menu("Edit", "Undo");
    r.say("팔레트에서 다시 설치한다");
    if find(&r.h, "headlight").is_none() {
        r.click_input("Search");
        r.type_slowly("headlight");
        r.key(egui::Key::Enter);
    }
    r.double_click("headlight");

    r.say("월드 메뉴의 Spawn으로 차량을 게임에 놓는다");
    r.menu("World", "Spawn");
    run_until(&mut r.h, "the map", 120, |h| {
        !h.state().model.busy() && top_kind(h).as_deref() != Some("vehicle_editor")
    });
    r.done();
    r.say("맵의 Map Editor 항목으로 에디터에 돌아온다");
    r.click("Map Editor");
    r.wait_kind("editor");
    r.say("Esc로 에디터를 나가 메인 메뉴로 돌아온다");
    r.leave();
    r.finish();
}
