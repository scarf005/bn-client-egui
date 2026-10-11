//! The native advanced inventory manager as two bordered panes, laid out like the curses screen:
//! each pane has its area and its weight and volume on top, a column header, one row per item with
//! amount, weight and volume, and its filter on the bottom border; the highlighted item's
//! description fills the box underneath. Everything the mouse does is a native action.

use super::{TuiApp, color};
use bn_client_core::{GameModel, Menu, MenuChoice, MenuPane};
use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph, Wrap};
use std::collections::VecDeque;
use unicode_width::UnicodeWidthStr;

/// Rows a wheel notch moves, as in a native list.
const WHEEL_ROWS: usize = 3;
const DESCRIPTION_LINES: u16 = 5;

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
    steps: VecDeque<Step>,
}

/// The action that makes pane `index` the one the engine acts on.
fn side(index: usize) -> &'static str {
    if index == 0 { "LEFT" } else { "RIGHT" }
}

fn column<'a>(row: &'a MenuChoice, name: &str) -> &'a str {
    row.columns
        .iter()
        .find(|(l, _)| l == name)
        .map_or("", |(_, v)| v.as_str())
}

/// The sum of a column whose values read like `0.15 kg`, with its unit.
fn total(rows: &[&MenuChoice], name: &str) -> String {
    let unit = rows
        .iter()
        .find_map(|r| column(r, name).split_whitespace().nth(1))
        .unwrap_or("");
    let sum = rows
        .iter()
        .filter_map(|r| {
            column(r, name)
                .split_whitespace()
                .next()?
                .parse::<f64>()
                .ok()
        })
        .sum::<f64>();
    format!("{sum:.2} {unit}").trim_end().to_string()
}

/// `text` cut to `width` terminal columns.
fn fit(text: &str, width: usize) -> String {
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
        let [main, description] = Layout::vertical([
            Constraint::Min(8),
            Constraint::Length(DESCRIPTION_LINES + 2),
        ])
        .areas(area);
        let halves = Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(main);
        self.aim.rows.clear();
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
            self.draw_aim_pane(f, halves[index], (index, pane, &rows));
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
    ) {
        self.aim.panes[index] = rect;
        let source = pane.role == "source";
        let title = format!("< {}: {} >", pane.label, pane.area);
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
        let totals = format!("{}  {}", total(rows, "Weight"), total(rows, "Volume"));
        let header = |left: &str, right: &str| {
            let gap = width.saturating_sub(left.width() + right.width());
            Line::from(format!("{left}{}{right}", " ".repeat(gap)))
        };
        f.render_widget(
            Paragraph::new(vec![
                header(&pane.area, &totals),
                header("  Name (charges)", "amt weight vol"),
            ]),
            Rect {
                height: inner.height.min(2),
                ..inner
            },
        );
        let list = Rect {
            y: inner.y + 2,
            height: inner.height.saturating_sub(2),
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
                column(row, "Count"),
                column(row, "Weight"),
                column(row, "Volume")
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
                if let Some((_, _, id)) = self.aim.rows.iter().find(|(r, _, _)| inside(*r)) {
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
