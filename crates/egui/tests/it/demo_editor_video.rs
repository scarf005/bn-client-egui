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

    /// Ctrl+X, Ctrl+C or Ctrl+V as the platform hands them over: as the clipboard's command `event`.
    fn clipboard(&mut self, event: egui::Event) {
        self.h.event(event);
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

/// The terrain cell of the first row of the legend in view whose terrain is a wall, else of the first with a
/// terrain: what a click on that row aims at.
fn legend_cell(h: &H) -> String {
    let top = h.state().model.screens().last().unwrap();
    let legend = serde_json::to_value(top.root.as_ref().unwrap().find("editor:legend")).unwrap();
    let rows = legend["rows"].as_array().unwrap();
    let terrains = rows
        .iter()
        .filter_map(|r| {
            r["columns"]
                .as_array()
                .and_then(|c| c.iter().find(|c| c["key"] == "terrain"))
                .and_then(|c| c["value"].as_str())
                .filter(|t| !t.is_empty() && find(h, t).is_some())
                .map(str::to_string)
        })
        .collect::<Vec<_>>();
    terrains
        .iter()
        .find(|t| t.contains("wall"))
        .or(terrains.first())
        .cloned()
        .expect("a terrain in view in the legend")
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

/// Whether egui's keyboard focus is on the widget named `label`.
fn focused_on(h: &H, label: &str) -> bool {
    let Some(id) = h.ctx.memory(|m| m.focused()) else {
        return false;
    };
    let rect = h
        .ctx
        .viewport(|v| v.prev_pass.widgets.get(id).map(|w| w.rect));
    h.root()
        .children_recursive()
        .any(|n| n.accesskit_node().label().as_deref() == Some(label) && Some(n.rect()) == rect)
}

/// The text of the component `id` of the screen on top, as the engine sent it.
fn engine_text(h: &H, id: &str) -> String {
    let top = h.state().model.screens().last().unwrap();
    serde_json::to_string(&top.root.as_ref().and_then(|r| r.find(id))).unwrap()
}

/// Why the screen on top refuses its affordance `id`.
fn refusal(h: &H, id: &str) -> String {
    let top = h.state().model.screens().last().unwrap();
    let a = top.affordances.iter().find(|a| a.id == id).unwrap();
    assert!(!a.enabled, "{id} is not refused");
    a.denial.as_ref().map(|d| d.shown()).unwrap_or_default()
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

    /// Presses Tab until the canvas has the keyboard.
    fn tab_to_canvas(&mut self) {
        for _ in 0..16 {
            if focused_on(&self.h, "Canvas") {
                return;
            }
            self.key(egui::Key::Tab);
        }
        panic!("Tab does not reach the canvas: {:?}", labels(&self.h));
    }
}

#[test]
#[ignore = "requires BN_ENGINE pointing at an engine binary"]
fn map_editor_demo() {
    let mut r = Rec::new("map-editor");
    r.open_editor("Map editor", "editor");

    r.say("World 메뉴의 Open overmap terrain에 지도에 보이는 이름 house를 적어 연다");
    r.menu("World", "Open overmap terrain");
    r.type_slowly("house");
    r.key(egui::Key::Enter);
    run_until(&mut r.h, "the opened house", 60, |h| {
        !rows_of(h, "editor:objects").is_empty() && !canvas_glyphs(h).is_empty()
    });
    r.done();
    println!("ENTRIES {:?}", rows_of(&r.h, "editor:objects"));
    println!("STATUS {}", engine_text(&r.h, "editor:status"));

    r.say("Entries 목록에서 항목을 누르면 그 항목이 캔버스에 열린다");
    let entries = rows_of(&r.h, "editor:objects");
    r.click(entries.last().unwrap());
    r.grab(HOLD_MS);
    r.click(&entries[0]);

    r.say("범례에서 기호를 고르고 연필로 칸을 마우스로 칠한다");
    let cell = legend_cell(&r.h);
    r.double_click(&cell);
    let symbol = selected_symbol(&r.h);
    r.tab("Pencil");
    r.paint(2, 2);
    r.paint(3, 2);
    assert_eq!(
        canvas_glyphs(&r.h)[2].chars().nth(3).map(String::from),
        Some(symbol.clone()),
        "the pencil did not draw"
    );

    r.say("Tab으로 캔버스에 가서 방향키로 옮기고 Enter로 칠한다");
    r.tab_to_canvas();
    for arrow in [
        egui::Key::ArrowRight,
        egui::Key::ArrowRight,
        egui::Key::ArrowDown,
    ] {
        r.key(arrow);
    }
    let before = canvas_glyphs(&r.h);
    r.key(egui::Key::Enter);
    assert_ne!(canvas_glyphs(&r.h), before, "Return drew nothing");

    r.say("고른 칸 없이 Ctrl+C를 누르면 엔진이 거절한 이유가 아래 줄에 나온다");
    let why = refusal(&r.h, "editor:copy");
    r.clipboard(egui::Event::Copy);
    assert!(
        find(&r.h, &why).is_some(),
        "no refusal {why:?}: {:?}",
        labels(&r.h)
    );
    r.grab(HOLD_MS * 3);

    r.say("선택 도구로 두 칸을 눌러 영역을 고르고 Ctrl+X로 잘라낸다");
    r.tab("Select");
    r.paint(1, 1);
    r.paint(4, 3);
    r.clipboard(egui::Event::Cut);
    let cut = canvas_glyphs(&r.h);
    r.say("붙여넣기 도구로 다른 곳을 눌러 붙인다");
    r.tab("Paste");
    r.paint(10, 10);
    assert_ne!(canvas_glyphs(&r.h), cut, "the paste changed nothing");

    r.say("Ctrl+Z로 되돌리면 상태 줄이 무엇을 되돌렸는지 말한다");
    r.key_mod(egui::Modifiers::CTRL, egui::Key::Z);
    assert!(
        engine_text(&r.h, "editor:status").contains("Undid"),
        "{}",
        engine_text(&r.h, "editor:status")
    );
    r.done();

    r.say("File 메뉴의 Save as로 모드 폴더에 저장한다");
    let path = crate::common::live::profile_dir().join("mods/zz_demo_house.json");
    r.menu("File", "Save as");
    r.key_mod(egui::Modifiers::COMMAND, egui::Key::A);
    r.paste(&path.to_string_lossy());
    r.key(egui::Key::Enter);
    assert!(path.is_file(), "no {}", path.display());

    r.say("Esc로 에디터를 나가 메인 메뉴로 돌아온다");
    r.leave();
    r.finish();
}

#[test]
#[ignore = "requires BN_ENGINE pointing at an engine binary"]
fn vehicle_editor_demo() {
    let mut r = Rec::new("vehicle-editor");
    r.open_editor("Vehicle editor", "vehicle_editor");

    r.say("File 메뉴의 New vehicle은 비어 있는 파일 이름을 미리 적어 준다");
    r.menu("File", "New vehicle");
    r.grab(HOLD_MS * 2);
    r.key(egui::Key::Enter);
    run_until(&mut r.h, "the new vehicle", 60, |h| {
        !rows_of(h, "vehicle:history").is_empty()
    });
    r.done();

    r.say("부품 팔레트에서 프레임을 골라 장착 칸 두 곳을 눌러 단다");
    r.install("frame_vertical", "frame", &["mount:0,0", "mount:1,0"]);
    r.say("트렁크, 포탑 받침, 포탑을 골라 단다");
    r.install("trunk", "trunk", &["mount:0,0"]);
    r.install("turret_mount", "turret mount", &["mount:1,0"]);
    r.install("m249", "mounted M249", &["mount:1,0"]);
    println!("HISTORY {:?}", rows_of(&r.h, "vehicle:history"));

    r.say("History 목록의 한 단계를 누르면 그때로 돌아가고, 마지막 단계로 다시 온다");
    let steps = rows_of(&r.h, "vehicle:history");
    r.double_click(&steps[1]);
    r.double_click(steps.last().unwrap());

    r.say("트렁크 칸의 Items here 탭은 아이템 팔레트를 열어 이 칸에 생길 아이템을 단다");
    r.mount("mount:0,0");
    r.tab("Items here");
    let item = rows_of(&r.h, "vehicle:item_catalog")
        .first()
        .cloned()
        .expect("an item palette");
    r.double_click(&item);

    r.say("World 메뉴의 Spawn으로 차량을 게임 맵의 플레이어 옆에 놓는다");
    r.menu("World", "Spawn");
    run_until(&mut r.h, "the map", 120, |h| {
        !h.state().model.busy()
            && h.state()
                .model
                .screens()
                .last()
                .is_some_and(|s| s.kind.as_deref() != Some("vehicle_editor"))
    });
    r.grab(HOLD_MS * 3);
    r.done();

    r.say("맵 에디터로 돌아와 World 메뉴에서 차량 에디터를 다시 연다");
    r.click("Map Editor");
    r.wait_kind("editor");
    r.menu("World", "Vehicle editor");
    r.wait_kind("vehicle_editor");

    r.say("World 탭에서 놓인 차량을 골라 Edit in place로 그 자리에서 고친다");
    r.tab("World");
    println!("WORLD {:?}", rows_of(&r.h, "vehicle:world"));
    let near = rows_of(&r.h, "vehicle:world")
        .into_iter()
        .find(|row| row.contains("my_vehicle"))
        .expect("the spawned vehicle in the world list");
    r.click_button(&near);
    r.say("Copy vehicle과 Paste vehicle은 복사본을 놓고 몇 칸 어디에 놓였는지 말한다");
    r.click("Copy vehicle");
    r.menu("World", "Paste vehicle");
    let pasted = engine_text(&r.h, "vehicle:status");
    println!("STATUS {pasted}");
    assert!(pasted.contains("Pasted"), "{pasted}");
    r.grab(HOLD_MS * 2);
    r.say("놓인 차량을 골라 Edit in place로 그 자리에서 고친다");
    r.click_button(&near);
    r.click("Edit in place");
    r.say("놓인 차량의 포탑을 골라 탄약을 채운다");
    r.mount("mount:1,0");
    println!("STACK {:?}", rows_of(&r.h, "vehicle:stack"));
    let turret = rows_of(&r.h, "vehicle:stack")
        .into_iter()
        .find(|row| row.contains("M249"))
        .expect("the turret in the stack");
    r.double_click(&turret);
    r.tab("Part condition");
    println!("FORM {}", engine_text(&r.h, "vehicle:live_form"));
    if !engine_text(&r.h, "vehicle:live_form").contains("\"value\":\"556") {
        r.click_input("Ammunition");
        r.key_mod(egui::Modifiers::COMMAND, egui::Key::A);
        r.type_slowly("556");
        r.key(egui::Key::Enter);
    }
    r.click_input("Rounds");
    r.key_mod(egui::Modifiers::COMMAND, egui::Key::A);
    r.type_slowly("100");
    r.key(egui::Key::Enter);
    println!("HISTORY {:?}", rows_of(&r.h, "vehicle:history"));
    println!("FORM {}", engine_text(&r.h, "vehicle:live_form"));
    r.grab(HOLD_MS * 2);

    r.say("World 메뉴의 Done으로 제자리 편집을 마친다");
    r.menu("World", "Done");
    println!("LABELS {:?}", labels(&r.h));
    r.done();

    r.say("Esc로 에디터를 나가 메인 메뉴로 돌아온다");
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

    /// Searches the parts palette for `search`, picks the row named `part` and clicks each of `mounts` to install it
    /// there.
    fn install(&mut self, search: &str, part: &str, mounts: &[&str]) {
        self.click_input("Search");
        self.key_mod(egui::Modifiers::COMMAND, egui::Key::A);
        self.type_slowly(search);
        self.key(egui::Key::Enter);
        assert!(
            rows_of(&self.h, "vehicle:parts").iter().any(|r| r == part),
            "no part {part} in the palette: {:?}",
            rows_of(&self.h, "vehicle:parts")
        );
        self.double_click(part);
        for id in mounts {
            self.mount(id);
        }
        println!(
            "INSTALLED {part}: {}",
            engine_text(&self.h, "vehicle:notice")
        );
    }
}
