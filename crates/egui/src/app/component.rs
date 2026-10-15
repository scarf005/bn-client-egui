//! The top screen of the engine's stack drawn from its components (protocol 0.2): lists, tabs, splits,
//! panels, forms, text, grids, the map and canvases, laid out the same way for every screen.
//!
//! Drawing reads the engine's data and collects what the player did as `Intent`s. Applying an intent only
//! calls the interaction core (`bn_client_core::interaction`), which decides what is sent.

use super::{App, sidebar};
use crate::layout::{self, Extent};
use crate::windows::{self, Place, Shown, WindowSpec};
use crate::{a11y, editor, text};
use bn_client_core::interaction::{self, ArgKind, InvokeError, Reason};
use bn_client_core::protocol::{
    Affordance, AffordanceRole, CanvasComponent, Component, Editor, EditorValue, FieldKind, Form,
    FormField, Grid, InvokeArgs, List, MapComponent, MapMode, OvermapTile, OvermapWire, PanelStat,
    Row, Screen, ScreenDock, ScreenRole, Selection, SidebarSide, Text, TextBlock, TextRole, Tone,
    WorldPos,
};
use bn_client_core::{Cell, GameModel, Pos, Rgb, markup};
use eframe::egui::{self, WidgetInfo, WidgetType};
use std::cell::{Cell as Slot, RefCell};
use std::collections::HashSet;

/// The margin around each pane of a split, on every side.
const PANE_MARGIN: i8 = 2;
/// Rows left below the last one shown before the next page is asked for.
const PREFETCH: usize = 20;
/// Height the description under a list may take.
const DESCRIPTION_HEIGHT: f32 = 96.0;
/// Side of a grid cell in points.
const GRID_CELL: f32 = 26.0;
const GOOD: egui::Color32 = egui::Color32::from_rgb(110, 210, 110);
const BAD: egui::Color32 = egui::Color32::from_rgb(235, 95, 95);

/// The client-local state of the top screen: what the interaction core keeps, and what the player is asked.
#[derive(Default)]
pub(super) struct Screens {
    interaction: bn_client_core::interaction::Interaction,
    /// Why the last refused affordance is off, in the engine's words.
    denial: Option<String>,
    /// The list the keys just moved the focus in, to be scrolled to it.
    reveal: Option<String>,
    /// The height the list drawn last had, for the rows a page key moves.
    list_height: Slot<f32>,
    /// The squares of this frame's list rows left for their item's sprite, painted once the window is drawn.
    icons: RefCell<Vec<RowIcon>>,
    /// What this frame's overmap draws, painted once the window is drawn, so that a tileset's sprites come
    /// before the marks over them.
    overmap: RefCell<Vec<OvermapPaint>>,
}

/// One thing the overmap draws, in the order it is drawn.
enum OvermapPaint {
    /// A square: its terrain sprite when the tileset has one, else its glyph.
    Square {
        painter: egui::Painter,
        rect: egui::Rect,
        tile: Option<OwnedTile>,
        glyph: String,
        color: Rgb,
        bg: Option<Rgb>,
        /// Something lies over the terrain (the player, a note, the route): the glyph is drawn over a sprite.
        overlay: bool,
        font: egui::FontId,
    },
    Fill {
        painter: egui::Painter,
        rect: egui::Rect,
        color: egui::Color32,
    },
    Outline {
        painter: egui::Painter,
        rect: egui::Rect,
    },
    /// A name or marker drawn over the squares, centred on `rect`.
    Text {
        painter: egui::Painter,
        rect: egui::Rect,
        text: String,
        color: Rgb,
        font: egui::FontId,
    },
}

/// An overmap tile that outlives the frame's data.
struct OwnedTile {
    id: String,
    rotation: u8,
    subtile: String,
    fallbacks: Vec<String>,
}

/// The widths a list's label and its columns need.
struct ListWidths {
    label: f32,
    columns: Vec<f32>,
}

/// The smallest font a canvas is drawn in.
const CANVAS_MIN_FONT: f32 = 10.0;

fn text_width(ui: &egui::Ui, text: &str, font: &egui::FontId) -> f32 {
    ui.painter()
        .layout_no_wrap(text.into(), font.clone(), egui::Color32::WHITE)
        .size()
        .x
}

/// The width of the longest line of `text`, unwrapped.
fn line_width(ui: &egui::Ui, text: &str, font: &egui::FontId) -> f32 {
    text.lines()
        .map(|line| text_width(ui, line, font))
        .fold(0.0, f32::max)
}

/// The width of the longest word of `text`: the least a wrapped label can be without breaking inside one.
fn word_width(ui: &egui::Ui, text: &str, font: &egui::FontId) -> f32 {
    text.split_whitespace()
        .map(|word| text_width(ui, word, font))
        .fold(0.0, f32::max)
}

/// A list row's item and the square it is drawn in.
struct RowIcon {
    painter: egui::Painter,
    rect: egui::Rect,
    cell: Cell,
}

/// What the player did while the screen was drawn.
enum Intent {
    Focus {
        component: String,
        row: String,
    },
    Mark {
        component: String,
        row: String,
    },
    MarkCount {
        component: String,
        row: String,
        count: u64,
    },
    Run(String),
    /// Runs the affordance that puts the cursor on a square.
    Move {
        affordance: String,
        at: WorldPos,
    },
    Fill {
        affordance: String,
        form: String,
        field: String,
        value: String,
    },
    More(String),
    Text(String),
    Pane(String),
    Confirm,
    Cancel,
}

/// The screen on top, when the engine describes it by components. The map is drawn by the map view.
pub(super) fn sliced(screens: &[Screen]) -> Option<&Screen> {
    screens
        .last()
        .filter(|s| s.role != ScreenRole::Map && s.root.is_some())
}

/// `component` and every component inside it, in reading order.
fn walk(component: &Component) -> Vec<&Component> {
    std::iter::once(component)
        .chain(component.children().into_iter().flat_map(walk))
        .collect()
}

fn find<'a>(screen: &'a Screen, id: &str) -> Option<&'a Affordance> {
    screen.affordances.iter().find(|a| a.id == id)
}

/// The room a window leaves the map's squares around the avatar and the engine's cursor, in whole squares.
const KEEP_CLEAR: f32 = 1.0;
/// The share of the map region a window beside the cursor, or docked to a side, takes in width.
const SIDE_SHARE: f32 = 0.4;
/// The space kept between a window and the edge of the map region.
const REGION_MARGIN: f32 = 8.0;

/// How a screen of the stack sits in `region` (the map panel; the sidebar stays visible): a full screen fills it, a
/// dialog is centred, a window that asked for a side sits there, and a screen that puts a cursor on the map sits
/// on the side away from it. `avoid` are the rectangles on the map that must stay uncovered.
fn placement(
    screen: &Screen,
    region: egui::Rect,
    sidebar: Option<(SidebarSide, egui::Rect)>,
    avoid: &[egui::Rect],
) -> Place {
    let inner = region.shrink(REGION_MARGIN);
    let max = egui::vec2(inner.width() * SIDE_SHARE, inner.height());
    let side = |left: bool| Place::Side {
        pivot: if left {
            egui::Align2::LEFT_TOP
        } else {
            egui::Align2::RIGHT_TOP
        },
        pos: if left {
            inner.left_top()
        } else {
            inner.right_top()
        },
        max,
    };
    let beside = |left: bool| {
        let (x0, x1) = if left {
            (inner.left(), inner.left() + max.x)
        } else {
            (inner.right() - max.x, inner.right())
        };
        egui::Rect::from_x_y_ranges(x0..=x1, inner.y_range())
    };
    // Away from the cursor: the window's side is the one whose half of the room has none of `avoid`.
    let away = || {
        let covered = |left: bool| avoid.iter().any(|r| beside(left).intersects(*r));
        let cursor_left = avoid
            .first()
            .is_some_and(|r| r.center().x < inner.center().x);
        match (covered(!cursor_left), covered(cursor_left)) {
            (false, _) => side(!cursor_left),
            (true, false) => side(cursor_left),
            (true, true) => side(!cursor_left),
        }
    };
    match (screen.dock, screen.role) {
        (Some(ScreenDock::Left), _) => side(true),
        (Some(ScreenDock::Right), _) => side(false),
        // Over the sidebar: its own place, level with the map's top.
        (Some(ScreenDock::Overlapping), _) => match sidebar {
            Some((_, rect)) => Place::Side {
                pivot: egui::Align2::LEFT_TOP,
                pos: egui::pos2(rect.left(), inner.top()),
                max: egui::vec2(rect.width(), inner.height()),
            },
            None => side(true),
        },
        (Some(ScreenDock::Cursor), _) => away(),
        // A screen that puts a cursor on the map (look, aim) must not hide what the cursor is on.
        _ if screen
            .root
            .as_ref()
            .is_some_and(|r| r.cursor_map().is_some()) =>
        {
            away()
        }
        (None, ScreenRole::Dialog) => Place::Center(inner),
        (None, _) => Place::Fill(inner),
    }
}

/// The ids of the affordances some component names.
fn claimed(root: &Component) -> HashSet<&str> {
    walk(root)
        .into_iter()
        .flat_map(|c| {
            let extra = match c {
                Component::Tabs(t) => t.select.as_deref(),
                Component::Form(f) => f.submit.as_deref(),
                _ => None,
            };
            c.head().affordances.iter().map(String::as_str).chain(extra)
        })
        .collect()
}

/// Whether a component's own widgets run the affordance, so it needs no button.
fn driven(component: &Component, affordance: &Affordance) -> bool {
    match component {
        Component::Tabs(t) => t.select.as_deref() == Some(affordance.id.as_str()),
        Component::Form(_) => affordance.args.as_ref().is_some_and(|a| a.text.is_some()),
        Component::Grid(g) => g
            .cells
            .iter()
            .any(|cell| cell.affordance.as_deref() == Some(affordance.id.as_str())),
        _ => false,
    }
}

/// The form field as the value editors take it.
fn editor_of(field: &FormField) -> Editor {
    let kind = match field.kind {
        FieldKind::Text => "text",
        FieldKind::Integer => "integer",
        FieldKind::Float => "float",
        FieldKind::Key => "key",
        FieldKind::Bool => "bool",
        FieldKind::Select => "select",
    };
    Editor {
        kind: kind.into(),
        value: field.value.clone(),
        values: field
            .options
            .iter()
            .flatten()
            .map(|o| EditorValue {
                id: o.id.clone(),
                label: o.label.shown(),
            })
            .collect(),
        minimum: field.min.clone(),
        maximum: field.max.clone(),
        max_length: field.max_length.unwrap_or(-1),
        multiline: field.multiline.unwrap_or(false),
    }
}

/// The client's words for a refusal the engine gave no reason for.
fn reason_words(reason: Reason) -> String {
    match reason {
        Reason::NoScreen | Reason::Unknown | Reason::BadArgs => "Not available.",
        Reason::Disabled | Reason::RowDisabled | Reason::NotOnThisRow => "Not available now.",
        Reason::Missing(_) => "Choose something first.",
        Reason::Busy => "Busy.",
    }
    .into()
}

/// `text` in the game colour, as the native markup writes it.
pub(super) fn tagged(color: Option<&str>, text: &str) -> String {
    match color {
        Some(color) => format!("<color_{color}>{text}</color>"),
        None => text.into(),
    }
}

impl<M: GameModel> App<M> {
    /// Takes the stack the engine published, and moves the engine's cursor where the player's focus went.
    fn sync_screens(&mut self) {
        self.screens.interaction.sync(self.model.screens());
        if let Some((id, args)) = self.screens.interaction.take_select() {
            let _ = self.model.invoke_affordance(&id, args);
        }
    }

    /// The screen on top drawn from its components when the engine describes it so, else the 0.1 menu.
    pub(super) fn menu_or_screen(&mut self, ctx: &egui::Context) {
        self.sync_screens();
        let Some(top) = sliced(self.model.screens()) else {
            return self.menu(ctx);
        };
        let mut out = vec![];
        let shown = self.screen_window(ctx, top, &mut out);
        if self.keep_window(ctx, "screen", &shown) {
            out.push(Intent::Cancel);
        }
        self.screens.reveal = None;
        let season = self.model.season();
        for icon in self.screens.icons.take() {
            let display = &mut self.display;
            sidebar::paint_cell(
                &icon.painter,
                icon.rect,
                display,
                &icon.cell,
                season.as_deref(),
            );
        }
        self.paint_overmap(ctx);
        out.into_iter().for_each(|intent| self.apply(intent));
    }

    /// The keys pressed this frame go to the interaction core, which decides what each one does.
    pub(super) fn component_keys(&mut self, ctx: &egui::Context) {
        self.keys_consumed = false;
        self.sync_screens();
        let page = self.page_rows(ctx);
        let keys = ctx.input(|i| {
            i.events
                .iter()
                .filter_map(|event| match event {
                    egui::Event::Text(t) => Some(if t == " " { "SPACE".into() } else { t.clone() }),
                    egui::Event::Key {
                        key,
                        pressed: true,
                        modifiers,
                        ..
                    } => super::key_name(*key, *modifiers),
                    _ => None,
                })
                .collect::<Vec<_>>()
        });
        // A text box has the typing; its keys are not the screen's, except Escape, which closes the screen.
        let typing = ctx.text_edit_focused();
        let keys = keys.into_iter().filter(|key| !typing || key == "ESC");
        for key in keys {
            let Some(top) = sliced(self.model.screens()) else {
                return;
            };
            let model = &self.model;
            let out = self
                .screens
                .interaction
                .press(top, &key, page, &|list| model.rows(list));
            self.settle(out);
        }
    }

    /// Shows what the core decided and sends the call it built.
    fn settle(&mut self, out: interaction::Outcome) {
        self.keys_consumed |= out.consumed;
        self.screens.denial = out
            .denial
            .as_ref()
            .map(Text::shown)
            .or_else(|| out.reason.map(reason_words));
        if out.revealed.is_some() {
            self.screens.reveal = out.revealed;
        }
        if let Some((id, args)) = out.invoke
            && let Err(InvokeError::Disabled(denial)) = self.model.invoke_affordance(&id, args)
        {
            self.screens.denial = denial.as_ref().map(Text::shown);
        }
    }

    /// The room a list has here: what the window has left, less its description and the buttons under it.
    fn list_height(ui: &egui::Ui) -> f32 {
        let spacing = ui.spacing();
        let buttons = 2.0 * (spacing.interact_size.y + spacing.item_spacing.y);
        (ui.available_height() - DESCRIPTION_HEIGHT - buttons).max(DESCRIPTION_HEIGHT)
    }

    /// How many rows a list shows at once: the rows of the list drawn last.
    fn page_rows(&self, ctx: &egui::Context) -> usize {
        let spacing = &ctx.global_style().spacing;
        let row = spacing.interact_size.y + spacing.item_spacing.y;
        ((self.screens.list_height.get() / row) as usize).max(1)
    }

    /// Hands what the player did to the interaction core.
    fn apply(&mut self, intent: Intent) {
        if let Intent::More(component) = &intent {
            return self.model.more_rows(component);
        }
        let interaction = &mut self.screens.interaction;
        let Some(top) = sliced(self.model.screens()) else {
            return;
        };
        let out = match intent {
            Intent::Focus { component, row } => return interaction.focus_row(&component, &row),
            Intent::Mark { component, row } => return interaction.toggle_pick(&component, &row),
            Intent::MarkCount {
                component,
                row,
                count,
            } => return interaction.set_pick_count(&component, &row, Some(count)),
            Intent::More(_) => return,
            Intent::Text(typed) => return interaction.answer_text(&typed),
            Intent::Cancel => return interaction.cancel(),
            Intent::Run(id) => interaction.activate(top, &id),
            Intent::Move { affordance, at } => {
                let args = InvokeArgs {
                    pos: Some(at),
                    ..InvokeArgs::default()
                };
                if let Err(InvokeError::Disabled(denial)) =
                    self.model.invoke_affordance(&affordance, args)
                {
                    self.screens.denial = denial.as_ref().map(Text::shown);
                }
                return;
            }
            Intent::Fill {
                affordance,
                form,
                field,
                value,
            } => interaction.fill(top, &affordance, &form, &field, &value),
            Intent::Pane(pane) => interaction.answer_pane(top, &pane),
            Intent::Confirm => interaction.confirm(top),
        };
        self.settle(out);
    }

    fn screen_window(
        &self,
        ctx: &egui::Context,
        screen: &Screen,
        out: &mut Vec<Intent>,
    ) -> Shown<()> {
        let title = markup::plain(&screen.title.as_ref().map(Text::shown).unwrap_or_default());
        let frame = super::menu::list_frame(ctx);
        let region = self.region.unwrap_or_else(|| ctx.content_rect());
        // The squares to keep clear: the engine's cursor and the avatar, a square around each.
        let avoid = self.map_view.map_or(vec![], |(origin, cell)| {
            let square = |at: Pos| {
                egui::Rect::from_min_size(
                    origin + egui::vec2(at.x as f32, at.y as f32) * cell,
                    egui::Vec2::splat(cell),
                )
                .expand(KEEP_CLEAR * cell)
            };
            self.model
                .focus()
                .into_iter()
                .chain([self.model.player()])
                .map(square)
                .collect()
        });
        let place = placement(screen, region, self.sidebar, &avoid);
        // A window that is not the whole room is as wide as its content, within the room.
        let width = match (place, &screen.root) {
            (Place::Fill(_), _) | (_, None) => None,
            (_, Some(root)) => {
                let builder = egui::UiBuilder::new().max_rect(region);
                let ui = egui::Ui::new(ctx.clone(), egui::Id::new("measure"), builder);
                let content = self.extent(&ui, root, region.width()).natural;
                Some(content + frame.total_margin().sum().x + ui.spacing().window_margin.sum().x)
            }
        };
        let base = self.window_spec(ctx, "screen", &title);
        let spec = WindowSpec {
            closable: interaction::with_role(screen, AffordanceRole::Cancel).is_some(),
            frame: Some(frame),
            place: Some(place),
            width,
            ..base
        };
        let shown = windows::show(ctx, spec, |ui| {
            let Some(root) = &screen.root else {
                return;
            };
            let named = claimed(root);
            let rest = screen
                .affordances
                .iter()
                .filter(|a| !named.contains(a.id.as_str()))
                .collect::<Vec<_>>();
            let footer = |ui: &mut egui::Ui, out: &mut Vec<Intent>| {
                self.question(ui, screen, out);
                if let Some(denial) = &self.screens.denial {
                    ui.colored_label(BAD, markup::plain(denial));
                }
                self.affordance_buttons(ui, &rest, None, out);
            };
            // A screen that fills the room keeps its footer under its body, whatever the body's height.
            if matches!(place, Place::Fill(_)) {
                egui::Panel::bottom(egui::Id::new(("footer", &screen.screen_id)))
                    .resizable(false)
                    .frame(egui::Frame::NONE)
                    .show_separator_line(false)
                    .show(ui, |ui| footer(ui, out));
                egui::CentralPanel::no_frame().show(ui, |ui| self.component(ui, screen, root, out));
            } else {
                self.component(ui, screen, root, out);
                footer(ui, out);
            }
        });
        shown
    }

    /// One line of the engine's text with its colour tags; what does not fit is cut.
    fn line(&self, ui: &egui::Ui, markup: &str) -> egui::WidgetText {
        let font = egui::TextStyle::Body.resolve(ui.style());
        let mut job = text::job(&self.model.styled(markup), &font);
        job.wrap.max_rows = 1;
        job.wrap.break_anywhere = true;
        job.into()
    }

    /// The game colour as a colour of the screen.
    fn tint(&self, name: &str) -> Option<egui::Color32> {
        Some(super::rgb(self.model.colors().resolve(name).0))
    }

    /// Buttons for the affordances: enabled as the affordance (and the row, when it overrides) says, with the
    /// engine's reason on hover when off.
    fn affordance_buttons(
        &self,
        ui: &mut egui::Ui,
        affordances: &[&Affordance],
        row: Option<&Row>,
        out: &mut Vec<Intent>,
    ) {
        if affordances.is_empty() {
            return;
        }
        ui.horizontal_wrapped(|ui| {
            for a in affordances
                .iter()
                .filter(|a| a.role != Some(AffordanceRole::Select))
            {
                let availability = interaction::row_availability(a, row);
                let (enabled, denial) =
                    (availability.enabled, availability.denial.map(|d| d.shown()));
                let key = a
                    .keys
                    .first()
                    .map(|k| format!(" [{k}]"))
                    .unwrap_or_default();
                let label = format!("{}{key}", markup::plain(&a.label.shown()));
                let button = self.control(ui, label, &a.id, enabled, false);
                let button = match (enabled, denial) {
                    (false, Some(why)) => button.on_disabled_hover_text(why),
                    _ => button,
                };
                if button.clicked() {
                    out.push(Intent::Run(a.id.clone()));
                }
            }
        });
    }

    /// The affordances a component names that its own widgets do not run.
    fn component_buttons(
        &self,
        ui: &mut egui::Ui,
        screen: &Screen,
        component: &Component,
        row: Option<&Row>,
        out: &mut Vec<Intent>,
    ) {
        let own = component
            .head()
            .affordances
            .iter()
            .filter_map(|id| find(screen, id))
            .filter(|a| !driven(component, a))
            .collect::<Vec<_>>();
        self.affordance_buttons(ui, &own, row, out);
    }

    fn component(&self, ui: &mut egui::Ui, screen: &Screen, c: &Component, out: &mut Vec<Intent>) {
        if let (Some(label), false) = (&c.head().label, matches!(c, Component::Panel(_))) {
            ui.label(egui::RichText::new(markup::plain(&label.shown())).strong());
        }
        match c {
            Component::List(list) => self.list(ui, screen, list, out),
            Component::Tabs(tabs) => {
                let select = tabs.select.as_deref().and_then(|id| find(screen, id));
                ui.horizontal_wrapped(|ui| {
                    for tab in &tabs.tabs {
                        let on = tab.enabled && select.is_some_and(|a| a.enabled);
                        let marked = format!(
                            "{}{}",
                            tab.label.shown(),
                            if tab.unread { " •" } else { "" }
                        );
                        let label = self.line(ui, &marked);
                        let button = self.control(ui, label, &tab.id, on, tab.selected);
                        a11y::tab(ui.ctx(), button.id, tab.selected);
                        if let (true, Some(a)) = (button.clicked(), select) {
                            out.push(Intent::Focus {
                                component: tabs.head.id.clone(),
                                row: tab.id.clone(),
                            });
                            out.push(Intent::Run(a.id.clone()));
                        }
                    }
                });
                if let Some(child) = &tabs.child {
                    self.component(ui, screen, child, out);
                }
            }
            Component::Split(split) => {
                // A banner is the screen's heading: it sits above the parts, whatever their number.
                let (banners, parts) = split.children.iter().partition::<Vec<_>, _>(
                    |c| matches!(c, Component::Text(t) if t.role == Some(TextRole::Banner)),
                );
                for banner in banners {
                    self.component(ui, screen, banner, out);
                }
                self.split(
                    ui,
                    screen,
                    &split.head.id,
                    &parts,
                    split.active.as_deref(),
                    out,
                );
            }
            Component::Panel(panel) => {
                egui::Frame::group(ui.style()).show(ui, |ui| {
                    // A panel the screen's own title already names has none.
                    let title = markup::plain(&panel.title.shown());
                    if !title.is_empty() {
                        ui.label(egui::RichText::new(title).strong());
                    }
                    if let Some(subtitle) = &panel.subtitle {
                        ui.label(self.line(ui, &subtitle.shown()));
                    }
                    for stat in panel.stats.iter().flatten() {
                        self.stat(ui, stat);
                    }
                    if let Some(filter) = &panel.filter {
                        ui.weak(format!("/{filter}"));
                    }
                    if let Some(child) = &panel.child {
                        self.component(ui, screen, child, out);
                    }
                });
            }
            Component::Form(form) => self.form(ui, screen, form, out),
            Component::Text(block) => self.text_block(ui, block),
            Component::Grid(grid) => self.grid(ui, grid, out),
            Component::Map(map) => self.map_slot(ui, screen, map, out),
            Component::Canvas(canvas) => self.canvas(ui, canvas),
        }
        if !matches!(c, Component::List(_)) {
            self.component_buttons(ui, screen, c, None, out);
        }
    }

    /// The panes side by side, each as wide as its content needs (`layout::split_widths`); when even the
    /// minimums do not fit, the split scrolls sideways instead of breaking words.
    fn split(
        &self,
        ui: &mut egui::Ui,
        screen: &Screen,
        id: &str,
        parts: &[&Component],
        active: Option<&str>,
        out: &mut Vec<Intent>,
    ) {
        let gap = ui.spacing().item_spacing.x;
        let room = ui.available_width();
        let margin = 2.0 * f32::from(PANE_MARGIN);
        let extents = parts
            .iter()
            .map(|c| {
                let e = self.extent(ui, c, room);
                Extent {
                    min: e.min + margin,
                    natural: e.natural + margin,
                    ..e
                }
            })
            .collect::<Vec<_>>();
        let widths = layout::split_widths(&extents, room, gap);
        let mut panes = |ui: &mut egui::Ui| {
            ui.horizontal_top(|ui| {
                for (child, width) in parts.iter().zip(&widths) {
                    let stroke = if active == Some(child.id()) {
                        ui.visuals().selection.stroke
                    } else {
                        egui::Stroke::NONE
                    };
                    let size = egui::vec2(*width, ui.available_height());
                    let down = egui::Layout::top_down(egui::Align::Min);
                    ui.allocate_ui_with_layout(size, down, |ui| {
                        egui::Frame::NONE
                            .stroke(stroke)
                            .inner_margin(PANE_MARGIN)
                            .show(ui, |ui| {
                                ui.set_width(width - margin);
                                self.component(ui, screen, child, out)
                            });
                    });
                }
            });
        };
        match layout::overflows(&widths, room, gap) {
            true => {
                egui::ScrollArea::horizontal()
                    .id_salt(("split", id))
                    .auto_shrink([false, true])
                    .show(ui, panes);
            }
            false => panes(ui),
        }
    }

    /// What `c` needs in width, measured without laying it out: `room` is what its surroundings offer.
    fn extent(&self, ui: &egui::Ui, c: &Component, room: f32) -> Extent {
        let gap = ui.spacing().item_spacing.x;
        let font = egui::TextStyle::Body.resolve(ui.style());
        let stacked = |a: Extent, b: Extent| Extent {
            min: a.min.max(b.min),
            natural: a.natural.max(b.natural),
            grows: a.grows || b.grows,
        };
        let fixed = |w: f32| Extent {
            min: w,
            natural: w,
            grows: false,
        };
        let prose = |text: &str| Extent {
            min: word_width(ui, text, &font),
            natural: line_width(ui, text, &font),
            grows: false,
        };
        let inside = |child: &Option<Box<Component>>| {
            child.as_ref().map(|child| self.extent(ui, child, room))
        };
        match c {
            Component::List(list) => {
                let rows = self.model.rows(list);
                let widths = self.list_widths(ui, list, &rows);
                let natural = widths.label + widths.columns.iter().map(|w| w + gap).sum::<f32>();
                Extent {
                    min: natural.min(room / 2.0),
                    natural,
                    grows: true,
                }
            }
            Component::Tabs(tabs) => {
                let pad = 2.0 * ui.spacing().button_padding.x;
                let labels = tabs
                    .tabs
                    .iter()
                    .map(|t| text_width(ui, &markup::plain(&t.label.shown()), &font) + pad)
                    .collect::<Vec<_>>();
                let row = Extent {
                    min: labels.iter().copied().fold(0.0, f32::max),
                    natural: labels.iter().map(|w| w + gap).sum(),
                    grows: false,
                };
                inside(&tabs.child).map_or(row, |child| stacked(row, child))
            }
            Component::Split(split) => {
                let margin = 2.0 * f32::from(PANE_MARGIN);
                let parts = split
                    .children
                    .iter()
                    .map(|c| self.extent(ui, c, room))
                    .map(|e| Extent {
                        min: e.min + margin,
                        natural: e.natural + margin,
                        ..e
                    })
                    .collect::<Vec<_>>();
                Extent {
                    min: parts.iter().map(|p| p.min).sum::<f32>() + layout::gaps(parts.len(), gap),
                    natural: parts.iter().map(|p| p.natural).sum::<f32>()
                        + layout::gaps(parts.len(), gap),
                    grows: parts.iter().any(|p| p.grows),
                }
            }
            Component::Panel(panel) => {
                let frame = egui::Frame::group(ui.style()).total_margin().sum().x;
                let stats = panel.stats.iter().flatten().map(|stat| {
                    let max = stat
                        .max
                        .as_ref()
                        .map_or(String::new(), |m| format!("/{}", m.shown()));
                    let unit = stat
                        .unit
                        .as_ref()
                        .map_or(String::new(), |u| format!(" {}", u.shown()));
                    let value = format!("{}{max}{unit}", markup::plain(&stat.value.shown()));
                    let label = markup::plain(&stat.label.shown());
                    fixed(text_width(ui, &label, &font) + gap + text_width(ui, &value, &font))
                });
                let lines = [Some(prose(&markup::plain(&panel.title.shown())))]
                    .into_iter()
                    .flatten()
                    .chain(
                        panel
                            .subtitle
                            .iter()
                            .map(|t| prose(&markup::plain(&t.shown()))),
                    )
                    .chain(stats)
                    .chain(inside(&panel.child))
                    .fold(fixed(0.0), stacked);
                Extent {
                    min: lines.min + frame,
                    natural: lines.natural + frame,
                    ..lines
                }
            }
            Component::Form(form) => {
                let label = form
                    .fields
                    .iter()
                    .map(|f| text_width(ui, &markup::plain(&f.label.shown()), &font))
                    .fold(0.0, f32::max);
                let w = label + ui.spacing().text_edit_width;
                Extent {
                    min: w,
                    natural: w,
                    grows: true,
                }
            }
            Component::Text(block) => {
                let text = markup::plain(&block.markup.shown());
                match block.role {
                    Some(TextRole::TerminalOutput | TextRole::Banner) => {
                        let mono = egui::TextStyle::Monospace.resolve(ui.style());
                        fixed(line_width(ui, &text, &mono))
                    }
                    None => Extent {
                        grows: true,
                        ..prose(&text)
                    },
                }
            }
            Component::Grid(grid) => fixed(grid.cols as f32 * (GRID_CELL + gap)),
            Component::Map(_) => Extent {
                min: 160.0,
                natural: room.min(480.0),
                grows: true,
            },
            Component::Canvas(canvas) => {
                let mono = egui::FontId::monospace(CANVAS_MIN_FONT);
                let cell = text_width(ui, "0", &mono);
                Extent {
                    grows: true,
                    ..fixed(canvas.cols as f32 * cell)
                }
            }
        }
    }

    /// The widths a list's label and each of its columns need: the widest caption or value in each.
    fn list_widths(&self, ui: &egui::Ui, list: &List, rows: &[&Row]) -> ListWidths {
        let font = egui::TextStyle::Body.resolve(ui.style());
        let spacing = ui.spacing();
        let height = spacing.interact_size.y;
        let pad = 2.0 * spacing.button_padding.x;
        let widest = |texts: &mut dyn Iterator<Item = f32>| texts.fold(0.0, f32::max);
        let label = widest(
            &mut rows.iter().filter(|r| r.header != Some(true)).map(|row| {
                let square = row.look.is_some() && self.display.tiles();
                let marked = Self::row_markup(list, row, false, square);
                let icon = if square {
                    height + spacing.item_spacing.x
                } else {
                    0.0
                };
                icon + text_width(ui, &markup::plain(&marked), &font) + pad
            }),
        );
        let columns = list
            .columns
            .iter()
            .map(|column| {
                let caption = text_width(ui, &markup::plain(&column.label.shown()), &font);
                let values = rows.iter().map(|row| {
                    let value = row
                        .columns
                        .iter()
                        .find(|c| c.key.as_deref() == Some(column.key.as_str()));
                    value.map_or(0.0, |c| {
                        text_width(ui, &markup::plain(&c.value.shown()), &font)
                    })
                });
                caption.max(widest(&mut { values }))
            })
            .collect();
        ListWidths { label, columns }
    }

    fn stat(&self, ui: &mut egui::Ui, stat: &PanelStat) {
        let max = stat.max.as_ref().map(|m| format!("/{}", m.shown()));
        let unit = stat.unit.as_ref().map(|u| format!(" {}", u.shown()));
        let value = markup::plain(&stat.value.shown());
        let label = markup::plain(&stat.label.shown());
        if value.is_empty() && max.is_none() && unit.is_none() {
            let tone = stat.tone.map(|t| if t == Tone::Good { GOOD } else { BAD });
            ui.label(egui::RichText::new(label).color(tone.unwrap_or(ui.visuals().text_color())));
            return;
        }
        let shown = format!(
            "{value}{}{}",
            max.unwrap_or_default(),
            unit.unwrap_or_default()
        );
        let tone = stat.tone.map(|t| if t == Tone::Good { GOOD } else { BAD });
        // The label and the value are parts of their own; the engine's text carries any punctuation.
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            ui.label(label);
            ui.label(egui::RichText::new(shown).color(tone.unwrap_or(ui.visuals().text_color())));
        });
    }

    fn text_block(&self, ui: &mut egui::Ui, block: &TextBlock) {
        let (font, wrap) = match block.role {
            Some(TextRole::TerminalOutput | TextRole::Banner) => (
                egui::TextStyle::Monospace.resolve(ui.style()),
                egui::TextWrapMode::Extend,
            ),
            None => (
                egui::TextStyle::Body.resolve(ui.style()),
                egui::TextWrapMode::Wrap,
            ),
        };
        let job = text::job(&self.model.styled(&block.markup.shown()), &font);
        let show = |ui: &mut egui::Ui| {
            ui.scope(|ui| {
                if let Some(tone) = block.tone {
                    ui.visuals_mut().override_text_color =
                        Some(if tone == Tone::Good { GOOD } else { BAD });
                }
                ui.add(egui::Label::new(job).wrap_mode(wrap));
            });
        };
        // A banner is the screen's heading and keeps its height; any other text scrolls past a window's.
        if block.role == Some(TextRole::Banner) {
            show(ui);
        } else {
            egui::ScrollArea::vertical()
                .max_height(ui.available_height())
                .auto_shrink(true)
                .show(ui, show);
        }
    }

    /// The engine's own window, a row of colored cells at a time, in a fixed-width font as large as
    /// lets the whole grid fit the window, so every cell keeps its column and row at any size.
    fn canvas(&self, ui: &mut egui::Ui, canvas: &CanvasComponent) {
        // The room is what the dialog offers its list; it does not depend on the dialog's own size,
        // which follows the canvas.
        // A row's height rounds up to a whole pixel, which a row of slack each pays for.
        let room = egui::vec2(
            ui.ctx().content_rect().width() * 0.9,
            ui.available_height() - canvas.rows.len() as f32,
        );
        let unit = 100.0;
        let cell = ui
            .painter()
            .layout_no_wrap(
                "0".into(),
                egui::FontId::monospace(unit),
                egui::Color32::WHITE,
            )
            .size()
            / unit;
        let fit = (room.x / (canvas.cols.max(1) as f32 * cell.x))
            .min(room.y / (canvas.rows.len().max(1) as f32 * cell.y));
        let font = egui::FontId::monospace(fit.clamp(10.0, 40.0));
        let colors = self.model.colors();
        let paint = |index: u8| {
            let c = colors.index(index);
            egui::Color32::from_rgb(c.0, c.1, c.2)
        };
        ui.spacing_mut().item_spacing.y = 0.0;
        for row in &canvas.rows {
            let mut job = egui::text::LayoutJob::default();
            for (text, fg, bg) in row {
                job.append(
                    text,
                    0.0,
                    egui::TextFormat {
                        color: paint(*fg),
                        background: if *bg == 0 {
                            egui::Color32::TRANSPARENT
                        } else {
                            paint(*bg)
                        },
                        font_id: font.clone(),
                        ..Default::default()
                    },
                );
            }
            // An empty row keeps its height.
            if row.is_empty() {
                job.append(
                    " ",
                    0.0,
                    egui::TextFormat::simple(font.clone(), egui::Color32::BLACK),
                );
            }
            ui.add(egui::Label::new(job).wrap_mode(egui::TextWrapMode::Extend));
        }
        ui.spacing_mut().item_spacing.y = 4.0;
    }

    fn form(&self, ui: &mut egui::Ui, screen: &Screen, form: &Form, out: &mut Vec<Intent>) {
        for field in &form.fields {
            let label = markup::plain(&field.label.shown());
            if let Some(description) = &field.description {
                ui.weak(markup::plain(&description.shown()));
            }
            let fill = interaction::form_fill(screen, form, &field.id);
            let on = fill.is_some_and(|a| a.enabled);
            let id = format!("{}/{}", form.head.id, field.id);
            // A prompt the engine confirms with Return takes each change as it is typed, so the screen can
            // follow it (a list filter).
            let submit = form
                .submit
                .as_deref()
                .and_then(|submit| find(screen, submit))
                .filter(|a| a.role == Some(AffordanceRole::Confirm));
            if let (FieldKind::Text, Some(fill), Some(submit)) = (field.kind, fill, submit) {
                let typed = editor::typing_prompt(
                    ui,
                    &screen.screen_id,
                    &id,
                    &label,
                    &field.value,
                    self.model.busy(),
                );
                let (affordance, value) = match (typed.submit, typed.changed) {
                    (Some(value), _) => (submit, value),
                    (None, Some(value)) => (fill, value),
                    _ => continue,
                };
                out.push(Intent::Fill {
                    affordance: affordance.id.clone(),
                    form: form.head.id.clone(),
                    field: field.id.clone(),
                    value,
                });
                continue;
            }
            let committed = match field.kind {
                FieldKind::Key => {
                    ui.label(&label);
                    on.then(|| editor::key_prompt(ui)).flatten()
                }
                _ => editor::value_editor(ui, &id, &label, &editor_of(field), on),
            };
            if let (Some(value), Some(a)) = (committed, fill) {
                out.push(Intent::Fill {
                    affordance: a.id.clone(),
                    form: form.head.id.clone(),
                    field: field.id.clone(),
                    value,
                });
            }
        }
    }

    fn grid(&self, ui: &mut egui::Ui, grid: &Grid, out: &mut Vec<Intent>) {
        let size = egui::vec2(GRID_CELL, GRID_CELL);
        ui.scope(|ui| {
            ui.spacing_mut().item_spacing = egui::Vec2::ZERO;
            for y in 0..grid.rows {
                ui.horizontal(|ui| {
                    for x in 0..grid.cols {
                        let Some(cell) = grid.cells.iter().find(|c| (c.x, c.y) == (x, y)) else {
                            ui.allocate_space(size);
                            continue;
                        };
                        let glyph = cell.glyph.clone().unwrap_or_else(|| " ".into());
                        let name = cell.label.as_ref().map_or(glyph.clone(), Text::shown);
                        let color = cell.color.as_deref().and_then(|c| self.tint(c));
                        let at_cursor = grid
                            .cursor
                            .is_some_and(|c| (c.x, c.y) == (x as i32, y as i32));
                        let actionable = cell.id.is_some() && cell.affordance.is_some();
                        let mut shown = egui::RichText::new(&glyph).monospace();
                        if let Some(color) = color {
                            shown = shown.color(color);
                        }
                        let marked = cell.selected == Some(true) || at_cursor;
                        if !actionable {
                            let shown = match marked {
                                true => shown.background_color(super::menu::BAR),
                                false => shown,
                            };
                            ui.add_sized(size, egui::Label::new(shown));
                            continue;
                        }
                        let button = ui.add_enabled(
                            cell.enabled,
                            egui::Button::new(shown).min_size(size).selected(marked),
                        );
                        button.widget_info(|| {
                            WidgetInfo::labeled(
                                WidgetType::Button,
                                button.enabled(),
                                markup::plain(&name),
                            )
                        });
                        let button = match &cell.denial {
                            Some(why) if !cell.enabled => {
                                button.on_disabled_hover_text(why.shown())
                            }
                            _ => button,
                        };
                        if let (true, Some(id), Some(a)) =
                            (button.clicked(), &cell.id, &cell.affordance)
                        {
                            out.push(Intent::Focus {
                                component: grid.head.id.clone(),
                                row: id.clone(),
                            });
                            out.push(Intent::Run(a.clone()));
                        }
                    }
                });
            }
        });
    }

    /// The overmap squares around the cursor, cut to the room the window has: hovering names a square, a
    /// click moves the cursor there, and a click on the cursor runs the map's other affordance (travel). The
    /// engine's lines for the cursor square and its notes stand beside them.
    fn overmap(
        &self,
        ui: &mut egui::Ui,
        screen: &Screen,
        map: &MapComponent,
        om: &OvermapWire,
        out: &mut Vec<Intent>,
    ) {
        let cursor = map.cursor().unwrap_or_else(|| om.origin.clone());
        let ids = &map.head.affordances;
        let by_args = |pos: bool| {
            ids.iter()
                .filter_map(|id| find(screen, id))
                .find(|a| a.args.as_ref().is_some_and(|args| args.pos.is_some()) == pos)
        };
        let (mover, runner) = (by_args(true), by_args(false));
        // What the engine says of the cursor square, a line each.
        let status = om.status();
        for line in &status {
            let label = ui.label(*line);
            ui.ctx()
                .accesskit_node_builder(label.id, |b| b.set_live(egui::accesskit::Live::Polite));
        }
        let status = status.join(" ");

        let cell_px = self.display.cell(super::CELL);
        let font = egui::FontId::monospace(cell_px - 2.0);
        // The squares that fit, kept around the cursor.
        let room = ui.available_size();
        let fit = |cells: usize, space: f32| ((space / cell_px) as usize).clamp(1, cells.max(1));
        let (cols, rows) = (
            fit(om.cols, room.x * 0.7),
            fit(om.rows, ui.available_height()),
        );
        let (cursor_col, cursor_row) = om.cell_of(&cursor).unwrap_or((om.cols / 2, om.rows / 2));
        let first = |at: usize, shown: usize, all: usize| {
            at.saturating_sub(shown / 2).min(all.saturating_sub(shown))
        };
        let (left, top) = (
            first(cursor_col, cols, om.cols),
            first(cursor_row, rows, om.rows),
        );
        let cols = cols.min(om.cols.saturating_sub(left));
        let rows = rows.min(om.rows.saturating_sub(top));
        let colors = self.model.colors();
        let square_status = |at: &WorldPos| {
            if *at == cursor {
                return status.clone();
            }
            [
                Some(format!("({}, {}, {})", at.x, at.y, at.z)),
                om.note(at).map(str::to_owned),
            ]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(" ")
        };
        ui.horizontal_top(|ui| {
            let (response, painter) = ui.allocate_painter(
                egui::vec2(cols as f32 * cell_px, rows as f32 * cell_px),
                egui::Sense::click(),
            );
            let title = screen
                .title
                .as_ref()
                .map(|t| markup::plain(&t.shown()))
                .unwrap_or_default();
            response.widget_info(|| WidgetInfo::labeled(WidgetType::Other, true, &title));
            ui.ctx().accesskit_node_builder(response.id, |b| {
                b.set_role(egui::accesskit::Role::Canvas);
                let notes = (!om.notes.is_empty())
                    .then(|| [om.notes_title.clone()].into_iter().chain(om.note_lines()))
                    .into_iter()
                    .flatten();
                b.set_description(
                    [status.clone()]
                        .into_iter()
                        .chain(notes)
                        .filter(|t| !t.is_empty())
                        .collect::<Vec<_>>()
                        .join(" "),
                );
            });
            let rect_of = |col: usize, row: usize| {
                egui::Rect::from_min_size(
                    response.rect.min
                        + egui::vec2((col - left) as f32, (row - top) as f32) * cell_px,
                    egui::Vec2::splat(cell_px),
                )
            };
            painter.rect_filled(response.rect, 0.0, egui::Color32::BLACK);
            let mut paints = self.screens.overmap.borrow_mut();
            for row in top..top + rows {
                for col in left..left + cols {
                    let Some((glyph, fg, bg)) = om.cells.get(row).and_then(|r| r.get(col)) else {
                        continue;
                    };
                    let at = om.square(col, row);
                    let bg = colors.index(*bg);
                    paints.push(OvermapPaint::Square {
                        painter: painter.clone(),
                        rect: rect_of(col, row),
                        tile: om.tile(col, row).map(|t| OwnedTile {
                            id: t.id.into(),
                            rotation: t.rotation,
                            subtile: t.subtile.into(),
                            fallbacks: t.fallbacks.to_vec(),
                        }),
                        glyph: glyph.clone(),
                        color: colors.index(*fg),
                        bg: (bg != Rgb(0, 0, 0)).then_some(bg),
                        overlay: at == om.player || om.note(&at).is_some() || om.path.contains(&at),
                        font: font.clone(),
                    });
                }
            }
            for (i, step) in om.path.iter().enumerate() {
                let Some((col, row)) = om.cell_of(step).filter(|(c, r)| {
                    (left..left + cols).contains(c) && (top..top + rows).contains(r)
                }) else {
                    continue;
                };
                let alpha = if i + 1 == om.path.len() { 140 } else { 70 };
                paints.push(OvermapPaint::Fill {
                    painter: painter.clone(),
                    rect: rect_of(col, row),
                    color: egui::Color32::from_rgba_unmultiplied(0, 200, 255, alpha),
                });
            }
            for label in &om.labels {
                let Some((col, row)) = om.cell_of(&label.pos).filter(|(c, r)| {
                    (left..left + cols).contains(c) && (top..top + rows).contains(r)
                }) else {
                    continue;
                };
                paints.push(OvermapPaint::Text {
                    painter: painter.clone(),
                    rect: rect_of(col, row),
                    text: label.text.clone(),
                    color: colors.index(label.foreground),
                    font: font.clone(),
                });
            }
            // The mission's target, marked at the edge of the squares shown when it lies beyond them.
            if let Some(target) = &om.mission_target {
                let (dx, dy) = (target.x - om.origin.x, target.y - om.origin.y);
                let inside = (left as i32..(left + cols) as i32).contains(&dx)
                    && (top as i32..(top + rows) as i32).contains(&dy);
                if !inside {
                    let col = dx.clamp(left as i32, (left + cols) as i32 - 1) as usize;
                    let row = dy.clamp(top as i32, (top + rows) as i32 - 1) as usize;
                    let (toward_x, toward_y) = (
                        (target.x - cursor.x).signum(),
                        (target.y - cursor.y).signum(),
                    );
                    let marker = match (toward_x, toward_y) {
                        (0, -1) => "^",
                        (1, -1) | (-1, 1) => "/",
                        (1, 0) => ">",
                        (1, 1) | (-1, -1) => "\\",
                        (0, 1) => "v",
                        (-1, 0) => "<",
                        _ => " ",
                    };
                    paints.push(OvermapPaint::Text {
                        painter: painter.clone(),
                        rect: rect_of(col, row),
                        text: marker.into(),
                        color: colors.index(1),
                        font: font.clone(),
                    });
                }
            }
            paints.push(OvermapPaint::Outline {
                painter: painter.clone(),
                rect: rect_of(cursor_col, cursor_row),
            });
            drop(paints);
            let square_at = |p: egui::Pos2| {
                let v = (p - response.rect.min) / cell_px;
                om.square(
                    left + (v.x as usize).min(cols - 1),
                    top + (v.y as usize).min(rows - 1),
                )
            };
            response.clone().on_hover_ui_at_pointer(|ui| {
                if let Some(p) = response.hover_pos() {
                    ui.add(
                        egui::Label::new(square_status(&square_at(p)))
                            .wrap_mode(egui::TextWrapMode::Extend),
                    );
                }
            });
            if let Some(p) = response
                .interact_pointer_pos()
                .filter(|_| response.clicked())
            {
                let at = square_at(p);
                match (at == cursor, &runner, &mover) {
                    (true, Some(run), _) => out.push(Intent::Run(run.id.clone())),
                    (_, _, Some(mover)) => out.push(Intent::Move {
                        affordance: mover.id.clone(),
                        at,
                    }),
                    _ => {}
                }
                ui.ctx().request_repaint();
            }
            ui.vertical(|ui| {
                for line in &om.legend {
                    ui.add(
                        egui::Label::new(egui::RichText::new(line).monospace())
                            .wrap_mode(egui::TextWrapMode::Extend),
                    );
                }
                if !om.notes.is_empty() {
                    ui.separator();
                    ui.heading(&om.notes_title);
                }
                for note in &om.notes {
                    let link = ui.link(format!("({}, {}) {}", note.pos.x, note.pos.y, note.text));
                    if let (true, Some(mover)) = (link.clicked(), &mover) {
                        out.push(Intent::Move {
                            affordance: mover.id.clone(),
                            at: note.pos.clone(),
                        });
                    }
                }
            });
        });
    }

    /// Paints what the overmap collected, now that the window is drawn.
    fn paint_overmap(&mut self, ctx: &egui::Context) {
        let paints = self.screens.overmap.take();
        if paints.is_empty() {
            return;
        }
        self.display.update(ctx);
        self.display.begin();
        for paint in paints {
            match paint {
                OvermapPaint::Square {
                    painter,
                    rect,
                    tile,
                    glyph,
                    color,
                    bg,
                    overlay,
                    font,
                } => {
                    // With a tileset the terrain is its sprite; what lies over it stays a glyph.
                    let sprite = tile.is_some_and(|t| {
                        self.display.paint_overmap(
                            &painter,
                            rect,
                            &OvermapTile {
                                id: &t.id,
                                rotation: t.rotation,
                                subtile: &t.subtile,
                                fallbacks: &t.fallbacks,
                            },
                        )
                    });
                    if sprite && !overlay {
                        continue;
                    }
                    if let Some(bg) = bg.filter(|_| !sprite) {
                        painter.rect_filled(rect, 0.0, super::rgb(bg));
                    }
                    if !glyph.trim().is_empty() {
                        painter.text(
                            rect.center(),
                            egui::Align2::CENTER_CENTER,
                            glyph,
                            font,
                            super::rgb(color),
                        );
                    }
                }
                OvermapPaint::Fill {
                    painter,
                    rect,
                    color,
                } => {
                    painter.rect_filled(rect, 0.0, color);
                }
                OvermapPaint::Text {
                    painter,
                    rect,
                    text,
                    color,
                    font,
                } => {
                    painter.text(
                        rect.center(),
                        egui::Align2::CENTER_CENTER,
                        text,
                        font,
                        super::rgb(color),
                    );
                }
                OvermapPaint::Outline { painter, rect } => {
                    painter.rect_stroke(
                        rect,
                        0.0,
                        egui::Stroke::new(1.0, egui::Color32::WHITE),
                        egui::StrokeKind::Inside,
                    );
                }
            }
        }
    }

    /// The map is the map view's; here it is the place it would take, or the words the engine says of the
    /// cursor's square when the screen puts a cursor on it.
    fn map_slot(
        &self,
        ui: &mut egui::Ui,
        screen: &Screen,
        map: &MapComponent,
        out: &mut Vec<Intent>,
    ) {
        if let (MapMode::Overmap, Some(om)) = (map.mode, &map.overmap) {
            return self.overmap(ui, screen, map, om, out);
        }
        let lines = map.cursor_lines();
        if !lines.is_empty() {
            let font = egui::TextStyle::Body.resolve(ui.style());
            let job = text::job(&self.model.styled(&lines.join("\n")), &font);
            ui.add(egui::Label::new(job).wrap_mode(egui::TextWrapMode::Wrap));
            return;
        }
        let name = match (&map.head.label, map.mode) {
            (Some(label), _) => markup::plain(&label.shown()),
            (None, MapMode::World) => "Map".into(),
            (None, MapMode::Overmap) => screen
                .title
                .as_ref()
                .map(|t| markup::plain(&t.shown()))
                .unwrap_or_default(),
        };
        let size = egui::vec2(ui.available_width().min(480.0), 120.0);
        let (rect, response) = ui.allocate_exact_size(size, egui::Sense::hover());
        let color = ui.visuals().weak_text_color();
        ui.painter().rect_stroke(
            rect,
            0,
            egui::Stroke::new(1.0, color),
            egui::StrokeKind::Inside,
        );
        ui.painter().text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            &name,
            egui::TextStyle::Body.resolve(ui.style()),
            color,
        );
        response.widget_info(|| WidgetInfo::labeled(WidgetType::Label, true, &name));
    }

    /// The row's text with its marks: the picking mark, the engine's mark, the key, the name in its colour. The
    /// item's glyph is left out when its square beside the row draws it.
    fn row_markup(list: &List, row: &Row, picked: bool, square: bool) -> String {
        let look = row.look.as_ref();
        let parts = [
            row.parent_id.is_some().then(|| "  ".to_string()),
            matches!(list.selection, Selection::Multi | Selection::Count)
                .then(|| if picked { "[x]" } else { "[ ]" }.to_string()),
            row.toggled
                .map(|on| if on { "[x]" } else { "[ ]" }.to_string()),
            row.mark
                .as_ref()
                .map(|m| tagged(m.color.as_deref(), &m.glyph)),
            look.filter(|_| !square)
                .map(|l| tagged(Some(&l.color), &l.glyph)),
            row.favorite.filter(|&f| f).map(|_| "*".to_string()),
            row.hotkey
                .as_ref()
                .map(|k| tagged(row.key_color.as_deref(), k)),
            Some(tagged(
                row.color.as_deref(),
                &row.markup.clone().unwrap_or_else(|| row.label.shown()),
            )),
        ];
        parts.into_iter().flatten().collect::<Vec<_>>().join(" ")
    }

    fn list(&self, ui: &mut egui::Ui, screen: &Screen, list: &List, out: &mut Vec<Intent>) {
        let id = &list.head.id;
        let interaction = &self.screens.interaction;
        let rows = self.model.rows(list);
        let focus = interaction.focus();
        let cursor = focus
            .row_id
            .as_deref()
            .filter(|_| focus.component_id.as_deref() == Some(id));
        let spacing = ui.spacing().item_spacing.x;
        // The rows sit in a scroll area, which keeps room for its bar.
        let width = ui.available_width() - ui.spacing().scroll.allocated_width();
        let height = ui.spacing().interact_size.y;
        // The columns take what their content needs; the label keeps the rest, less the item's square.
        let widths = self.list_widths(ui, list, &rows);
        let squares = rows
            .iter()
            .any(|r| r.look.is_some() && self.display.tiles());
        let icon = if squares { height + spacing } else { 0.0 };
        let columns_w = widths.columns.iter().map(|w| w + spacing).sum::<f32>();
        let label_w = (width - columns_w - icon).max(0.0);
        if !list.columns.is_empty() {
            ui.horizontal(|ui| {
                ui.allocate_space(egui::vec2(label_w + icon, height));
                for (column, w) in list.columns.iter().zip(&widths.columns) {
                    let caption = markup::plain(&column.label.shown());
                    ui.add_sized(
                        [*w, height],
                        egui::Label::new(egui::RichText::new(caption).weak()).truncate(),
                    );
                }
            });
        }
        let list_height = Self::list_height(ui);
        self.screens.list_height.set(list_height);
        let mut area = egui::ScrollArea::vertical()
            .id_salt(("list", id))
            .auto_shrink([false, true])
            .max_height(list_height);
        if self.screens.reveal.as_deref() == Some(id.as_str()) {
            let step = height + ui.spacing().item_spacing.y;
            area = area.vertical_scroll_offset(interaction.scroll(id) as f32 * step);
        }
        area.show_rows(ui, height, rows.len(), |ui, range| {
            for row in &rows[range.clone()] {
                ui.horizontal(|ui| {
                    if row.header == Some(true) {
                        let heading = markup::plain(&row.label.shown());
                        ui.label(egui::RichText::new(heading).strong());
                        return;
                    }
                    let pick = interaction.picks(id).iter().find(|p| p.id == row.id);
                    let on = row.enabled && row.selectable;
                    let cell = row
                        .look
                        .as_ref()
                        .filter(|_| self.display.tiles())
                        .and_then(|look| self.model.look_cell(look));
                    if let Some(cell) = &cell {
                        let size = egui::Vec2::splat(height);
                        let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
                        self.screens.icons.borrow_mut().push(RowIcon {
                            painter: ui.painter_at(rect),
                            rect,
                            cell: cell.clone(),
                        });
                    }
                    let marked = Self::row_markup(list, row, pick.is_some(), cell.is_some());
                    // A refused row is plain dim text: the game's colours would hide that it is.
                    let text = if row.enabled {
                        self.line(ui, &marked)
                    } else {
                        egui::RichText::new(markup::plain(&marked))
                            .color(ui.visuals().weak_text_color())
                            .into()
                    };
                    let highlighted =
                        cursor == Some(row.id.as_str()) || (cursor.is_none() && row.selected);
                    let layout = egui::Layout::top_down_justified(egui::Align::Min);
                    let button = ui
                        .allocate_ui_with_layout(egui::vec2(label_w, height), layout, |ui| {
                            self.control(ui, text, &row.id, on, highlighted)
                        })
                        .inner;
                    button.widget_info(|| {
                        WidgetInfo::selected(
                            WidgetType::SelectableLabel,
                            button.enabled(),
                            highlighted,
                            markup::plain(&row.label.shown()),
                        )
                    });
                    let denial = row.denial.shown();
                    let button = match (on, denial.is_empty()) {
                        (false, false) => button.on_disabled_hover_text(denial),
                        _ => button,
                    };
                    if button.clicked() {
                        out.push(Intent::Focus {
                            component: id.clone(),
                            row: row.id.clone(),
                        });
                        match (list.selection, &list.activate) {
                            (Selection::Multi | Selection::Count, _) => out.push(Intent::Mark {
                                component: id.clone(),
                                row: row.id.clone(),
                            }),
                            (_, Some(activate)) => out.push(Intent::Run(activate.clone())),
                            _ => {}
                        }
                    }
                    if let (Selection::Count, Some(pick), Some(max)) =
                        (list.selection, pick, row.max)
                    {
                        let mut count = pick.count.unwrap_or(max);
                        let drag = ui.add(egui::DragValue::new(&mut count).range(0..=max));
                        if drag.changed() {
                            out.push(Intent::MarkCount {
                                component: id.clone(),
                                row: row.id.clone(),
                                count,
                            });
                        }
                    }
                    for (column, column_w) in list.columns.iter().zip(&widths.columns) {
                        let value = row
                            .columns
                            .iter()
                            .find(|c| c.key.as_deref() == Some(column.key.as_str()))
                            .map(|c| tagged(c.color.as_deref(), &c.value.shown()))
                            .unwrap_or_default();
                        ui.add_sized(
                            [*column_w, height],
                            egui::Label::new(self.line(ui, &value)).truncate(),
                        );
                    }
                });
            }
            if range.end + PREFETCH >= rows.len() {
                out.push(Intent::More(id.clone()));
            }
        });
        let shown = cursor.and_then(|c| rows.iter().find(|r| r.id == c));
        if let Some(row) = shown {
            let description = row.description.shown();
            if !description.is_empty() {
                egui::ScrollArea::vertical()
                    .id_salt(("description", id))
                    .max_height(DESCRIPTION_HEIGHT)
                    .show(ui, |ui| ui.label(self.rich(ui, &description)));
            }
            if !row.enabled && !row.denial.shown().is_empty() {
                ui.colored_label(BAD, markup::plain(&row.denial.shown()));
            }
        }
        let own = list
            .head
            .affordances
            .iter()
            .filter_map(|a| find(screen, a))
            .collect::<Vec<_>>();
        self.affordance_buttons(ui, &own, shown.copied(), out);
    }

    /// The count, text or pane the affordance being started still asks the player for.
    fn question(&self, ui: &mut egui::Ui, screen: &Screen, out: &mut Vec<Intent>) {
        let Some(ask) = self.screens.interaction.ask() else {
            return;
        };
        let Some(aff) = find(screen, &ask.affordance_id) else {
            return;
        };
        ui.group(|ui| {
            ui.label(egui::RichText::new(markup::plain(&aff.label.shown())).strong());
            if let (Some(count), Some(ArgKind::Count)) = (ask.count, ask.asking) {
                ui.label(egui::RichText::new(count.to_string()).monospace());
            }
            if let Some(select) = &ask.select {
                ui.horizontal_wrapped(|ui| {
                    for option in &select.options {
                        let label = markup::plain(&option.label.shown());
                        if ui.button(label).clicked() {
                            out.push(Intent::Text(option.id.clone()));
                            out.push(Intent::Confirm);
                        }
                    }
                });
            } else if ask.capture {
                ui.label("Press the new key");
            } else if let (Some(typed), Some(ArgKind::Text)) = (&ask.text, ask.asking) {
                let mut shown = typed.clone();
                let edit = ui.text_edit_singleline(&mut shown);
                if edit.changed() {
                    out.push(Intent::Text(shown));
                }
                if edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    out.push(Intent::Confirm);
                }
            }
            ui.horizontal_wrapped(|ui| {
                for pane in &ask.panes {
                    let found = screen.root.as_ref().and_then(|root| root.find(pane));
                    if let Some(Component::Panel(p)) = found
                        && ui.button(markup::plain(&p.title.shown())).clicked()
                    {
                        out.push(Intent::Pane(p.head.id.clone()));
                    }
                }
            });
            ui.horizontal(|ui| {
                if ui.button("OK").clicked() {
                    out.push(Intent::Confirm);
                }
                if ui.button("Cancel").clicked() {
                    out.push(Intent::Cancel);
                }
            });
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(toggled: Option<bool>) -> Row {
        serde_json::from_value(serde_json::json!({
            "id": "r", "label": "Lighting", "description": "", "denial": "", "enabled": true,
            "selectable": true, "selected": false, "columns": [], "toggled": toggled
        }))
        .unwrap()
    }

    fn list() -> List {
        serde_json::from_value(serde_json::json!({
            "id": "l", "kind": "list", "columns": [], "rows": [], "total": 0, "selection": "single"
        }))
        .unwrap()
    }

    #[test]
    fn a_switch_row_shows_whether_it_is_on_and_a_plain_row_shows_nothing() {
        let text = |toggled| {
            App::<bn_client_core::EngineModel>::row_markup(&list(), &row(toggled), false, false)
        };
        assert_eq!(text(Some(true)), "[x] Lighting");
        assert_eq!(text(Some(false)), "[ ] Lighting");
        assert_eq!(text(None), "Lighting");
    }
}
