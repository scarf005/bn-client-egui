//! The top screen of the engine's stack drawn from its components (protocol 0.2): lists, tabs, splits,
//! panels, forms, text, grids, the map and canvases, laid out the same way for every screen.
//!
//! Drawing reads the engine's data and collects what the player did as `Intent`s. Applying an intent only
//! calls the interaction core (`bn_client_core::interaction`), which decides what is sent.

mod affordances;
mod detail;
mod footer;
mod form;
mod grid;
mod intent;
mod list;
mod measure;
mod overmap;
mod panel;
mod panel_window;
mod panes;
mod placement;
mod screen;
mod tabs;
mod target;
mod text;
mod workspace;

use super::input::{self, KeyWindow};
use super::{App, sidebar};
use crate::kit::section::{self, Fact};
use crate::kit::split::{self, Extent};
use crate::kit::tokens::{self, BAD, GOOD};
use crate::windows::{self, Place, Shown, WindowSpec};
use crate::{a11y, editor};
use affordances::*;
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
use intent::*;
use list::*;
use measure::*;
pub(super) use measure::{control_line, text_width};
pub(super) use overmap::WorldMap;
use overmap::*;
use panel::*;
pub(super) use panel_window::PANELS;
use panes::*;
use placement::*;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet, VecDeque};
use target::*;

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
    /// The `select` invoke the focus moving called for while the engine was too busy to take it, sent once it takes
    /// commands again; a later one takes its place.
    select: Option<(String, InvokeArgs)>,
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

/// A list's scroll offset in the last frame, and the row of the focus it last brought into sight.
#[derive(Default)]
struct ListScroll {
    top: f32,
    shown: Option<String>,
}

fn tone_color(tone: Tone) -> egui::Color32 {
    if tone == Tone::Good { GOOD } else { BAD }
}

/// The screen's title without its colour tags; empty without one.
fn title(screen: &Screen) -> String {
    markup::plain(&screen.title.as_ref().map(Text::shown).unwrap_or_default())
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

/// What was kept under `key` in the last pass or this one: what was kept before belongs to an earlier showing.
fn recent<T: Clone + Send + Sync + 'static>(ctx: &egui::Context, key: egui::Id) -> Option<T> {
    let pass = ctx.cumulative_pass_nr();
    let kept = ctx.data(|d| d.get_temp::<(T, u64)>(key));
    kept.filter(|(_, at)| at + 1 >= pass)
        .map(|(value, _)| value)
}

/// Keeps `value` under `key` for `recent` to read this pass and the next.
fn keep_recent<T: Clone + Send + Sync + 'static>(ctx: &egui::Context, key: egui::Id, value: T) {
    let pass = ctx.cumulative_pass_nr();
    ctx.data_mut(|d| d.insert_temp(key, (value, pass)));
}

/// `text` in the game colour, as the native markup writes it.
pub(super) fn tagged(color: Option<&str>, text: &str) -> String {
    match color {
        Some(color) => format!("<color_{color}>{text}</color>"),
        None => text.into(),
    }
}

impl<M: GameModel> App<M> {
    /// One line of the engine's text with its colour tags; what does not fit is cut.
    fn line(&self, ui: &egui::Ui, markup: &str) -> egui::WidgetText {
        self.line_on(ui, markup, tokens::ground(ui))
    }

    /// `line` drawn on `ground`.
    fn line_on(&self, ui: &egui::Ui, markup: &str, ground: egui::Color32) -> egui::WidgetText {
        let font = egui::TextStyle::Body.resolve(ui.style());
        self.styled_job(markup, &font, ground).into()
    }

    /// The engine's `markup` in `font`, its colour tags drawn in colours that read on `ground`.
    fn styled_job(
        &self,
        markup: &str,
        font: &egui::FontId,
        ground: egui::Color32,
    ) -> egui::text::LayoutJob {
        crate::text::job(&self.model.styled(markup), font, ground)
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
            Component::Tabs(tabs) => self.tabs(ui, screen, tabs, out),
            Component::Split(split) => self.split(ui, screen, split, out),
            Component::Panel(panel) => self.panel_section(ui, screen, panel, out),
            Component::Form(form) => self.form(ui, screen, form, out),
            Component::Text(block) => self.text_block(ui, block),
            Component::Grid(grid) => {
                let buttons = !own_buttons(screen, c).is_empty();
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
}
