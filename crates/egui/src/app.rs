use crate::a11y;
use bn_client_core::{GameModel, Loading, LogLine, Pos};
use eframe::egui::{self, accesskit};

/// A command must outlast this before the controls show it; keeps short commands from blinking the UI.
const SETTLE: std::time::Duration = std::time::Duration::from_millis(150);

/// Side length of one map cell in points.
pub const CELL: f32 = 16.0;

#[derive(Clone, Copy, PartialEq)]
enum Column {
    Ground,
    Inventory,
}

/// Drag payload: which column the item comes from and its index there.
#[derive(Clone, Copy)]
struct Dragged {
    from: Column,
    index: usize,
}

fn rgb(c: bn_client_core::Rgb) -> egui::Color32 {
    egui::Color32::from_rgb(c.0, c.1, c.2)
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
    /// A typed key already ran an engine action this frame.
    keys_consumed: bool,
    /// Title of the menu that last took focus, so a menu is entered once, not on every frame.
    menu_title: Option<String>,
    /// A menu opened and its first control has not taken focus yet (it may be disabled for a frame).
    focus_menu: bool,
    /// The menu just closed; the map takes focus back.
    refocus_map: bool,
    /// What the two message lists announce, so a burst in one frame is read whole.
    native_live: a11y::Announcer,
    command_live: a11y::Announcer,
    /// The enabled controls of the open menu, in Tab order, as of the last frame.
    menu_ids: Vec<egui::Id>,
    /// The cursor has been put on the player since the map first appeared.
    cursor_placed: bool,
}

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
            keys_consumed: false,
            menu_title: None,
            focus_menu: false,
            refocus_map: false,
            cursor_placed: false,
            menu_ids: Vec::new(),
            native_live: a11y::Announcer::default(),
            command_live: a11y::Announcer::default(),
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
        self
    }

    /// Native-style loading screen: splash image, current step as live text, the step list.
    fn loading(&self, ui: &mut egui::Ui, loading: &Loading) {
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
                if i == loading.index {
                    label.scroll_to_me(None);
                }
            }
        });
    }

    pub fn show(&mut self, ui: &mut egui::Ui) {
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
        if let Some(loading) = self.model.loading() {
            self.loading(ui, &loading);
            return;
        }
        if self.model.live() {
            // Safety net only: incoming engine messages repaint through the waker.
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_secs(1));
        }
        if self.pending_since.is_some() && !self.settled() {
            ui.ctx().request_repaint_after(SETTLE);
        }
        self.walk(ui.ctx());
        self.game_keys(ui.ctx());
        if self.model.menu().is_none() {
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
        egui::Panel::top("status").show(ui, |ui| self.status(ui));
        egui::Panel::left("actions")
            .resizable(false)
            .show(ui, |ui| self.actions(ui));
        egui::Panel::bottom("log").show(ui, |ui| self.log(ui));
        egui::Panel::right("items")
            .resizable(false)
            .show(ui, |ui| self.items(ui));
        self.map(ui);
        self.menu(ui.ctx());
    }

    /// Typed characters and special keys are game keys; the first one that runs an action wins.
    fn game_keys(&mut self, ctx: &egui::Context) {
        let typed = ctx.input(|i| {
            i.events
                .iter()
                .filter_map(|e| match e {
                    egui::Event::Text(t) => Some(if t == " " { "SPACE".into() } else { t.clone() }),
                    _ => None,
                })
                .collect::<Vec<_>>()
        });
        let arrows = ctx.input(|i| {
            [
                (egui::Key::ArrowUp, "UP"),
                (egui::Key::ArrowDown, "DOWN"),
                (egui::Key::ArrowLeft, "LEFT"),
                (egui::Key::ArrowRight, "RIGHT"),
            ]
            .into_iter()
            .filter(|(key, _)| i.key_pressed(*key))
            .map(|(_, token)| token.to_string())
            .collect::<Vec<_>>()
        });
        let arrows = if self.model.menu().is_none() {
            arrows
        } else {
            vec![]
        };
        self.keys_consumed = false;
        for key in typed.into_iter().chain(arrows) {
            if self.model.press(&key) {
                self.keys_consumed = true;
            }
        }
    }

    /// The pressed control's name and stage, announced politely; stays until the next command.
    fn status(&self, ui: &mut egui::Ui) {
        let text = self
            .model
            .command()
            .map_or_else(|| "Ready".into(), |s| format!("{}: {}", s.label, s.stage));
        let label = ui.label(text);
        ui.ctx()
            .accesskit_node_builder(label.id, |b| b.set_live(accesskit::Live::Polite));
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
        text: String,
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
        button
    }

    /// Everything the engine accepts right now, each with its game key, clickable.
    fn actions(&mut self, ui: &mut egui::Ui) {
        ui.heading("Actions");
        let mut invoke = None;
        egui::ScrollArea::vertical().show(ui, |ui| {
            for a in self.model.actions() {
                let label = if a.keys.is_empty() {
                    a.name.clone()
                } else {
                    format!("{} ({})", a.name, a.keys.join(", "))
                };
                if self.control(ui, label, &a.id, true, false).clicked() {
                    invoke = Some(a.id);
                }
            }
        });
        if let Some(id) = invoke {
            ui.ctx().request_repaint();
            self.model.invoke_action(&id);
        }
    }

    /// The engine's current interaction, drawn generically: title, message, choices, cancel.
    fn menu(&mut self, ctx: &egui::Context) {
        let Some(menu) = self.model.menu() else {
            self.focus_menu = false;
            self.menu_ids.clear();
            self.refocus_map |= self.menu_title.take().is_some();
            return;
        };
        let entered = self.focus_menu
            | (self.menu_title.replace(menu.title.clone()).as_ref() != Some(&menu.title));
        a11y::trap_tab(ctx, &self.menu_ids);
        let mut ids = Vec::new();
        let mut focused = false;
        let mut chosen = None;
        let mut cancel = ctx.input(|i| i.key_pressed(egui::Key::Escape)) && menu.can_cancel;
        egui::Window::new(&menu.title)
            .id(a11y::dialog_id(&menu.title))
            .show(ctx, |ui| {
                if !menu.message.is_empty() {
                    ui.label(&menu.message);
                }
                let tabs = menu.tabs();
                ui.horizontal_wrapped(|ui| {
                    for tab in &tabs {
                        let button =
                            self.control(ui, tab.label.clone(), &tab.id, tab.enabled, tab.selected);
                        a11y::tab(ui.ctx(), button.id, tab.selected);
                        ids.extend(button.enabled().then_some(button.id));
                        if entered && tab.selected {
                            button.request_focus();
                            focused = button.has_focus();
                        }
                        if button.clicked() {
                            chosen = Some(tab.id.clone());
                        }
                    }
                });
                for (i, choice) in menu.entries().into_iter().enumerate() {
                    let button =
                        self.control(ui, choice.label.clone(), &choice.id, choice.enabled, false);
                    ids.extend(button.enabled().then_some(button.id));
                    if entered && i == 0 && tabs.is_empty() {
                        button.request_focus();
                        focused = button.has_focus();
                    }
                    ui.ctx().accesskit_node_builder(button.id, |b| {
                        b.set_role(accesskit::Role::MenuItem)
                    });
                    if !choice.enabled && !choice.denial.is_empty() {
                        ui.label(&choice.denial);
                    }
                    if button.clicked() {
                        chosen = Some(choice.id.clone());
                    }
                }
                if menu.can_cancel {
                    let button = ui.add_enabled(!self.model.busy(), egui::Button::new("Cancel"));
                    ids.extend(button.enabled().then_some(button.id));
                    cancel |= button.clicked();
                }
            });
        self.menu_ids = ids;
        self.focus_menu = entered && !focused;
        a11y::dialog(ctx, &menu.title, &menu.message);
        if let Some(id) = chosen {
            ctx.request_repaint();
            self.model.choose(&id);
        } else if cancel {
            self.model.cancel_menu();
        }
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
        // The room left for the map, less the status line above it, in whole cells.
        let room =
            ui.available_size() - egui::vec2(0.0, ui.text_style_height(&egui::TextStyle::Body));
        self.model.set_viewport(Pos {
            x: (room.x / CELL) as i32,
            y: (room.y / CELL) as i32,
        });
        let size = self.model.size();
        if size.x <= 0 || size.y <= 0 {
            ui.label("No map loaded");
            return;
        }
        // The map may have been replaced by a smaller one since the cursor was set.
        self.cursor = Pos {
            x: self.cursor.x.clamp(0, size.x - 1),
            y: self.cursor.y.clamp(0, size.y - 1),
        };
        let player = self.model.player();
        if !std::mem::replace(&mut self.cursor_placed, true) {
            self.cursor = player;
        }
        let route = self.model.route();
        let mut status = format!(
            "Player at ({}, {}). Cursor ({}, {}): {}.",
            player.x,
            player.y,
            self.cursor.x,
            self.cursor.y,
            self.model.cell(self.cursor).desc
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
            egui::vec2(size.x as f32 * CELL, size.y as f32 * CELL),
            egui::Sense::click().union(egui::Sense::focusable_noninteractive()),
        );
        response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Other, true, "Map"));
        ui.ctx().accesskit_node_builder(response.id, |b| {
            b.set_role(accesskit::Role::Canvas);
            b.set_description(status.as_str());
        });
        let cell_at = |p: egui::Pos2| {
            let v = (p - response.rect.min) / CELL;
            Pos {
                x: (v.x as i32).clamp(0, size.x - 1),
                y: (v.y as i32).clamp(0, size.y - 1),
            }
        };
        // Only a moving pointer moves the cursor, so it cannot undo keyboard navigation.
        let hover = response.hover_pos();
        if let Some(p) = hover.filter(|p| self.last_hover != Some(*p)) {
            self.cursor = cell_at(p);
        }
        self.last_hover = hover;
        if response.clicked() {
            self.go_to(self.cursor);
            ui.ctx().request_repaint();
        }
        if response.secondary_clicked() {
            self.menu_cell = self.cursor;
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
            let (dx, dy, look) = ui.input(|i| {
                let k = |key| i.key_pressed(key) as i32;
                (
                    k(egui::Key::ArrowRight) - k(egui::Key::ArrowLeft),
                    k(egui::Key::ArrowDown) - k(egui::Key::ArrowUp),
                    i.key_pressed(egui::Key::L),
                )
            });
            self.cursor = Pos {
                x: (self.cursor.x + dx).clamp(0, size.x - 1),
                y: (self.cursor.y + dy).clamp(0, size.y - 1),
            };
            if look && !self.keys_consumed {
                self.look(self.cursor);
            }
        }
        response.clone().on_hover_ui_at_pointer(|ui| {
            ui.label(self.model.cell(self.cursor).desc);
        });
        let menu = response.context_menu(|ui| {
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
        if let Some(menu) = menu {
            ui.ctx().accesskit_node_builder(menu.response.id, |b| {
                b.set_role(accesskit::Role::Menu);
                b.set_label("Map actions");
            });
        }
        let font = egui::FontId::monospace(CELL - 2.0);
        for y in 0..size.y {
            for x in 0..size.x {
                let cell = self.model.cell(Pos { x, y });
                let center = response.rect.min + egui::vec2(x as f32 + 0.5, y as f32 + 0.5) * CELL;
                if let Some(bg) = cell.bg {
                    painter.rect_filled(
                        egui::Rect::from_center_size(center, egui::Vec2::splat(CELL)),
                        0.0,
                        rgb(bg),
                    );
                }
                painter.text(
                    center,
                    egui::Align2::CENTER_CENTER,
                    cell.glyph,
                    font.clone(),
                    rgb(cell.color),
                );
            }
        }
        let rect = |p: Pos| {
            egui::Rect::from_min_size(
                response.rect.min + egui::vec2(p.x as f32, p.y as f32) * CELL,
                egui::Vec2::splat(CELL),
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
        let origin = self.model.origin();
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
        let center = rect(player).center();
        painter.text(
            center,
            egui::Align2::CENTER_CENTER,
            '@',
            font,
            egui::Color32::YELLOW,
        );
        painter.rect_stroke(
            rect(self.cursor),
            0.0,
            egui::Stroke::new(1.0, egui::Color32::WHITE),
            egui::StrokeKind::Inside,
        );
    }

    fn items(&mut self, ui: &mut egui::Ui) {
        let mut moved = None;
        ui.columns(2, |cols| {
            for (ui, column) in cols.iter_mut().zip([Column::Ground, Column::Inventory]) {
                let (title, verb, items) = match column {
                    Column::Ground => ("Ground", "Pick up", self.model.ground()),
                    Column::Inventory => ("Inventory", "Drop", self.model.inventory()),
                };
                ui.heading(title);
                let (_, dropped) =
                    ui.dnd_drop_zone::<Dragged, _>(egui::Frame::group(ui.style()), |ui| {
                        ui.set_min_size(egui::vec2(ui.available_width(), 200.0));
                        for (index, name) in items.iter().enumerate() {
                            ui.horizontal(|ui| {
                                // Drag only: the button after it is the row's one keyboard stop.
                                let grip = ui.add(egui::Label::new(name).sense(egui::Sense::DRAG));
                                grip.dnd_set_drag_payload(Dragged {
                                    from: column,
                                    index,
                                });
                                let button = ui.button(verb);
                                ui.ctx().accesskit_node_builder(button.id, |b| {
                                    b.set_label(format!("{verb} {name}"))
                                });
                                if button.clicked() {
                                    moved = Some(Dragged {
                                        from: column,
                                        index,
                                    });
                                }
                            });
                        }
                    });
                if let Some(d) = dropped.filter(|d| d.from != column) {
                    moved = Some(*d);
                }
            }
        });
        match moved {
            Some(Dragged {
                from: Column::Ground,
                index,
            }) => self.model.pick_up(index),
            Some(Dragged {
                from: Column::Inventory,
                index,
            }) => self.model.drop_item(index),
            None => {}
        }
    }

    fn log(&mut self, ui: &mut egui::Ui) {
        let native = self.model.native_log();
        let shown = native.iter().map(LogLine::shown).collect::<Vec<_>>();
        let native_live = self.native_live.update(&shown).to_owned();
        let command_live = self.command_live.update(self.model.messages()).to_owned();
        message_log(
            ui,
            native,
            self.model.messages(),
            [&native_live, &command_live],
        );
    }
}

/// The message log panel: the game's own lines in their native colours, then the client's command
/// results and errors in a separate scroll area. What each list gained since the last frame is a
/// polite live region, so history is never re-announced.
fn message_log(ui: &mut egui::Ui, native: &[crate::LogLine], commands: &[String], live: [&str; 2]) {
    ui.heading("Message log");
    if !native.is_empty() {
        let live = ui.label(live[0]);
        ui.ctx()
            .accesskit_node_builder(live.id, |b| b.set_live(accesskit::Live::Polite));
    }
    egui::ScrollArea::vertical()
        .id_salt("native_log")
        .max_height(100.0)
        .stick_to_bottom(true)
        .show(ui, |ui| {
            for line in &native[native.len().saturating_sub(100)..] {
                let text = egui::RichText::new(line.shown());
                ui.label(line.color.map_or(text.clone(), |c| text.color(rgb(c))));
            }
        });
    if !commands.is_empty() {
        ui.separator();
        let live = ui.label(live[1]);
        ui.ctx()
            .accesskit_node_builder(live.id, |b| b.set_live(accesskit::Live::Polite));
    }
    egui::ScrollArea::vertical()
        .id_salt("command_log")
        .max_height(100.0)
        .stick_to_bottom(true)
        .show(ui, |ui| {
            for message in &commands[commands.len().saturating_sub(50)..] {
                ui.label(message);
            }
        });
}

impl<M: GameModel> eframe::App for App<M> {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.show(ui);
    }
}
