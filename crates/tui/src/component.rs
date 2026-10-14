//! The engine's 0.2 screens in the terminal: every component kind laid out generically, and the screen's
//! affordances with their keys and denials. This module only draws and moves the client-local focus; the
//! core (`interaction::*`) gathers every argument and the model sends every command.

use super::settings::engine_key;
use super::{color, token};
use bn_client_core::colors::ColorMap;
use bn_client_core::interaction::{self, ArgKind, Interaction, InvokeError, Reason, Window};
use bn_client_core::markup::Run;
use bn_client_core::protocol::{
    AffordanceRole, Component, FieldKind, Form, Grid, GridCell, InvokeArgs, List, MapComponent,
    Panel, Row, RowColumn, Screen, ScreenDock, ScreenRole, Selection, Split, Tabs, TextBlock,
    TextRole, Tone, WorldPos,
};
use bn_client_core::{GameModel, Rgb};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
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

/// The area of a window docked at `dock`: half the terminal, against the sidebar's inner edge on the sidebar's
/// side, else against the terminal's edge; `overlapping` starts where the sidebar does.
fn dock_area(full: Rect, dock: ScreenDock, sidebar: Option<(Rect, bool)>) -> Rect {
    let half = full.width / 2;
    let (left_edge, right_edge) = match sidebar {
        Some((side, true)) => (side.x + side.width, full.x + full.width),
        Some((side, false)) => (full.x, side.x),
        None => (full.x, full.x + full.width),
    };
    let width = |room: u16| half.min(room);
    match dock {
        ScreenDock::Right => {
            let w = width(right_edge.saturating_sub(full.x));
            Rect::new(right_edge - w, full.y, w, full.height)
        }
        ScreenDock::Overlapping => {
            let x = sidebar.map_or(full.x, |(side, _)| side.x);
            Rect::new(x, full.y, width(full.x + full.width - x), full.height)
        }
        _ => Rect::new(
            left_edge,
            full.y,
            width(full.x + full.width - left_edge),
            full.height,
        ),
    }
}

/// The client-local state of the described screens: the core's focus and ask, and how much each scrolling
/// component showed.
#[derive(Default)]
pub(super) struct View {
    ui: Interaction,
    /// What the last key was refused for, until the next key.
    note: Option<String>,
    /// How much of each list and text the last frame showed.
    extents: HashMap<String, Window>,
    /// The text of the prompt on top, which goes out as it is typed.
    typing: Option<Typing>,
    /// The sidebar of the last frame and whether it is on the left: a docked window sits beside its inner
    /// edge.
    sidebar: Option<(Rect, bool)>,
    /// Where the overmap's squares were drawn in the last frame, for a click.
    overmap: Option<OvermapHit>,
}

/// The overmap squares of one frame: where they are on the terminal and which squares they are.
struct OvermapHit {
    area: Rect,
    /// The cell of the engine's squares drawn in the top-left corner of `area`.
    first: (usize, usize),
    origin: WorldPos,
    cursor: WorldPos,
    /// The affordance that puts the cursor on a square, and the one a click on the cursor runs.
    mover: Option<String>,
    runner: Option<String>,
}

/// A text prompt the engine confirms with Return: a list filter, a name. Its text goes out with each key.
struct Typing {
    screen: String,
    form: String,
    field: String,
    fill: String,
    submit: String,
    text: String,
    /// The text last sent, so it goes out once.
    handed: String,
    /// The next key names a binding, as the engine writes it, and is submitted at once.
    named: bool,
    /// Return was pressed and the text is to be submitted once the engine can take it.
    submitting: bool,
}

impl Typing {
    /// The prompt of `screen`, started from the engine's value for it.
    fn of(screen: &Screen) -> Option<Self> {
        nodes(screen).into_iter().find_map(|c| {
            let Component::Form(form) = c else {
                return None;
            };
            let submit = screen
                .affordances
                .iter()
                .find(|a| Some(&a.id) == form.submit.as_ref())
                .filter(|a| a.role == Some(AffordanceRole::Confirm))?;
            let field = form
                .fields
                .iter()
                .find(|f| matches!(f.kind, FieldKind::Text | FieldKind::Key))?;
            let fill = interaction::form_fill(screen, form, &field.id)?;
            Some(Self {
                screen: screen.screen_id.clone(),
                form: form.head.id.clone(),
                field: field.id.clone(),
                fill: fill.id.clone(),
                submit: submit.id.clone(),
                text: field.value.clone(),
                handed: field.value.clone(),
                named: field.kind == FieldKind::Key,
                submitting: false,
            })
        })
    }

    /// A key typed into the prompt; false when the screen's own keys take it.
    fn key(&mut self, key: KeyEvent) -> bool {
        if self.named {
            // Esc is left to cancel the prompt.
            let Some(name) = engine_key(key).filter(|_| key.code != KeyCode::Esc) else {
                return false;
            };
            self.text = name;
            self.submitting = true;
            return true;
        }
        let plain = !key
            .modifiers
            .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT);
        match key.code {
            KeyCode::Char(c) if plain => self.text.push(c),
            KeyCode::Backspace => {
                self.text.pop();
            }
            KeyCode::Enter => self.submitting = true,
            _ => return false,
        }
        true
    }
}

impl View {
    /// Where the sidebar was drawn, for the windows that dock beside it.
    pub(super) fn set_sidebar(&mut self, area: Rect, left: bool) {
        self.sidebar = Some((area, left));
    }

    /// Follows the stack the engine published.
    fn sync<M: GameModel>(&mut self, model: &mut M) {
        self.ui.sync(model.screens());
        if let Some((id, args)) = self.ui.take_select() {
            let _ = model.invoke_affordance(&id, args);
        }
        let top = described(model.screens());
        if self.typing.as_ref().map(|t| &t.screen) != top.map(|s| &s.screen_id) {
            self.typing = top.and_then(Typing::of);
        }
    }

    /// Sends the prompt's newest text, or submits it, once the engine can take it.
    fn send_typed<M: GameModel>(&mut self, model: &mut M) {
        let (Some(typing), Some(screen)) = (self.typing.as_mut(), described(model.screens()))
        else {
            return;
        };
        if model.busy() || (typing.text == typing.handed && !typing.submitting) {
            return;
        }
        let affordance = if typing.submitting {
            &typing.submit
        } else {
            &typing.fill
        };
        let out = self.ui.fill(
            screen,
            affordance,
            &typing.form,
            &typing.field,
            &typing.text,
        );
        typing.handed.clone_from(&typing.text);
        typing.submitting = false;
        if let Some((id, args)) = out.invoke {
            let _ = model.invoke_affordance(&id, args);
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
        self.send_typed(model);
        let model = &*model;
        let Some(screen) = described(model.screens()) else {
            return;
        };
        let full = f.area();
        let area = match (placement, screen.dock) {
            (Placement::Full, _) => full,
            (
                Placement::Over,
                Some(dock @ (ScreenDock::Left | ScreenDock::Right | ScreenDock::Overlapping)),
            ) => dock_area(full, dock, self.sidebar),
            (Placement::Over, _) => Rect::new(
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
            self.overmap = None;
            let mut painter = Painter {
                model,
                screen,
                colors: model.colors(),
                ui: &mut self.ui,
                extents: &mut self.extents,
                typing: self.typing.as_ref(),
                overmap: &mut self.overmap,
            };
            painter.component(f, root, body);
        }
        if let Some(prompt) = prompt {
            f.render_widget(Paragraph::new(prompt), prompt_area);
        }
        f.render_widget(Paragraph::new(bar), bar_area);
    }

    /// What the core is asking for, else what the last key was refused for.
    fn prompt(&self, screen: &Screen) -> Option<Line<'static>> {
        if let Some(ask) = self.ui.ask() {
            let label = screen
                .affordances
                .iter()
                .find(|a| a.id == ask.affordance_id)
                .map(|a| a.label.shown())
                .unwrap_or_default();
            let value = match ask.asking {
                Some(ArgKind::Count) => ask.count.map(|n| n.to_string()),
                Some(ArgKind::Text) => match &ask.select {
                    Some(select) => select.options.get(select.at).map(|o| o.label.shown()),
                    None => ask.text.clone(),
                },
                Some(ArgKind::Pane) => ask.panes.get(ask.pane_at).map(|id| {
                    nodes(screen)
                        .into_iter()
                        .find_map(|c| match c {
                            Component::Panel(p) if &p.head.id == id => Some(p.title.shown()),
                            _ => None,
                        })
                        .unwrap_or_else(|| id.clone())
                }),
                _ => None,
            };
            return Some(Line::from(format!(
                "{label}: {}_",
                value.unwrap_or_default()
            )));
        }
        let note = self.note.as_ref()?;
        Some(Line::styled(note.clone(), Style::new().fg(Color::Yellow)))
    }

    /// A click on an overmap square moves the engine's cursor there; a click on the cursor runs the map's
    /// other affordance (travel).
    pub(super) fn click<M: GameModel>(&mut self, model: &mut M, column: u16, row: u16) {
        let Some(hit) = &self.overmap else {
            return;
        };
        let area = hit.area;
        if !(area.x..area.x + area.width).contains(&column)
            || !(area.y..area.y + area.height).contains(&row)
        {
            return;
        }
        let at = WorldPos {
            dim: hit.origin.dim.clone(),
            x: hit.origin.x + (hit.first.0 + usize::from(column - area.x)) as i32,
            y: hit.origin.y + (hit.first.1 + usize::from(row - area.y)) as i32,
            z: hit.origin.z,
        };
        let (id, args) = match (&hit.runner, &hit.mover) {
            (Some(run), _) if at == hit.cursor => (run.clone(), InvokeArgs::default()),
            (_, Some(mover)) => (
                mover.clone(),
                InvokeArgs {
                    pos: Some(at),
                    ..InvokeArgs::default()
                },
            ),
            _ => return,
        };
        let _ = model.invoke_affordance(&id, args);
    }

    pub(super) fn key<M: GameModel>(&mut self, model: &mut M, key: KeyEvent) {
        self.sync(model);
        self.note = None;
        if self.ui.ask().is_none() && self.typing.as_mut().is_some_and(|typing| typing.key(key)) {
            // A named key goes out now, the typed text of a prompt with the next tick.
            if self.typing.as_ref().is_some_and(|typing| typing.named) {
                self.send_typed(model);
            }
            return;
        }
        let Some(name) = token(key) else {
            return;
        };
        let outcome = match described(model.screens()) {
            Some(screen) => {
                let outcome = self.ui.press(screen, &name, PAGE, &|list| model.rows(list));
                if !outcome.consumed {
                    self.scroll_text(screen, &name);
                }
                outcome
            }
            None => return,
        };
        if let Some((id, args)) = outcome.invoke {
            if let Err(e) = model.invoke_affordance(&id, args) {
                self.note = Some(match e {
                    InvokeError::Disabled(Some(reason)) => reason.shown(),
                    InvokeError::Disabled(None) => words(Reason::Disabled),
                    InvokeError::NoScreen => words(Reason::NoScreen),
                    InvokeError::Unknown(_) => words(Reason::Unknown),
                    InvokeError::Busy => words(Reason::Busy),
                    InvokeError::Args(_) => words(Reason::BadArgs),
                });
            }
        } else if let Some(denial) = outcome.denial {
            self.note = Some(denial.shown());
        } else if let Some(reason) = outcome.reason {
            self.note = Some(words(reason));
        }
    }
}

impl View {
    /// A key nothing took scrolls the first text, which the client shows in parts on its own.
    fn scroll_text(&mut self, screen: &Screen, key: &str) {
        let by = match key {
            "UP" => -1,
            "DOWN" => 1,
            "PPAGE" => -(PAGE as isize),
            "NPAGE" => PAGE as isize,
            "HOME" => isize::MIN / 2,
            "END" => isize::MAX / 2,
            _ => return,
        };
        let text = nodes(screen)
            .into_iter()
            .find(|c| matches!(c, Component::Text(_)));
        if let Some((id, window)) = text.and_then(|c| Some((c.id(), *self.extents.get(c.id())?))) {
            self.ui.scroll_by(id, by, window);
        }
    }
}

/// The client's words for a refusal the engine gave no reason for.
fn words(reason: Reason) -> String {
    match reason {
        Reason::NoScreen => "no screen",
        Reason::Unknown => "unknown action",
        Reason::Disabled => "unavailable",
        Reason::RowDisabled | Reason::NotOnThisRow => "not for this row",
        Reason::Missing(ArgKind::Row | ArgKind::Rows) => "choose a row",
        Reason::Missing(ArgKind::Count) => "enter a count",
        Reason::Missing(ArgKind::Text) => "enter text",
        Reason::Missing(ArgKind::Pos) => "choose a square",
        Reason::Missing(ArgKind::Pane) => "choose a pane",
        Reason::Busy => "busy",
        Reason::BadArgs => "wrong arguments",
    }
    .into()
}

/// Each affordance with its keys, the off ones dim with their reason, flowed into lines.
fn affordance_lines(screen: &Screen, width: usize) -> Vec<Line<'static>> {
    let mut lines: Vec<Vec<Span<'static>>> = vec![vec![]];
    let mut used = 0;
    for a in screen
        .affordances
        .iter()
        .filter(|a| a.role != Some(AffordanceRole::Select))
    {
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
    typing: Option<&'a Typing>,
    overmap: &'a mut Option<OvermapHit>,
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
        // A banner is the screen's heading: it sits above the parts, as tall as its lines but for at most
        // half the area, so a small terminal still shows the parts.
        let (banners, parts) = split.children.iter().partition::<Vec<_>, _>(
            |c| matches!(c, Component::Text(t) if t.role == Some(TextRole::Banner)),
        );
        let mut area = area;
        for banner in banners {
            let lines = match banner {
                Component::Text(t) => t.markup.shown().lines().count(),
                _ => 0,
            };
            let height = u16::try_from(lines)
                .unwrap_or(u16::MAX)
                .min(area.height / 2);
            let [top, rest] =
                Layout::vertical([Constraint::Length(height), Constraint::Min(0)]).areas(area);
            self.component(f, banner, top);
            area = rest;
        }
        let n = parts.len().max(1);
        let side = usize::from(area.width) >= 30 * n;
        let cells = match side {
            true => Layout::horizontal(vec![Constraint::Fill(1); n]).split(area),
            false => Layout::vertical(vec![Constraint::Fill(1); n]).split(area),
        };
        for (child, cell) in parts.into_iter().zip(cells.iter()) {
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
                Span::styled(
                    format!(
                        " {}{} ",
                        tab.label.shown(),
                        if tab.unread { " •" } else { "" }
                    ),
                    style,
                ),
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
                _ => match self
                    .typing
                    .filter(|t| t.form == form.head.id && t.field == field.id)
                {
                    Some(typing) => format!("{}_", typing.text),
                    None => field.value.clone(),
                },
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
        // A banner is art: its lines are cut at the window's edge, never wrapped.
        let width = match text.role {
            Some(TextRole::Banner) => usize::from(u16::MAX),
            _ => usize::from(area.width),
        };
        let lines = flow(&runs, width);
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

    /// The overmap squares around the engine's cursor, cut to the room, with what it says of the cursor
    /// square; the world map is the 0.1 view's, so it is an empty frame here.
    fn map(&mut self, f: &mut Frame, map: &MapComponent, area: Rect) {
        let block = Block::bordered();
        let inner = block.inner(area);
        f.render_widget(block, area);
        let lines = map.cursor_lines();
        if !lines.is_empty() {
            let runs = self.model.styled(&lines.join("\n"));
            let shown = flow(&runs, usize::from(inner.width))
                .iter()
                .map(|l| line_of(l, Style::new()))
                .collect::<Vec<_>>();
            f.render_widget(Paragraph::new(shown), inner);
            return;
        }
        let Some(om) = &map.overmap else {
            return;
        };
        let cursor = map.cursor().unwrap_or_else(|| om.origin.clone());
        let mut notes = om.legend.clone();
        notes.extend(om.cursor_here.clone());
        notes.push(om.cursor_description.clone());
        notes.extend(om.cursor_note.clone());
        notes.extend(om.route_text.clone());
        notes.extend(
            om.notes
                .iter()
                .map(|n| format!("({}, {}) {}", n.pos.x, n.pos.y, n.text)),
        );
        let notes = notes
            .into_iter()
            .filter(|n| !n.is_empty())
            .collect::<Vec<_>>();
        let rows = om
            .rows
            .min(usize::from(inner.height).saturating_sub(notes.len()).max(1));
        let cols = om.cols.min(usize::from(inner.width));
        let (cursor_col, cursor_row) = om.cell_of(&cursor).unwrap_or((om.cols / 2, om.rows / 2));
        // The squares that fit, kept around the cursor.
        let first = |at: usize, shown: usize, all: usize| {
            at.saturating_sub(shown / 2).min(all.saturating_sub(shown))
        };
        let (left, top) = (
            first(cursor_col, cols, om.cols),
            first(cursor_row, rows, om.rows),
        );
        let [grid, foot] =
            Layout::vertical([Constraint::Length(rows as u16), Constraint::Min(0)]).areas(inner);
        let lines = (top..top + rows).map(|row| {
            Line::from(
                (left..left + cols)
                    .filter_map(|col| {
                        let (glyph, fg, bg) = om.cells.get(row)?.get(col)?;
                        let mut style = Style::new().fg(color(self.colors.index(*fg)));
                        if *bg != 0 {
                            style = style.bg(color(self.colors.index(*bg)));
                        }
                        if om.path.contains(&om.square(col, row)) {
                            style = style.bg(Color::Cyan);
                        }
                        if (col, row) == (cursor_col, cursor_row) {
                            style = style.add_modifier(Modifier::REVERSED);
                        }
                        Some(Span::styled(glyph.clone(), style))
                    })
                    .collect::<Vec<_>>(),
            )
        });
        f.render_widget(Paragraph::new(lines.collect::<Vec<_>>()), grid);
        f.render_widget(
            Paragraph::new(notes.into_iter().map(Line::from).collect::<Vec<_>>()),
            foot,
        );
        let named = |pos: bool| {
            map.head
                .affordances
                .iter()
                .filter_map(|id| self.screen.affordances.iter().find(|a| &a.id == id))
                .find(|a| a.args.as_ref().is_some_and(|args| args.pos.is_some()) == pos)
                .map(|a| a.id.clone())
        };
        *self.overmap = Some(OvermapHit {
            area: Rect::new(grid.x, grid.y, cols as u16, rows as u16),
            first: (left, top),
            origin: om.origin.clone(),
            cursor,
            mover: named(true),
            runner: named(false),
        });
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
        let any_toggled = shown.iter().any(|r| r.toggled.is_some());
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
                        any_toggled,
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
            any_toggled,
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
            _ if any_toggled => match row.toggled {
                Some(true) => "[x] ".into(),
                Some(false) => "[ ] ".into(),
                None => "    ".into(),
            },
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
    any_toggled: bool,
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
