use crate::a11y;
use crate::display::Display;
use crate::effects::Effects;
use crate::iso::{self, View3d};
use crate::text;
use crate::windows::{self, Origin, Shown, WindowSpec};
use bn_client_core::protocol::SidebarSide;
use bn_client_core::{Failure, GameModel, Loading, Pos, interaction, keys};
use eframe::egui::{self, accesskit};

mod component;
mod menu;
mod palette;
mod sidebar;
mod sounds;

/// A command must outlast this before the controls show it; keeps short commands from blinking the UI.
const SETTLE: std::time::Duration = std::time::Duration::from_millis(150);

/// The sidebar's initial share of the window width, and its limits in points; independent of its content.
const SIDEBAR_SHARE: f32 = 0.28;
/// 430 pt holds the longest status line (R LEG ... Stm 10000/10000) on one line; a small window gives up at most 40%.
const SIDEBAR_WIDTH: (f32, f32) = (430.0, 600.0);
/// The most of the window width the sidebar's minimum may take.
const SIDEBAR_MIN_SHARE: f32 = 0.4;
/// Points around the columns of a native sidebar: panel margins and the scroll bar.
const SIDEBAR_CHROME: f32 = 28.0;
/// The most of the window width the sidebar may be dragged to; a saved wider size shrinks to it.
const SIDEBAR_MAX_SHARE: f32 = 0.4;

/// Side length of one map cell in points.
pub const CELL: f32 = 16.0;

fn rgb(c: bn_client_core::Rgb) -> egui::Color32 {
    egui::Color32::from_rgb(c.0, c.1, c.2)
}

impl<M: GameModel> App<M> {
    /// A window of `kind` titled `title`, where the player left windows of its kind, else at the top centre,
    /// 12% down, so the same click lands on the same control.
    fn window_spec(&self, ctx: &egui::Context, kind: &str, title: &str) -> WindowSpec {
        WindowSpec {
            kind: kind.into(),
            title: title.into(),
            id: a11y::dialog_id(title),
            saved: self.display.window(kind),
            origin: Origin::top_center(ctx),
            place: None,
            movable: false,
            width: None,
            height: None,
            max_width: None,
            frame: None,
            title_bar: !title.is_empty(),
            closable: false,
            help: None,
        }
    }

    /// Remembers where the player left the window just shown, written once the pointer is up; true when its close
    /// button was pressed.
    fn keep_window<R>(&mut self, ctx: &egui::Context, kind: &str, shown: &Shown<R>) -> bool {
        if let Some(state) = shown.state {
            let settled = !ctx.input(|i| i.pointer.any_down());
            self.display.keep_window(kind, state, settled);
        }
        shown.closed
    }
}

/// The tallest a 0.1 menu's list grows: the room under the window's top, less its title, tabs and description.
fn dialog_scroll_height(ctx: &egui::Context) -> f32 {
    (ctx.content_rect().height() * 0.88 - 150.0).max(150.0)
}

/// Names the drag handle of the panel with this id for screen readers.
fn label_resize_handle(ctx: &egui::Context, panel: &str, label: &str) {
    ctx.accesskit_node_builder(egui::Id::new(panel).with("__resize"), |b| {
        b.set_role(accesskit::Role::Splitter);
        b.set_label(label);
    });
}

/// A menu button that screen readers announce as a menu item.
fn menu_item(ui: &mut egui::Ui, text: &str) -> bool {
    let button = ui.button(text);
    ui.ctx()
        .accesskit_node_builder(button.id, |b| b.set_role(accesskit::Role::MenuItem));
    button.clicked()
}

pub struct App<M: GameModel> {
    pub model: M,
    cursor: Pos,
    menu_cell: Pos,
    /// Absolute square of the last map click, marked until its command finishes.
    marker: Option<Pos>,
    /// When the command now in flight was first seen; short ones never change the controls.
    pending_since: Option<std::time::Duration>,
    clock: bn_client_core::Clock,
    last_hover: Option<egui::Pos2>,
    /// Wheel travel over the map short of a whole zoom step, in lines.
    wheel: f32,
    wired: bool,
    images: bool,
    /// Game root, for files the engine names relative to it (loading images).
    root: Option<std::path::PathBuf>,
    /// Where the display choice is kept; none keeps it only for this run.
    settings: Option<std::path::PathBuf>,
    display: Display,
    /// A typed key already ran an engine action this frame.
    keys_consumed: bool,
    /// The physical key whose press started the held walk; the walk ends when it is up.
    walk_key: Option<egui::Key>,
    /// The action palette's search text while it is open.
    palette: Option<String>,
    /// The client settings window is open.
    client_settings: bool,
    /// The message log's height once the user has dragged its edge.
    log_height: Option<f32>,
    /// Title of the menu that last took focus, so a menu is entered once, not on every frame.
    menu_title: Option<String>,
    /// A menu opened and its first control has not taken focus yet (it may be disabled for a frame).
    focus_menu: bool,
    /// The menu just closed; the map takes focus back.
    refocus_map: bool,
    /// What the two message lists announce, so a burst in one frame is read whole.
    native_live: a11y::Announcer,
    command_live: a11y::Announcer,
    status_live: a11y::Announcer,
    /// The enabled controls of the open menu, in Tab order, as of the last frame.
    menu_ids: Vec<egui::Id>,
    /// The controls of the open menu that stand for an engine action, for Alt+Up and Alt+Down.
    action_ids: Vec<egui::Id>,
    /// The same for the action palette.
    palette_ids: Vec<egui::Id>,
    /// Where the engine's cursor was in the open menu, so keyboard focus can follow it.
    menu_focus: Option<String>,
    /// The engine's text prompt as last seen (field id, value), and the text typed into it.
    prompt: ((String, String), String),
    /// The newest prompt text not yet sent, and whether Enter asked to submit it.
    prompt_send: Option<(String, String, bool)>,
    /// The texts sent to the prompt since it opened, whose echoes from the engine are not news.
    prompt_sent: Vec<String>,
    /// An IME composition is open in the prompt: its preedit text is not typed yet.
    prompt_composing: bool,
    /// Shots, explosions and floating text now playing on the map.
    effects: Effects,
    /// The map as a 3D scene, when asked for.
    iso: View3d,
    /// The client-local state of the top screen when the engine describes it by components.
    screens: component::Screens,
    /// The sidebar as last drawn: the side it is on and its rectangle, which a docked window sits beside.
    sidebar: Option<(SidebarSide, egui::Rect)>,
    /// The room the map panel took in the last frame: where screens and dialogs sit.
    region: Option<egui::Rect>,
    /// The map canvas's top left corner and a square's side in the last frame, to place a window off the cursor.
    map_view: Option<(egui::Pos2, f32)>,
    /// The engine's selected row of the open menu, to keep it in view when the wheel moves it.
    menu_cursor: Option<String>,
    /// The text prompt that already took the keyboard focus when it opened.
    prompt_focused: Option<String>,
    /// The engine's sound cues on the audio device.
    audio: crate::audio::Audio,
    /// The loading step the list was last scrolled to; scrolling again every frame would repaint forever.
    scrolled_to: Option<usize>,
}

impl<M: GameModel> App<M> {
    pub fn new(model: M) -> Self {
        let cursor = model.player();
        Self {
            model,
            cursor,
            menu_cell: cursor,
            marker: None,
            pending_since: None,
            clock: bn_client_core::Clock::default(),
            last_hover: None,
            wheel: 0.0,
            wired: false,
            images: false,
            root: None,
            settings: None,
            display: Display::new(None, None),
            keys_consumed: false,
            walk_key: None,
            palette: None,
            client_settings: false,
            log_height: None,
            menu_title: None,
            focus_menu: false,
            refocus_map: false,
            prompt: Default::default(),
            prompt_send: None,
            prompt_sent: Vec::new(),
            prompt_composing: false,
            effects: Effects::default(),
            iso: View3d::default(),
            menu_ids: Vec::new(),
            action_ids: Vec::new(),
            palette_ids: Vec::new(),
            menu_focus: None,
            native_live: a11y::Announcer::new(" "),
            command_live: a11y::Announcer::new(" "),
            status_live: a11y::Announcer::new(", "),
            screens: Default::default(),
            sidebar: None,
            region: None,
            map_view: None,
            menu_cursor: None,
            prompt_focused: None,
            audio: Default::default(),
            scrolled_to: None,
        }
    }

    /// The clicked map square in view coordinates while its command is in flight.
    pub fn marker(&self) -> Option<Pos> {
        let origin = self.model.origin();
        let m = self.marker.filter(|_| self.model.busy())?;
        Some(Pos {
            x: m.x - origin.x,
            y: m.y - origin.y,
        })
    }

    /// The pass format the 3D map is drawn into; the window's renderer tells it, so only a test sets it.
    pub fn with_target_format(mut self, format: egui_wgpu::wgpu::TextureFormat) -> Self {
        self.iso.target = Some(format);
        self
    }

    /// The 3D map's camera, which the right button and Alt+Left and Alt+Right turn.
    pub fn camera(&self) -> iso::camera::Camera {
        self.iso.camera
    }

    /// What the last paint of the 3D map drew.
    pub fn iso_stats(&self) -> &iso::gpu::Stats {
        &self.iso.stats
    }

    /// Times the settle delay with `clock`, so tests can advance it instead of sleeping.
    pub fn with_clock(mut self, clock: bn_client_core::Clock) -> Self {
        self.clock = clock;
        self
    }

    pub fn clock(&self) -> &bn_client_core::Clock {
        &self.clock
    }

    pub fn with_root(mut self, root: std::path::PathBuf) -> Self {
        self.root = Some(root);
        self.display = Display::new(self.root.as_deref(), self.settings.clone());
        self
    }

    /// Keeps the tiles or ASCII choice and the tileset in this file between runs.
    pub fn with_settings(mut self, path: std::path::PathBuf) -> Self {
        self.settings = Some(path);
        self.display = Display::new(self.root.as_deref(), self.settings.clone());
        self
    }

    /// How many map animations are playing now.
    pub fn playing_effects(&self) -> usize {
        self.effects.len()
    }

    /// The map display: tiles or ASCII, and the tileset.
    pub fn display(&self) -> &Display {
        &self.display
    }

    /// `text` with its colour tags drawn as colours.
    fn rich(&self, ui: &egui::Ui, text: &str) -> egui::WidgetText {
        // Text the engine padded into `|` separated columns lines up only in a fixed-width font.
        let style = if text.contains(" | ") {
            egui::TextStyle::Monospace
        } else {
            egui::TextStyle::Body
        };
        let font = style.resolve(ui.style());
        text::job(&self.model.styled(text), &font).into()
    }

    /// Native-style loading screen: splash image, current step as live text, the step list.
    /// The engine is gone or silent: say so with its last stderr lines, and offer a restart.
    fn failure(&mut self, ui: &mut egui::Ui, failure: &Failure) {
        let heading = ui.heading(&failure.message);
        ui.ctx()
            .accesskit_node_builder(heading.id, |b| b.set_live(accesskit::Live::Assertive));
        if let Some(engine) = &failure.engine {
            ui.label(format!("Engine: {}", engine.display()));
        }
        if !failure.stderr.is_empty() {
            ui.label("Last engine output:");
            egui::ScrollArea::vertical().show(ui, |ui| {
                for line in &failure.stderr {
                    ui.monospace(line);
                }
            });
        }
        let restart = ui.button("Restart");
        if restart.clicked() {
            self.model.restart();
        }
    }

    fn loading(&mut self, ui: &mut egui::Ui, loading: &Loading) {
        ui.vertical_centered(|ui| self.loading_content(ui, loading));
    }

    fn loading_content(&mut self, ui: &mut egui::Ui, loading: &Loading) {
        ui.heading(&loading.title);
        if let Some(step) = loading.entries.get(loading.index) {
            let status = format!(
                "{}: {step} ({} of {})",
                loading.title,
                loading.index + 1,
                loading.entries.len()
            );
            let live = ui.label(status);
            ui.ctx()
                .accesskit_node_builder(live.id, |b| b.set_live(accesskit::Live::Polite));
        }
        if let (Some(root), Some(image)) = (&self.root, &loading.image) {
            let uri = format!("file://{}", root.join(image).display());
            ui.add(
                egui::Image::new(uri)
                    .alt_text("Loading image")
                    .max_size(egui::vec2(
                        ui.available_width(),
                        ui.available_height() * 0.6,
                    ))
                    .maintain_aspect_ratio(true),
            );
        }
        egui::ScrollArea::vertical().show(ui, |ui| {
            for (i, entry) in loading.entries.iter().enumerate() {
                let text = egui::RichText::new(entry);
                let label = ui.label(if i == loading.index {
                    text.strong()
                } else {
                    text
                });
                if i == loading.index && self.scrolled_to.replace(i) != Some(i) {
                    label.scroll_to_me(None);
                }
            }
        });
    }

    pub fn show(&mut self, ui: &mut egui::Ui) {
        profiling::scope!("App::show");
        if !self.wired {
            let ctx = ui.ctx().clone();
            self.model
                .set_waker(std::sync::Arc::new(move || ctx.request_repaint()));
            self.wired = true;
        }
        self.model.poll();
        if let Some(soon) = self
            .audio
            .update(self.model.soundpack(), self.model.take_cues())
        {
            ui.ctx().request_repaint_after(soon);
        }
        self.display.follow_engine(self.model.display_default());
        self.display.follow_mods(self.model.mod_tilesets());
        self.model.set_tiles(self.display.tiles());
        self.model.set_tint_ids(self.display.tint_ids());
        a11y::name_window(ui.ctx(), "Cataclysm: Bright Nights client");
        self.pending_since = self
            .model
            .busy()
            .then(|| self.pending_since.unwrap_or_else(|| self.clock.now()));
        if !self.images {
            egui_extras::install_image_loaders(ui.ctx());
            self.images = true;
        }
        if let Some(failure) = self.model.failure() {
            self.failure(ui, &failure);
            return;
        }
        if let Some(loading) = self.model.loading() {
            self.loading(ui, &loading);
            return;
        }
        self.scrolled_to = None;
        if self.model.live() {
            // Safety net only: incoming engine messages repaint through the waker.
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_secs(1));
        }
        if self.pending_since.is_some() && !self.settled() {
            ui.ctx().request_repaint_after(SETTLE);
        }
        if interaction::titled(self.model.screens()) {
            // The game is over: the next one starts with the cursor on its player and and no travel marker.
            self.marker = None;
        }
        // Esc interrupts a running activity; with none running it goes to the game like any key
        // (leaves look mode, acknowledges a popup, else opens the main menu).
        let interrupted =
            ui.ctx().input(|i| i.key_pressed(egui::Key::Escape)) && self.model.interrupt();
        self.palette_keys(ui.ctx());
        // The palette and an open menu of the client (the map's right-click menu) have the keyboard; Escape closes
        // them.
        let client = self.palette.is_some() || egui::Popup::is_any_open(ui.ctx());
        match component::sliced(self.model.screens()) {
            _ if client => {}
            Some(_) => self.component_keys(ui.ctx()),
            None => self.game_keys(ui.ctx(), interrupted),
        }
        // Whichever screen took the keys, the walk ends with its key.
        self.release_hold_when_up(ui.ctx());
        let open = self.model.menu();
        if menu_forwards_keys(open.as_ref()) {
            // Arrow keys belong to the game, whichever widget was clicked last; Tab still moves focus,
            // except on a tabbed screen where it is the engine's tab key.
            let tab = open.as_ref().is_some_and(tab_is_engines);
            let ctx = ui.ctx();
            if let Some(id) = ctx.memory(|m| m.focused()) {
                ctx.memory_mut(|m| {
                    m.set_focus_lock_filter(
                        id,
                        egui::EventFilter {
                            tab,
                            horizontal_arrows: true,
                            vertical_arrows: true,
                            ..Default::default()
                        },
                    )
                });
            }
        }
        egui::Panel::top("bar").show(ui, |ui| self.bar(ui));
        // Without a map the room is what the top bar leaves; with one, what the sidebar leaves.
        self.region = Some(ui.available_rect_before_wrap());
        if interaction::titled(self.model.screens()) {
            // The title screen is only its menu: no map, sidebar or log of a game that is not running.
            return self.windows(ui.ctx());
        }
        let room = ui.available_width();
        let layout = self.model.sidebar_layout();
        let narrowest = SIDEBAR_WIDTH.0.min(room * SIDEBAR_MIN_SHARE);
        // The game's panel options set the width and the side; a changed width is a new panel, which starts
        // at the new width. The sidebar's chrome (margins, scroll bar) takes SIDEBAR_CHROME on top of its columns.
        let native = layout.as_ref().map(|l| {
            let width = crate::sidebar::width_of(ui, l.width) + SIDEBAR_CHROME;
            (l.side, l.width, width)
        });
        let narrowest = native.map_or(narrowest, |(_, _, w)| narrowest.min(w));
        // Never narrower than its minimum, and never more than the share that leaves the map most of the window.
        let widest = (room * SIDEBAR_MAX_SHARE).clamp(narrowest, SIDEBAR_WIDTH.1);
        let initial = native.map_or(room * SIDEBAR_SHARE, |(_, _, w)| w);
        let name = native.map_or("sidebar".to_string(), |(side, cols, _)| {
            format!("sidebar-{side:?}-{cols}")
        });
        let panel = match native {
            Some((SidebarSide::Left, ..)) => egui::Panel::left(egui::Id::new(&name)),
            _ => egui::Panel::right(egui::Id::new(&name)),
        };
        let side = native.map_or(SidebarSide::Right, |(side, ..)| side);
        let sidebar = panel
            .default_size(initial.clamp(narrowest, widest))
            .size_range(narrowest..=widest)
            .show(ui, |ui| {
                profiling::scope!("sidebar");
                // The panel is as wide as it was given, never as wide as its widest line: what does not fit is
                // cut at its edge, not allowed to push the panel over the map.
                let rect = ui.available_rect_before_wrap();
                let mut inner = ui.new_child(egui::UiBuilder::new().max_rect(rect));
                inner.set_clip_rect(rect.intersect(ui.clip_rect()));
                self.sidebar_panel(&mut inner, layout.as_ref());
                ui.allocate_rect(rect, egui::Sense::hover());
            });
        self.sidebar = Some((side, sidebar.response.rect));
        label_resize_handle(ui.ctx(), &name, "Resize sidebar");
        crate::toolbar::show(ui, &mut self.model);
        self.region = Some(ui.available_rect_before_wrap());
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(egui::Color32::BLACK))
            .show(ui, |ui| {
                profiling::scope!("map");
                self.map(ui)
            });
        self.windows(ui.ctx());
    }

    /// The windows over the game, and over the title screen: the action palette, the client settings and the
    /// engine's menu or screen.
    fn windows(&mut self, ctx: &egui::Context) {
        self.palette(ctx);
        self.client_settings(ctx);
        self.menu_or_screen(ctx);
    }

    /// The client's own choices (view mode, tileset, motion), which are no part of the game.
    fn client_settings(&mut self, ctx: &egui::Context) {
        if !self.client_settings {
            return;
        }
        let spec = WindowSpec {
            closable: true,
            id: a11y::dialog_id("Client settings"),
            ..self.window_spec(ctx, "client-settings", "Client settings")
        };
        self.client_settings = !windows::show(ctx, spec, |ui| self.display.controls(ui)).closed;
    }

    /// Typed characters and special keys are game keys; the first one that runs an action wins.
    /// Named like the engine's key names (`SPACE`, `ESC`, `RETURN`, `UP`, `PPAGE`).
    fn game_keys(&mut self, ctx: &egui::Context, interrupted: bool) {
        use egui::Key;
        let typed = ctx.input(|i| {
            i.events
                .iter()
                .filter_map(|e| match e {
                    egui::Event::Text(t) => Some(if t == " " { "SPACE".into() } else { t.clone() }),
                    _ => None,
                })
                .collect::<Vec<_>>()
        });
        let menu = self.model.menu();
        // Zoom is the client's own view; the keys the engine binds to its zoom actions never reach it.
        let typed = if menu.is_none() {
            let actions = self.model.actions();
            let zoom = |token: &str| {
                let bound = actions.iter().find(|a| a.keys.iter().any(|k| k == token));
                bound.and_then(|a| match a.id.as_str() {
                    "zoom_in" => Some(1),
                    "zoom_out" => Some(-1),
                    _ => None,
                })
            };
            let (zooms, rest): (Vec<_>, Vec<_>) =
                typed.into_iter().partition(|t| zoom(t).is_some());
            self.display
                .zoom_by(zooms.iter().filter_map(|t| zoom(t)).sum());
            rest
        } else {
            typed
        };
        // In the 3D map Alt+Left and Alt+Right turn the camera, unless the engine binds those keys; no other key
        // of the game is taken.
        let turning = menu.is_none() && self.iso.active(&self.display);
        let turn_key = |key: egui::Key, mods: egui::Modifiers| {
            let name = match key {
                egui::Key::ArrowLeft => "LEFT",
                egui::Key::ArrowRight => "RIGHT",
                _ => return None,
            };
            let chord = format!("ALT+{name}");
            (turning && mods.alt && !self.model.key_bound(&chord)).then(|| {
                if key == egui::Key::ArrowLeft {
                    1.0
                } else {
                    -1.0
                }
            })
        };
        let turns = ctx.input(|i| {
            i.events
                .iter()
                .filter_map(|e| match e {
                    egui::Event::Key {
                        key,
                        pressed: true,
                        modifiers,
                        ..
                    } => turn_key(*key, *modifiers),
                    _ => None,
                })
                .sum::<f32>()
        });
        self.iso.camera.orbit(turns * iso::TURN, 0.0);
        // A menu is an engine input context: its navigation, paging, confirm and cancel keys are the
        // player's bindings, and the engine moves its own cursor. The exceptions are the toolkit's
        // accessibility keys: Tab and Shift+Tab move focus, and Space and Enter act on a focused widget.
        // Menus with a text field keep the keyboard for typing.
        // Only a focused control of the menu itself takes Space and Enter; with the focus left on the
        // map, they are the engine's (they close a popup).
        let focused = ctx
            .memory(|m| m.focused())
            .is_some_and(|id| self.menu_ids.contains(&id));
        let toolkit = |token: &str| match &menu {
            // A composed screen has no widgets: every key, Tab included, is the engine's.
            None => false,
            Some(m) if m.screen => false,
            Some(m) => {
                // Tab switches a tabbed screen's tab, unless a text box has the keyboard.
                (matches!(token, "TAB" | "BACKTAB") && !tab_is_engines(m))
                    || (focused && matches!(token, "SPACE" | "RETURN"))
            }
        };
        let forwards = menu_forwards_keys(menu.as_ref());
        let typed = if forwards { typed } else { vec![] };
        // Esc closes a menu through its cancel button, which also covers menus the engine binds no key to.
        let esc_cancels = menu
            .as_ref()
            .is_some_and(|m| m.can_cancel && !m.acknowledges());
        let named = if forwards {
            ctx.input(|i| {
                i.events
                    .iter()
                    .filter_map(|e| match e {
                        egui::Event::Key {
                            key,
                            pressed: true,
                            modifiers,
                            ..
                        } if !(*key == Key::Escape && (interrupted || esc_cancels))
                            // Alt with an arrow moves the focus among the action rows, not the engine's cursor.
                            && !(modifiers.alt && matches!(key, Key::ArrowUp | Key::ArrowDown))
                            && turn_key(*key, *modifiers).is_none() =>
                        {
                            key_name(*key, *modifiers)
                        }
                        _ => None,
                    })
                    .collect::<Vec<_>>()
            })
        } else {
            vec![]
        };
        let (typed, named): (Vec<_>, Vec<_>) = (
            typed.into_iter().filter(|t| !toolkit(t)).collect(),
            named.into_iter().filter(|t| !toolkit(t)).collect(),
        );
        self.keys_consumed = false;
        self.focus_action_row(ctx);
        let mut keys = typed.into_iter().chain(named).collect::<Vec<_>>();
        // A popup takes one key per frame; the rest of a burst would land on the game it covers.
        if menu.as_ref().is_some_and(|m| m.acknowledges()) {
            keys.truncate(1);
        }
        // A key press is one step, as in the native game. Once the OS repeats the key, the step is held, not
        // repeated: the engine walks until key up. Any other key is a screen-scoped affordance, which the engine
        // queues behind the command in flight.
        let down = ctx.input(|i| {
            i.events.iter().rev().find_map(|e| match e {
                egui::Event::Key {
                    key,
                    pressed: true,
                    repeat,
                    ..
                } => Some((*key, *repeat)),
                _ => None,
            })
        });
        let repeated = down.filter(|(_, repeat)| *repeat).map(|(key, _)| key);
        for key in keys {
            if repeated.is_some() && self.model.hold(&key) {
                self.walk_key = repeated;
                self.keys_consumed = true;
            } else {
                self.keys_consumed |= self.model.press(&key) || self.model.key_bound(&key);
            }
        }
    }

    /// The walk ends when the key that started it is up, or the window lost the keyboard (a key-up would never come).
    fn release_hold_when_up(&mut self, ctx: &egui::Context) {
        let walk_key = self.walk_key;
        if ctx.input(|i| !i.focused || walk_key.is_none_or(|k| !i.keys_down.contains(&k))) {
            self.walk_key = None;
            self.model.release_hold();
        }
    }

    /// The pressed control's name and stage, announced politely; stays until the next command.
    fn bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            if ui.button("Actions").clicked() {
                self.palette = self.palette.is_none().then(String::new);
            }
            if ui.button("Client settings").clicked() {
                self.client_settings = !self.client_settings;
            }
            self.status(ui);
        });
    }

    /// The command's name and stage is announced politely, with nothing drawn for it; only one that failed, or
    /// the busy state with its interrupt, is shown.
    fn status(&mut self, ui: &mut egui::Ui) {
        let command = self.model.command();
        let failed = command.as_ref().filter(|s| s.finished && s.stage != "done");
        let text = command
            .as_ref()
            .filter(|s| !(s.finished && s.stage == "done"))
            .map_or_else(|| "Ready".into(), |s| format!("{}: {}", s.label, s.stage));
        // Every stage reached since the last frame is said, even one that is not shown afterwards.
        let news = command
            .as_ref()
            .map(|s| (&s.label, self.status_live.update(&s.history).to_owned()))
            .filter(|_| self.status_live.changed());
        let text = news.map_or(text, |(label, stages)| format!("{label}: {stages}"));
        let live = match failed {
            Some(_) => ui.label(&text),
            None => ui.allocate_response(egui::Vec2::ZERO, egui::Sense::hover()),
        };
        live.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Label, true, &text));
        ui.ctx()
            .accesskit_node_builder(live.id, |b| b.set_live(accesskit::Live::Polite));
        // With the log panel off, the client's own results and errors are said here instead.
        if self.model.sidebar_layout().is_some_and(|l| !l.has_log()) {
            if let Some(said) = self.model.messages().last() {
                ui.label(said);
            }
        }
        // Stops a wait or other activity, like the native interrupt key.
        if self.model.busy() {
            // Still, the activity is told by the status text above; only the motion goes.
            if self.display.reduced_motion() {
                ui.label("…");
            } else {
                ui.spinner();
            }
            if ui.button("Interrupt").clicked() {
                self.model.interrupt();
            }
        }
    }

    /// No command has been in flight long enough to show; a controls row dims only after that, so a wheel step
    /// or a short command never blinks it.
    fn idle(&self) -> bool {
        !(self.settled() && self.model.busy())
    }

    /// The command in flight has lasted long enough to show; shorter ones never blink the controls.
    fn settled(&self) -> bool {
        self.pending_since
            .is_some_and(|t| self.clock.now().saturating_sub(t) >= SETTLE)
    }

    /// Alt+Down and Alt+Up put the keyboard focus on the next and previous action row of the open menu,
    /// where Enter sends that action; the engine's own keys never reach those rows.
    fn focus_action_row(&mut self, ctx: &egui::Context) {
        let step = ctx.input(|i| {
            i.events.iter().fold(0_isize, |step, e| match e {
                egui::Event::Key {
                    key: egui::Key::ArrowDown,
                    pressed: true,
                    modifiers,
                    ..
                } if modifiers.alt => step + 1,
                egui::Event::Key {
                    key: egui::Key::ArrowUp,
                    pressed: true,
                    modifiers,
                    ..
                } if modifiers.alt => step - 1,
                _ => step,
            })
        });
        if step == 0 || self.action_ids.is_empty() {
            return;
        }
        let at = ctx
            .memory(|m| m.focused())
            .and_then(|id| self.action_ids.iter().position(|a| *a == id));
        let last = self.action_ids.len() as isize - 1;
        let to = match at {
            Some(i) => (i as isize + step).clamp(0, last),
            None if step > 0 => 0,
            None => last,
        };
        ctx.memory_mut(|m| m.request_focus(self.action_ids[to as usize]));
    }

    /// A button for a choice or action. Once a command has lasted a while, the pressed one is
    /// highlighted and the rest wait for the engine; the label never changes, so nothing re-lays out.
    fn control(
        &self,
        ui: &mut egui::Ui,
        text: impl Into<egui::WidgetText>,
        id: &str,
        enabled: bool,
        highlighted: bool,
    ) -> egui::Response {
        let settled = self.settled();
        let pending = settled
            && self
                .model
                .command()
                .is_some_and(|s| !s.finished && s.target.as_deref() == Some(id));
        let usable = enabled && !(settled && self.model.busy());
        let button = ui.add_enabled(
            usable,
            egui::Button::new(text).selected(pending || highlighted),
        );
        a11y::button(ui.ctx(), button.id, pending);
        // A clicked button must not keep the focus ring, nor Enter and Space for itself, once it has run.
        if button.clicked() {
            button.surrender_focus();
        }
        button
    }

    /// A click on a map square: the engine plans the route, a repeat walks it.
    fn go_to(&mut self, view: Pos) {
        self.model.travel(view);
        let origin = self.model.origin();
        self.marker = self.model.busy().then_some(Pos {
            x: view.x + origin.x,
            y: view.y + origin.y,
        });
    }

    fn map(&mut self, ui: &mut egui::Ui) {
        self.display.update(ui.ctx());
        let cell_px = self.display.cell(CELL) * self.display.zoom();
        self.map_view = None;
        self.display.begin();
        // The room left for the map, in whole cells.
        let room = ui.available_size();
        let three = self.iso.active(&self.display);
        self.model.set_viewport(match three {
            true => Pos {
                x: iso::REACH,
                y: iso::REACH,
            },
            false => Pos {
                x: (room.x / cell_px) as i32,
                y: (room.y / cell_px) as i32,
            },
        });
        // Facts are always taken, so a long pause never replays them; reduced motion drops them.
        let now = ui.input(|i| i.time);
        let produced = self.model.take_effects();
        self.effects
            .add(now, produced, self.display.reduced_motion());
        let size = self.model.size();
        if size.x <= 0 || size.y <= 0 {
            ui.label("No map loaded");
            return;
        }
        // The cursor is kept on a map square, so it stays there when the view scrolls; the engine's own
        // cursor (look mode) wins. The map may also have been replaced by a smaller one since it was set.
        let origin = self.model.origin();
        let at_view = |abs: Pos| Pos {
            x: (abs.x - origin.x).clamp(0, size.x - 1),
            y: (abs.y - origin.y).clamp(0, size.y - 1),
        };
        let to_abs = |view: Pos| Pos {
            x: view.x + origin.x,
            y: view.y + origin.y,
        };
        let looking = self.model.focus();
        if let Some(focus) = looking {
            self.cursor = to_abs(focus);
        }
        let player = self.model.player();
        // Native shows no cursor outside look mode and the mouse: without a pointer it is the player's square.
        if looking.is_none() && self.last_hover.is_none() {
            self.cursor = to_abs(player);
        }
        let mut cursor = at_view(self.cursor);
        let route = self.model.route();
        let mut status = format!(
            "Player at ({}, {}). Cursor ({}, {}): {}.",
            to_abs(player).x,
            to_abs(player).y,
            to_abs(cursor).x,
            to_abs(cursor).y,
            // The engine's mouse-view text when it has answered for this square.
            self.model
                .hover_text(cursor)
                .map_or_else(
                    || self.model.cell(cursor).desc,
                    |lines| {
                        let lines = lines.iter().map(|l| l.trim()).filter(|l| !l.is_empty());
                        lines.collect::<Vec<_>>().join(" ")
                    },
                )
                .trim_end_matches('.')
        );
        if let Some(end) = route.last() {
            status += &format!(
                " Route of {} squares to ({}, {}); click it again to go.",
                route.len(),
                end.x,
                end.y
            );
        }
        if let Some(aiming) = self.model.aiming() {
            status = format!("{aiming}. {status}");
        }
        // The cursor's square is said politely and never drawn; the map's own description says it too.
        let said = ui.interact(
            egui::Rect::from_min_size(ui.max_rect().min, egui::Vec2::ZERO),
            ui.id().with("map_status"),
            egui::Sense::hover(),
        );
        said.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Label, true, &status));
        ui.ctx()
            .accesskit_node_builder(said.id, |b| b.set_live(accesskit::Live::Polite));
        // The squares fill whole cells, so what is left over is shared by both sides, not left as a band.
        // The 3D map fills the room and turns with the right button; the flat one is its squares.
        let (extent, sense) = match three {
            true => (ui.available_size(), egui::Sense::click_and_drag()),
            false => (
                egui::vec2(size.x as f32 * cell_px, size.y as f32 * cell_px),
                egui::Sense::click(),
            ),
        };
        let (response, painter) = ui
            .with_layout(egui::Layout::top_down(egui::Align::Center), |ui| {
                ui.allocate_painter(extent, sense.union(egui::Sense::focusable_noninteractive()))
            })
            .inner;
        let view = three.then(|| {
            let scale = self
                .display
                .tileset()
                .map_or(cell_px, |(t, _)| iso::scene::world_scale(t, cell_px));
            self.iso.viewport(player, scale, response.rect.size())
        });
        let season = self.model.season();
        // The scene is built before the pointer is read: picking casts its ray into it.
        let shown = match (&view, self.iso.target) {
            (Some(_), Some(_)) => {
                let ctx = ui.ctx().clone();
                self.iso
                    .scene(&ctx, &self.model, &self.display, season.as_deref())
            }
            _ => None,
        };
        if view.is_some() && response.dragged_by(egui::PointerButton::Secondary) {
            self.iso.drag(response.drag_delta());
            ui.ctx().request_repaint();
        }
        response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Other, true, "Map"));
        // One zoom step per wheel line over the map, not per smoothed frame; a touchpad's points add up to lines.
        if response.hovered() {
            let line = ui.ctx().options(|o| o.input_options.line_scroll_speed);
            self.wheel += ui.input(|i| {
                i.raw
                    .events
                    .iter()
                    .map(|e| match e {
                        egui::Event::MouseWheel {
                            unit: egui::MouseWheelUnit::Point,
                            delta,
                            ..
                        } => delta.y / line,
                        // A page is one step too: the zoom has no page.
                        egui::Event::MouseWheel { delta, .. } => delta.y,
                        _ => 0.0,
                    })
                    .sum::<f32>()
            });
            let steps = self.wheel.trunc();
            self.wheel -= steps;
            if steps != 0.0 {
                self.display.zoom_by(steps as i8);
                ui.ctx().request_repaint();
            }
        }
        ui.ctx().accesskit_node_builder(response.id, |b| {
            b.set_role(accesskit::Role::Canvas);
            b.set_description(status.as_str());
        });
        self.map_view = Some((response.rect.min, cell_px));
        let cell_at = |p: egui::Pos2| {
            let at = match &view {
                // The square the ray under the pointer meets first, else the ground of the avatar's level.
                Some(v) => {
                    let p = p - response.rect.min;
                    let p = glam::Vec2::new(p.x, p.y);
                    let drawn = shown.as_ref().and_then(|(scene, _)| scene.pick(v, p));
                    drawn.map_or_else(
                        || v.pick(p, 0.0).map_or(glam::Vec2::ZERO, |h| h.floor()),
                        |(x, y)| glam::Vec2::new(x as f32, y as f32),
                    )
                }
                None => {
                    let v = (p - response.rect.min) / cell_px;
                    glam::Vec2::new(v.x.floor(), v.y.floor())
                }
            };
            Pos {
                x: (at.x as i32).clamp(0, size.x - 1),
                y: (at.y as i32).clamp(0, size.y - 1),
            }
        };
        // Only a moving pointer moves the cursor, so it cannot undo keyboard navigation.
        let hover = response.hover_pos();
        if let Some(p) = hover.filter(|p| self.last_hover != Some(*p)) {
            cursor = cell_at(p);
            // The engine's own cursor follows the mouse while it is looking or aiming; otherwise the
            // native mouse view describes the square.
            if !self.model.aim(cursor, false) {
                self.model.hover(cursor);
            }
        }
        self.last_hover = hover;
        let mut menu_key = false;
        // A click goes to the square it landed on, not to where the pointer was last seen hovering.
        let clicked = response.interact_pointer_pos().map_or(cursor, cell_at);
        if response.clicked() {
            // A click while the engine owns the cursor moves or confirms it; otherwise it travels.
            if !self.model.aim(clicked, true) {
                self.go_to(clicked);
            }
            ui.ctx().request_repaint();
        }
        if response.secondary_clicked() {
            self.menu_cell = clicked;
        }
        // With nothing focused, an arrow key would move egui's focus onto a toolbar widget and a later
        // Space would press it, so the map holds the focus.
        if (self.refocus_map || ui.memory(|m| m.focused()).is_none()) && self.model.menu().is_none()
        {
            self.refocus_map = false;
            response.request_focus();
        }
        if response.has_focus() {
            // Keep arrow keys from moving egui's focus away from the map.
            ui.memory_mut(|m| {
                m.set_focus_lock_filter(
                    response.id,
                    egui::EventFilter {
                        // Tab is the engine's here (auto-fight); it must not walk the focus over the sidebar.
                        tab: true,
                        horizontal_arrows: true,
                        vertical_arrows: true,
                        ..Default::default()
                    },
                )
            });
            // A keyboard cursor gets the same native description a hovering pointer does. The arrow keys
            // only walk: outside look and aim mode the cursor follows the mouse, else it is the player's square.
            if self.model.focus().is_none() && !response.context_menu_opened() {
                self.model.hover(cursor);
            }
            // Shift+F10, the keyboard's context-menu key, is the right click on the cursor square.
            if ui.input(|i| i.modifiers.shift && i.key_pressed(egui::Key::F10)) {
                self.menu_cell = cursor;
                menu_key = true;
            }
        }
        self.cursor = to_abs(cursor);
        // A dialog on top owns the pointer; the tooltip would hang over it.
        let tip = self.model.menu().is_none()
            && self.palette.is_none()
            && !response.context_menu_opened();
        // The tooltip is about the square under the pointer; keyboard moves of the cursor leave it alone.
        let under = response.hover_pos().map_or(cursor, cell_at);
        response.clone().on_hover_ui_at_pointer(|ui| {
            if !tip {
                return;
            }
            // The engine's own mouse-view text when it has answered for this square.
            if let Some(lines) = self.model.hover_text(under) {
                for line in lines {
                    // A tooltip at the pointer gets almost no width to wrap in; one line never wraps.
                    ui.add(egui::Label::new(line).wrap_mode(egui::TextWrapMode::Extend));
                }
            } else {
                let desc = self.model.cell(under).desc;
                if desc != "unknown" {
                    ui.add(egui::Label::new(desc).wrap_mode(egui::TextWrapMode::Extend));
                }
            }
        });
        // The right click lists what the engine says the avatar can do with the square; the choice is sent as
        // that action. A keyboard cursor opens the same menu on its square. The menu sits at its square, which a
        // keyboard has no pointer for.
        let mut popup = egui::Popup::context_menu(&response).anchor(egui::Rect::from_min_size(
            match view {
                // The 3D scene has no square grid to place the menu on; it opens at the map's centre.
                Some(_) => response.rect.center(),
                None => {
                    response.rect.min
                        + egui::vec2(self.menu_cell.x as f32, self.menu_cell.y as f32) * cell_px
                }
            },
            egui::Vec2::splat(cell_px),
        ));
        if menu_key {
            popup = popup.open_memory(egui::SetOpenCommand::Bool(true));
        }
        let menu = popup.show(|ui| {
            let at = self.menu_cell;
            let Some(actions) = self.model.tile_actions(at) else {
                // The engine has not answered for this square yet.
                ui.add_enabled(false, egui::Label::new("…"));
                ui.ctx().request_repaint();
                return;
            };
            if actions.is_empty() {
                ui.close();
            }
            for action in actions {
                let key = action
                    .keys
                    .first()
                    .map_or(String::new(), |k| format!(" [{k}]"));
                let label = format!(
                    "{}{key}",
                    bn_client_core::markup::plain(&action.label.shown())
                );
                if menu_item(ui, &label) {
                    self.model.tile_action(at, &action.id);
                    ui.ctx().request_repaint();
                    ui.close();
                }
            }
        });
        if let Some(menu) = menu {
            ui.ctx().accesskit_node_builder(menu.response.id, |b| {
                b.set_role(accesskit::Role::Menu);
                b.set_label("Map actions");
            });
        }
        let font = egui::FontId::monospace(cell_px - 2.0);
        let tiles = self.display.tiles();
        if tiles {
            painter.rect_filled(response.rect, 0.0, egui::Color32::BLACK);
        }
        profiling::scope!("map cells");
        if let (Some(v), Some(target)) = (&view, self.iso.target) {
            if let Some(shown) = shown {
                let stats = self.iso.stats.clone();
                let scene = shown.0.clone();
                painter.add(iso::callback(response.rect, v, target, shown, stats));
                iso::paint_glyphs(&painter, response.rect, v, &scene, cell_px);
            }
        } else {
            for y in 0..size.y {
                for x in 0..size.x {
                    let cell = self.model.cell(Pos { x, y });
                    let origin = response.rect.min + egui::vec2(x as f32, y as f32) * cell_px;
                    let square = egui::Rect::from_min_size(origin, egui::Vec2::splat(cell_px));
                    if tiles
                        && self
                            .display
                            .paint(&painter, square, &cell, season.as_deref())
                    {
                        continue;
                    }
                    if let Some(bg) = cell.bg {
                        painter.rect_filled(square, 0.0, rgb(bg));
                    }
                    painter.text(
                        square.center(),
                        egui::Align2::CENTER_CENTER,
                        cell.glyph,
                        font.clone(),
                        rgb(cell.color),
                    );
                }
            }
        }
        // A square on the screen: its rectangle on the flat map, the diamond of its ground in the 3D one.
        let rect = |p: Pos| match &view {
            Some(v) => egui::Rect::from_center_size(
                ground_point(v, response.rect, p, (0.5, 0.5), 0.5),
                egui::Vec2::splat(cell_px),
            ),
            None => egui::Rect::from_min_size(
                response.rect.min + egui::vec2(p.x as f32, p.y as f32) * cell_px,
                egui::Vec2::splat(cell_px),
            ),
        };
        let corners = |v: &iso::camera::Viewport, p: Pos| {
            [(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)]
                .map(|(dx, dy)| ground_point(v, response.rect, p, (dx, dy), 0.0))
        };
        let fill = |p: Pos, color: egui::Color32| match &view {
            Some(v) => {
                painter.add(egui::Shape::convex_polygon(
                    corners(v, p).to_vec(),
                    color,
                    egui::Stroke::NONE,
                ));
            }
            None => {
                painter.rect_filled(rect(p), 0.0, color);
            }
        };
        let outline = |p: Pos, stroke: egui::Stroke| match &view {
            Some(v) => {
                painter.add(egui::Shape::closed_line(corners(v, p).to_vec(), stroke));
            }
            None => {
                painter.rect_stroke(rect(p), 0.0, stroke, egui::StrokeKind::Inside);
            }
        };
        for (i, p) in route.iter().enumerate() {
            let in_view = (0..size.x).contains(&p.x) && (0..size.y).contains(&p.y);
            if in_view {
                let alpha = if i + 1 == route.len() { 140 } else { 70 };
                fill(
                    *p,
                    egui::Color32::from_rgba_unmultiplied(0, 200, 255, alpha),
                );
            }
        }
        if let Some(m) = self.marker.filter(|_| self.model.busy()) {
            let at = Pos {
                x: m.x - origin.x,
                y: m.y - origin.y,
            };
            outline(
                at,
                egui::Stroke::new(2.0, egui::Color32::from_rgb(0, 200, 255)),
            );
        }
        // The tile view draws the avatar as its own sprite.
        let avatar_sprite =
            tiles && (self.model.cell(player).layers.last()).is_some_and(|l| l.creature);
        if !avatar_sprite {
            painter.text(
                rect(player).center(),
                egui::Align2::CENTER_CENTER,
                '@',
                font,
                egui::Color32::YELLOW,
            );
        }
        if looking.is_some() || hover.is_some() {
            outline(cursor, egui::Stroke::new(1.0, egui::Color32::WHITE));
        }
        let ascii = !self.display.tiles();
        self.effects
            .paint(&painter, now, cell_px, rect, &mut self.display, ascii);
        if self.effects.active() {
            ui.ctx().request_repaint();
        }
    }
}

/// Where the point `at` (across and down the square, 0 to 1) of square `p`, `z` over its ground, lies on the
/// screen of a 3D map drawn into `rect`.
fn ground_point(
    v: &iso::camera::Viewport,
    rect: egui::Rect,
    p: Pos,
    at: (f32, f32),
    z: f32,
) -> egui::Pos2 {
    let on = v.project(glam::Vec3::new(p.x as f32 + at.0, p.y as f32 + at.1, z));
    rect.min + egui::vec2(on.x, on.y)
}

/// The engine's name for a pressed key, with Ctrl and Shift folded in (`CTRL+S`, `BACKTAB`). Named like the
/// engine's key names (`SPACE`, `ESC`, `RETURN`, `UP`, `PPAGE`); typed characters arrive as text instead.
fn key_name(key: egui::Key, mods: egui::Modifiers) -> Option<String> {
    use egui::Key;
    const NAMED: [(Key, &str); 13] = [
        (Key::ArrowUp, "UP"),
        (Key::ArrowDown, "DOWN"),
        (Key::ArrowLeft, "LEFT"),
        (Key::ArrowRight, "RIGHT"),
        (Key::Escape, "ESC"),
        (Key::Enter, "RETURN"),
        (Key::Tab, "TAB"),
        (Key::Backspace, "BACKSPACE"),
        (Key::Delete, "DELETE"),
        (Key::Home, "HOME"),
        (Key::End, "END"),
        (Key::PageUp, "PPAGE"),
        (Key::PageDown, "NPAGE"),
    ];
    const FUNCTION: [Key; 12] = [
        Key::F1,
        Key::F2,
        Key::F3,
        Key::F4,
        Key::F5,
        Key::F6,
        Key::F7,
        Key::F8,
        Key::F9,
        Key::F10,
        Key::F11,
        Key::F12,
    ];
    let base = NAMED
        .iter()
        .find(|(k, _)| *k == key)
        .map(|(_, n)| n.to_string())
        .or_else(|| {
            FUNCTION
                .iter()
                .position(|k| *k == key)
                .map(|n| keys::function(n as u8 + 1))
        })
        .or_else(|| mods.ctrl.then(|| key.name().to_string()))?;
    Some(keys::chord(&base, mods.ctrl, mods.shift))
}

/// Whether Tab and Shift+Tab are the engine's: its screen has tabs and no text box takes the keyboard.
fn tab_is_engines(menu: &bn_client_core::Menu) -> bool {
    menu.field.is_none() && !menu.tabs().is_empty()
}

/// Whether the keys pressed go to the engine: with no menu open, or in a menu that has no text field
/// to type into.
fn menu_forwards_keys(menu: Option<&bn_client_core::Menu>) -> bool {
    menu.is_none_or(|m| m.acknowledges() || m.field.is_none())
}

impl<M: GameModel> eframe::App for App<M> {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        self.iso.target = frame.wgpu_render_state().map(|s| s.target_format);
        self.show(ui);
    }
}
