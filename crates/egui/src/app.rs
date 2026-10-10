use crate::a11y;
use crate::display::Display;
use crate::effects::Effects;
use crate::text;
use bn_client_core::{Failure, GameModel, Loading, Pos, keys};
use eframe::egui::{self, accesskit};
use std::collections::VecDeque;

mod crafting;
mod menu;
mod palette;
mod pickup;
mod sidebar;

/// A command must outlast this before the controls show it; keeps short commands from blinking the UI.
const SETTLE: std::time::Duration = std::time::Duration::from_millis(150);

/// The sidebar's initial share of the window width, and its limits in points; independent of its content.
const SIDEBAR_SHARE: f32 = 0.25;
const SIDEBAR_WIDTH: (f32, f32) = (280.0, 480.0);

/// After a popup closes, Space is held back until it has been quiet this long: the player is still dismissing.
const POPUP_SPACE_HOLD: std::time::Duration = std::time::Duration::from_millis(500);

/// Side length of one map cell in points.
pub const CELL: f32 = 16.0;

fn rgb(c: bn_client_core::Rgb) -> egui::Color32 {
    egui::Color32::from_rgb(c.0, c.1, c.2)
}

/// Every dialog opens at the top centre, 12% down, so the same click lands on the same control.
fn dialog_offset(ctx: &egui::Context) -> [f32; 2] {
    [0.0, ctx.content_rect().height() * 0.12]
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
    destination: Option<Pos>,
    /// Absolute square of the last map click, marked until its command finishes.
    marker: Option<Pos>,
    /// When the command now in flight was first seen; short ones never change the controls.
    pending_since: Option<std::time::Duration>,
    clock: bn_client_core::Clock,
    last_hover: Option<egui::Pos2>,
    wired: bool,
    images: bool,
    /// Game root, for files the engine names relative to it (loading images).
    root: Option<std::path::PathBuf>,
    /// Where the display choice is kept; none keeps it only for this run.
    settings: Option<std::path::PathBuf>,
    display: Display,
    /// A typed key already ran an engine action this frame.
    keys_consumed: bool,
    /// The action palette's search text while it is open.
    palette: Option<String>,
    /// The message log's height once the user has dragged its edge.
    log_height: Option<f32>,
    /// Title of the menu that last took focus, so a menu is entered once, not on every frame.
    menu_title: Option<String>,
    /// The popup message that already took its one key press, so a burst of Space cannot reach the game behind it.
    ack_key: Option<String>,
    /// When the last popup closed, to hold Space back for a moment.
    ack_closed: Option<std::time::Duration>,
    /// The menu on screen last frame was a popup.
    in_popup: bool,
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
    /// The same for the action palette.
    palette_ids: Vec<egui::Id>,
    /// Where the engine's cursor was in the open menu, so keyboard focus can follow it.
    menu_focus: Option<String>,
    /// The cursor has been put on the player since the map first appeared.
    cursor_placed: bool,
    /// The engine's text prompt as last seen (field id, value), and the text typed into it.
    prompt: ((String, String), String),
    /// The newest prompt text not yet sent, and whether Enter asked to submit it.
    prompt_send: Option<(String, String, bool)>,
    /// Shots, explosions and floating text now playing on the map.
    effects: Effects,
    /// The engine's look cursor was up at the last frame.
    was_looking: bool,
    crafting: crafting::Crafting,
    pickup: pickup::Pickup,
    /// The engine's selected row of the open menu, to keep it in view when the wheel moves it.
    menu_cursor: Option<String>,
    /// Input pressed while a command ran, in order; each goes out as soon as the one before it is done.
    queued: VecDeque<Queued>,
    /// The loading step the list was last scrolled to; scrolling again every frame would repaint forever.
    scrolled_to: Option<usize>,
}

enum Queued {
    Action(String),
    Key(String),
}

/// How much input may wait for a running command. A held key repeats faster than the engine can step, and
/// releasing it must stop the walk soon.
const QUEUE: usize = 2;

impl<M: GameModel> App<M> {
    pub fn new(model: M) -> Self {
        let cursor = model.player();
        Self {
            model,
            cursor,
            menu_cell: cursor,
            destination: None,
            marker: None,
            pending_since: None,
            clock: bn_client_core::Clock::default(),
            last_hover: None,
            wired: false,
            images: false,
            root: None,
            settings: None,
            display: Display::new(None, None),
            keys_consumed: false,
            palette: None,
            log_height: None,
            menu_title: None,
            ack_key: None,
            ack_closed: None,
            in_popup: false,
            focus_menu: false,
            refocus_map: false,
            cursor_placed: false,
            prompt: Default::default(),
            prompt_send: None,
            effects: Effects::default(),
            was_looking: false,
            menu_ids: Vec::new(),
            palette_ids: Vec::new(),
            menu_focus: None,
            native_live: a11y::Announcer::new(" "),
            command_live: a11y::Announcer::new(" "),
            status_live: a11y::Announcer::new(", "),
            crafting: Default::default(),
            pickup: Default::default(),
            menu_cursor: None,
            queued: VecDeque::new(),
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
        let font = egui::TextStyle::Body.resolve(ui.style());
        text::job(&self.model.styled(text), &font).into()
    }

    /// Native-style loading screen: splash image, current step as live text, the step list.
    /// The engine is gone or silent: say so with its last stderr lines, and offer a restart.
    fn failure(&mut self, ui: &mut egui::Ui, failure: &Failure) {
        let heading = ui.heading(&failure.message);
        ui.ctx()
            .accesskit_node_builder(heading.id, |b| b.set_live(accesskit::Live::Assertive));
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
        if self.model.menu().is_some_and(|m| m.context == "MAIN_MENU") {
            // The game is over: the next one starts with the cursor on its player and no walk pending.
            (self.cursor_placed, self.destination, self.marker) = (false, None, None);
        }
        self.run_queued();
        // Esc interrupts a running activity; with none running it goes to the game like any key
        // (leaves look mode, acknowledges a popup, else opens the main menu).
        let interrupted =
            ui.ctx().input(|i| i.key_pressed(egui::Key::Escape)) && self.model.interrupt();
        self.walk(ui.ctx());
        self.palette_keys(ui.ctx());
        self.game_keys(ui.ctx(), interrupted);
        if menu_forwards_keys(self.model.menu().as_ref()) {
            // Arrow keys belong to the game, whichever widget was clicked last; Tab still moves focus.
            let ctx = ui.ctx();
            if let Some(id) = ctx.memory(|m| m.focused()) {
                ctx.memory_mut(|m| {
                    m.set_focus_lock_filter(
                        id,
                        egui::EventFilter {
                            horizontal_arrows: true,
                            vertical_arrows: true,
                            ..Default::default()
                        },
                    )
                });
            }
        }
        egui::Panel::top("bar").show(ui, |ui| self.bar(ui));
        if self.model.menu().is_some_and(|m| m.context == "MAIN_MENU") {
            // The title screen is only its menu: no map, sidebar or log of a game that is not running.
            self.menu(ui.ctx());
            return;
        }
        egui::Panel::right("sidebar")
            .default_size(
                (ui.available_width() * SIDEBAR_SHARE).clamp(SIDEBAR_WIDTH.0, SIDEBAR_WIDTH.1),
            )
            .size_range(SIDEBAR_WIDTH.0..=SIDEBAR_WIDTH.1)
            .show(ui, |ui| {
                profiling::scope!("sidebar");
                ui.set_min_width(ui.available_width());
                self.sidebar_panel(ui)
            });
        label_resize_handle(ui.ctx(), "sidebar", "Resize sidebar");
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(egui::Color32::BLACK))
            .show(ui, |ui| {
                profiling::scope!("map");
                self.map(ui)
            });
        self.palette(ui.ctx());
        self.menu(ui.ctx());
    }

    /// Typed characters and special keys are game keys; the first one that runs an action wins.
    /// Named like the engine's key names (`SPACE`, `ESC`, `RETURN`, `UP`, `PPAGE`).
    fn game_keys(&mut self, ctx: &egui::Context, interrupted: bool) {
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
        // The engine's name for a pressed key, with Ctrl and Shift folded in (`CTRL+S`, `BACKTAB`).
        let name = |key: Key, mods: egui::Modifiers| {
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
        };
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
        // A menu is an engine input context: its navigation, paging, confirm and cancel keys are the
        // player's bindings, and the engine moves its own cursor. The exceptions are the toolkit's
        // accessibility keys: Tab and Shift+Tab move focus, and Space and Enter act on a focused widget.
        // Menus with a text field keep the keyboard for typing.
        let focused = ctx.memory(|m| m.focused()).is_some();
        let toolkit = |token: &str| match &menu {
            // A composed screen has no widgets: every key, Tab included, is the engine's.
            None => false,
            Some(m) if m.screen => false,
            Some(_) => {
                token == "TAB"
                    || token == "BACKTAB"
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
                        } if !(*key == Key::Escape && (interrupted || esc_cancels)) => {
                            name(*key, *modifiers)
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
        if self.palette.is_some() {
            return;
        }
        let mut keys = typed.into_iter().chain(named).collect::<Vec<_>>();
        match menu.as_ref().filter(|m| m.acknowledges()) {
            // A popup takes one key; the rest of a burst would land on the game it covers.
            Some(m) if self.ack_key.as_ref() == Some(&m.message) => keys.clear(),
            Some(m) => {
                keys.truncate(1);
                self.ack_key = (!keys.is_empty()).then(|| m.message.clone());
            }
            None => {
                self.ack_key = None;
                let now = self.clock.now();
                let held = self
                    .ack_closed
                    .is_some_and(|t| now.saturating_sub(t) < POPUP_SPACE_HOLD);
                // Each Space held back extends the hold: only a pause lets a Space through.
                if held && keys.iter().any(|k| k == "SPACE") {
                    self.ack_closed = Some(now);
                }
                keys.retain(|k| !(held && k == "SPACE"));
            }
        }
        for key in keys {
            if !self.model.busy() {
                self.keys_consumed |= self.model.press(&key);
            } else if self.model.key_bound(&key) {
                self.enqueue(Queued::Key(key));
                self.keys_consumed = true;
            }
        }
    }

    /// The pressed control's name and stage, announced politely; stays until the next command.
    fn bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            if ui.button("Actions").clicked() {
                self.palette = self.palette.is_none().then(String::new);
            }
            self.status(ui);
        });
    }

    fn status(&mut self, ui: &mut egui::Ui) {
        // A command that went well is over; only one still going or one that failed is worth showing.
        let text = self
            .model
            .command()
            .filter(|s| !(s.finished && s.stage == "done"))
            .map_or_else(|| "Ready".into(), |s| format!("{}: {}", s.label, s.stage));
        // Every stage reached since the last frame is said, even one that is not shown afterwards.
        let news = self
            .model
            .command()
            .map(|s| (s.label, self.status_live.update(&s.history).to_owned()))
            .filter(|_| self.status_live.changed());
        let text = news.map_or(text, |(label, stages)| format!("{label}: {stages}"));
        ui.horizontal(|ui| {
            let label = ui.label(text);
            ui.ctx()
                .accesskit_node_builder(label.id, |b| b.set_live(accesskit::Live::Polite));
            // Stops a wait or other activity, like the native interrupt key.
            if self.model.busy() && ui.button("Interrupt").clicked() {
                self.model.interrupt();
            }
        });
    }

    /// The command in flight has lasted long enough to show; shorter ones never blink the controls.
    fn settled(&self) -> bool {
        self.pending_since
            .is_some_and(|t| self.clock.now().saturating_sub(t) >= SETTLE)
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

    /// One step per frame toward the destination.
    fn walk(&mut self, ctx: &egui::Context) {
        let Some(dest) = self.destination else {
            return;
        };
        // The destination is stored in model coordinates, which the view may have scrolled since.
        let origin = self.model.origin();
        let target = Pos {
            x: dest.x - origin.x,
            y: dest.y - origin.y,
        };
        let moved = self.model.step_toward(target);
        if !moved {
            self.model.say("Something blocks the way.".into());
        }
        if !moved || self.model.player() == target {
            self.destination = None;
        } else if !self.model.busy() {
            ctx.request_repaint();
        }
    }

    /// The model's own travel if it has one (engine: first click plans, second walks), else our walk.
    fn go_to(&mut self, view: Pos) {
        if !self.model.travel(view) {
            self.walk_to(view);
        }
        let origin = self.model.origin();
        self.marker = self.model.busy().then_some(Pos {
            x: view.x + origin.x,
            y: view.y + origin.y,
        });
    }

    fn walk_to(&mut self, view: Pos) {
        let origin = self.model.origin();
        self.destination = Some(Pos {
            x: view.x + origin.x,
            y: view.y + origin.y,
        });
    }

    fn look(&mut self, at: Pos) {
        let desc = self.model.cell(at).desc;
        self.model
            .say(format!("You see {desc} at ({}, {}).", at.x, at.y));
    }

    fn map(&mut self, ui: &mut egui::Ui) {
        self.display.update(ui.ctx());
        self.display.controls(ui);
        let cell_px = self.display.cell(CELL);
        self.display.begin();
        // The room left for the map, less the status line above it, in whole cells.
        let room =
            ui.available_size() - egui::vec2(0.0, ui.text_style_height(&egui::TextStyle::Body));
        self.model.set_viewport(Pos {
            x: (room.x / cell_px) as i32,
            y: (room.y / cell_px) as i32,
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
        } else if std::mem::take(&mut self.was_looking) {
            // Native shows no cursor outside look mode: it goes back to the player.
            self.cursor = to_abs(self.model.player());
        }
        self.was_looking = looking.is_some();
        let player = self.model.player();
        if !std::mem::replace(&mut self.cursor_placed, true) {
            self.cursor = to_abs(player);
        }
        let mut cursor = at_view(self.cursor);
        let route = self.model.route();
        let mut status = format!(
            "Player at ({}, {}). Cursor ({}, {}): {}.",
            player.x,
            player.y,
            cursor.x,
            cursor.y,
            self.model.cell(cursor).desc
        );
        if let Some(end) = route.last() {
            status += &format!(
                " Route of {} squares to ({}, {}); click it again to go.",
                route.len(),
                end.x,
                end.y
            );
        }
        let status_label = ui.label(&status);
        ui.ctx()
            .accesskit_node_builder(status_label.id, |b| b.set_live(accesskit::Live::Polite));
        let (response, painter) = ui.allocate_painter(
            egui::vec2(size.x as f32 * cell_px, size.y as f32 * cell_px),
            egui::Sense::click().union(egui::Sense::focusable_noninteractive()),
        );
        response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Other, true, "Map"));
        ui.ctx().accesskit_node_builder(response.id, |b| {
            b.set_role(accesskit::Role::Canvas);
            b.set_description(status.as_str());
        });
        let cell_at = |p: egui::Pos2| {
            let v = (p - response.rect.min) / cell_px;
            Pos {
                x: (v.x as i32).clamp(0, size.x - 1),
                y: (v.y as i32).clamp(0, size.y - 1),
            }
        };
        // Only a moving pointer moves the cursor, so it cannot undo keyboard navigation.
        let hover = response.hover_pos();
        if let Some(p) = hover.filter(|p| self.last_hover != Some(*p)) {
            cursor = cell_at(p);
            // The engine's own cursor follows the mouse while it is looking or aiming; otherwise the
            // native mouse view describes the square.
            if !self.model.aim(cursor) {
                self.model.hover(cursor);
            }
        }
        self.last_hover = hover;
        // A click goes to the square it landed on, not to where the pointer was last seen hovering.
        let clicked = response.interact_pointer_pos().map_or(cursor, cell_at);
        if response.clicked() {
            // A click while the engine owns the cursor moves or confirms it; otherwise it travels.
            if !self.model.aim(clicked) {
                self.go_to(clicked);
            }
            ui.ctx().request_repaint();
        }
        if response.secondary_clicked() && !self.model.context_click(clicked) {
            self.menu_cell = clicked;
        }
        if self.refocus_map && self.model.menu().is_none() {
            self.refocus_map = false;
            response.request_focus();
        }
        if response.has_focus() {
            // Keep arrow keys from moving egui's focus away from the map.
            ui.memory_mut(|m| {
                m.set_focus_lock_filter(
                    response.id,
                    egui::EventFilter {
                        horizontal_arrows: true,
                        vertical_arrows: true,
                        ..Default::default()
                    },
                )
            });
            let (dx, dy) = ui.input(|i| {
                let k = |key| i.key_pressed(key) as i32;
                (
                    k(egui::Key::ArrowRight) - k(egui::Key::ArrowLeft),
                    k(egui::Key::ArrowDown) - k(egui::Key::ArrowUp),
                )
            });
            // While the engine owns the cursor its own arrow handling moves it.
            if self.model.focus().is_none() {
                cursor = Pos {
                    x: (cursor.x + dx).clamp(0, size.x - 1),
                    y: (cursor.y + dy).clamp(0, size.y - 1),
                };
            }
        }
        self.cursor = to_abs(cursor);
        response.clone().on_hover_ui_at_pointer(|ui| {
            // The engine's own mouse-view text when it has answered for this square.
            if let Some(lines) = self.model.hover_text(cursor) {
                for line in lines {
                    // A tooltip at the pointer gets almost no width to wrap in; one line never wraps.
                    ui.add(egui::Label::new(line).wrap_mode(egui::TextWrapMode::Extend));
                }
            } else {
                let desc = self.model.cell(cursor).desc;
                if desc != "unknown" {
                    ui.add(egui::Label::new(desc).wrap_mode(egui::TextWrapMode::Extend));
                }
            }
        });
        let native_context = self.model.native_context();
        let menu = response.context_menu(|ui| {
            if native_context {
                ui.close();
                return;
            }
            let at = self.menu_cell;
            if menu_item(ui, "Look") {
                self.look(at);
                ui.close();
            }
            if menu_item(ui, "Move here") {
                self.go_to(at);
                ui.ctx().request_repaint();
                ui.close();
            }
        });
        if let Some(menu) = menu.filter(|_| !native_context) {
            ui.ctx().accesskit_node_builder(menu.response.id, |b| {
                b.set_role(accesskit::Role::Menu);
                b.set_label("Map actions");
            });
        }
        let font = egui::FontId::monospace(cell_px - 2.0);
        let tiles = self.display.tiles();
        let season = self.model.season();
        if tiles {
            painter.rect_filled(response.rect, 0.0, egui::Color32::BLACK);
        }
        profiling::scope!("map cells");
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
        let rect = |p: Pos| {
            egui::Rect::from_min_size(
                response.rect.min + egui::vec2(p.x as f32, p.y as f32) * cell_px,
                egui::Vec2::splat(cell_px),
            )
        };
        for (i, p) in route.iter().enumerate() {
            let in_view = (0..size.x).contains(&p.x) && (0..size.y).contains(&p.y);
            if in_view {
                let alpha = if i + 1 == route.len() { 140 } else { 70 };
                painter.rect_filled(
                    rect(*p),
                    0.0,
                    egui::Color32::from_rgba_unmultiplied(0, 200, 255, alpha),
                );
            }
        }
        if let Some(m) = self.marker.filter(|_| self.model.busy()) {
            painter.rect_stroke(
                rect(Pos {
                    x: m.x - origin.x,
                    y: m.y - origin.y,
                }),
                0.0,
                egui::Stroke::new(2.0, egui::Color32::from_rgb(0, 200, 255)),
                egui::StrokeKind::Inside,
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
        painter.rect_stroke(
            rect(cursor),
            0.0,
            egui::Stroke::new(1.0, egui::Color32::WHITE),
            egui::StrokeKind::Inside,
        );
        self.effects
            .paint(&painter, now, cell_px, rect, &mut self.display);
        if self.effects.active() {
            ui.ctx().request_repaint();
        }
    }
}

/// Whether the keys pressed go to the engine: with no menu open, or in a menu that has no text field
/// to type into (the crafting screen takes its own keys).
fn menu_forwards_keys(menu: Option<&bn_client_core::Menu>) -> bool {
    menu.is_none_or(|m| m.acknowledges() || (m.field.is_none() && m.context != "CRAFTING"))
}

impl<M: GameModel> eframe::App for App<M> {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.show(ui);
    }
}
