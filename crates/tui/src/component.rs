//! The engine's 0.2 screens in the terminal: every component kind laid out generically, and the screen's
//! affordances with their keys and denials. This module only draws and moves the client-local focus; the
//! core (`interaction::*`) gathers every argument and the model sends every command.

use super::{color, token};
use bn_client_core::colors::ColorMap;
use bn_client_core::interaction::{self, ArgKind, Interaction, InvokeError, Window};
use bn_client_core::markup::Run;
use bn_client_core::protocol::{
    Affordance, AffordanceRole, Component, FieldKind, Form, FormField, Grid, GridCell, InvokeArgs,
    List, MapComponent, Panel, Row, RowColumn, Screen, ScreenRole, Selection, Split, Tabs,
    TextBlock, TextRole, Tone,
};
use bn_client_core::{GameModel, Rgb};
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph};
use std::collections::HashMap;
use unicode_width::UnicodeWidthChar;

/// Rows shown before a list asks for the next page.
const PAGE: usize = 10;
/// A value column is never wider than this.
const COLUMN_MAX: usize = 16;
/// The affordance bar shows at most this many lines.
const BAR_LINES: usize = 3;

/// Where a described screen is drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Placement {
    /// A screen of its own, over everything.
    Full,
    /// A dialog over the screen beneath it.
    Over,
}

/// How the top screen is drawn: none when the map is on top or the engine has not described the screen by
/// components yet, which stay with the 0.1 drawing.
pub(super) fn placement(screens: &[Screen]) -> Option<Placement> {
    let top = screens.last().filter(|s| s.root.is_some())?;
    match top.role {
        ScreenRole::Map => None,
        ScreenRole::Screen => Some(Placement::Full),
        ScreenRole::Dialog => Some(Placement::Over),
    }
}

fn described(screens: &[Screen]) -> Option<&Screen> {
    placement(screens).and(screens.last())
}

/// The component and the ones inside it, outermost first.
fn flatten(c: &Component) -> Vec<&Component> {
    std::iter::once(c)
        .chain(c.children().into_iter().flat_map(flatten))
        .collect()
}

fn nodes(screen: &Screen) -> Vec<&Component> {
    screen.root.iter().flat_map(flatten).collect()
}

/// What the player is typing for an affordance that asks for a count or a text.
struct Collect {
    affordance: String,
    needs: Vec<ArgKind>,
    /// What was typed, for the prompt.
    typed: String,
    /// The form field the text goes to.
    field: Option<FormField>,
}

/// What a key came to.
enum Step {
    Send(String, InvokeArgs),
    Note(String),
    Done,
}

/// The client-local state of the described screens: what the core keeps of the focus and the player's
/// input, what is being typed, and how much each scrolling component showed.
#[derive(Default)]
pub(super) struct View {
    ui: Interaction,
    collecting: Option<Collect>,
    /// What the last key was refused for, until the next key.
    note: Option<String>,
    /// How much of each list and text the last frame showed.
    extents: HashMap<String, Window>,
    colors: ColorMap,
}

impl View {
    /// Follows the stack, drops what belongs to a screen or an affordance that is gone, and puts the focus on
    /// the row the engine marks selected when it names none.
    fn sync<M: GameModel>(&mut self, model: &M) {
        let stack = model.screens();
        self.ui.sync(stack);
        let top = described(stack);
        let kept =
            |c: &Collect| top.is_some_and(|s| s.affordances.iter().any(|a| a.id == c.affordance));
        if self.collecting.as_ref().is_some_and(|c| !kept(c)) {
            self.collecting = None;
        }
        if self.ui.focus().row_id.is_some() {
            return;
        }
        let selected = top.and_then(|screen| {
            nodes(screen).into_iter().find_map(|c| match c {
                Component::List(list) => model
                    .rows(list)
                    .into_iter()
                    .find(|row| row.selected)
                    .map(|row| (list.head.id.clone(), row.id.clone())),
                _ => None,
            })
        });
        if let Some((component, row)) = selected {
            self.ui.focus_row(&component, &row);
        }
    }

    /// Asks for the next page of each list the view is near the end of.
    fn page<M: GameModel>(&self, model: &mut M) {
        let near = |list: &List| {
            let known = model.rows(list).len();
            let id = &list.head.id;
            let shown = self.extents.get(id).map_or(PAGE, |w| w.visible);
            known < list.total as usize && self.ui.scroll(id) + shown + PAGE >= known
        };
        let ids = described(model.screens())
            .map(nodes)
            .unwrap_or_default()
            .into_iter()
            .filter_map(|c| match c {
                Component::List(list) if near(list) => Some(list.head.id.clone()),
                _ => None,
            })
            .collect::<Vec<_>>();
        ids.iter().for_each(|id| model.more_rows(id));
    }

    pub(super) fn draw<M: GameModel>(
        &mut self,
        f: &mut Frame,
        model: &mut M,
        placement: Placement,
    ) {
        self.sync(model);
        self.page(model);
        let model = &*model;
        let Some(screen) = described(model.screens()) else {
            return;
        };
        let full = f.area();
        let area = match placement {
            Placement::Full => full,
            Placement::Over => Rect::new(
                full.x + full.width / 20,
                full.y + full.height / 10,
                full.width - full.width / 10,
                full.height - full.height / 5,
            ),
        };
        f.render_widget(Clear, area);
        let title = screen.title.as_ref().map(|t| t.shown()).unwrap_or_default();
        let block = Block::bordered().title(title);
        let inner = block.inner(area);
        f.render_widget(block, area);
        let mut bar = affordance_lines(screen, usize::from(inner.width));
        bar.truncate(BAR_LINES);
        let prompt = self.prompt(screen);
        let [body, prompt_area, bar_area] = Layout::vertical([
            Constraint::Min(1),
            Constraint::Length(u16::from(prompt.is_some())),
            Constraint::Length(bar.len() as u16),
        ])
        .areas(inner);
        if let Some(root) = &screen.root {
            let mut painter = Painter {
                model,
                screen,
                colors: &self.colors,
                ui: &mut self.ui,
                extents: &mut self.extents,
            };
            painter.component(f, root, body);
        }
        if let Some(prompt) = prompt {
            f.render_widget(Paragraph::new(prompt), prompt_area);
        }
        f.render_widget(Paragraph::new(bar), bar_area);
    }

    /// What is being typed, else what the last key was refused for.
    fn prompt(&self, screen: &Screen) -> Option<Line<'static>> {
        if let Some(c) = &self.collecting {
            let label = screen
                .affordances
                .iter()
                .find(|a| a.id == c.affordance)
                .map(|a| a.label.shown())
                .unwrap_or_default();
            let typed = match &c.field {
                Some(FormField {
                    kind: FieldKind::Select,
                    options: Some(options),
                    ..
                }) => options
                    .iter()
                    .find(|o| o.id == c.typed)
                    .map_or(c.typed.clone(), |o| o.label.shown()),
                _ => c.typed.clone(),
            };
            return Some(Line::from(format!("{label}: {typed}_")));
        }
        let note = self.note.as_ref()?;
        Some(Line::styled(note.clone(), Style::new().fg(Color::Yellow)))
    }

    pub(super) fn key<M: GameModel>(&mut self, model: &mut M, key: KeyEvent) {
        self.sync(model);
        self.note = None;
        let step = match described(model.screens()) {
            Some(screen) => self.decide(&*model, screen, key),
            None => Step::Done,
        };
        match step {
            Step::Send(id, args) => {
                if let Err(e) = model.invoke_affordance(&id, args) {
                    self.note = Some(refusal(e));
                }
            }
            Step::Note(text) => self.note = Some(text),
            Step::Done => {}
        }
    }

    /// A key the screen binds goes to the engine; the others move the focus here.
    fn decide<M: GameModel>(&mut self, model: &M, screen: &Screen, key: KeyEvent) -> Step {
        let tok = token(key);
        if self.collecting.is_some() {
            return self.collect_key(screen, key, tok);
        }
        if let Some(t) = tok.as_deref()
            && interaction::binds_key(screen, t)
        {
            return match interaction::affordance_for_key(screen, t) {
                Some(a) => self.trigger(screen, &a.id),
                None => Step::Note(denial_of(screen, t)),
            };
        }
        // A screen with nothing to move in waits for any key.
        let idle = !nodes(screen)
            .iter()
            .any(|c| focusable(c) || matches!(c, Component::Tabs(_)));
        if idle
            && let Some(a) = tok
                .as_deref()
                .and_then(|t| interaction::affordance_for_key(screen, t))
            && a.role == Some(AffordanceRole::AnyKey)
        {
            return self.trigger(screen, &a.id);
        }
        match self.navigate(model, screen, key, tok.as_deref()) {
            Some(step) => step,
            None => match tok
                .as_deref()
                .and_then(|t| interaction::affordance_for_key(screen, t))
            {
                Some(a) => self.trigger(screen, &a.id),
                None => Step::Done,
            },
        }
    }

    /// Starts or finishes the arguments of an affordance, which the core gathers.
    fn trigger(&mut self, screen: &Screen, id: &str) -> Step {
        let Some(affordance) = screen.affordances.iter().find(|a| a.id == id) else {
            return Step::Done;
        };
        if !affordance.enabled {
            return Step::Note(
                affordance
                    .denial
                    .as_ref()
                    .map(|d| d.shown())
                    .unwrap_or_default(),
            );
        }
        let missing = self.ui.begin(affordance);
        if missing.is_empty() {
            return match self.ui.finish(affordance) {
                Ok(args) => Step::Send(id.into(), args),
                Err(missing) => Step::Note(format!("{missing:?}")),
            };
        }
        if !missing
            .iter()
            .all(|kind| matches!(kind, ArgKind::Count | ArgKind::Text))
        {
            self.ui.cancel();
            return Step::Note(format!("{missing:?}"));
        }
        let field = self.field_of(screen, affordance);
        let typed = match &field {
            Some(f) if f.kind != FieldKind::Key && missing.contains(&ArgKind::Text) => {
                self.ui.set_text(&f.value);
                f.value.clone()
            }
            _ => String::new(),
        };
        self.collecting = Some(Collect {
            affordance: id.into(),
            needs: missing,
            typed,
            field,
        });
        Step::Done
    }

    /// The form field a text goes to: the one the affordance names, else the one the focus is on.
    fn field_of(&self, screen: &Screen, affordance: &Affordance) -> Option<FormField> {
        let named = affordance
            .args
            .as_ref()
            .and_then(|a| a.text.as_ref())
            .and_then(|t| t.field_id.as_deref());
        let wanted = named.or(self.ui.focus().row_id.as_deref())?;
        nodes(screen).into_iter().find_map(|c| match c {
            Component::Form(form) => form.fields.iter().find(|f| f.id == wanted).cloned(),
            _ => None,
        })
    }

    fn collect_key(&mut self, screen: &Screen, key: KeyEvent, tok: Option<String>) -> Step {
        let Some(collect) = self.collecting.as_mut() else {
            return Step::Done;
        };
        let kind = collect.field.as_ref().map(|f| f.kind);
        let finish = key.code == KeyCode::Enter || kind == Some(FieldKind::Key);
        match key.code {
            KeyCode::Esc => {
                self.ui.cancel();
                self.collecting = None;
                return Step::Done;
            }
            _ if kind == Some(FieldKind::Key) => {
                if let Some(t) = &tok {
                    self.ui.set_text(t);
                }
            }
            KeyCode::Backspace => {
                self.ui.pop_count();
                self.ui.pop_text();
                collect.typed.pop();
            }
            KeyCode::Left | KeyCode::Right if kind == Some(FieldKind::Select) => {
                let options = collect
                    .field
                    .iter()
                    .flat_map(|f| f.options.iter().flatten());
                let ids = options.map(|o| o.id.as_str()).collect::<Vec<_>>();
                let at = ids.iter().position(|id| *id == collect.typed);
                let by = if key.code == KeyCode::Left { -1 } else { 1 };
                let next = at.map_or(0, |at| {
                    at.saturating_add_signed(by)
                        .min(ids.len().saturating_sub(1))
                });
                if let Some(id) = ids.get(next) {
                    collect.typed = id.to_string();
                    self.ui.set_text(id);
                }
            }
            KeyCode::Char(c) if collect.needs.contains(&ArgKind::Text) => {
                self.ui.push_text(&c.to_string());
                collect.typed.push(c);
            }
            KeyCode::Char(c) => {
                if self.ui.push_count(c) {
                    collect.typed.push(c);
                }
            }
            _ => {}
        }
        if !finish {
            return Step::Done;
        }
        let Some(affordance) = screen
            .affordances
            .iter()
            .find(|a| Some(&a.id) == self.collecting.as_ref().map(|c| &c.affordance))
        else {
            return Step::Done;
        };
        match self.ui.finish(affordance) {
            Ok(args) => {
                self.collecting = None;
                Step::Send(affordance.id.clone(), args)
            }
            Err(missing) => Step::Note(format!("{missing:?}")),
        }
    }

    /// The focus keys. `None`: the key is not one of them.
    fn navigate<M: GameModel>(
        &mut self,
        model: &M,
        screen: &Screen,
        key: KeyEvent,
        tok: Option<&str>,
    ) -> Option<Step> {
        let all = nodes(screen);
        let current = self.current(&all);
        let step = match key.code {
            KeyCode::Up => self.vertical(model, &all, current, -1),
            KeyCode::Down => self.vertical(model, &all, current, 1),
            KeyCode::PageUp => self.vertical(model, &all, current, -(PAGE as isize)),
            KeyCode::PageDown => self.vertical(model, &all, current, PAGE as isize),
            KeyCode::Home => self.vertical(model, &all, current, isize::MIN / 2),
            KeyCode::End => self.vertical(model, &all, current, isize::MAX / 2),
            KeyCode::Left => self.horizontal(screen, &all, current, -1),
            KeyCode::Right => self.horizontal(screen, &all, current, 1),
            KeyCode::Tab => self.cycle(model, &all, current, 1),
            KeyCode::BackTab => self.cycle(model, &all, current, -1),
            KeyCode::Enter => self.activate(screen, current),
            KeyCode::Esc => match interaction::with_role(screen, AffordanceRole::Cancel) {
                Some(a) => self.trigger(screen, &a.id),
                None => Step::Done,
            },
            KeyCode::Char(' ') => self.mark(current),
            KeyCode::Char(c) => return self.typed_key(model, screen, current, c, tok),
            _ => return None,
        };
        Some(step)
    }

    /// The component the focus is in, else the first one that takes it.
    fn current<'a>(&self, all: &[&'a Component]) -> Option<&'a Component> {
        let focused = all
            .iter()
            .find(|c| Some(c.id()) == self.ui.focus().component_id.as_deref());
        focused
            .into_iter()
            .chain(all)
            .find(|c| focusable(c))
            .copied()
    }

    fn vertical<M: GameModel>(
        &mut self,
        model: &M,
        all: &[&Component],
        current: Option<&Component>,
        by: isize,
    ) -> Step {
        match current {
            Some(Component::List(list)) => {
                let rows = model.rows(list);
                let id = &list.head.id;
                self.ui.move_focus(id, &rows, by);
                let at = self
                    .ui
                    .focus()
                    .row_id
                    .as_ref()
                    .and_then(|row| rows.iter().position(|r| &r.id == row));
                let visible = self.extents.get(id).map_or(PAGE, |w| w.visible);
                if let Some(at) = at {
                    let window = Window {
                        total: rows.len(),
                        visible,
                    };
                    self.ui.reveal(id, at, window);
                }
            }
            Some(Component::Form(form)) => self.move_field(form, by.signum()),
            Some(Component::Grid(grid)) => self.move_cell(grid, 0, by.signum() as i32),
            _ => {
                let text = all.iter().find(|c| matches!(c, Component::Text(_)));
                if let Some((id, window)) =
                    text.and_then(|c| Some((c.id(), self.extents.get(c.id())?)))
                {
                    self.ui
                        .scroll_by(id, by.clamp(-(PAGE as isize), PAGE as isize), *window);
                }
            }
        }
        Step::Done
    }

    fn horizontal(
        &mut self,
        screen: &Screen,
        all: &[&Component],
        current: Option<&Component>,
        by: isize,
    ) -> Step {
        let tabs = all.iter().find_map(|c| match c {
            Component::Tabs(tabs) => Some(tabs),
            _ => None,
        });
        if let Some(tabs) = tabs {
            let at = tabs.tabs.iter().position(|t| t.selected).unwrap_or(0);
            let next = match by < 0 {
                true => tabs.tabs[..at].iter().rev().find(|t| t.enabled),
                false => tabs.tabs.iter().skip(at + 1).find(|t| t.enabled),
            };
            return match (next, &tabs.select) {
                (Some(tab), Some(select)) => {
                    self.ui.focus_row(&tabs.head.id, &tab.id);
                    self.trigger(screen, select)
                }
                _ => Step::Done,
            };
        }
        if let Some(Component::Grid(grid)) = current {
            self.move_cell(grid, by as i32, 0);
        }
        Step::Done
    }

    /// Moves to the next component that takes the focus, onto its first row.
    fn cycle<M: GameModel>(
        &mut self,
        model: &M,
        all: &[&Component],
        current: Option<&Component>,
        by: isize,
    ) -> Step {
        let takers = all.iter().filter(|c| focusable(c)).collect::<Vec<_>>();
        let at = takers
            .iter()
            .position(|c| Some(c.id()) == current.map(|c| c.id()));
        let next = at.map_or(0, |at| {
            (at as isize + by).rem_euclid(takers.len() as isize) as usize
        });
        match takers.get(next) {
            Some(Component::List(list)) => {
                self.ui.move_focus(&list.head.id, &model.rows(list), 0);
            }
            Some(Component::Form(form)) => {
                if let Some(field) = form.fields.first() {
                    self.ui.focus_row(&form.head.id, &field.id);
                }
            }
            Some(Component::Grid(grid)) => {
                if let Some(id) = grid.cells.iter().find_map(|c| c.id.as_deref()) {
                    self.ui.focus_row(&grid.head.id, id);
                }
            }
            _ => {}
        }
        Step::Done
    }

    fn move_field(&mut self, form: &Form, by: isize) {
        let at = self
            .ui
            .focus()
            .row_id
            .as_ref()
            .filter(|_| self.ui.focus().component_id.as_ref() == Some(&form.head.id))
            .and_then(|id| form.fields.iter().position(|f| &f.id == id));
        let last = form.fields.len().saturating_sub(1);
        let next = match (at, by < 0) {
            (Some(at), _) => at.saturating_add_signed(by).min(last),
            (None, true) => last,
            (None, false) => 0,
        };
        if let Some(field) = form.fields.get(next) {
            self.ui.focus_row(&form.head.id, &field.id);
        }
    }

    /// The cell the focus is on, else the engine's cursor, else the first.
    fn cell_at<'a>(&self, grid: &'a Grid) -> Option<&'a GridCell> {
        let focused = self
            .ui
            .focus()
            .row_id
            .as_deref()
            .filter(|_| self.ui.focus().component_id.as_deref() == Some(grid.head.id.as_str()));
        let by_id = focused.and_then(|id| grid.cells.iter().find(|c| c.id.as_deref() == Some(id)));
        let by_cursor = grid.cursor.and_then(|at| {
            grid.cells
                .iter()
                .find(|c| i64::from(c.x) == i64::from(at.x) && i64::from(c.y) == i64::from(at.y))
        });
        by_id
            .or(by_cursor)
            .or_else(|| grid.cells.iter().find(|c| c.id.is_some()))
    }

    fn move_cell(&mut self, grid: &Grid, dx: i32, dy: i32) {
        let Some(from) = self.cell_at(grid) else {
            return;
        };
        let (x, y) = (
            i64::from(from.x) + i64::from(dx),
            i64::from(from.y) + i64::from(dy),
        );
        let to = grid
            .cells
            .iter()
            .find(|c| i64::from(c.x) == x && i64::from(c.y) == y && c.id.is_some());
        if let Some(id) = to.and_then(|c| c.id.as_deref()) {
            self.ui.focus_row(&grid.head.id, id);
        }
    }

    /// Enter on the component the focus is in: its activate, submit or cell affordance.
    fn activate(&mut self, screen: &Screen, current: Option<&Component>) -> Step {
        let id = match current {
            Some(Component::List(list)) => list.activate.clone(),
            Some(Component::Form(form)) => {
                if self.ui.focus().component_id.as_ref() != Some(&form.head.id) {
                    self.move_field(form, 0);
                }
                form.submit.clone()
            }
            Some(Component::Grid(grid)) => {
                let cell = self.cell_at(grid);
                if let Some(id) = cell.and_then(|c| c.id.as_deref()) {
                    self.ui.focus_row(&grid.head.id, id);
                }
                cell.and_then(|c| c.affordance.clone())
            }
            _ => interaction::with_role(screen, AffordanceRole::Confirm).map(|a| a.id.clone()),
        };
        match id {
            Some(id) => self.trigger(screen, &id),
            None => Step::Done,
        }
    }

    /// Space marks the focused row of a list that takes several.
    fn mark(&mut self, current: Option<&Component>) -> Step {
        if let Some(Component::List(list)) = current
            && matches!(list.selection, Selection::Multi | Selection::Count)
            && self.ui.focus().component_id.as_ref() == Some(&list.head.id)
            && let Some(row) = self.ui.focus().row_id.clone()
        {
            self.ui.toggle_pick(&list.head.id, &row);
        }
        Step::Done
    }

    /// A character: a row's hotkey, or the first digit of a count.
    fn typed_key<M: GameModel>(
        &mut self,
        model: &M,
        screen: &Screen,
        current: Option<&Component>,
        c: char,
        tok: Option<&str>,
    ) -> Option<Step> {
        let Some(Component::List(list)) = current else {
            return None;
        };
        let hotkey = model
            .rows(list)
            .into_iter()
            .find(|row| row.enabled && row.hotkey.as_deref() == tok);
        if let Some(row) = hotkey {
            self.ui.focus_row(&list.head.id, &row.id);
            return Some(match &list.activate {
                Some(id) => self.trigger(screen, id),
                None => Step::Done,
            });
        }
        if list.selection != Selection::Count || !c.is_ascii_digit() {
            return None;
        }
        let counted = screen.affordances.iter().find(|a| {
            list.head.affordances.contains(&a.id)
                && a.args.as_ref().is_some_and(|s| s.count.is_some())
        })?;
        let step = self.trigger(screen, &counted.id);
        if let Some(collect) = self.collecting.as_mut()
            && self.ui.push_count(c)
        {
            collect.typed.push(c);
        }
        Some(step)
    }
}

fn focusable(c: &Component) -> bool {
    matches!(
        c,
        Component::List(_) | Component::Form(_) | Component::Grid(_)
    )
}

/// Why an affordance did not go out: the engine's own reason, else the core's.
fn refusal(e: InvokeError) -> String {
    match e {
        InvokeError::Disabled(Some(reason)) => reason.shown(),
        e => format!("{e:?}"),
    }
}

/// The reason the affordances bound to `key` are off.
fn denial_of(screen: &Screen, key: &str) -> String {
    screen
        .affordances
        .iter()
        .filter(|a| a.keys.iter().any(|k| k == key))
        .find_map(|a| a.denial.as_ref().map(|d| d.shown()))
        .unwrap_or_default()
}

/// Each affordance with its keys, the off ones dim with their reason, flowed into lines.
fn affordance_lines(screen: &Screen, width: usize) -> Vec<Line<'static>> {
    let mut lines: Vec<Vec<Span<'static>>> = vec![vec![]];
    let mut used = 0;
    for a in &screen.affordances {
        let keys = match a.keys.is_empty() {
            true => String::new(),
            false => format!(" [{}]", a.keys.join(",")),
        };
        let reason = match (&a.denial, a.enabled) {
            (Some(denial), false) if !denial.shown().is_empty() => format!(" ({})", denial.shown()),
            _ => String::new(),
        };
        let text = format!("{}{keys}{reason}", a.label.shown());
        let style = match a.enabled {
            true => Style::new(),
            false => Style::new().fg(Color::DarkGray),
        };
        let size = text.chars().map(|c| c.width().unwrap_or(0)).sum::<usize>();
        if used > 0 && used + size + 2 > width {
            lines.push(vec![]);
            used = 0;
        }
        let line = lines.last_mut().expect("starts with one line");
        if used > 0 {
            line.push(Span::raw("  "));
        }
        line.push(Span::styled(text, style));
        used += size + usize::from(used > 0) * 2;
    }
    lines
        .into_iter()
        .filter(|line| !line.is_empty())
        .map(Line::from)
        .collect()
}

/// `text` cut to `width` terminal columns and padded with spaces to it.
fn fit(text: &str, width: usize) -> String {
    let mut out = String::new();
    let mut used = 0;
    for c in text.chars() {
        let w = c.width().unwrap_or(0);
        if used + w > width {
            break;
        }
        out.push(c);
        used += w;
    }
    out + &" ".repeat(width - used)
}

/// `spans` cut to `width` terminal columns and padded with `pad` to it.
fn clip(spans: Vec<Span<'static>>, width: usize, pad: Style) -> Vec<Span<'static>> {
    let mut left = width;
    let mut out = vec![];
    'spans: for span in spans {
        let mut cut = String::new();
        for c in span.content.chars() {
            let w = c.width().unwrap_or(0);
            if w > left {
                out.push(Span::styled(cut, span.style));
                break 'spans;
            }
            cut.push(c);
            left -= w;
        }
        out.push(Span::styled(cut, span.style));
    }
    out.push(Span::styled(" ".repeat(left), pad));
    out
}

/// Characters with the colour they were marked in, broken into lines of at most `width` columns at spaces
/// where there are any.
fn flow(runs: &[Run], width: usize) -> Vec<Vec<(char, Option<Rgb>)>> {
    let chars = runs
        .iter()
        .flat_map(|run| run.text.chars().map(|c| (c, run.color)))
        .collect::<Vec<_>>();
    chars
        .split(|(c, _)| *c == '\n')
        .flat_map(|line| flow_line(line, width.max(1)))
        .collect()
}

fn flow_line(line: &[(char, Option<Rgb>)], width: usize) -> Vec<Vec<(char, Option<Rgb>)>> {
    let mut out = vec![];
    let mut rest = line;
    loop {
        let mut used = 0;
        let fits = rest.iter().position(|(c, _)| {
            used += c.width().unwrap_or(0);
            used > width
        });
        let Some(end) = fits else {
            out.push(rest.to_vec());
            return out;
        };
        let end = end.max(1);
        let cut = rest[..end]
            .iter()
            .rposition(|(c, _)| *c == ' ')
            .filter(|at| *at > 0)
            .unwrap_or(end);
        out.push(rest[..cut].to_vec());
        rest = &rest[cut..];
        if rest.first().is_some_and(|(c, _)| *c == ' ') {
            rest = &rest[1..];
        }
    }
}

fn line_of(chars: &[(char, Option<Rgb>)], base: Style) -> Line<'static> {
    let mut runs: Vec<(String, Option<Rgb>)> = vec![];
    for &(c, rgb) in chars {
        match runs.last_mut() {
            Some((text, last)) if *last == rgb => text.push(c),
            _ => runs.push((c.to_string(), rgb)),
        }
    }
    Line::from(
        runs.into_iter()
            .map(|(text, rgb)| Span::styled(text, rgb.map_or(base, |c| base.fg(color(c)))))
            .collect::<Vec<_>>(),
    )
}

struct Painter<'a, M> {
    model: &'a M,
    screen: &'a Screen,
    colors: &'a ColorMap,
    ui: &'a mut Interaction,
    extents: &'a mut HashMap<String, Window>,
}

impl<M: GameModel> Painter<'_, M> {
    fn focused(&self, id: &str) -> bool {
        self.ui.focus().component_id.as_deref() == Some(id)
    }

    /// The colour the engine named, as in a row or a cell.
    fn named(&self, name: &Option<String>) -> Style {
        match name {
            Some(name) => Style::new().fg(color(self.colors.resolve(name).0)),
            None => Style::new(),
        }
    }

    /// The game's colour markup in `text` as spans on `base`.
    fn marked(&self, text: &str, base: Style) -> Vec<Span<'static>> {
        self.model
            .styled(text)
            .into_iter()
            .map(|run| Span::styled(run.text, run.color.map_or(base, |c| base.fg(color(c)))))
            .collect()
    }

    fn component(&mut self, f: &mut Frame, c: &Component, area: Rect) {
        if area.width == 0 || area.height == 0 {
            return;
        }
        let head = c.head();
        let titled = !matches!(
            c,
            Component::Panel(_) | Component::Split(_) | Component::Canvas(_)
        );
        let area = match (&head.label, titled) {
            (Some(label), true) => {
                let mark = if self.focused(&head.id) { "> " } else { "" };
                let title = Line::styled(
                    format!("{mark}{}", label.shown()),
                    Style::new().add_modifier(Modifier::BOLD),
                );
                let [top, rest] =
                    Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).areas(area);
                f.render_widget(Paragraph::new(title), top);
                rest
            }
            _ => area,
        };
        match c {
            Component::List(list) => self.list(f, list, area),
            Component::Tabs(tabs) => self.tabs(f, tabs, area),
            Component::Split(split) => self.split(f, split, area),
            Component::Panel(panel) => self.panel(f, panel, area),
            Component::Form(form) => self.form(f, form, area),
            Component::Text(text) => self.text(f, text, area),
            Component::Grid(grid) => self.grid(f, grid, area),
            Component::Map(map) => self.map(f, map, area),
            Component::Canvas(canvas) => {
                let runs = canvas.rows.iter().map(|row| {
                    row.iter()
                        .map(|(text, fg, bg)| {
                            let style = Style::new().fg(color(self.colors.index(*fg)));
                            let style = match bg {
                                0 => style,
                                bg => style.bg(color(self.colors.index(*bg))),
                            };
                            Span::styled(text.clone(), style)
                        })
                        .collect::<Vec<_>>()
                });
                self.centered(f, runs.map(Line::from).collect(), canvas.cols, area);
            }
        }
    }

    /// `lines` centered in `area`, as much of them as it holds.
    fn centered(&self, f: &mut Frame, lines: Vec<Line<'static>>, cols: usize, area: Rect) {
        let width = u16::try_from(cols).map_or(area.width, |w| w.min(area.width));
        let height = u16::try_from(lines.len()).map_or(area.height, |h| h.min(area.height));
        let at = Rect::new(
            area.x + (area.width - width) / 2,
            area.y + (area.height - height) / 2,
            width,
            height,
        );
        f.render_widget(Paragraph::new(lines), at);
    }

    fn split(&mut self, f: &mut Frame, split: &Split, area: Rect) {
        let n = split.children.len().max(1);
        let side = usize::from(area.width) >= 30 * n;
        let cells = match side {
            true => Layout::horizontal(vec![Constraint::Fill(1); n]).split(area),
            false => Layout::vertical(vec![Constraint::Fill(1); n]).split(area),
        };
        for (child, cell) in split.children.iter().zip(cells.iter()) {
            self.component(f, child, *cell);
        }
    }

    fn tabs(&mut self, f: &mut Frame, tabs: &Tabs, area: Rect) {
        let [top, rest] = Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).areas(area);
        let labels = tabs.tabs.iter().flat_map(|tab| {
            let style = match (tab.selected, tab.enabled) {
                (true, _) => Style::new().add_modifier(Modifier::REVERSED),
                (false, true) => Style::new(),
                (false, false) => Style::new().fg(Color::DarkGray),
            };
            [
                Span::styled(format!(" {} ", tab.label.shown()), style),
                Span::raw(" "),
            ]
        });
        f.render_widget(Paragraph::new(Line::from(labels.collect::<Vec<_>>())), top);
        if let Some(child) = &tabs.child {
            self.component(f, child, rest);
        }
    }

    fn panel(&mut self, f: &mut Frame, panel: &Panel, area: Rect) {
        let block = Block::bordered().title(panel.title.shown());
        let inner = block.inner(area);
        f.render_widget(block, area);
        let mut lines = panel
            .subtitle
            .iter()
            .map(|s| Line::styled(s.shown(), Style::new().fg(Color::DarkGray)))
            .collect::<Vec<_>>();
        for stat in panel.stats.iter().flatten() {
            let style = match stat.tone {
                Some(Tone::Good) => Style::new().fg(Color::Green),
                Some(Tone::Bad) => Style::new().fg(Color::Red),
                None => Style::new(),
            };
            let max = stat
                .max
                .iter()
                .map(|m| format!("/{}", m.shown()))
                .collect::<String>();
            let unit = stat
                .unit
                .iter()
                .map(|u| format!(" {}", u.shown()))
                .collect::<String>();
            lines.push(Line::from(vec![
                Span::raw(format!("{}: ", stat.label.shown())),
                Span::styled(format!("{}{max}{unit}", stat.value.shown()), style),
            ]));
        }
        let [head, rest] = Layout::vertical([
            Constraint::Length(lines.len().min(usize::from(inner.height)) as u16),
            Constraint::Min(0),
        ])
        .areas(inner);
        f.render_widget(Paragraph::new(lines), head);
        if let Some(child) = &panel.child {
            self.component(f, child, rest);
        }
    }

    fn form(&mut self, f: &mut Frame, form: &Form, area: Rect) {
        let here = self.focused(&form.head.id);
        let on = self.ui.focus().row_id.clone().filter(|_| here);
        let mut lines = vec![];
        for field in &form.fields {
            let current = on.as_deref() == Some(field.id.as_str());
            let shown = match (&field.kind, &field.options) {
                (FieldKind::Select, Some(options)) => options
                    .iter()
                    .find(|o| o.id == field.value)
                    .map_or(field.value.clone(), |o| o.label.shown()),
                _ => field.value.clone(),
            };
            let range = match (&field.min, &field.max) {
                (Some(min), Some(max)) => format!(" [{min}..{max}]"),
                _ => String::new(),
            };
            let style = match current {
                true => Style::new().add_modifier(Modifier::REVERSED),
                false => Style::new(),
            };
            let mark = if current { "> " } else { "  " };
            lines.push(Line::styled(
                format!("{mark}{}: {shown}{range}", field.label.shown()),
                style,
            ));
        }
        let detail = form
            .fields
            .iter()
            .find(|field| on.as_deref() == Some(field.id.as_str()))
            .and_then(|field| field.description.as_ref())
            .map(|d| d.shown())
            .unwrap_or_default();
        let height = lines.len().min(usize::from(area.height)) as u16;
        let [fields, rest] =
            Layout::vertical([Constraint::Length(height), Constraint::Min(0)]).areas(area);
        f.render_widget(Paragraph::new(lines), fields);
        let runs = [Run {
            text: detail,
            color: None,
        }];
        let wrapped = flow(&runs, usize::from(rest.width));
        let dim = Style::new().fg(Color::DarkGray);
        let lines = wrapped.iter().map(|l| line_of(l, dim)).collect::<Vec<_>>();
        f.render_widget(Paragraph::new(lines), rest);
    }

    fn text(&mut self, f: &mut Frame, text: &TextBlock, area: Rect) {
        let base = match (text.tone, text.role) {
            (Some(Tone::Good), _) => Style::new().fg(Color::Green),
            (Some(Tone::Bad), _) => Style::new().fg(Color::Red),
            (None, Some(TextRole::TerminalOutput)) => Style::new().fg(Color::Green),
            (None, Some(TextRole::Banner)) => Style::new().add_modifier(Modifier::BOLD),
            (None, None) => Style::new(),
        };
        let runs = self.model.styled(&text.markup.shown());
        let lines = flow(&runs, usize::from(area.width));
        let window = Window {
            total: lines.len(),
            visible: usize::from(area.height),
        };
        let id = &text.head.id;
        self.ui.scroll_by(id, 0, window);
        self.extents.insert(id.clone(), window);
        let top = self.ui.scroll(id);
        let shown = lines
            .iter()
            .skip(top)
            .take(usize::from(area.height))
            .map(|l| line_of(l, base))
            .collect::<Vec<_>>();
        f.render_widget(Paragraph::new(shown), area);
    }

    fn grid(&mut self, f: &mut Frame, grid: &Grid, area: Rect) {
        let cols = (grid.cols as usize).min(usize::from(area.width));
        let rows = (grid.rows as usize).min(usize::from(area.height));
        let here = self.focused(&grid.head.id);
        let on = self.ui.focus().row_id.clone().filter(|_| here);
        let mut cells = vec![vec![Span::raw(" "); cols]; rows];
        let mut described: Option<&GridCell> = None;
        for cell in &grid.cells {
            let (x, y) = (cell.x as usize, cell.y as usize);
            if x >= cols || y >= rows {
                continue;
            }
            let glyph = cell
                .glyph
                .as_deref()
                .and_then(|g| g.chars().next())
                .unwrap_or(' ');
            let mut style = self.named(&cell.color);
            if cell.selected == Some(true) {
                style = style.add_modifier(Modifier::BOLD);
            }
            if !cell.enabled {
                style = style.add_modifier(Modifier::DIM);
            }
            let at_cursor = grid
                .cursor
                .is_some_and(|c| i64::from(c.x) == x as i64 && i64::from(c.y) == y as i64);
            let focused = cell.id.is_some() && cell.id == on;
            if focused || (at_cursor && on.is_none()) {
                style = style.add_modifier(Modifier::REVERSED);
                described = Some(cell);
            }
            cells[y][x] = Span::styled(glyph.to_string(), style);
        }
        let height = rows.min(usize::from(area.height).saturating_sub(1)) as u16;
        let [body, foot] =
            Layout::vertical([Constraint::Length(height), Constraint::Length(1)]).areas(area);
        let lines = cells.into_iter().map(Line::from).collect::<Vec<_>>();
        f.render_widget(Paragraph::new(lines), body);
        let line = described.map(|cell| {
            let label = cell.label.iter().map(|l| l.shown()).collect::<String>();
            match (&cell.denial, cell.enabled) {
                (Some(denial), false) => Line::styled(
                    format!("{label} ({})", denial.shown()),
                    Style::new().fg(Color::DarkGray),
                ),
                _ => Line::raw(label),
            }
        });
        if let Some(line) = line {
            f.render_widget(Paragraph::new(line), foot);
        }
    }

    /// The overmap the engine drew in curses cells with what it says of the cursor square; the world map
    /// is the 0.1 view's, so it is an empty frame here.
    fn map(&mut self, f: &mut Frame, map: &MapComponent, area: Rect) {
        let block = Block::bordered();
        let inner = block.inner(area);
        f.render_widget(block, area);
        let Some(om) = &map.overmap else {
            return;
        };
        let mut notes = om.legend.clone();
        notes.extend(om.cursor_here.clone());
        notes.push(om.cursor_description.clone());
        notes.extend(om.cursor_note.clone());
        notes.extend(om.route_text.clone());
        let notes = notes
            .into_iter()
            .filter(|n| !n.is_empty())
            .collect::<Vec<_>>();
        let rows = om
            .cells
            .len()
            .min(usize::from(inner.height).saturating_sub(notes.len()).max(1))
            as u16;
        let [grid, foot] =
            Layout::vertical([Constraint::Length(rows), Constraint::Min(0)]).areas(inner);
        let lines = om.cells.iter().map(|row| {
            Line::from(
                row.iter()
                    .map(|(glyph, fg, bg)| {
                        let style = Style::new().fg(color(self.colors.index(*fg)));
                        let style = match bg {
                            0 => style,
                            bg => style.bg(color(self.colors.index(*bg))),
                        };
                        Span::styled(glyph.clone(), style)
                    })
                    .collect::<Vec<_>>(),
            )
        });
        f.render_widget(Paragraph::new(lines.collect::<Vec<_>>()), grid);
        f.render_widget(
            Paragraph::new(notes.into_iter().map(Line::from).collect::<Vec<_>>()),
            foot,
        );
    }

    fn list(&mut self, f: &mut Frame, list: &List, area: Rect) {
        let model = self.model;
        let rows = model.rows(list);
        let id = &list.head.id;
        let here = self.focused(id);
        let on = self.ui.focus().row_id.clone().filter(|_| here);
        let focused_row = rows
            .iter()
            .find(|r| on.as_deref() == Some(r.id.as_str()))
            .copied();
        let note = focused_row.map(|row| self.detail(row)).unwrap_or_default();
        let note_height = note.len().min(usize::from(area.height).saturating_sub(3)) as u16;
        let header = u16::from(!list.columns.is_empty());
        let [head, body, foot, info] = Layout::vertical([
            Constraint::Length(header.min(area.height)),
            Constraint::Min(0),
            Constraint::Length(note_height),
            Constraint::Length(u16::from(area.height > 2)),
        ])
        .areas(area);
        let visible = usize::from(body.height);
        let window = Window {
            total: rows.len(),
            visible,
        };
        match on
            .as_ref()
            .and_then(|on| rows.iter().position(|r| &r.id == on))
        {
            Some(at) => self.ui.reveal(id, at, window),
            None => self.ui.scroll_by(id, 0, window),
        }
        self.extents.insert(id.clone(), window);
        let top = self.ui.scroll(id);
        let shown = rows
            .iter()
            .skip(top)
            .take(visible)
            .copied()
            .collect::<Vec<_>>();
        let widths = list
            .columns
            .iter()
            .enumerate()
            .map(|(i, column)| {
                let values = shown
                    .iter()
                    .map(|row| cell_text(row, i, &column.key).chars().count());
                values
                    .chain([column.label.shown().chars().count()])
                    .max()
                    .unwrap_or(0)
                    .min(COLUMN_MAX)
            })
            .collect::<Vec<_>>();
        let side = widths.iter().map(|w| w + 1).sum::<usize>();
        let label_width = usize::from(area.width).saturating_sub(side + 6).max(1);
        let bold = Style::new().add_modifier(Modifier::BOLD);
        if header > 0 {
            let mut spans = vec![Span::styled(fit("", 6 + label_width), bold)];
            for (column, width) in list.columns.iter().zip(&widths) {
                spans.push(Span::styled(
                    fit(&format!(" {}", column.label.shown()), width + 1),
                    bold,
                ));
            }
            f.render_widget(Paragraph::new(Line::from(spans)), head);
        }
        let any_hotkey = shown.iter().any(|r| r.hotkey.is_some());
        let lines = shown
            .iter()
            .map(|row| {
                let current = on.as_deref() == Some(row.id.as_str());
                self.row_line(
                    list,
                    row,
                    RowDraw {
                        current,
                        any_hotkey,
                        label_width,
                        widths: &widths,
                    },
                )
            })
            .collect::<Vec<_>>();
        f.render_widget(Paragraph::new(lines), body);
        f.render_widget(Paragraph::new(note), foot);
        let total = list.total as usize;
        let last = list.offset.unwrap_or(0) as usize + top + shown.len();
        let first = list.offset.unwrap_or(0) as usize + top + 1;
        if !shown.is_empty() {
            let more = if rows.len() < total { "+" } else { "" };
            let text = format!("{first}-{last}/{total}{more}");
            f.render_widget(
                Paragraph::new(Line::styled(text, Style::new().fg(Color::DarkGray))),
                info,
            );
        }
    }

    /// What the focused row says: why it is off, why its affordances are, else its description.
    fn detail(&self, row: &Row) -> Vec<Line<'static>> {
        let mut lines = vec![];
        if !row.enabled && !row.denial.shown().is_empty() {
            lines.push(Line::styled(
                row.denial.shown(),
                Style::new().fg(Color::Red),
            ));
        }
        for allowed in row.affordances.iter().flatten().filter(|a| !a.enabled) {
            let label = self
                .screen
                .affordances
                .iter()
                .find(|a| a.id == allowed.id)
                .map(|a| a.label.shown())
                .unwrap_or_default();
            if let Some(denial) = allowed.denial.as_ref().filter(|d| !d.is_empty()) {
                lines.push(Line::styled(
                    format!("{label}: {denial}"),
                    Style::new().fg(Color::DarkGray),
                ));
            }
        }
        let description = row.description.shown();
        if !description.is_empty() {
            let dim = Style::new().fg(Color::DarkGray);
            let runs = [Run {
                text: description,
                color: None,
            }];
            lines.extend(flow(&runs, 60).iter().map(|l| line_of(l, dim)));
        }
        lines
    }

    fn row_line(&self, list: &List, row: &Row, draw: RowDraw) -> Line<'static> {
        let RowDraw {
            current,
            any_hotkey,
            label_width,
            widths,
        } = draw;
        let mut style = self.named(&row.color);
        if row.header == Some(true) {
            style = style.add_modifier(Modifier::BOLD);
        }
        if !row.enabled {
            style = style.fg(Color::DarkGray);
        }
        if current {
            style = style.add_modifier(Modifier::REVERSED);
        }
        let picked = self.ui.picks(&list.head.id).iter().any(|p| p.id == row.id);
        let boxed = matches!(list.selection, Selection::Multi | Selection::Count);
        let mark = match (&row.mark, boxed) {
            (Some(mark), _) => format!("{} ", mark.glyph),
            (None, true) if picked => "x ".into(),
            (None, true) => "- ".into(),
            (None, false) => String::new(),
        };
        let hotkey = match (&row.hotkey, any_hotkey) {
            (Some(key), _) => format!("{key} "),
            (None, true) => "  ".into(),
            (None, false) => String::new(),
        };
        let cursor = if current { "> " } else { "  " };
        let indent = if row.parent_id.is_some() { "  " } else { "" };
        let count = row
            .max
            .map(|max| format!(" {}/{max}", row.selected_count.unwrap_or(0)))
            .unwrap_or_default();
        let star = if row.favorite == Some(true) { " *" } else { "" };
        let prefix = format!("{cursor}{mark}{hotkey}{indent}");
        let text = row.markup.clone().unwrap_or_else(|| row.label.shown());
        let mut label = self.marked(&text, style);
        label.push(Span::styled(format!("{count}{star}"), style));
        let used = prefix
            .chars()
            .map(|c| c.width().unwrap_or(0))
            .sum::<usize>();
        let mut spans = vec![Span::styled(prefix, style)];
        spans.extend(clip(label, (label_width + 6).saturating_sub(used), style));
        for (i, (column, width)) in list.columns.iter().zip(widths).enumerate() {
            let value = cell_text(row, i, &column.key);
            let tint = row_column(row, i, &column.key)
                .map_or(style, |c| style.patch(self.named(&c.color)));
            spans.push(Span::styled(fit(&format!(" {value}"), width + 1), tint));
        }
        Line::from(spans)
    }
}

struct RowDraw<'a> {
    current: bool,
    any_hotkey: bool,
    label_width: usize,
    widths: &'a [usize],
}

/// The row's value for a list column: the one with its key, else the one at the column's position.
fn row_column<'a>(row: &'a Row, at: usize, key: &str) -> Option<&'a RowColumn> {
    row.columns
        .iter()
        .find(|c| c.key.as_deref() == Some(key))
        .or_else(|| row.columns.get(at).filter(|c| c.key.is_none()))
}

fn cell_text(row: &Row, at: usize, key: &str) -> String {
    row_column(row, at, key)
        .map(|c| c.value.shown())
        .unwrap_or_default()
}
