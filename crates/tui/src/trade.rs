//! The native NPC trade window as two bordered panes, the NPC's items left and yours right, each row with
//! its mark, unit price, count, weight and volume; the balance line on top, the highlighted row's
//! description under the panes and the menu's own actions on the last line. Rows, the balance and the
//! actions are the engine's; the client only types a count for a marked stack.

use super::{TuiApp, color};
use bn_client_core::{GameModel, Menu, MenuChoice, MenuPane};
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph, Wrap};
use unicode_width::UnicodeWidthStr;

const DESCRIPTION_LINES: u16 = 4;
/// The menu's actions in the order the last line lists them.
const ACTIONS: [(&str, &str); 4] = [
    ("CONFIRM", "Confirm trade"),
    ("AUTOBALANCE", "Autobalance"),
    ("CATEGORY_SELECTION", "Category select"),
    ("EXAMINE", "Examine"),
];

#[derive(Default)]
pub(super) struct Trade {
    /// Where each row was last drawn, for mouse clicks.
    rows: Vec<(Rect, String)>,
    /// The count being typed for the highlighted stack.
    count: Option<String>,
}

fn column<'a>(row: &'a MenuChoice, name: &str) -> &'a str {
    row.columns
        .iter()
        .find(|(l, _)| l == name)
        .map_or("", |(_, v)| v.as_str())
}

/// Rows of the pane, in the engine's order.
fn rows<'a>(menu: &'a Menu, pane: &MenuPane) -> Vec<&'a MenuChoice> {
    let all = menu.choices.iter();
    all.filter(|c| c.group.as_deref() == Some(pane.id.as_str()))
        .collect()
}

/// How many of a marked stack of several are offered, the stack the count box is for.
fn countable(row: &MenuChoice) -> Option<u64> {
    row.available_count.filter(|n| row.selected && *n > 1)
}

impl<M: GameModel> TuiApp<M> {
    /// Digits type a count for the highlighted marked stack and Enter sends it; Space marks the row;
    /// every other key is the engine's.
    pub(super) fn trade_key(&mut self, menu: &Menu, key: KeyEvent) {
        let row = menu.choices.iter().find(|c| c.highlighted);
        if let Some(text) = self.trade.count.take() {
            let available = row.and_then(countable);
            match (key.code, row, available) {
                (KeyCode::Char(d @ '0'..='9'), ..) if text.len() < 9 => {
                    self.trade.count = Some(format!("{text}{d}"));
                }
                (KeyCode::Backspace, ..) => {
                    self.trade.count = Some(text[..text.len().saturating_sub(1)].to_string());
                }
                (KeyCode::Enter, Some(row), Some(available)) => {
                    let n = text
                        .parse::<u64>()
                        .ok()
                        .filter(|n| (1..=available).contains(n));
                    n.into_iter().for_each(|n| self.model.set_count(&row.id, n));
                }
                // Anything else, such as Esc, drops the count.
                _ => {}
            }
            return;
        }
        match (key.code, row) {
            (KeyCode::Char(d @ '0'..='9'), Some(row)) if countable(row).is_some() => {
                self.trade.count = Some(d.to_string());
            }
            (KeyCode::Char(' '), Some(row)) if row.enabled => self.model.choose(&row.id),
            _ => self.menu_key(menu, key),
        }
    }

    /// A left click on a row marks it.
    pub(super) fn trade_click(&mut self, column: u16, row: u16) -> bool {
        let hit = self.trade.rows.iter().find(|(r, _)| {
            (r.x..r.x + r.width).contains(&column) && (r.y..r.y + r.height).contains(&row)
        });
        let id = hit.map(|(_, id)| id.clone());
        id.inspect(|id| self.model.choose(id)).is_some()
    }

    pub(super) fn draw_trade(&mut self, f: &mut Frame, menu: &Menu) {
        let area = f.area();
        // The window covers the map and the panels, like the curses one covers the whole terminal.
        f.render_widget(Clear, area);
        let message = super::wrap(&bn_client_core::markup::plain(&menu.message), 100).len() as u16;
        let [top, main, description, keys] = Layout::vertical([
            Constraint::Length(message.max(1) + 2),
            Constraint::Min(6),
            Constraint::Length(DESCRIPTION_LINES + 2),
            Constraint::Length(1),
        ])
        .areas(area);
        let lines = bn_client_core::markup::plain(&menu.message)
            .lines()
            .map(|l| Line::raw(l.to_string()))
            .collect::<Vec<_>>();
        f.render_widget(
            Paragraph::new(lines)
                .block(Block::bordered().title(bn_client_core::markup::plain(&menu.title))),
            top,
        );
        let halves = Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(main);
        self.trade.rows.clear();
        let mut described = String::new();
        for (pane, rect) in menu.panes.iter().take(2).zip(halves.iter()) {
            let rows = rows(menu, pane);
            if let Some(focus) = rows.iter().find(|r| r.highlighted) {
                described = focus.description.clone();
            }
            self.draw_trade_pane(f, *rect, (pane, &rows));
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
        let offered = self.model.actions();
        let mut spans = vec![];
        for (id, name) in ACTIONS {
            let Some(action) = offered.iter().find(|a| a.id == id) else {
                continue;
            };
            let key = action
                .keys
                .first()
                .map_or(String::new(), |k| k.to_lowercase());
            spans.push(Span::styled(
                format!("{key} "),
                Style::new().fg(Color::LightGreen),
            ));
            spans.push(Span::raw(format!("{name} ")));
            if let (Some(on), "CATEGORY_SELECTION") = (menu.category_mode, id) {
                // The native `[ON/OFF]`, the active state lit.
                let lit = |text: &'static str, lit: bool| {
                    let fg = if lit {
                        Color::LightGreen
                    } else {
                        Color::DarkGray
                    };
                    Span::styled(text, Style::new().fg(fg))
                };
                spans.extend([
                    Span::raw("["),
                    lit("ON", on),
                    Span::raw("|"),
                    lit("OFF", !on),
                ]);
                spans.push(Span::raw("]"));
            }
            spans.push(Span::raw(" "));
        }
        if let Some(text) = &self.trade.count {
            spans.push(Span::styled(
                format!("Count: {text}_"),
                Style::new().add_modifier(Modifier::BOLD),
            ));
        } else {
            let can_mark = menu.choices.iter().any(|c| c.highlighted);
            if can_mark {
                spans.push(Span::styled("space ", Style::new().fg(Color::LightGreen)));
                spans.push(Span::raw("Mark  "));
            }
            if menu.can_cancel {
                spans.push(Span::styled("esc ", Style::new().fg(Color::LightGreen)));
                spans.push(Span::raw("Cancel"));
            }
        }
        f.render_widget(Paragraph::new(Line::from(spans)), keys);
    }

    fn draw_trade_pane(
        &mut self,
        f: &mut Frame,
        rect: Rect,
        (pane, rows): (&MenuPane, &[&MenuChoice]),
    ) {
        let filter = match pane.filter.is_empty() {
            true => String::new(),
            false => format!("< Filter: {} >", pane.filter),
        };
        let mut block = Block::bordered()
            .title(format!("{}: {}", pane.label, pane.area))
            .title_bottom(filter);
        if let Some((number, count)) = pane.page {
            let page = Line::from(format!("< Page {number}/{count} >")).right_aligned();
            block = block.title_bottom(page);
        }
        let inner = block.inner(rect);
        f.render_widget(block, rect);
        let width = usize::from(inner.width);
        f.render_widget(
            Paragraph::new(Line::styled(
                super::wrap(&pane.description, width)
                    .into_iter()
                    .next()
                    .unwrap_or_default(),
                Style::new().fg(Color::Gray),
            )),
            Rect {
                height: 1.min(inner.height),
                ..inner
            },
        );
        let list = Rect {
            y: inner.y + 1,
            height: inner.height.saturating_sub(1),
            ..inner
        };
        // The highlighted row stays in view.
        let at = rows.iter().position(|r| r.highlighted).unwrap_or(0);
        let offset = (at + 1).saturating_sub(usize::from(list.height));
        let right = 1 + 8 + 1 + 7 + 1 + 9 + 1 + 9;
        for (n, row) in rows
            .iter()
            .enumerate()
            .skip(offset)
            .take(usize::from(list.height))
        {
            let cell = Rect {
                y: list.y + (n - offset) as u16,
                height: 1,
                ..list
            };
            self.trade.rows.push((cell, row.id.clone()));
            let mark = match row.selected {
                true => "[x] ",
                false => "[ ] ",
            };
            let name_width = width.saturating_sub(right + 4 + mark.len());
            // The native key column: the row's key in green, blank where the row has none.
            let key = match row.hotkey.as_str() {
                "" => Span::raw("  "),
                k => Span::styled(format!("{k} "), Style::new().fg(Color::LightGreen)),
            };
            let mut spans = vec![
                Span::raw(if row.highlighted { "> " } else { "  " }),
                key,
                Span::raw(mark),
            ];
            let mut used = 0;
            for run in self.model.styled(&row.label) {
                let text = super::aim::fit(&run.text, name_width.saturating_sub(used));
                used += text.width();
                let style = run
                    .color
                    .map_or(Style::new(), |c| Style::new().fg(color(c)));
                spans.push(Span::styled(text, style));
            }
            spans.push(Span::raw(" ".repeat(name_width.saturating_sub(used))));
            let count = match (countable(row), row.selected_count) {
                (Some(available), Some(n)) => format!("{n}/{available}"),
                _ => String::new(),
            };
            spans.push(Span::raw(format!(
                " {:>8} {:>7} {:>9} {:>9}",
                column(row, "Unit price"),
                count,
                column(row, "Weight"),
                column(row, "Volume"),
            )));
            let style = match (row.enabled, row.highlighted) {
                (_, true) => Style::new().add_modifier(Modifier::REVERSED),
                (false, _) => Style::new().fg(Color::DarkGray),
                _ => Style::new(),
            };
            f.render_widget(Paragraph::new(Line::from(spans).style(style)), cell);
        }
    }
}
