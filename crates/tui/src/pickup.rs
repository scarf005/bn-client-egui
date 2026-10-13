//! The native pickup window: the predicted carry line on top, one row per stack with its key, nesting
//! backslash and `+`/`#`/`-` mark, the unmark/mark/page hints under the list, and the highlighted
//! row's description in a bordered window below. Rows, the header and the keys are the engine's;
//! the client only types a count (digits, then Enter, the mark key or a row key applies it).

use super::{SIDE_WIDTH, TuiApp, color};
use bn_client_core::{GameModel, Menu, MenuChoice};
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::Frame;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph, Wrap};
use unicode_width::UnicodeWidthStr;

/// Column of the item name; the key, nesting mark and selection mark come before it.
const NAME_AT: u16 = 6;

#[derive(Default)]
pub(super) struct Pickup {
    /// Where each row was last drawn, for mouse clicks.
    rows: Vec<(Rect, String)>,
    /// The count being typed, kept until a row takes it.
    count: Option<String>,
}

/// `text` in the game color `name`, as markup.
fn tinted(name: &str, text: &str) -> String {
    match name {
        "" => text.to_string(),
        name => format!("<color_{name}>{text}</color>"),
    }
}

impl<M: GameModel> TuiApp<M> {
    /// Digits type a count; Enter, the mark key or a row key hands it to a row. Every other key is the engine's.
    pub(super) fn pickup_key(&mut self, menu: &Menu, key: KeyEvent) {
        let rows = menu.entries();
        let actions = self.model.actions();
        let row = rows.iter().find(|r| r.highlighted).copied();
        let count = self.pickup.count.clone();
        match (key.code, count) {
            (KeyCode::Char(d @ '0'..='9'), text) if row.is_some() => {
                let text = text.unwrap_or_default();
                if text.len() < 9 && !(text.is_empty() && d == '0') {
                    self.pickup.count = Some(format!("{text}{d}"));
                }
            }
            (KeyCode::Backspace, Some(text)) => {
                self.pickup.count = Some(text[..text.len().saturating_sub(1)].to_string())
                    .filter(|t| !t.is_empty());
            }
            (KeyCode::Esc, Some(_)) => self.pickup.count = None,
            (KeyCode::Enter | KeyCode::Right, Some(_)) if row.is_some() => {
                self.take_count(row.unwrap());
            }
            (KeyCode::Char(c), _) => {
                let typed = c.to_string();
                let bound = actions.iter().any(|a| a.keys.contains(&typed));
                let hit = rows.iter().position(|r| r.hotkey == typed);
                match hit.filter(|_| !bound) {
                    Some(i) => self.take_count(rows[i]),
                    None => self.menu_key(menu, key),
                }
            }
            _ => self.menu_key(menu, key),
        }
    }

    /// Marks `row` with the typed count (all of it when the count reaches the stack), else toggles it.
    fn take_count(&mut self, row: &MenuChoice) {
        let typed = self.pickup.count.take().and_then(|t| t.parse::<u64>().ok());
        match (typed, row.available_count) {
            (Some(n), Some(available)) if (1..available).contains(&n) => {
                self.model.set_count(&row.id, n);
            }
            _ => self.model.choose(&row.id),
        }
    }

    /// A left click on a row marks it.
    pub(super) fn pickup_click(&mut self, column: u16, row: u16) -> bool {
        let hit = self.pickup.rows.iter().find(|(r, _)| {
            (r.x..r.x + r.width).contains(&column) && (r.y..r.y + r.height).contains(&row)
        });
        let id = hit.map(|(_, id)| id.clone());
        id.inspect(|id| self.model.choose(id)).is_some()
    }

    pub(super) fn draw_pickup(&mut self, f: &mut Frame, menu: &Menu) {
        let screen = f.area();
        let rows = menu.entries();
        let actions = self.model.actions();
        // Native sizing: a list of 9 to (height - info - border) rows, 44 columns or the longest name + 10.
        let info = (screen.height / 2).min(25);
        let most = screen.height.saturating_sub(info + 3).max(9);
        let shown = (rows.len() as u16).clamp(9, most);
        let widest = rows
            .iter()
            .map(|r| bn_client_core::markup::plain(&r.label).width() + 10)
            .max()
            .unwrap_or(0);
        let width = (widest as u16)
            .clamp(44, screen.width.max(44))
            .min(screen.width);
        let list_height = (shown + 3).min(screen.height);
        // PICKUP_POSITION: the list is docked against the sidebar at the right, otherwise at the left.
        let placement = menu.panes.first().map_or("", |p| p.placement.as_str());
        let window = Rect {
            x: if placement == "right" {
                screen.width.saturating_sub(SIDE_WIDTH + width)
            } else {
                0
            },
            y: 0,
            width,
            height: list_height,
        };
        let details = Rect {
            y: list_height,
            height: screen.height.saturating_sub(list_height),
            ..window
        };
        f.render_widget(Clear, window);
        f.render_widget(Clear, details);
        // The native header the engine words: predicted against capacity, red where over, then the filter.
        let head = menu.message.replace('\n', "  ");
        let head_line = match &self.pickup.count {
            Some(text) => format!("{head}  Count: {text}_"),
            None => head,
        };
        let mut head_spans = Vec::new();
        let mut head_used = 0;
        for run in self.model.styled(&head_line) {
            let text = super::aim::fit(&run.text, usize::from(width).saturating_sub(head_used));
            head_used += text.width();
            let style = run
                .color
                .map_or(Style::new().fg(Color::White), |c| Style::new().fg(color(c)));
            head_spans.push(Span::styled(text, style));
        }
        f.render_widget(
            Paragraph::new(Line::from(head_spans)),
            Rect {
                height: 1,
                ..window
            },
        );
        let at = rows.iter().position(|r| r.highlighted).unwrap_or(0);
        let start = usize::from(at as u16 / shown * shown);
        self.pickup.rows.clear();
        for (n, row) in rows.iter().enumerate().skip(start).take(shown.into()) {
            let cell = Rect {
                y: 1 + (n - start) as u16,
                height: 1,
                ..window
            };
            self.pickup.rows.push((cell, row.id.clone()));
            let hilite = Style::new().add_modifier(if row.highlighted {
                Modifier::REVERSED
            } else {
                Modifier::empty()
            });
            let key = Some(row.hotkey.clone())
                .filter(|k| !k.is_empty())
                .unwrap_or_else(|| " ".into());
            // The native backslash of a stack inside another, in the color the engine lit it.
            let paint = |tone: &str, text: &str| {
                let runs = self.model.styled(&tinted(tone, text));
                let span = |run: bn_client_core::markup::Run| {
                    let style = run
                        .color
                        .map_or(Style::new(), |c| Style::new().fg(color(c)));
                    Span::styled(run.text, style)
                };
                runs.into_iter().map(span).collect::<Vec<_>>()
            };
            let child = match row.nest_color.as_str() {
                "" => vec![Span::raw(" ")],
                nest => paint(nest, "\\"),
            };
            let mark = row.mark.as_ref().map_or(vec![Span::raw("  ")], |m| {
                paint(m.color.as_deref().unwrap_or(""), &format!("{} ", m.glyph))
            });
            let name_width = usize::from(width.saturating_sub(NAME_AT));
            let mut spans = vec![Span::styled(key, hilite)];
            spans.extend(child);
            spans.extend(mark);
            spans.push(Span::raw("  "));
            let mut used = 0;
            // The row as the native list prints it: its markup (count, symbol, stolen mark) in the item's color.
            let name = match row.markup.as_str() {
                "" => tinted(&row.color, &row.label),
                markup => tinted(&row.color, markup),
            };
            for run in self.model.styled(&name) {
                let text = super::aim::fit(&run.text, name_width.saturating_sub(used));
                used += text.width();
                let style = run
                    .color
                    .map_or(Style::new(), |c| Style::new().fg(color(c)));
                spans.push(Span::styled(text, style.patch(hilite)));
            }
            f.render_widget(Paragraph::new(Line::from(spans)), cell);
        }
        let hint = |id: &str, name: &str| {
            let key = actions
                .iter()
                .find(|a| a.id == id)
                .and_then(|a| a.keys.first())
                .map_or(String::new(), |k| match k.chars().count() {
                    1 => k.clone(),
                    _ => k.to_lowercase(),
                });
            format!("[{key}] {name}")
        };
        for (line, (left, middle, right)) in [
            (
                hint("LEFT", "Unmark"),
                hint("HELP_KEYBINDINGS", "Help"),
                hint("RIGHT", "Mark"),
            ),
            (
                hint("PREV_TAB", "Prev"),
                hint("SELECT_ALL", "All"),
                hint("NEXT_TAB", "Next"),
            ),
        ]
        .into_iter()
        .enumerate()
        {
            let y = shown + 1 + line as u16;
            let bar = Rect {
                y,
                height: 1,
                ..window
            };
            let gray = Style::new().fg(Color::Gray);
            f.render_widget(Paragraph::new(Line::styled(left, gray)), bar);
            f.render_widget(
                Paragraph::new(Line::styled(middle, gray)).alignment(Alignment::Center),
                bar,
            );
            f.render_widget(
                Paragraph::new(Line::styled(right, gray)).alignment(Alignment::Right),
                bar,
            );
        }
        let focus = rows.iter().find(|r| r.highlighted);
        let title = focus.map_or(String::new(), |r| {
            format!("< {} >", bn_client_core::markup::plain(&r.label))
        });
        // Each line of the description is its own row; a line of two dashes is the native rule.
        let text = focus
            .map(|r| {
                r.description
                    .lines()
                    .map(|line| match line {
                        "--" => Line::styled(
                            "─".repeat(usize::from(details.width.saturating_sub(2))),
                            Style::new().fg(Color::Gray),
                        ),
                        _ => Line::from(
                            self.model
                                .styled(line)
                                .into_iter()
                                .map(|run| {
                                    let style = run
                                        .color
                                        .map_or(Style::new(), |c| Style::new().fg(color(c)));
                                    Span::styled(run.text, style)
                                })
                                .collect::<Vec<_>>(),
                        ),
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        f.render_widget(
            Paragraph::new(text)
                .wrap(Wrap { trim: true })
                .block(Block::bordered().title(title)),
            details,
        );
    }
}
