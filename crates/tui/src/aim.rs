//! The native advanced inventory manager as two bordered panes, laid out like the curses screen:
//! the last game message on top; each pane has its sort on the top border, its area beside the
//! clickable area markers, its weight and volume as the engine reports them, a column header, one
//! row per item with amount, weight and volume, and its filter on the bottom border; the
//! highlighted item's description fills the box underneath. Everything the mouse does is a native
//! action.

use super::{TuiApp, color};
use bn_client_core::{GameModel, Menu, MenuChoice, MenuPane, PaneTotals};
use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph, Wrap};
use std::collections::VecDeque;
use unicode_width::UnicodeWidthStr;

/// Rows a wheel notch moves, as in a native list.
const WHEEL_ROWS: usize = 3;
const DESCRIPTION_LINES: u16 = 5;
const MESSAGE_LINES: u16 = 2;
/// Rows above the item list: the marker grid, the totals and the column header.
const HEADER_ROWS: u16 = 5;
const GRID_WIDTH: u16 = 20;

/// The native area markers as the curses screen lays them out, each with the action it presses and
/// the area it names and its column offset in the grid: Container and Grabbed Vehicle over Surrounding area and Worn Items
/// beside the compass, Inventory level with the middle row, Above and Below at the right edge.
const MARKERS: [&[(&str, &str, &str, u16)]; 3] = [
    &[
        ("C", "ITEMS_CONTAINER", "Container", 0),
        ("D", "ITEMS_DRAGGED_CONTAINER", "Grabbed Vehicle", 3),
        ("7", "ITEMS_NW", "North West", 8),
        ("8", "ITEMS_N", "North", 11),
        ("9", "ITEMS_NE", "North East", 14),
        ("<", "ITEMS_UP", "Above", 17),
    ],
    &[
        ("I", "ITEMS_INVENTORY", "Inventory", 3),
        ("4", "ITEMS_W", "West", 8),
        ("5", "ITEMS_CE", "Directly below you", 11),
        ("6", "ITEMS_E", "East", 14),
        (">", "ITEMS_DOWN", "Below", 17),
    ],
    &[
        ("A", "ITEMS_AROUND", "Surrounding area", 0),
        ("W", "ITEMS_WORN", "Worn Items", 3),
        ("1", "ITEMS_SW", "South West", 8),
        ("2", "ITEMS_S", "South", 11),
        ("3", "ITEMS_SE", "South East", 14),
    ],
];

/// One input for the engine, sent when the previous one has finished.
pub(super) enum Step {
    Choose(String),
    Action(String),
}

#[derive(Default)]
pub(super) struct Aim {
    /// Where each pane and each row were last drawn, for mouse clicks.
    panes: [Rect; 2],
    rows: Vec<(Rect, usize, String)>,
    /// Each drawn area marker that the engine offers: where it is, its pane and its action.
    markers: Vec<(Rect, usize, &'static str)>,
    steps: VecDeque<Step>,
}

/// The action that makes pane `index` the one the engine acts on.
fn side(index: usize) -> &'static str {
    if index == 0 { "LEFT" } else { "RIGHT" }
}

/// The pane's weight and volume as the native header prints them, in red once over capacity.
fn totals_spans(t: &PaneTotals) -> Vec<Span<'static>> {
    let shown = |text: String, over: bool| match over {
        true => Span::styled(text, Style::new().fg(Color::Red)),
        false => Span::raw(text),
    };
    vec![
        shown(t.weight_text(), t.over_weight),
        Span::raw("  "),
        shown(t.volume_text(), t.over_volume),
    ]
}

/// `text` cut to `width` terminal columns.
pub(super) fn fit(text: &str, width: usize) -> String {
    let mut out = String::new();
    for c in text.chars() {
        if out.width() + unicode_width::UnicodeWidthChar::width(c).unwrap_or(0) > width {
            break;
        }
        out.push(c);
    }
    out
}

impl<M: GameModel> TuiApp<M> {
    pub(super) fn draw_aim(&mut self, f: &mut Frame, menu: &Menu) {
        let area = f.area();
        // The screen covers the map and the panels, like the curses one covers the whole terminal.
        f.render_widget(Clear, area);
        let [message, main, description] = Layout::vertical([
            Constraint::Length(MESSAGE_LINES + 2),
            Constraint::Min(8),
            Constraint::Length(DESCRIPTION_LINES + 2),
        ])
        .areas(area);
        let halves = Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(main);
        let last = self
            .model
            .native_log()
            .iter()
            .rev()
            .take(usize::from(MESSAGE_LINES))
            .rev()
            .map(|l| Line::raw(l.shown()))
            .collect::<Vec<_>>();
        f.render_widget(
            Paragraph::new(last).block(Block::bordered().title_top("< [?] keybindings >")),
            message,
        );
        self.aim.rows.clear();
        self.aim.markers.clear();
        let offered = self
            .model
            .actions()
            .into_iter()
            .map(|a| a.id)
            .collect::<Vec<_>>();
        let mut described = String::new();
        for (index, pane) in menu.panes.iter().take(2).enumerate() {
            let rows = menu
                .choices
                .iter()
                .filter(|c| c.group.as_deref() == Some(pane.id.as_str()))
                .collect::<Vec<_>>();
            if let Some(focus) = rows.iter().find(|r| r.highlighted) {
                described = focus.description.clone();
            }
            self.draw_aim_pane(f, halves[index], (index, pane, &rows), &offered);
        }
        let text = self
            .model
            .styled(&described)
            .into_iter()
            .map(|run| {
                let style = run
                    .color
                    .map_or(Style::new(), |c| Style::new().fg(color(c)));
                Span::styled(run.text, style)
            })
            .collect::<Vec<_>>();
        f.render_widget(
            Paragraph::new(Line::from(text))
                .wrap(Wrap { trim: true })
                .block(Block::bordered().title("Description")),
            description,
        );
    }

    fn draw_aim_pane(
        &mut self,
        f: &mut Frame,
        rect: Rect,
        (index, pane, rows): (usize, &MenuPane, &[&MenuChoice]),
        offered: &[String],
    ) {
        self.aim.panes[index] = rect;
        let source = pane.role == "source";
        let title = format!("< [s or S] Sort: {} >", pane.sort);
        let filter = match pane.filter.is_empty() {
            true => "< [f, F or /] Filter >".to_string(),
            false => format!("< Filter: {} >", pane.filter),
        };
        let border = match source {
            true => Style::new().add_modifier(Modifier::BOLD),
            false => Style::new(),
        };
        let block = Block::bordered()
            .border_style(border)
            .title(title)
            .title_bottom(filter);
        let inner = block.inner(rect);
        f.render_widget(block, rect);
        let width = usize::from(inner.width);
        let header = |left: &str, right: &str| {
            let gap = width.saturating_sub(left.width() + right.width());
            Line::from(format!("{left}{}{right}", " ".repeat(gap)))
        };
        let grid_x = inner.x + inner.width.saturating_sub(GRID_WIDTH);
        let grid = MARKERS
            .iter()
            .enumerate()
            .map(|(row, cells)| {
                let y = inner.y + row as u16;
                let mut line = vec![Span::raw(" ".repeat(usize::from(grid_x - inner.x)))];
                if row == 0 {
                    line = vec![Span::raw(fit(&pane.area, usize::from(grid_x - inner.x)))];
                    line.push(Span::raw(" ".repeat(
                        usize::from(grid_x - inner.x).saturating_sub(line[0].width()),
                    )));
                }
                let mut at = 0;
                for &(label, action, area, offset) in *cells {
                    line.push(Span::raw(" ".repeat(usize::from(offset - at))));
                    let on = offered.iter().any(|a| a == action);
                    // As the native picker: red where the pane cannot put items, white for its own
                    // area, grey for the rest, and dimmed when the engine offers no such action.
                    let style = match (pane.blocked.iter().any(|b| b == action), pane.area == area)
                    {
                        _ if !on => Style::new().fg(Color::DarkGray),
                        (true, _) => Style::new().fg(Color::Red),
                        (false, true) => Style::new().fg(Color::White).add_modifier(Modifier::BOLD),
                        (false, false) => Style::new().fg(Color::Gray),
                    };
                    line.push(Span::styled(format!("[{label}]"), style));
                    at = offset + 3;
                    if on {
                        let rect = Rect {
                            x: grid_x + offset,
                            y,
                            width: 3,
                            height: 1,
                        };
                        self.aim.markers.push((rect, index, action));
                    }
                }
                Line::from(line)
            })
            .collect::<Vec<_>>();
        let totals = pane.totals.as_ref().map_or(vec![], totals_spans);
        let totals_width = totals.iter().map(|s| s.width()).sum::<usize>();
        let totals = std::iter::once(Span::raw(" ".repeat(width.saturating_sub(totals_width))))
            .chain(totals)
            .collect::<Vec<_>>();
        f.render_widget(
            Paragraph::new(
                grid.into_iter()
                    .chain([
                        Line::from(totals),
                        header("  Name (charges)", "amt weight vol"),
                    ])
                    .collect::<Vec<_>>(),
            ),
            Rect {
                height: inner.height.min(HEADER_ROWS),
                ..inner
            },
        );
        let list = Rect {
            y: inner.y + HEADER_ROWS,
            height: inner.height.saturating_sub(HEADER_ROWS),
            ..inner
        };
        // The highlighted row stays in view.
        let at = rows.iter().position(|r| r.highlighted).unwrap_or(0);
        let offset = (at + 1).saturating_sub(usize::from(list.height));
        let right_width = 1 + 4 + 1 + 9 + 1 + 9;
        for (n, row) in rows
            .iter()
            .enumerate()
            .skip(offset)
            .take(usize::from(list.height))
        {
            let y = list.y + (n - offset) as u16;
            let cell = Rect {
                y,
                height: 1,
                ..list
            };
            self.aim.rows.push((cell, index, row.id.clone()));
            let name_width = width.saturating_sub(right_width + 2);
            let mut spans = vec![Span::raw(if row.highlighted { "> " } else { "  " })];
            let mut used = 0;
            for run in self.model.styled(&row.label) {
                let text = fit(&run.text, name_width.saturating_sub(used));
                used += text.width();
                let style = run
                    .color
                    .map_or(Style::new(), |c| Style::new().fg(color(c)));
                spans.push(Span::styled(text, style));
            }
            spans.push(Span::raw(" ".repeat(name_width.saturating_sub(used))));
            spans.push(Span::raw(format!(
                " {:>4} {:>9} {:>9}",
                row.column("count"),
                row.column("weight"),
                row.column("volume")
            )));
            let line = Line::from(spans);
            let line = match row.highlighted && source {
                true => line.style(Style::new().add_modifier(Modifier::REVERSED)),
                false => line,
            };
            f.render_widget(Paragraph::new(line), cell);
        }
    }

    /// A click on a row chooses it, a click elsewhere in a pane points the engine at that pane, and the
    /// wheel moves the engine's cursor by three rows in the pane under it.
    pub(super) fn aim_mouse(&mut self, menu: &Menu, ev: MouseEvent) {
        let inside = |r: Rect| {
            (r.x..r.x + r.width).contains(&ev.column) && (r.y..r.y + r.height).contains(&ev.row)
        };
        let pane = (0..2).find(|&i| inside(self.aim.panes[i]));
        let source = menu.panes.iter().position(|p| p.role == "source");
        match ev.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                if let Some(&(_, index, action)) =
                    self.aim.markers.iter().find(|(r, _, _)| inside(*r))
                {
                    if Some(index) != source {
                        self.aim.steps.push_back(Step::Action(side(index).into()));
                    }
                    self.aim.steps.push_back(Step::Action(action.into()));
                } else if let Some((_, _, id)) = self.aim.rows.iter().find(|(r, _, _)| inside(*r)) {
                    self.aim.steps.push_back(Step::Choose(id.clone()));
                } else if let Some(index) = pane.filter(|i| Some(*i) != source) {
                    self.aim.steps.push_back(Step::Action(side(index).into()));
                }
            }
            MouseEventKind::ScrollUp | MouseEventKind::ScrollDown => {
                let Some(index) = pane else {
                    return;
                };
                let action = match ev.kind == MouseEventKind::ScrollUp {
                    true => "UP",
                    false => "DOWN",
                };
                if Some(index) != source {
                    self.aim.steps.push_back(Step::Action(side(index).into()));
                }
                (0..WHEEL_ROWS).for_each(|_| self.aim.steps.push_back(Step::Action(action.into())));
            }
            _ => {}
        }
    }

    /// Sends the next queued input once the engine is free; one that cannot apply drops the rest.
    pub(super) fn run_aim_steps(&mut self) {
        if self.model.busy() {
            return;
        }
        let Some(step) = self.aim.steps.pop_front() else {
            return;
        };
        let sent = match &step {
            Step::Choose(id) => {
                self.model.choose(id);
                true
            }
            Step::Action(id) => self.model.invoke_action(id),
        };
        if !sent {
            self.aim.steps.clear();
        }
    }
}
