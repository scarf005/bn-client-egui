//! The top screen of the engine's stack drawn from its components (protocol 0.2): lists, tabs, splits,
//! panels, forms, text, grids, the map and canvases, laid out the same way for every screen.
//!
//! Drawing reads the engine's data and collects what the player did as `Intent`s. Applying an intent only
//! calls the interaction core (`bn_client_core::interaction`), which decides what is sent.

mod detail;
mod footer;
mod form;
mod grid;
mod list;
mod measure;
mod overmap;
mod panel;
mod placement;
mod tabs;
mod target;
mod workspace;

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
    FormField, Grid, GridCell, GridDrag, GridRegion, GridRole, InvokeArgs, List, MapComponent,
    MapMode, OvermapTile, OvermapWire, PanelStat, Relation, Row, Screen, ScreenDock, ScreenRole,
    Selection, SidebarSide, Slot, Split, Text, TextBlock, TextRole, Tone, WorldPos,
};
use bn_client_core::{Cell, GameModel, Pos, Rgb, markup};
use eframe::egui::{self, WidgetInfo, WidgetType};
use footer::*;
use form::*;
use grid::*;
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
    /// The heights the rows of the lists of the column drawn now are given, by list.
    column_caps: RefCell<HashMap<String, f32>>,
    /// The heights the rows of the lists of the column drawn now want and take, in the order they are drawn.
    column_lists: RefCell<Vec<ListFit>>,
    /// The squares a drag crossed that the engine has not taken yet: it holds few commands at once.
    paints: VecDeque<Paint>,
    /// A workspace under the top screen is being drawn: it is shown and takes no input.
    under: std::cell::Cell<bool>,
    /// The form the player last sent or filled, by screen and form id, while nothing else was run since: the
    /// engine's news is shown under it.
    sent_form: Option<(String, String)>,
    /// The value of a form field the client refused, and why, or the affordance it was sent with, until the field
    /// is edited or filled again.
    field_refusal: RefCell<Option<FieldRefusal>>,
    /// The engine's news to show under the form of this id, this frame.
    news: RefCell<Option<(String, Vec<TextBlock>)>>,
}

/// A form field whose value the client refused or sent, by screen, form and field id.
struct FieldRefusal {
    screen: String,
    form: String,
    field: String,
    /// The client's refusal of the value.
    text: Option<String>,
    /// The affordance the value was sent with, whose command the engine may refuse.
    sent: Option<String>,
}

/// The height the rows of the list `id` want, all of those it holds, and the height they take.
#[derive(Clone, Default)]
struct ListFit {
    id: String,
    wanted: f32,
    taken: f32,
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

/// Which form `intent` acts on, when it runs something: none for an action of no form; nothing to say for one
/// that runs nothing.
fn acted_form(screen: &Screen, intent: &Intent) -> Option<Option<String>> {
    let form_of = |id: &str| {
        screen.components().into_iter().find_map(|c| match c {
            Component::Form(form)
                if form.submit.as_deref() == Some(id)
                    || form.head.affordances.iter().any(|a| a == id) =>
            {
                Some(form.head.id.clone())
            }
            _ => None,
        })
    };
    match intent {
        Intent::Fill { form, .. } => Some(Some(form.clone())),
        Intent::Run(id) => Some(form_of(id)),
        Intent::Paint(_)
        | Intent::Move { .. }
        | Intent::Search { .. }
        | Intent::Reach(_)
        | Intent::Pane(_)
        | Intent::Confirm => Some(None),
        _ => None,
    }
}

fn find<'a>(screen: &'a Screen, id: &str) -> Option<&'a Affordance> {
    screen.affordances.iter().find(|a| a.id == id)
}

/// The row or tab of a list or strip that had the keyboard, by the engine's id and the widget's.
type Keyed = (String, egui::Id);

/// Whether the stop `now` of a list or strip takes the keyboard: the arrows moved it off the `held` one, and no
/// other widget took the keyboard since.
fn takes_keyboard(ctx: &egui::Context, held: Option<&Keyed>, now: &str) -> bool {
    held.is_some_and(|(id, widget)| {
        id != now && ctx.memory(|m| m.focused().is_none_or(|f| f == *widget))
    })
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
        // A form sent on a screen that closed is no place for the news of one opened after it.
        let screens = self.model.screens();
        if let Some((at, _)) = &self.screens.sent_form
            && !screens.iter().any(|s| &s.screen_id == at)
        {
            self.screens.sent_form = None;
        }
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
        self.backdrop(ctx);
        self.paint_icons();
        self.paint_overmap();
        // Only the lists of the top screen drawn this frame have a page: the backdrop's take no keys.
        self.screens.list_pages.borrow_mut().clear();
        let Some(top) = interaction::described(self.model.screens()) else {
            return self.menu(ctx);
        };
        let mut out = vec![];
        let cancel = interaction::with_role(top, AffordanceRole::Cancel).map(|a| a.id.clone());
        let kind = self.window_kind(top);
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

    /// The topmost workspace under a top screen that is not one, drawn behind it and taking no input, as an
    /// application stays in view under its dialogs.
    fn backdrop(&self, ctx: &egui::Context) {
        let Some((top, below)) = self.model.screens().split_last() else {
            return;
        };
        let under = below
            .iter()
            .rev()
            .find(|s| workspace::laid_out(s).is_some());
        let Some(under) = under.filter(|_| workspace::laid_out(top).is_none()) else {
            return;
        };
        self.screens.under.set(true);
        // What the player does on it is nothing: it has no affordances, and its widgets are off.
        self.screen_window(ctx, under, &mut vec![]);
        self.screens.under.set(false);
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
            if out.invoke.is_some() {
                self.screens.sent_form = None;
            }
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
        let Some(top) = interaction::described(self.model.screens()) else {
            return;
        };
        // The form the player sends takes the engine's news once it goes out; an act outside the forms gives the
        // news back to the status line.
        let acted = acted_form(top, &intent);
        if let Some(None) = acted {
            self.screens.sent_form = None;
        }
        let sent_form = acted.flatten().map(|form| (top.screen_id.clone(), form));
        let filled = match &intent {
            Intent::Fill { form, field, .. } => {
                Some((top.screen_id.clone(), form.clone(), field.clone()))
            }
            _ => None,
        };
        let interaction = &mut self.screens.interaction;
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
        if out.invoke.is_some() && sent_form.is_some() {
            self.screens.sent_form = sent_form;
        }
        // A value the client refused stays by its field, and only there; one sent takes the refusal away.
        if let Some((screen, form, field)) = filled {
            let refusal = out.invoke.is_none().then(|| out.note()).flatten();
            let refused = refusal.is_some();
            let sent = out.invoke.as_ref().map(|(id, _)| id.clone());
            self.screens.field_refusal.replace(Some(FieldRefusal {
                screen,
                form,
                field,
                text: refusal,
                sent,
            }));
            if refused {
                self.keys_consumed |= out.consumed;
                self.screens.denial = None;
                return;
            }
        }
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
        // Under a screen kept as its window, such as its list filter's prompt, it is a window of its own.
        let under = self.screens.under.get();
        let shared = under
            && self.model.screens().last().is_some_and(|top| {
                let title = markup::plain(&top.title.as_ref().map(Text::shown).unwrap_or_default());
                a11y::dialog_id(self.window_kind(top).as_deref().unwrap_or(&title)) == base.id
            });
        let spec = WindowSpec {
            id: match shared {
                true => base.id.with("under"),
                false => base.id,
            },
            inert: under,
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
        // A grid of no squares, nor the buttons that act on them.
        if let Component::Grid(grid) = c
            && (grid.cols == 0 || grid.rows == 0)
        {
            return;
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
            Component::Grid(grid) => {
                let buttons = !footer::own_buttons(screen, c).is_empty();
                self.grid(ui, screen, (grid, buttons), out)
            }
            Component::Map(map) => self.map_slot(ui, screen, map, out),
            Component::Canvas(canvas) => self.canvas(ui, canvas),
        }
        if !matches!(c, Component::List(_)) {
            self.component_buttons(ui, screen, c, None, out);
        }
        // The engine's news after the form was sent, under its buttons.
        if let Component::Form(form) = c
            && let Some((_, news)) = self
                .screens
                .news
                .borrow()
                .as_ref()
                .filter(|(id, _)| id == &form.head.id)
        {
            news.iter().for_each(|t| self.text_block(ui, t));
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
}
