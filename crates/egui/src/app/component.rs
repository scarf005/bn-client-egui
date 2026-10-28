//! The top screen of the engine's stack drawn from its components (protocol 0.2): lists, tabs, splits,
//! panels, forms, text, grids, the map and canvases, laid out the same way for every screen.
//!
//! Drawing reads the engine's data and collects what the player did as `Intent`s. Applying an intent only
//! calls the interaction core (`bn_client_core::interaction`), which decides what is sent.

mod canvas;
mod detail;
mod drag;
mod footer;
mod form;
mod list;
mod measure;
mod overmap;
mod panel;
mod placement;
mod region;
mod tabs;
mod target;
mod workspace;

use self::drag::Square;
use super::input::KeyWindow;
use super::{App, sidebar};
use crate::kit::section::{self, Fact};
use crate::kit::split::{self, Extent};
use crate::kit::tokens::{self, BAD, GOOD};
use crate::windows::{self, Place, Shown, WindowSpec};
use crate::{a11y, editor, text};
use bn_client_core::interaction::{self, ArgKind, InvokeError};
use bn_client_core::protocol::{
    Affordance, AffordanceRole, CanvasComponent, Component, Editor, EditorValue, FieldKind, Form,
    FormField, Grid, GridDrag, GridRegion, GridRole, InvokeArgs, List, MapComponent, MapMode,
    OvermapTile, OvermapWire, PanelStat, Relation, Row, Screen, ScreenDock, ScreenRole, Selection,
    SidebarSide, Slot, Split, Text, TextBlock, TextRole, Tone, WorldPos,
};
use bn_client_core::{Cell, GameModel, Pos, Rgb, markup};
use eframe::egui::{self, WidgetInfo, WidgetType};
use footer::*;
use form::*;
use list::*;
pub(super) use measure::text_width;
use measure::*;
pub(super) use overmap::WorldMap;
use overmap::*;
pub(super) use panel::PANELS;
use placement::*;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet, VecDeque};
use target::*;

/// The margin around each pane of a split, on every side.
const PANE_MARGIN: i8 = 2;
/// The most rows a list in a peer pane shows before it scrolls.
const PEER_ROWS: usize = 12;
/// Side of a grid cell in points.
const GRID_CELL: f32 = 26.0;
/// The largest a grid's cell grows to when the room allows.
const GRID_MAX_CELL: f32 = 2.0 * GRID_CELL;
/// The opacity of the squares a grid marks faded.
const GRID_FADED: f32 = 0.45;
/// The most the zoom gesture scales a canvas grid's cells from the size that fills its room.
const GRID_ZOOM_MAX: f32 = 4.0;

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
    pub(super) list_pages: RefCell<HashMap<(String, String), usize>>,
    /// The squares of this frame's list rows left for their item's sprite, painted once the window is drawn.
    icons: RefCell<Vec<RowIcon>>,
    /// The block outlines of this frame's grids, painted after the sprites they run over.
    rules: RefCell<Vec<(egui::Painter, Vec<egui::Shape>)>>,
    /// What this frame's overmap draws, painted once the window is drawn, so that a tileset's sprites come
    /// before the marks over them.
    overmap: RefCell<Vec<OvermapPaint>>,
    /// A panel window is being drawn: its screen takes no command, so its tabs and rows only move its focus.
    pub(super) passive: bool,
    /// The row to choose once the filter prompt that showed it has confirmed its text.
    reach: Option<String>,
    /// A key asked for the list's search box: it takes the keyboard when it is next drawn.
    focus_search: std::cell::Cell<bool>,
    /// The most rows the lists drawn now show before they scroll, when fewer than their room holds.
    list_rows: std::cell::Cell<Option<usize>>,
    /// The parts drawn now are in a workspace's column, which scrolls: its lists keep their rows' names whole
    /// before their values, as palettes, and are as tall as their rows up to `list_rows`, however little of the
    /// column is left below them.
    in_column: std::cell::Cell<bool>,
    /// The squares a drag crossed that the engine has not taken yet: it holds few commands at once.
    paints: VecDeque<Paint>,
}

/// A square of a canvas grid a drag runs its affordance on.
struct Paint {
    component: String,
    /// The square's row; none for the affordance that opens or closes a drag.
    row: Option<String>,
    affordance: String,
    /// The text the affordance takes with the row, when it takes one.
    text: Option<String>,
}

/// Where a grid's squares were drawn.
struct GridLayout {
    /// The top left of the first square drawn, which is the column and the row `first` names.
    origin: egui::Pos2,
    first: Square,
    size: egui::Vec2,
    /// Clipped to what the grid shows.
    painter: egui::Painter,
}

/// What a window kept off the cursor holds while its screen is open: its side, and the pass it was last shown in.
#[derive(Clone, Copy)]
struct Anchor {
    left: bool,
    pass: u64,
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
    /// A drag crossed a square of a canvas grid.
    Paint(Paint),
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
    /// The text a list's search box holds now, for the affordance that sets the list's filter.
    Search {
        affordance: String,
        text: String,
    },
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
                Component::Tabs(t) => vec![t.select.as_deref()],
                Component::Form(f) => vec![f.submit.as_deref()],
                // A grid's drags run these, so no button does.
                Component::Grid(g) => vec![
                    g.drag_begin.as_deref(),
                    g.drag_end.as_deref(),
                    g.region_drag.as_deref(),
                ],
                // Its rows run it on a double click or Enter, so it is no button.
                Component::List(l) => vec![l.activate.as_deref()],
                _ => vec![],
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
                .chain(extra.into_iter().flatten())
                .chain(rows)
        })
        .collect()
}

/// The affordances of `screen` that take a row of its list `id` and that no part names: the list offers them under
/// its rows as its own.
fn row_actions<'a>(screen: &'a Screen, id: &str) -> impl Iterator<Item = &'a Affordance> {
    let named = screen.root.as_ref().map_or_else(HashSet::new, claimed);
    screen.affordances.iter().filter(move |a| {
        !named.contains(a.id.as_str())
            && a.args
                .as_ref()
                .and_then(|args| args.row.as_ref())
                .is_some_and(|row| row.component_id == id)
    })
}

/// The engine's affordance that opens the prompt of a list's filter.
const FILTER: &str = "FILTER";

/// Whether `a` is the filter of `component`, a list with a search box: the box sets the text, and its keys
/// take the box instead of opening the prompt.
fn searched(screen: &Screen, list: &List, a: &Affordance) -> bool {
    list.search(screen)
        .is_some_and(|search| search.id == a.id || a.id == FILTER)
}

/// Whether a drag over one of the screen's grids runs the affordance, so no button does.
fn gesture(screen: &Screen, affordance: &Affordance) -> bool {
    let id = Some(affordance.id.as_str());
    screen.components().into_iter().any(|c| match c {
        Component::Grid(g) => [&g.drag_begin, &g.drag_end, &g.region_drag]
            .into_iter()
            .any(|gesture| gesture.as_deref() == id),
        _ => false,
    })
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
        self.paint_overmap();
        out.into_iter().for_each(|intent| self.apply(intent));
        self.send_paints(ctx);
    }

    /// Runs the squares a drag crossed in order, as many as the engine takes now; the rest wait for a later frame.
    fn send_paints(&mut self, ctx: &egui::Context) {
        while let Some(paint) = self.screens.paints.front() {
            let Some(top) = interaction::described(self.model.screens()) else {
                return self.screens.paints.clear();
            };
            let interaction = &mut self.screens.interaction;
            if let Some(row) = &paint.row {
                interaction.focus_row(&paint.component, row);
            }
            // An affordance that takes the drag's text is called with it as it is; the core only builds the others.
            let invoke = match &paint.text {
                Some(text) => Some((
                    paint.affordance.clone(),
                    InvokeArgs {
                        row: paint.row.clone(),
                        text: Some(text.clone()),
                        ..InvokeArgs::default()
                    },
                )),
                None => {
                    let out = interaction.activate(top, &paint.affordance);
                    self.screens.denial = out.note();
                    out.invoke
                }
            };
            if let Some((id, args)) = invoke {
                match self.model.invoke_affordance(&id, args) {
                    Err(InvokeError::Busy) => return ctx.request_repaint(),
                    Err(InvokeError::Disabled(denial)) => {
                        self.screens.denial = denial.as_ref().map(Text::shown);
                    }
                    _ => {}
                }
            }
            self.screens.paints.pop_front();
        }
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
        for (painter, rules) in self.screens.rules.take() {
            painter.extend(rules);
        }
    }

    /// The keys pressed this frame go to the interaction core, which decides what each one does.
    pub(super) fn component_keys(&mut self, ctx: &egui::Context) {
        self.keys_consumed = false;
        canvas::keep_space(ctx);
        self.sync_screens();
        let pages = self.screens.list_pages.borrow().clone();
        // A text box has the typing; its keys are not the screen's, except Escape, which closes the screen (a filled filter box clears first).
        let typing = ctx.text_edit_focused();
        self.search_keys(ctx, typing);
        let keys = super::input::frame_keys(ctx)
            .into_iter()
            .filter(|k| !self.settings_key(k))
            .map(|k| k.token);
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

    /// The keys that take the screen's search box: Ctrl+F, and the keys the engine binds to its filter prompt, which
    /// the box replaces. They are the box's, not the screen's, and are not typed into it.
    fn search_keys(&mut self, ctx: &egui::Context, typing: bool) {
        let Some(top) = interaction::described(self.model.screens()) else {
            return;
        };
        let searched = top
            .components()
            .into_iter()
            .any(|c| matches!(c, Component::List(list) if list.search(top).is_some()));
        if !searched || typing {
            return;
        }
        let prompt = find(top, FILTER).map_or(vec![], |a| a.keys.clone());
        let takes = |e: &egui::Event| {
            let ctrl_f = matches!(e, egui::Event::Key { key: egui::Key::F, pressed: true, modifiers, .. }
                if modifiers.command && !modifiers.alt);
            ctrl_f || super::input::frame_key(e).is_some_and(|k| prompt.contains(&k.token))
        };
        if ctx.input(|i| i.events.iter().any(takes)) {
            self.screens.focus_search.set(true);
            ctx.input_mut(|i| i.events.retain(|e| !takes(e)));
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
            Intent::Search { affordance, text } => {
                let args = InvokeArgs {
                    text: Some(text),
                    ..InvokeArgs::default()
                };
                if let Err(InvokeError::Disabled(denial)) =
                    self.model.invoke_affordance(&affordance, args)
                {
                    self.screens.denial = denial.as_ref().map(Text::shown);
                }
                return;
            }
            Intent::Run(id) => interaction.activate(top, &id),
            Intent::Paint(paint) => return self.screens.paints.push_back(paint),
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
        // A workspace fills the view edge to edge; its panels keep their own margins.
        let frame = match workspace::laid_out(screen) {
            Some(_) => egui::Frame::NONE.fill(tokens::DIALOG),
            None => super::menu::list_frame(ctx),
        };
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
        // A window kept off the cursor holds its side while its screen is open; its size is the player's.
        let anchored = keeps_off_cursor(screen);
        let anchor_key = egui::Id::new(("anchored", &screen.screen_id));
        // One not shown in the last pass belongs to an earlier opening of the screen.
        let pass = ctx.cumulative_pass_nr();
        let anchor = ctx
            .data(|d| d.get_temp::<Anchor>(anchor_key))
            .filter(|a| anchored && a.pass + 1 >= pass);
        let room = Room {
            region,
            sidebar: self.sidebar,
            avoid: &avoid,
            map: !interaction::game_hidden(self.model.screens()),
            kept_left: anchor.map(|a| a.left),
        };
        let placement = placement(screen, &room);
        let place = placement.place;
        let base = self.window_spec(ctx, self.window_kind(screen).as_deref(), &title);
        // A workspace is the whole view, with no title bar: its title, help and leave end its menu bar.
        let workspace = workspace::laid_out(screen).is_some();
        let title_bar = base.title_bar || workspace;
        let named = screen.root.as_ref().map_or_else(HashSet::new, claimed);
        let lists = screen.root.as_ref().map_or(vec![], |root| {
            root.nested()
                .into_iter()
                .filter_map(|c| match c {
                    Component::List(l) => Some(l.head.id.as_str()),
                    _ => None,
                })
                .collect()
        });
        let listed = lists
            .iter()
            .flat_map(|id| row_actions(screen, id))
            .map(|a| a.id.as_str())
            .collect::<HashSet<_>>();
        // An action on a grid's square runs from the squares whose cells name it, never from a button.
        let grids = screen.root.as_ref().map_or(vec![], |root| {
            root.nested()
                .into_iter()
                .filter_map(|c| match c {
                    Component::Grid(g) => Some(g.head.id.as_str()),
                    _ => None,
                })
                .collect()
        });
        let on_square = |a: &Affordance| {
            a.args
                .as_ref()
                .and_then(|args| args.row.as_ref())
                .is_some_and(|row| grids.contains(&row.component_id.as_str()))
        };
        let rest = screen
            .affordances
            .iter()
            .filter(|a| {
                !named.contains(a.id.as_str()) && !listed.contains(a.id.as_str()) && !on_square(a)
            })
            .collect::<Vec<_>>();
        // The help sits in the title bar, or last in the footer of a window that has none.
        let titled = interaction::with_role(screen, AffordanceRole::Help)
            .filter(|a| a.enabled && base.title_bar && !workspace);
        let spec = WindowSpec {
            title_bar: base.title_bar && !workspace,
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
                if let Some(split) = workspace::laid_out(screen) {
                    return self.workspace(ui, screen, (split, &rest), out);
                }
                egui::Panel::bottom(egui::Id::new(("footer", &screen.screen_id)))
                    .resizable(false)
                    .frame(egui::Frame::NONE)
                    .show_separator_line(false)
                    .show(ui, |ui| footer(ui, out));
                self.menu_bar(ui, screen, out);
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
                self.menu_bar(ui, screen, out);
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
        if let (true, Some(left)) = (anchored, at_left(place)) {
            ctx.data_mut(|d| d.insert_temp(anchor_key, Anchor { left, pass }));
        }
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
        text::job(&self.model.styled(markup), &font, ground).into()
    }

    /// The game colour as a colour of the screen.
    fn tint(&self, name: &str) -> egui::Color32 {
        super::rgb(self.model.colors().resolve(name).0)
    }

    fn component(&self, ui: &mut egui::Ui, screen: &Screen, c: &Component, out: &mut Vec<Intent>) {
        if let (Some(label), false) = (&c.head().label, matches!(c, Component::Panel(_))) {
            ui.label(egui::RichText::new(markup::plain(&label.shown())).strong());
        }
        self.component_body(ui, screen, c, out);
    }

    /// `c` without its label.
    fn component_body(
        &self,
        ui: &mut egui::Ui,
        screen: &Screen,
        c: &Component,
        out: &mut Vec<Intent>,
    ) {
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
                        let (levels, others) = split_levels(screen, stats);
                        Self::facts(ui, &panel.head.id, &others);
                        self.aim_levels(ui, &levels, out);
                    }
                    if let Some(filter) = &panel.filter {
                        ui.weak(format!("/{filter}"));
                    }
                    if let Some(map) = panel.child.as_deref().and_then(Component::cursor_map) {
                        self.target_candidates(ui, screen, map, out);
                    }
                    if let Some(child) = &panel.child {
                        self.component(ui, screen, child, out);
                    }
                });
            }
            Component::Form(form) => self.form(ui, screen, form, out),
            Component::Text(block) => self.text_block(ui, block),
            Component::Grid(grid) => self.grid(ui, screen, grid, out),
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
                // A line holding a map or a canvas takes all the room; the lines under it are as tall as what they
                // hold, a list up to `PEER_ROWS` rows, and scroll below.
                let takes = |line: &std::ops::Range<usize>| {
                    parts[line.clone()]
                        .iter()
                        .flat_map(|c| c.nested())
                        .any(takes_room)
                };
                let below = lines[..n].iter().any(takes);
                let height = match (natural || below, takes(line)) {
                    (_, true) => ui.available_height().max(floor),
                    (true, false) => f32::INFINITY,
                    (false, false) => (ui.available_height() / (count - n) as f32).max(floor),
                };
                // Inside a peer, a pane that shares its height still has none to share.
                let rows = match natural || below {
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
                let scroll = egui::ScrollArea::vertical()
                    .id_salt(("split-lines", &split.head.id))
                    .auto_shrink([false, true]);
                let room = ui.available_height();
                body_scroll(ui, scroll, room, draw);
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
    fn facts(ui: &mut egui::Ui, id: &str, stats: &[&PanelStat]) {
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

    fn grid(&self, ui: &mut egui::Ui, screen: &Screen, grid: &Grid, out: &mut Vec<Intent>) {
        let (xs, ys) = drawn_span(grid);
        // The cells grow with the room the grid has, up to twice their size, their glyphs with them; a canvas's
        // the same, and the zoom gesture over it scales them from there.
        let fit = (ui.available_width() / xs.len().max(1) as f32)
            .min(ui.available_height() / ys.len().max(1) as f32);
        let canvas = grid.role == Some(GridRole::Canvas);
        // A canvas zooms out until the whole grid fits in its room, and in until a cell is as large as the room.
        let base = fit.clamp(GRID_CELL, GRID_MAX_CELL);
        let limits = ((fit / base).min(1.0), GRID_ZOOM_MAX.max(fit / base));
        let view_key = egui::Id::new(("grid view", &screen.screen_id, &grid.head.id));
        let view = ui
            .data(|d| d.get_temp::<canvas::View>(view_key))
            .filter(|v| v.span == (xs.len(), ys.len()));
        let anchor = view.and_then(|view| canvas::zoomed(ui, view, limits));
        let zoom = anchor
            .as_ref()
            .map_or(view.map_or(1.0, |v| v.zoom), |a| a.zoom)
            .clamp(limits.0, limits.1);
        let cell = match canvas {
            true => base * zoom,
            false => fit.clamp(GRID_CELL, GRID_MAX_CELL),
        };
        let size = egui::vec2(cell, cell);
        let glyph_size = egui::TextStyle::Monospace.resolve(ui.style()).size * cell / GRID_CELL;
        let mut squares = HashMap::new();
        let layout;
        if canvas {
            // The wheel zooms it; a middle-button drag, or Space and the primary one, pans it.
            let panning = canvas::panning(ui);
            let pan = ui.input(|i| {
                (i.pointer.middle_down() || (panning && i.pointer.primary_down()))
                    .then(|| i.pointer.delta())
            });
            // The viewport is a whole number of cells tall, so none is cut across.
            let whole = ui.available_size();
            let rows = (whole.y / cell).floor().max(1.0);
            let room = egui::vec2(whole.x, rows * cell);
            let grid_size = egui::vec2(xs.len() as f32, ys.len() as f32) * cell;
            // The area clamps an asked offset to last frame's content, so the margins of a new zoom take it only
            // the frame after: it is asked for again then.
            let zoomed = zoom > limits.0 + 1e-3;
            let placed = canvas::place(
                (view, anchor.as_ref()),
                zoomed,
                (whole, room, grid_size),
                cell,
            );
            let offset = placed.offset;
            let asked = anchor.is_some().then_some(offset).flatten();

            let mut area = egui::ScrollArea::both()
                .id_salt(("grid", &screen.screen_id, &grid.head.id))
                .max_height(rows * cell)
                .auto_shrink([true, true])
                // The whole grid is in view until it is zoomed past that.
                .scroll_bar_visibility(match zoomed {
                    true => egui::scroll_area::ScrollBarVisibility::VisibleWhenNeeded,
                    false => egui::scroll_area::ScrollBarVisibility::AlwaysHidden,
                })
                .scroll_source(egui::scroll_area::ScrollSource {
                    drag: egui::scroll_area::DragScroll::Never,
                    mouse_wheel: false,
                    scroll_bar: true,
                });
            if let Some(offset) = offset {
                area = area.scroll_offset(offset);
            }
            let shown = area.show(ui, |ui| {
                canvas::note_clip(ui.ctx(), ui.clip_rect());
                canvas::note_hover(ui);
                if let Some(pan) = pan.filter(|_| ui.rect_contains_pointer(ui.clip_rect())) {
                    ui.scroll_with_delta(pan);
                }
                let cells = GridCells { size, glyph_size };
                // The cells sit `lead` in from the corner, and the area runs on `tail` past them.
                let corner = ui.cursor().min;
                let laid = ui.scope_builder(
                    egui::UiBuilder::new()
                        .max_rect(egui::Rect::from_min_size(corner + placed.lead, grid_size)),
                    |ui| self.grid_cells(ui, grid, cells, &mut squares, out),
                );
                let whole = placed.lead + grid_size + placed.tail;
                ui.allocate_rect(
                    egui::Rect::from_min_size(corner, whole),
                    egui::Sense::hover(),
                );
                laid.inner
            });
            layout = Some(shown.inner);
            // The canvas takes its room's whole width, which the grid is centred in.
            ui.allocate_space(egui::vec2(ui.available_width(), 0.0));
            let view = canvas::View {
                span: (xs.len(), ys.len()),
                zoom,
                cell,
                rect: shown.inner_rect,
                // While zooming, the offset asked for: one a smaller square clamped must not drift the anchor.
                offset: offset.unwrap_or(shown.state.offset),
                pending: asked,
                lead: placed.lead,
                room: whole,
            };
            ui.data_mut(|d| d.insert_temp(view_key, view));
            self.hover_outline(ui, shown.inner_rect, &squares);
        } else {
            let cells = GridCells { size, glyph_size };
            layout = Some(self.grid_cells(ui, grid, cells, &mut squares, out));
        }
        if let Some(mode) = grid.drag {
            self.drag_squares(ui, grid, mode, &squares, out);
        }
        if let (Some(affordance), Some(layout)) = (&grid.region_drag, layout) {
            self.drag_region(ui, grid, affordance, (&squares, &layout), out);
        }
    }

    /// The square under the pointer, outlined over the sprites.
    fn hover_outline(
        &self,
        ui: &egui::Ui,
        viewport: egui::Rect,
        squares: &HashMap<drag::Square, (egui::Rect, String, String)>,
    ) {
        let at = canvas::pointer_over(ui, viewport);
        let Some((rect, ..)) = at.and_then(|at| squares.values().find(|s| s.0.contains(at))) else {
            return;
        };
        let shapes = outline_shapes(*rect, tokens::TEXT);
        let painter = ui
            .painter()
            .with_clip_rect(viewport.intersect(ui.clip_rect()));
        self.screens.rules.borrow_mut().push((painter, shapes));
    }

    /// The cells of `grid`, each `size`, their glyphs `glyph_size` large; a cell names what it holds on hover.
    fn grid_cells(
        &self,
        ui: &mut egui::Ui,
        grid: &Grid,
        GridCells { size, glyph_size }: GridCells,
        squares: &mut HashMap<drag::Square, (egui::Rect, String, String)>,
        out: &mut Vec<Intent>,
    ) -> GridLayout {
        let (xs, ys) = drawn_span(grid);
        let opacity = ui.opacity();
        let drawn = ui.scope(|ui| {
            ui.spacing_mut().item_spacing = egui::Vec2::ZERO;
            for y in ys.clone() {
                ui.horizontal(|ui| {
                    for x in xs.clone() {
                        let Some(cell) = grid.cells.iter().find(|c| (c.x, c.y) == (x, y)) else {
                            ui.allocate_space(size);
                            continue;
                        };
                        // Another level seen through open air is drawn faded, as its sprites and its glyph.
                        ui.set_opacity(match cell.faded {
                            Some(true) => opacity * GRID_FADED,
                            _ => opacity,
                        });
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
                        // An empty square to draw on shows as an outlined one, not a gap in the grid;
                        // a blank glyph that shows something, such as open air, is no empty square.
                        if glyph.trim().is_empty() && looks.is_empty() {
                            ui.painter().rect_stroke(
                                button.rect,
                                0.0,
                                egui::Stroke::new(1.0, tokens::HOVER),
                                egui::StrokeKind::Inside,
                            );
                        }
                        if let (true, Some(id), Some(a)) =
                            (cell.enabled, &cell.id, &cell.affordance)
                        {
                            // Only the part of the square in view takes a drag.
                            let seen = button.rect.intersect(ui.clip_rect());
                            squares.insert((x, y), (seen, id.clone(), a.clone()));
                        }
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
                            (_, Some(label)) if !label.shown().is_empty() => {
                                button.on_hover_text(markup::plain(&name))
                            }
                            _ => button,
                        };
                        // A pointer click on a draggable canvas is the drag's press and release.
                        let pointer_click =
                            grid.drag.is_some() && ui.input(|i| i.pointer.any_released());
                        if let (true, Some(id), Some(a)) = (
                            button.clicked() && !pointer_click && !canvas::panning(ui),
                            &cell.id,
                            &cell.affordance,
                        ) {
                            out.push(Intent::Focus {
                                component: self.row_component(a, &grid.head.id),
                                row: id.clone(),
                            });
                            out.push(Intent::Run(a.clone()));
                        }
                    }
                });
            }
        });
        let origin = drawn.response.rect.min;
        let layout = GridLayout {
            origin,
            first: (xs.start, ys.start),
            size,
            painter: ui.painter().clone(),
        };
        let outlines = grid
            .regions
            .iter()
            .flat_map(|region| self.outline(ui, &layout, region))
            .collect();
        self.screens
            .rules
            .borrow_mut()
            .push((layout.painter.clone(), outlines));
        if let Some(block) = grid.block.filter(|b| b.x > 0 && b.y > 0) {
            // A light line over a dark one, so it reads on pale and dark squares alike.
            let strokes = [
                egui::Stroke::new(3.0, egui::Color32::from_black_alpha(160)),
                egui::Stroke::new(1.0, egui::Color32::WHITE),
            ];
            let along = |range: &std::ops::Range<u32>, step: u32| {
                (range.start..=range.end)
                    .filter(move |n| n % step == 0)
                    .map(|n| (n - range.start) as f32)
                    .collect::<Vec<_>>()
            };
            let (width, height) = (xs.len() as f32 * size.x, ys.len() as f32 * size.y);
            let lines = along(&xs, block.x)
                .into_iter()
                .map(|n| origin.x + n * size.x)
                .map(|x| [egui::pos2(x, origin.y), egui::pos2(x, origin.y + height)])
                .chain(
                    along(&ys, block.y)
                        .into_iter()
                        .map(|n| origin.y + n * size.y)
                        .map(|y| [egui::pos2(origin.x, y), egui::pos2(origin.x + width, y)]),
                )
                .flat_map(|points| strokes.map(|stroke| egui::Shape::line_segment(points, stroke)))
                .collect();
            self.screens
                .rules
                .borrow_mut()
                .push((ui.painter().clone(), lines));
        }
        layout
    }

    /// The outline of `region` over the squares of `layout`, and its label in the top left when it fits.
    fn outline(&self, ui: &egui::Ui, layout: &GridLayout, region: &GridRegion) -> Vec<egui::Shape> {
        let rect = layout.rect([region.from.x, region.from.y, region.to.x, region.to.y]);
        let picked = region.selected == Some(true);
        let mut shapes = outline_shapes(
            rect,
            if picked {
                tokens::REGION_PICKED
            } else {
                tokens::REGION
            },
        );
        let label = region.label.as_deref().filter(|l| !l.is_empty());
        if let Some(label) = label {
            let font = egui::TextStyle::Small.resolve(ui.style());
            let galley = layout
                .painter
                .layout_no_wrap(label.to_owned(), font, tokens::TEXT);
            if galley.size().x + 4.0 <= rect.width() && galley.size().y + 4.0 <= rect.height() {
                let at = rect.min + egui::vec2(3.0, 2.0);
                let back = egui::Rect::from_min_size(at, galley.size()).expand(1.0);
                shapes.push(egui::Shape::rect_filled(
                    back,
                    0.0,
                    egui::Color32::from_black_alpha(170),
                ));
                shapes.push(egui::Shape::galley(at, galley, tokens::TEXT));
            }
        }
        shapes
    }

    /// The component whose row `affordance` acts on: the one its row argument names, else `own`.
    fn row_component(&self, affordance: &str, own: &str) -> String {
        interaction::described(self.model.screens())
            .and_then(|top| top.affordances.iter().find(|a| a.id == affordance))
            .and_then(|a| a.args.as_ref()?.row.as_ref())
            .map_or(own, |of| of.component_id.as_str())
            .to_owned()
    }

    /// Runs `affordance` for a pointer drag of one of the grid's regions, and outlines where it would land.
    fn drag_region(
        &self,
        ui: &egui::Ui,
        grid: &Grid,
        affordance: &str,
        (squares, layout): (
            &HashMap<drag::Square, (egui::Rect, String, String)>,
            &GridLayout,
        ),
        out: &mut Vec<Intent>,
    ) {
        let key = egui::Id::new(("region-drag", &grid.head.id));
        let pass = ui.ctx().cumulative_pass_nr();
        let mut held = ui
            .data(|d| d.get_temp::<(Option<region::Grab>, u64)>(key))
            .filter(|(_, last)| last + 1 >= pass)
            .and_then(|(held, _)| held);
        let pointer = grid_pointer(ui, squares);
        if let Some(moved) = region::advance(&grid.regions, &mut held, pointer) {
            out.push(Intent::Paint(Paint {
                component: self.row_component(affordance, &grid.head.id),
                row: Some(moved.id),
                affordance: affordance.to_owned(),
                text: Some(moved.text),
            }));
        }
        if let Some(grab) = held.as_ref().filter(|g| g.deltas() != [0; 4]) {
            let color = tokens::REGION_PICKED;
            self.screens.rules.borrow_mut().push((
                layout.painter.clone(),
                outline_shapes(layout.rect(grab.preview()), color),
            ));
        }
        ui.data_mut(|d| d.insert_temp(key, (held, pass)));
    }

    /// Runs the affordance of the squares a pointer drag over `squares` crosses.
    fn drag_squares(
        &self,
        ui: &egui::Ui,
        grid: &Grid,
        mode: GridDrag,
        squares: &HashMap<drag::Square, (egui::Rect, String, String)>,
        out: &mut Vec<Intent>,
    ) {
        let key = egui::Id::new(("grid-drag", &grid.head.id));
        // A drag not kept in the last pass belongs to an earlier showing of the grid: it ends unsent.
        let pass = ui.ctx().cumulative_pass_nr();
        let (mut held, mut began) = ui
            .data(|d| d.get_temp::<(Option<drag::Drag>, Option<String>, u64)>(key))
            .filter(|(_, _, last)| last + 1 >= pass)
            .map(|(held, began, _)| (held, began))
            .unwrap_or_default();
        let was_held = held.is_some();
        let run = drag::advance(mode, &mut held, grid_pointer(ui, squares));
        let marker = |affordance: &Option<String>| {
            affordance.clone().map(|affordance| {
                Intent::Paint(Paint {
                    component: grid.head.id.clone(),
                    row: None,
                    affordance,
                    text: None,
                })
            })
        };
        // A drag begins on the press and ends on the frame that lets go of it.
        out.extend(marker(&grid.drag_begin).filter(|_| !was_held && held.is_some()));
        // Only the squares of the affordance the drag was pressed on: a stroke does not run a neighbour's, nor
        // what a grid shown anew under the pointer puts there.
        if !was_held && held.is_some() {
            began = run
                .first()
                .and_then(|at| squares.get(at))
                .map(|s| s.2.clone());
        }
        for at in run {
            let Some((_, row, affordance)) =
                squares.get(&at).filter(|s| began.as_ref() == Some(&s.2))
            else {
                continue;
            };
            out.push(Intent::Paint(Paint {
                component: self.row_component(affordance, &grid.head.id),
                row: Some(row.clone()),
                affordance: affordance.clone(),
                text: None,
            }));
        }
        out.extend(marker(&grid.drag_end).filter(|_| was_held && held.is_none()));
        let began = began.filter(|_| held.is_some());
        ui.data_mut(|d| d.insert_temp(key, (held, began, pass)));
    }
}

impl GridLayout {
    /// The rectangle of the squares from the west and north ones to the east and south ones, both included.
    fn rect(&self, [west, north, east, south]: [i32; 4]) -> egui::Rect {
        let at = |x: i32, y: i32| {
            self.origin
                + egui::vec2(
                    (x - self.first.0 as i32) as f32 * self.size.x,
                    (y - self.first.1 as i32) as f32 * self.size.y,
                )
        };
        let (a, b) = (
            at(west.min(east), north.min(south)),
            at(west.max(east) + 1, north.max(south) + 1),
        );
        egui::Rect::from_min_max(a, b)
    }
}

/// The pointer over the drawn squares of a grid this frame; a press counts only on the grid's own layer.
fn grid_pointer(
    ui: &egui::Ui,
    squares: &HashMap<drag::Square, (egui::Rect, String, String)>,
) -> drag::Pointer {
    let at = ui.input(|i| i.pointer.interact_pos());
    let on = at.and_then(|at| {
        squares
            .iter()
            .find(|(_, s)| s.0.contains(at) && ui.clip_rect().contains(at))
            .map(|(&square, s)| (square, (at - s.0.min) / s.0.size()))
    });
    let on_top = ui.ctx().layer_id_at(at.unwrap_or_default()) == Some(ui.layer_id());
    let panning = canvas::panning(ui);
    ui.input(|i| drag::Pointer {
        over: on.map(|(square, _)| square),
        spot: on.map(|(_, spot)| (spot.x, spot.y)),
        pressed: i.pointer.button_pressed(egui::PointerButton::Primary) && on_top && !panning,
        down: i.pointer.primary_down() && !panning,
        released: i.pointer.button_released(egui::PointerButton::Primary) && !panning,
    })
}

/// A dark line under a light one, so the outline reads on pale and dark squares alike.
fn outline_shapes(rect: egui::Rect, color: egui::Color32) -> Vec<egui::Shape> {
    let rect = rect.shrink(1.0);
    let strokes = [
        egui::Stroke::new(4.0, egui::Color32::from_black_alpha(160)),
        egui::Stroke::new(2.0, color),
    ];
    strokes
        .map(|stroke| egui::Shape::rect_stroke(rect, 0.0, stroke, egui::StrokeKind::Middle))
        .into()
}

/// The sizes a grid's cells are drawn at.
#[derive(Clone, Copy)]
struct GridCells {
    size: egui::Vec2,
    /// Side of the glyphs.
    glyph_size: f32,
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
