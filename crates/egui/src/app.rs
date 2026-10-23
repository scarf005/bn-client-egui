use crate::a11y;
use crate::display::Display;
use crate::effects::Effects;
use crate::iso::{self, View3d};
use crate::text;
use crate::windows::{self, Origin, Shown, WindowSpec};
use bn_client_core::localise::{Value as Arg, tr, tr_format};
use bn_client_core::protocol::{PanelKind, SidebarSide};
use bn_client_core::{CommandStatus, Failure, GameModel, Loading, Pos, interaction};
use eframe::egui::{self, accesskit};
use map::MapView;

mod component;
mod input;
pub(crate) use input::key_name;
mod map;
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
    /// 12% down, so the same click lands on the same control. A window of no kind starts there every run.
    fn window_spec(&self, ctx: &egui::Context, kind: Option<&str>, title: &str) -> WindowSpec {
        WindowSpec {
            title: title.into(),
            id: a11y::dialog_id(kind.unwrap_or(title)),
            saved: kind.and_then(|kind| self.display.window(kind)),
            origin: Origin::top_center(ctx),
            place: None,
            movable: false,
            width: None,
            scrolls: true,
            frame: None,
            // Over the game every window has one; the title screen's untitled menu is the home they open from.
            title_bar: !title.is_empty() || !interaction::titled(self.model.screens()),
            closable: false,
            help: None,
        }
    }

    /// Remembers where the player left the window of `kind` just shown, written once the pointer is up; true when
    /// its close button was pressed.
    fn keep_window<R>(
        &mut self,
        ctx: &egui::Context,
        kind: Option<&str>,
        shown: &Shown<R>,
    ) -> bool {
        if let (Some(kind), Some(state)) = (kind, shown.state) {
            let settled = !ctx.input(|i| i.pointer.any_down());
            self.display.keep_window(kind, state, settled);
        }
        shown.closed
    }
}

/// The kind of window `screen` is kept as: the engine's name for the screen, none where it names none.
fn screen_kind(screen: &bn_client_core::protocol::Screen) -> Option<String> {
    screen.kind.as_ref().map(|kind| format!("screen:{kind}"))
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
    /// Absolute square the map's right-click menu was opened on; its actions are about it while the view scrolls.
    menu_cell: Pos,
    /// Absolute square of the last map click, marked until its command finishes.
    marker: Option<Pos>,
    /// When the command now in flight was first seen; short ones never change the controls.
    pending_since: Option<std::time::Duration>,
    clock: bn_client_core::Clock,
    last_hover: Option<egui::Pos2>,
    /// Where the pointer was last seen, over the map or not.
    last_pointer: Option<egui::Pos2>,
    /// Where the pointer sent the engine's cursor, drawn until the engine has answered.
    aim: Option<map::Aim>,
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
    /// The client's map window, while it is open.
    world_map: Option<component::WorldMap>,
    /// The panel windows open, each with its own focus.
    panels: std::collections::HashMap<PanelKind, interaction::Interaction>,
    /// The client's own window that has the keyboard: the last one opened or clicked. Its keys come first, and
    /// the game gets the ones it does not use; none leaves the keyboard with the game.
    key_window: Option<input::KeyWindow>,
    /// The client's own windows that were open last frame, to tell a window just opened.
    known_windows: Vec<input::KeyWindow>,
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
    map_view: Option<MapView>,
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
            last_pointer: None,
            aim: None,
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
            world_map: None,
            panels: Default::default(),
            key_window: None,
            known_windows: Vec::new(),
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
            status_live: a11y::Announcer::new(" "),
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
        let ground = crate::kit::tokens::ground(ui);
        text::job(&self.model.styled(text), &font, ground).into()
    }

    /// Native-style loading screen: splash image, current step as live text, the step list.
    /// The engine is gone or silent: say so with its last stderr lines, and offer a restart.
    fn failure(&mut self, ui: &mut egui::Ui, failure: &Failure) {
        let heading = ui.heading(&failure.message);
        ui.ctx()
            .accesskit_node_builder(heading.id, |b| b.set_live(accesskit::Live::Assertive));
        if let Some(engine) = &failure.engine {
            ui.label(tr_format(
                "Engine: %s",
                &[Arg::Text(engine.display().to_string())],
            ));
        }
        if !failure.stderr.is_empty() {
            ui.label(tr("Last engine output:"));
            egui::ScrollArea::vertical().show(ui, |ui| {
                for line in &failure.stderr {
                    ui.monospace(line);
                }
            });
        }
        let restart = ui.button(tr("Restart"));
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
            let status = tr_format(
                "%s: %s (%d of %d)",
                &[
                    Arg::Text(loading.title.clone()),
                    Arg::Text(step.clone()),
                    Arg::Int(loading.index as i64 + 1),
                    Arg::Int(loading.entries.len() as i64),
                ],
            );
            let live = ui.label(status);
            ui.ctx()
                .accesskit_node_builder(live.id, |b| b.set_live(accesskit::Live::Polite));
        }
        if let (Some(root), Some(image)) = (&self.root, &loading.image) {
            let uri = format!("file://{}", root.join(image).display());
            ui.add(
                egui::Image::new(uri)
                    .alt_text(tr("Loading image"))
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
        // A failed engine plays nothing more: its loops and music stop with the device.
        let soundpack = self
            .model
            .failure()
            .is_none()
            .then(|| self.model.soundpack())
            .flatten();
        if let Some(soon) = self.audio.update(soundpack, self.model.take_cues()) {
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
            // A steady text cursor: nothing moves that the player did not move.
            ui.ctx()
                .all_styles_mut(|s| s.visuals.text_cursor.blink = false);
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
        self.keys(ui.ctx());
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
        label_resize_handle(ui.ctx(), &name, &tr("Resize sidebar"));
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
        self.world_map(ctx);
        self.panel_windows(ctx);
        self.menu_or_screen(ctx);
    }

    /// The client's own choices (view mode, tileset, motion), which are no part of the game.
    fn client_settings(&mut self, ctx: &egui::Context) {
        if !self.client_settings {
            return;
        }
        let spec = WindowSpec {
            closable: true,
            ..self.window_spec(
                ctx,
                Some(&input::KeyWindow::Settings.kind()),
                &tr("Client settings"),
            )
        };
        self.client_settings = !windows::show(ctx, spec, |ui| self.display.controls(ui)).closed;
    }

    /// The pressed control's name and stage, announced politely; stays until the next command.
    fn bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            if ui.button(tr("Actions")).clicked() {
                self.palette = self.palette.is_none().then(String::new);
            }
            if ui.button(tr("Client settings")).clicked() {
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
            .map_or_else(|| tr("Ready"), CommandStatus::line);
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
        if self.model.sidebar_layout().is_some_and(|l| !l.has_log())
            && let Some(said) = self.model.messages().last()
        {
            ui.label(said);
        }
        if let Some(activity) = self.model.activity() {
            ui.label(&activity.name);
            if let Some(done) = activity.done {
                let bar = egui::ProgressBar::new(f32::from(done) / 1000.0)
                    .desired_width(120.0)
                    .text(format!("{}%", done / 10))
                    .corner_radius(0);
                ui.add(bar);
            }
        }
        // Stops a wait or other activity, like the native interrupt key; a command too short to show offers
        // neither motion nor the button, and the key still interrupts it.
        if self.settled() && self.model.busy() {
            // Still, the activity is told by the status text above; only the motion goes.
            if self.display.reduced_motion() {
                ui.label("…");
            } else {
                ui.spinner();
            }
            if ui.button(tr("Interrupt")).clicked() {
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

    /// A button for a choice or action. Once a command has lasted a while, the pressed one is
    /// highlighted and the rest wait for the engine; the label never changes, so nothing re-lays out.
    fn control<'a>(
        &self,
        ui: &mut egui::Ui,
        text: impl egui::IntoAtoms<'a>,
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
        // A panel window's controls run nothing, so they never wait for the engine.
        let usable = enabled && (self.screens.passive || !(settled && self.model.busy()));
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
