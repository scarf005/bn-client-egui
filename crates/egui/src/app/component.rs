//! The top screen of the engine's stack drawn from its components (protocol 0.2): lists, tabs, splits,
//! panels, forms, text, grids, the map and canvases, laid out the same way for every screen.
//!
//! Drawing reads the engine's data and collects what the player did as `Intent`s. Applying an intent only
//! calls the interaction core (`bn_client_core::interaction`), which decides what is sent.

mod detail;
mod footer;
mod form;
mod list;
mod measure;
mod overmap;
mod panel;
mod placement;
mod tabs;

use super::{App, sidebar};
use crate::kit::section::{self, Fact};
use crate::kit::split::{self, Extent};
use crate::kit::tokens::{self, BAD, GOOD};
use crate::windows::{self, Place, Shown, WindowSpec};
use crate::{a11y, editor, text};
use bn_client_core::interaction::{self, ArgKind, InvokeError};
use bn_client_core::protocol::{
    Affordance, AffordanceRole, CanvasComponent, Component, Editor, EditorValue, FieldKind, Form,
    FormField, Grid, GridRole, InvokeArgs, List, MapComponent, MapMode, OvermapTile, OvermapWire,
    PanelStat, Relation, Row, Screen, ScreenDock, ScreenRole, Selection, SidebarSide, Split, Text,
    TextBlock, TextRole, Tone, WorldPos,
};
use bn_client_core::{Cell, GameModel, Pos, Rgb, markup};
use eframe::egui::{self, WidgetInfo, WidgetType};
use footer::*;
use form::*;
use list::*;
pub(super) use measure::text_width;
use measure::*;
use overmap::*;
pub(super) use panel::PANELS;
use placement::*;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

/// The margin around each pane of a split, on every side.
const PANE_MARGIN: i8 = 2;
/// The most rows a list in a peer pane shows before it scrolls.
const PEER_ROWS: usize = 12;
/// Side of a grid cell in points.
const GRID_CELL: f32 = 26.0;
/// The largest a grid's cell grows to when the room allows.
const GRID_MAX_CELL: f32 = 2.0 * GRID_CELL;
/// The least and the most the zoom gesture scales a canvas grid's cells from the size that fills its room.
const GRID_ZOOM: (f32, f32) = (0.5, 4.0);

/// The client-local state of the top screen: what the interaction core keeps, and what the player is asked.
#[derive(Default)]
pub(super) struct Screens {
    interaction: bn_client_core::interaction::Interaction,
    /// Why the last refused affordance is off, in the engine's words.
    denial: Option<String>,
    /// How far each list is scrolled and the focus it last scrolled into sight, by screen and component id: a
    /// panel window and the engine's own screen may hold lists of the same id.
    list_scroll: RefCell<HashMap<(String, String), ListScroll>>,
    /// The rows each list had room for in the last frame, by screen and component id: what a page key moves in it.
    list_pages: RefCell<HashMap<(String, String), usize>>,
    /// The squares of this frame's list rows left for their item's sprite, painted once the window is drawn.
    icons: RefCell<Vec<RowIcon>>,
    /// What this frame's overmap draws, painted once the window is drawn, so that a tileset's sprites come
    /// before the marks over them.
    overmap: RefCell<Vec<OvermapPaint>>,
    /// A panel window is being drawn: its screen takes no command, so its tabs and rows only move its focus.
    pub(super) passive: bool,
    /// The row to choose once the filter prompt that showed it has confirmed its text.
    reach: Option<String>,
    /// The most rows the lists drawn now show before they scroll, when fewer than their room holds.
    list_rows: std::cell::Cell<Option<usize>>,
}

/// A list's scroll offset in the last frame, and the row of the focus it last brought into sight.
#[derive(Default)]
struct ListScroll {
    top: f32,
    shown: Option<String>,
}

fn tone_color(tone: Tone) -> egui::Color32 {
    if tone == Tone::Good { GOOD } else { BAD }
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
    /// A row of a list that runs nothing was double-clicked.
    Open,
    /// A result row of a list filter's prompt was clicked: confirm the text, then choose the row.
    Reach(String),
    Text(String),
    Pane(String),
    Confirm,
    Cancel,
}

fn find<'a>(screen: &'a Screen, id: &str) -> Option<&'a Affordance> {
    screen.affordances.iter().find(|a| a.id == id)
}

/// Whether `c` is running text: a text block, or a panel holding only one.
fn prose(c: &Component) -> bool {
    match c {
        Component::Text(t) => t.role.is_none(),
        Component::Panel(p) => {
            p.stats.as_ref().is_none_or(Vec::is_empty) && p.child.as_deref().is_some_and(prose)
        }
        _ => false,
    }
}

/// Whether `c` heads its split above the parts: the screen's banner, or the question the parts answer.
fn heading(c: &Component) -> bool {
    matches!(c, Component::Text(t) if matches!(t.role, Some(TextRole::Banner | TextRole::Prompt)))
}

/// Whether the rows of one of the screen's lists run the affordance, on a double click or Enter.
fn run_by_rows(screen: &Screen, affordance: &Affordance) -> bool {
    let lists = screen.root.iter().flat_map(Component::nested);
    lists
        .filter_map(|c| match c {
            Component::List(l) => l.activate.as_deref(),
            _ => None,
        })
        .any(|id| id == affordance.id)
}

/// The ids of the affordances some component names.
fn claimed(root: &Component) -> HashSet<&str> {
    root.nested()
        .into_iter()
        .flat_map(|c| {
            let extra = match c {
                Component::Tabs(t) => t.select.as_deref(),
                Component::Form(f) => f.submit.as_deref(),
                // Its rows run it on a double click or Enter, so it is no button.
                Component::List(l) => l.activate.as_deref(),
                _ => None,
            };
            // A row that does what an affordance does offers it, so it is no button either.
            let rows = match c {
                Component::List(l) => l
                    .rows
                    .iter()
                    .filter_map(|r| r.affordance.as_deref())
                    .collect(),
                _ => vec![],
            };
            c.head()
                .affordances
                .iter()
                .map(String::as_str)
                .chain(extra)
                .chain(rows)
        })
        .collect()
}

/// Whether a component's own widgets run the affordance, so it needs no button; a click on the map moves
/// its cursor.
fn driven(component: &Component, affordance: &Affordance) -> bool {
    match component {
        Component::Map(_) => affordance.args.as_ref().is_some_and(|a| a.pos.is_some()),
        Component::Tabs(t) => t.select.as_deref() == Some(affordance.id.as_str()),
        Component::Form(_) => affordance.args.as_ref().is_some_and(|a| a.text.is_some()),
        Component::Grid(g) => g
            .cells
            .iter()
            .any(|cell| cell.affordance.as_deref() == Some(affordance.id.as_str())),
        _ => false,
    }
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
        self.reach_row();
    }

    /// Chooses the row a filter's result click named once the prompt is gone and the list has it; a list that
    /// lacks it, a screen that is not a list, or one that is not described ends the wait.
    fn reach_row(&mut self) {
        let Some(row) = self.screens.reach.clone() else {
            return;
        };
        let Some(top) = interaction::described(self.model.screens()) else {
            self.screens.reach = None;
            return;
        };
        if form::filter_submit(top).is_some() {
            return;
        }
        self.screens.reach = None;
        let found = top.components().into_iter().find_map(|c| match c {
            Component::List(list) if self.model.rows(list).iter().any(|r| r.id == row) => {
                Some((list.head.id.clone(), list.activate.clone()))
            }
            _ => None,
        });
        if let Some((component, Some(activate))) = found {
            self.screens.interaction.focus_row(&component, &row);
            let out = self.screens.interaction.activate(top, &activate);
            self.settle(out);
        }
    }

    /// What the window of `screen` is remembered as: a list filter's prompt is the window of the screen it filters,
    /// so typing in it neither moves nor resizes that window.
    fn window_kind(&self, screen: &Screen) -> Option<String> {
        let stack = self.model.screens();
        let below = stack.len().checked_sub(2).and_then(|at| stack.get(at));
        match (form::filter_submit(screen), below) {
            (Some(_), Some(below)) => super::screen_kind(below),
            _ => super::screen_kind(screen),
        }
    }

    /// The screen on top drawn from its components when the engine describes it so, else the 0.1 menu.
    pub(super) fn menu_or_screen(&mut self, ctx: &egui::Context) {
        self.sync_screens();
        let Some(top) = interaction::described(self.model.screens()) else {
            return self.menu(ctx);
        };
        let mut out = vec![];
        let cancel = interaction::with_role(top, AffordanceRole::Cancel).map(|a| a.id.clone());
        let kind = self.window_kind(top);
        // Only the lists drawn this frame have a page.
        self.screens.list_pages.borrow_mut().clear();
        let shown = self.screen_window(ctx, top, &mut out);
        if self.keep_window(ctx, kind.as_deref(), &shown)
            && let Some(cancel) = cancel
        {
            out.push(Intent::Run(cancel));
        }
        self.paint_icons();
        self.paint_overmap(ctx);
        out.into_iter().for_each(|intent| self.apply(intent));
    }

    /// Paints the items of the list rows drawn, now that their window is drawn.
    fn paint_icons(&mut self) {
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
    }

    /// The keys pressed this frame go to the interaction core, which decides what each one does.
    pub(super) fn component_keys(&mut self, ctx: &egui::Context) {
        self.keys_consumed = false;
        self.sync_screens();
        let pages = self.screens.list_pages.borrow().clone();
        let keys = super::input::frame_keys(ctx).into_iter().map(|k| k.token);
        // A text box has the typing; its keys are not the screen's, except Escape, which closes the screen (a filled filter box clears first).
        let typing = ctx.text_edit_focused();
        let clears = crate::editor::clears_on_escape(ctx);
        let keys = keys
            .filter(|key| !typing || (key == "ESC" && !clears))
            .collect::<Vec<_>>();
        // A Tab the screen takes is its own: egui must not also walk its focus onto a widget. Any other Tab
        // walks egui's focus over the buttons.
        let takes = |key: &String| {
            interaction::described(self.model.screens()).is_some_and(|top| {
                top.affordances
                    .iter()
                    .any(|a| a.enabled && a.keys.contains(key))
            })
        };
        if keys
            .iter()
            .any(|k| (k == "TAB" || k == "BACKTAB") && takes(k))
        {
            ctx.memory_mut(|m| m.move_focus(egui::FocusDirection::None));
        }
        for key in keys {
            let Some(top) = interaction::described(self.model.screens()) else {
                return;
            };
            // An enabled key field takes the key as its text, and the screen does not see it; Escape still
            // closes the screen.
            if let Some(fill) = key_fill(top, &key).filter(|_| key != "ESC") {
                self.keys_consumed = true;
                return self.apply(fill);
            }
            let page = |id: &str| {
                let at = (top.screen_id.clone(), id.to_string());
                pages.get(&at).copied().unwrap_or(1)
            };
            let model = &self.model;
            let out = self
                .screens
                .interaction
                .press(top, &key, &page, &|list| model.rows(list));
            self.settle(out);
        }
    }

    /// Shows what the core decided and sends the call it built.
    fn settle(&mut self, out: interaction::Outcome) {
        self.keys_consumed |= out.consumed;
        self.screens.denial = out.note();
        if let Some((id, args)) = out.invoke
            && let Err(InvokeError::Disabled(denial)) = self.model.invoke_affordance(&id, args)
        {
            self.screens.denial = denial.as_ref().map(Text::shown);
        }
    }

    /// Hands what the player did to the interaction core.
    fn apply(&mut self, intent: Intent) {
        let interaction = &mut self.screens.interaction;
        let Some(top) = interaction::described(self.model.screens()) else {
            return;
        };
        let out = match intent {
            Intent::More(component) => return self.model.more_rows(&component),
            Intent::Focus { component, row } => return interaction.focus_row(&component, &row),
            Intent::Mark { component, row } => return interaction.toggle_pick(&component, &row),
            Intent::MarkCount {
                component,
                row,
                count,
            } => return interaction.set_pick_count(&component, &row, Some(count)),
            Intent::Text(typed) => return interaction.answer_text(&typed),
            Intent::Cancel => return interaction.cancel(),
            Intent::Open => return,
            Intent::Reach(row) => {
                let Some(confirm) = form::filter_submit(top) else {
                    return;
                };
                self.screens.reach = Some(row);
                return self.apply(confirm);
            }
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
        let avoid = self.map_view.map_or(vec![], |view| {
            let square = |at: Pos| view.square(at).expand(KEEP_CLEAR * view.cell);
            self.model
                .focus()
                .into_iter()
                .chain([self.model.player()])
                .map(square)
                .collect()
        });
        let room = Room {
            region,
            sidebar: self.sidebar,
            avoid: &avoid,
            map: !interaction::titled(self.model.screens()),
        };
        let placement = placement(screen, &room);
        let place = placement.place;
        let base = self.window_spec(ctx, self.window_kind(screen).as_deref(), &title);
        let title_bar = base.title_bar;
        let named = screen.root.as_ref().map_or_else(HashSet::new, claimed);
        let rest = screen
            .affordances
            .iter()
            .filter(|a| !named.contains(a.id.as_str()))
            .collect::<Vec<_>>();
        // The help sits in the title bar, or last in the footer of a window that has none.
        let titled = interaction::with_role(screen, AffordanceRole::Help)
            .filter(|a| a.enabled && base.title_bar);
        let spec = WindowSpec {
            closable: interaction::with_role(screen, AffordanceRole::Cancel).is_some(),
            frame: Some(frame),
            place: Some(place),
            movable: !keeps_off_cursor(screen) && placement.class != Class::Full,
            width: placement.first_width(),
            scrolls: false,
            help: titled.map(|a| windows::TitleHelp {
                label: markup::plain(&a.label.shown()),
                tip: glyph_tip(a),
            }),
            ..base
        };
        let shown = windows::show(ctx, spec, |ui| {
            let Some(root) = &screen.root else {
                return;
            };
            let footer = |ui: &mut egui::Ui, out: &mut Vec<Intent>| {
                self.question(ui, screen, out);
                if let Some(denial) = &self.screens.denial {
                    ui.colored_label(BAD, markup::plain(denial));
                }
                self.footer(ui, &rest, title_bar, out);
            };
            // A screen that fills the room keeps its footer under its body, whatever the body's height.
            if placement.class == Class::Full {
                egui::Panel::bottom(egui::Id::new(("footer", &screen.screen_id)))
                    .resizable(false)
                    .frame(egui::Frame::NONE)
                    .show_separator_line(false)
                    .show(ui, |ui| footer(ui, out));
                // A body taller than the room, a full-size grid over further parts, scrolls instead of being cut.
                egui::CentralPanel::no_frame().show(ui, |ui| {
                    let scroll = egui::ScrollArea::vertical()
                        .id_salt(("body", &screen.screen_id))
                        .auto_shrink(false);
                    let room = ui.available_height();
                    body_scroll(ui, scroll, room, |ui| match placement.column {
                        true => {
                            // A row's bar reaches half a gap past each side of its table.
                            let width = ui.available_width().min(COLUMN_WIDTH)
                                - ui.spacing().item_spacing.x;
                            ui.vertical_centered(|ui| {
                                ui.scope(|ui| {
                                    ui.set_width(width);
                                    ui.with_layout(
                                        egui::Layout::top_down(egui::Align::Min),
                                        |ui| self.component(ui, screen, root, out),
                                    );
                                })
                            });
                        }
                        false => self.component(ui, screen, root, out),
                    });
                });
            } else {
                // A body taller than the room scrolls above the footer, which keeps the height it had last frame,
                // by the window's kind.
                let kept = ui.id().with("footer height");
                let reserve = ctx.data(|d| d.get_temp::<f32>(kept)).unwrap_or(0.0);
                let scroll = egui::ScrollArea::vertical()
                    .id_salt("body")
                    .auto_shrink([false, true]);
                let room = ui.available_height() - reserve;
                body_scroll(ui, scroll, room, |ui| self.component(ui, screen, root, out));
                let top = ui.min_rect().bottom();
                footer(ui, out);
                let height = ui.min_rect().bottom() - top;
                ctx.data_mut(|d| d.insert_temp(kept, height));
            }
        });
        if let Some(a) = titled.filter(|_| shown.help) {
            out.push(Intent::Run(a.id.clone()));
        }
        shown
    }

    /// One line of the engine's text with its colour tags; what does not fit is cut.
    fn line(&self, ui: &egui::Ui, markup: &str) -> egui::WidgetText {
        self.line_on(ui, markup, tokens::ground(ui))
    }

    /// `line` drawn on `ground`.
    fn line_on(&self, ui: &egui::Ui, markup: &str, ground: egui::Color32) -> egui::WidgetText {
        let font = egui::TextStyle::Body.resolve(ui.style());
        let mut job = text::job(&self.model.styled(markup), &font, ground);
        job.wrap.max_rows = 1;
        job.wrap.break_anywhere = true;
        job.into()
    }

    /// The game colour as a colour of the screen.
    fn tint(&self, name: &str) -> egui::Color32 {
        super::rgb(self.model.colors().resolve(name).0)
    }

    fn component(&self, ui: &mut egui::Ui, screen: &Screen, c: &Component, out: &mut Vec<Intent>) {
        if let (Some(label), false) = (&c.head().label, matches!(c, Component::Panel(_))) {
            ui.label(egui::RichText::new(markup::plain(&label.shown())).strong());
        }
        match c {
            Component::List(list) => self.described_list(ui, screen, c, list, out),
            Component::Tabs(tabs) => {
                self.tab_strip(ui, screen, tabs, out);
                if let Some(child) = &tabs.child {
                    self.component(ui, screen, child, out);
                }
            }
            Component::Split(split) => {
                // A banner and a prompt head the parts, whatever their number.
                let (headings, parts) =
                    split.children.iter().partition::<Vec<_>, _>(|c| heading(c));
                for heading in headings {
                    self.component(ui, screen, heading, out);
                }
                self.split(ui, screen, split, &parts, out);
            }
            Component::Panel(panel) => {
                // A panel the screen's own title already names has none.
                let title = markup::plain(&panel.title.shown());
                section::show(ui, &title, |ui| {
                    if let Some(subtitle) = &panel.subtitle {
                        ui.label(self.line(ui, &subtitle.shown()));
                    }
                    if let Some(stats) = &panel.stats {
                        Self::facts(ui, &panel.head.id, stats);
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

    /// The panes side by side, each as wide as its content needs (`split::split_widths`), the only prose of
    /// peers as wide as the rest (`split::prose_widths`) and a list beside its detail at least two fifths of the
    /// room (`split::detail_widths`). Panes whose minimums do not fit go on to further lines
    /// (`split::wrap_lines`), a detail under its list; a line whose minimums still do not fit scrolls sideways
    /// instead of breaking words.
    /// Peers on more than one line are as tall as what they hold, a list up to `PEER_ROWS` rows, and scroll
    /// down together; one line of panes, peers around a map or a canvas, and a list beside its detail share the
    /// height they have.
    fn split(
        &self,
        ui: &mut egui::Ui,
        screen: &Screen,
        split: &Split,
        parts: &[&Component],
        out: &mut Vec<Intent>,
    ) {
        // A list filter's box is above the results it filters.
        if form::filter_submit(screen).is_some() {
            return parts
                .iter()
                .for_each(|child| self.component(ui, screen, child, out));
        }
        let gap = ui.spacing().item_spacing.x;
        let room = ui.available_width();
        let texts = (0..parts.len())
            .filter(|&i| prose(parts[i]))
            .collect::<Vec<_>>();
        let prose_at = match (split.relation, texts.as_slice()) {
            (Relation::Peers, [at]) => Some(*at),
            _ => None,
        };
        let fills = parts.iter().flat_map(|c| c.nested()).any(takes_room);
        // Beside a map or a canvas, which take the room left, the other panes keep their natural width.
        let measure = |ui: &egui::Ui, panes: &[&Component], room: f32| {
            panes
                .iter()
                .map(|c| {
                    let extent = with_margin(self.extent(ui, screen, c, room));
                    Extent {
                        grows: extent.grows && (!fills || c.nested().into_iter().any(takes_room)),
                        ..extent
                    }
                })
                .collect::<Vec<_>>()
        };
        let laid = |ui: &egui::Ui, room: f32| {
            let extents = measure(ui, parts, room);
            let lines = split::wrap_lines(&extents, room, gap);
            (extents, lines)
        };
        let (mut extents, mut lines) = laid(ui, room);
        let natural = split.relation == Relation::Peers && !fills && lines.len() > 1;
        // Peers that scroll down keep room beside them for the scroll bar.
        let room = match natural {
            true => room - ui.spacing().scroll.allocated_width(),
            false => room,
        };
        if natural {
            (extents, lines) = laid(ui, room);
        }
        let count = lines.len();
        let mut draw = |ui: &mut egui::Ui| {
            for (n, line) in lines.iter().enumerate() {
                let panes = &parts[line.clone()];
                let fit = &extents[line.clone()];
                let widths = match (split.relation, prose_at.filter(|at| line.contains(at))) {
                    (Relation::Detail, _) if !matches!(panes.first(), Some(Component::Text(_))) => {
                        split::detail_widths(fit, room, gap)
                    }
                    // Beside the prose, which takes at least an equal share, the panes are offered the rest.
                    (Relation::Peers, Some(at)) => {
                        let offered = room - room / panes.len() as f32;
                        split::prose_widths(
                            &measure(ui, panes, offered),
                            at - line.start,
                            room,
                            gap,
                        )
                    }
                    _ => split::split_widths(fit, room, gap),
                };
                // Peers as tall as they hold have no bottom; sharing the height, each line gets its share of
                // what is left, past a few rows.
                let spacing = ui.spacing();
                let floor = 4.0 * (spacing.interact_size.y + spacing.item_spacing.y);
                let height = match natural {
                    true => f32::INFINITY,
                    false => (ui.available_height() / (count - n) as f32).max(floor),
                };
                // Inside a peer, a pane that shares its height still has none to share.
                let rows = match natural {
                    true => Some(PEER_ROWS),
                    false => self.screens.list_rows.get(),
                };
                let mut line_ui = |ui: &mut egui::Ui| {
                    let mut active = None;
                    let drawn = ui.horizontal_top(|ui| {
                        for (child, width) in panes.iter().zip(&widths) {
                            let size = egui::vec2(*width, height);
                            let rect = egui::Rect::from_min_size(ui.cursor().min, size);
                            let down = egui::Layout::top_down(egui::Align::Min);
                            let mut pane =
                                ui.new_child(egui::UiBuilder::new().max_rect(rect).layout(down));
                            pane.shrink_clip_rect(egui::Rect::from_x_y_ranges(
                                rect.x_range(),
                                ui.clip_rect().y_range(),
                            ));
                            // The margin keeps room inside the pane's width for the active outline.
                            let frame = egui::Frame::NONE.inner_margin(PANE_MARGIN);
                            frame.show(&mut pane, |ui| {
                                ui.set_width(width - frame.total_margin().sum().x);
                                self.with_list_rows(rows, || self.component(ui, screen, child, out))
                            });
                            // The pane takes its width and no more, whatever its content draws: wider, it
                            // would widen the window that the next frame's room comes from, every frame.
                            let drawn = egui::vec2(*width, pane.min_rect().height());
                            let (slot, _) = ui.allocate_exact_size(drawn, egui::Sense::hover());
                            if split.active.as_deref() == Some(child.id()) && !self.empty(child) {
                                active = Some(slot.x_range());
                            }
                        }
                    });
                    // The active pane is outlined over its whole slot, as tall as its line.
                    if let Some(x) = active {
                        let rect = egui::Rect::from_x_y_ranges(x, drawn.response.rect.y_range());
                        let stroke = ui.visuals().selection.stroke;
                        ui.painter()
                            .rect_stroke(rect, 0, stroke, egui::StrokeKind::Inside);
                    }
                };
                match split::overflows(&widths, room, gap) {
                    true => {
                        egui::ScrollArea::horizontal()
                            .id_salt(("split", &split.head.id, n))
                            .auto_shrink([false, true])
                            .show(ui, line_ui);
                    }
                    false => line_ui(ui),
                }
            }
        };
        match natural {
            true => {
                egui::ScrollArea::vertical()
                    .id_salt(("split-lines", &split.head.id))
                    .auto_shrink([false, true])
                    .show(ui, draw);
            }
            false => draw(ui),
        }
    }

    /// Draws with lists showing at most `rows` rows before they scroll, or as many as their room holds.
    fn with_list_rows<R>(&self, rows: Option<usize>, draw: impl FnOnce() -> R) -> R {
        let was = self.screens.list_rows.replace(rows);
        let drawn = draw();
        self.screens.list_rows.set(was);
        drawn
    }

    /// Whether `c` shows nothing: a list without rows.
    fn empty(&self, c: &Component) -> bool {
        matches!(c, Component::List(list) if self.known_rows(list).is_empty())
    }

    /// The panel's stats as facts: label and value, or a stat without a value as its label in its tone.
    fn facts(ui: &mut egui::Ui, id: &str, stats: &[PanelStat]) {
        let facts = stats
            .iter()
            .map(|stat| {
                let shown = stat_text(stat);
                let label = egui::RichText::new(markup::plain(&stat.label.shown()));
                let color = stat.tone.map_or(ui.visuals().text_color(), tone_color);
                match shown.is_empty() {
                    true => Fact {
                        label: label.color(color),
                        value: None,
                    },
                    // The engine's text carries any punctuation.
                    false => Fact {
                        label,
                        value: Some(egui::RichText::new(shown).color(color)),
                    },
                }
            })
            .collect::<Vec<_>>();
        section::facts(ui, egui::Id::new(("facts", id)), &facts);
    }

    fn text_block(&self, ui: &mut egui::Ui, block: &TextBlock) {
        let (font, wrap) = match block.role {
            Some(TextRole::TerminalOutput | TextRole::Banner) => (
                egui::TextStyle::Monospace.resolve(ui.style()),
                egui::TextWrapMode::Extend,
            ),
            Some(TextRole::Prompt) | None => (
                egui::TextStyle::Body.resolve(ui.style()),
                egui::TextWrapMode::Wrap,
            ),
        };
        let ground = tokens::ground(ui);
        let job = text::job(&self.model.styled(&block.markup.shown()), &font, ground);
        let show = |ui: &mut egui::Ui| {
            ui.scope(|ui| {
                if let Some(tone) = block.tone {
                    ui.visuals_mut().override_text_color = Some(tone_color(tone));
                }
                ui.add(egui::Label::new(job).wrap_mode(wrap));
            });
        };
        // A banner is the screen's heading and keeps its height; any other text scrolls past a window's.
        if block.role == Some(TextRole::Banner) {
            ui.vertical_centered(show);
        } else {
            // The viewport is a whole number of lines tall, so none is cut across.
            let line = ui.fonts_mut(|f| f.row_height(&font));
            let lines = (ui.available_height() / line).floor().max(1.0);
            egui::ScrollArea::vertical()
                .max_height(lines * line)
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
        let window = tokens::ground(ui);
        let ink = |fg: u8, bg: u8| {
            let ground = if bg == 0 { window } else { paint(bg) };
            tokens::readable(colors.index(fg), ground)
        };
        // The rows touch; what follows the canvas keeps the theme's spacing.
        ui.scope(|ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            for row in &canvas.rows {
                let mut job = egui::text::LayoutJob::default();
                for (text, fg, bg) in row {
                    job.append(
                        text,
                        0.0,
                        egui::TextFormat {
                            color: ink(*fg, *bg),
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
        });
    }

    fn grid(&self, ui: &mut egui::Ui, grid: &Grid, out: &mut Vec<Intent>) {
        let (xs, ys) = drawn_span(grid);
        // The cells grow with the room the grid has, up to twice their size, their glyphs with them; a canvas's
        // fill its room, and the zoom gesture over it scales them from there.
        let fit = (ui.available_width() / xs.len().max(1) as f32)
            .min(ui.available_height() / ys.len().max(1) as f32);
        let canvas = grid.role == Some(GridRole::Canvas);
        let zoom_key = egui::Id::new(("grid zoom", &grid.head.id));
        let zoom = ui.data(|d| d.get_temp::<f32>(zoom_key)).unwrap_or(1.0);
        let cell = match canvas {
            true => fit.max(GRID_CELL) * zoom,
            false => fit.clamp(GRID_CELL, GRID_MAX_CELL),
        };
        let size = egui::vec2(cell, cell);
        let glyph_size = egui::TextStyle::Monospace.resolve(ui.style()).size * cell / GRID_CELL;
        if canvas {
            let shown = egui::ScrollArea::both()
                .id_salt(("grid", &grid.head.id))
                .auto_shrink([true, true])
                .show(ui, |ui| self.grid_cells(ui, grid, size, glyph_size, out));
            let delta = ui.input(|i| i.zoom_delta());
            if delta != 1.0 && ui.rect_contains_pointer(shown.inner_rect) {
                let zoom = (zoom * delta).clamp(GRID_ZOOM.0, GRID_ZOOM.1);
                ui.data_mut(|d| d.insert_temp(zoom_key, zoom));
            }
        } else {
            self.grid_cells(ui, grid, size, glyph_size, out);
        }
    }

    /// The cells of `grid`, each `size`, their glyphs `glyph_size` large; a cell names what it holds on hover.
    fn grid_cells(
        &self,
        ui: &mut egui::Ui,
        grid: &Grid,
        size: egui::Vec2,
        glyph_size: f32,
        out: &mut Vec<Intent>,
    ) {
        let (xs, ys) = drawn_span(grid);
        ui.scope(|ui| {
            ui.spacing_mut().item_spacing = egui::Vec2::ZERO;
            for y in ys {
                ui.horizontal(|ui| {
                    for x in xs.clone() {
                        let Some(cell) = grid.cells.iter().find(|c| (c.x, c.y) == (x, y)) else {
                            ui.allocate_space(size);
                            continue;
                        };
                        let glyph = cell.glyph.clone().unwrap_or_else(|| " ".into());
                        let name = cell.label.as_ref().map_or(glyph.clone(), Text::shown);
                        // With a tileset the cell is its sprites, drawn over it once its window is drawn.
                        let looks = cell
                            .looks
                            .iter()
                            .filter_map(|&i| grid.looks.get(i).cloned())
                            .collect::<Vec<_>>();
                        let sprite = (self.display.tiles() && !looks.is_empty())
                            .then(|| self.model.looks_cell(&looks, x | y << 16))
                            .flatten();
                        let glyph = if sprite.is_some() { " ".into() } else { glyph };
                        let color = cell.color.as_deref().map(|c| self.tint(c));
                        let at_cursor = grid
                            .cursor
                            .is_some_and(|c| (c.x, c.y) == (x as i32, y as i32));
                        let actionable = cell.id.is_some() && cell.affordance.is_some();
                        let mut shown = egui::RichText::new(&glyph).monospace().size(glyph_size);
                        if let Some(color) = color {
                            shown = shown.color(color);
                        }
                        let marked = cell.selected == Some(true) || at_cursor;
                        // A marked cell keeps a rim of its highlight around the sprite.
                        let queue = |ui: &egui::Ui, rect: egui::Rect| {
                            if let Some(cell) = sprite.clone() {
                                let rect = rect.shrink(if marked { 3.0 } else { 0.0 });
                                self.screens.icons.borrow_mut().push(RowIcon {
                                    painter: ui.painter_at(rect),
                                    rect,
                                    cell,
                                });
                            }
                        };
                        if !actionable {
                            let shown = match marked {
                                true => shown.background_color(tokens::SELECTION),
                                false => shown,
                            };
                            let label = ui.add_sized(size, egui::Label::new(shown));
                            queue(ui, label.rect);
                            continue;
                        }
                        let button = ui.add_enabled(
                            cell.enabled,
                            egui::Button::new(shown).min_size(size).selected(marked),
                        );
                        queue(ui, button.rect);
                        button.widget_info(|| {
                            WidgetInfo::labeled(
                                WidgetType::Button,
                                button.enabled(),
                                markup::plain(&name),
                            )
                        });
                        let button = match (&cell.denial, &cell.label) {
                            (Some(why), _) if !cell.enabled => {
                                button.on_disabled_hover_text(why.shown())
                            }
                            (_, Some(_)) => button.on_hover_text(markup::plain(&name)),
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
}

/// The columns and rows of `grid` from its first to its last cell or cursor: the empty edges around them draw
/// nothing, so they take no room.
pub(super) fn drawn_span(grid: &Grid) -> (std::ops::Range<u32>, std::ops::Range<u32>) {
    let cursor = grid
        .cursor
        .and_then(|c| Some((u32::try_from(c.x).ok()?, u32::try_from(c.y).ok()?)));
    let at = grid.cells.iter().map(|c| (c.x, c.y)).chain(cursor);
    let span = |pick: fn((u32, u32)) -> u32, end: u32| {
        let (lo, hi) = at
            .clone()
            .map(pick)
            .filter(|&v| v < end)
            .fold((end, 0), |(lo, hi), v| (lo.min(v), hi.max(v + 1)));
        lo.min(hi)..hi
    };
    (span(|p| p.0, grid.cols), span(|p| p.1, grid.rows))
}
