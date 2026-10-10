//! Terminal client over the same `GameModel` as the egui client.

use bn_client_core::{GameModel, Loading, Menu, Pos, Rgb};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Gauge, Paragraph};

const LOG_LINES: usize = 6;
const SIDE_WIDTH: u16 = 36;

pub struct TuiApp<M: GameModel> {
    pub model: M,
    selected: usize,
    /// Identity of the menu `selected` refers to; a different menu resets the selection.
    menu_sig: Option<(String, usize)>,
    quit: bool,
}

fn color(c: Rgb) -> Color {
    Color::Rgb(c.0, c.1, c.2)
}

/// The game's key token for a terminal key, or `None` for keys the game has no name for.
fn token(key: KeyEvent) -> Option<String> {
    Some(match key.code {
        KeyCode::Char(' ') => "SPACE".into(),
        KeyCode::Char(c) => c.to_string(),
        KeyCode::Up => "UP".into(),
        KeyCode::Down => "DOWN".into(),
        KeyCode::Left => "LEFT".into(),
        KeyCode::Right => "RIGHT".into(),
        KeyCode::Home => "HOME".into(),
        KeyCode::End => "END".into(),
        KeyCode::PageUp => "PAGE_UP".into(),
        KeyCode::PageDown => "PAGE_DOWN".into(),
        KeyCode::Enter => "RETURN".into(),
        KeyCode::Tab => "TAB".into(),
        KeyCode::Esc => "ESC".into(),
        _ => return None,
    })
}

fn arrow(code: KeyCode) -> Option<(i32, i32)> {
    match code {
        KeyCode::Up => Some((0, -1)),
        KeyCode::Down => Some((0, 1)),
        KeyCode::Left => Some((-1, 0)),
        KeyCode::Right => Some((1, 0)),
        _ => None,
    }
}

impl<M: GameModel> TuiApp<M> {
    pub fn new(model: M) -> Self {
        Self {
            model,
            selected: 0,
            menu_sig: None,
            quit: false,
        }
    }

    pub fn quit(&self) -> bool {
        self.quit
    }

    /// Label of the highlighted menu row.
    pub fn selected_label(&mut self) -> Option<String> {
        let menu = self.menu()?;
        menu.choices.get(self.selected).map(|c| c.label.clone())
    }

    /// The current menu, resetting the highlight when the engine shows a different one.
    fn menu(&mut self) -> Option<Menu> {
        let menu = self.model.menu();
        let sig = menu.as_ref().map(|m| (m.title.clone(), m.choices.len()));
        if sig != self.menu_sig {
            self.selected = 0;
            self.menu_sig = sig;
        }
        menu
    }

    pub fn handle_key(&mut self, key: KeyEvent) {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            self.quit = true;
            return;
        }
        if self.model.loading().is_some() {
            return;
        }
        if let Some(menu) = self.menu() {
            match key.code {
                KeyCode::Up => self.selected = self.selected.saturating_sub(1),
                KeyCode::Down => {
                    self.selected = (self.selected + 1).min(menu.choices.len().saturating_sub(1));
                }
                KeyCode::Enter => {
                    if let Some(c) = menu.choices.get(self.selected) {
                        self.model.choose(&c.id);
                    }
                }
                KeyCode::Esc => self.model.cancel_menu(),
                _ => {}
            }
            return;
        }
        let ran = token(key).is_some_and(|t| self.model.press(&t));
        if let (false, Some((dx, dy))) = (ran, arrow(key.code)) {
            let me = self.model.player();
            self.model.step_toward(Pos {
                x: me.x + dx,
                y: me.y + dy,
            });
        }
    }

    pub fn draw(&mut self, f: &mut Frame) {
        if let Some(loading) = self.model.loading() {
            return Self::draw_loading(f, &loading);
        }
        let [main, log] =
            Layout::vertical([Constraint::Min(5), Constraint::Length(LOG_LINES as u16 + 2)])
                .areas(f.area());
        let [map, side] =
            Layout::horizontal([Constraint::Min(10), Constraint::Length(SIDE_WIDTH)]).areas(main);
        let [actions, items] =
            Layout::vertical([Constraint::Min(3), Constraint::Length(12)]).areas(side);
        self.draw_map(f, map);
        self.draw_actions(f, actions);
        self.draw_items(f, items);
        let messages = self.model.messages();
        let shown = &messages[messages.len().saturating_sub(LOG_LINES)..];
        let lines = shown
            .iter()
            .map(|m| Line::raw(m.as_str()))
            .collect::<Vec<_>>();
        f.render_widget(
            Paragraph::new(lines).block(Block::bordered().title("Message log")),
            log,
        );
        if let Some(menu) = self.menu() {
            self.draw_menu(f, &menu);
        }
    }

    /// Title, a progress bar over the steps, and the step list (the splash image is skipped).
    fn draw_loading(f: &mut Frame, loading: &Loading) {
        let total = loading.entries.len();
        let step = (loading.index + 1).min(total);
        let [title, bar, list] = Layout::vertical([
            Constraint::Length(1),
            Constraint::Length(3),
            Constraint::Min(1),
        ])
        .areas(f.area());
        f.render_widget(
            Paragraph::new(Line::styled(
                loading.title.clone(),
                Style::new().add_modifier(Modifier::BOLD),
            )),
            title,
        );
        let ratio = if total == 0 {
            0.0
        } else {
            step as f64 / total as f64
        };
        f.render_widget(
            Gauge::default()
                .block(Block::bordered())
                .ratio(ratio)
                .label(format!("{step}/{total}")),
            bar,
        );
        let lines = loading
            .entries
            .iter()
            .enumerate()
            .map(|(i, e)| match i == loading.index {
                true => Line::styled(format!("> {e}"), Style::new().add_modifier(Modifier::BOLD)),
                false => Line::raw(format!("  {e}")),
            });
        f.render_widget(Paragraph::new(lines.collect::<Vec<_>>()), list);
    }

    fn draw_map(&mut self, f: &mut Frame, area: Rect) {
        let block = Block::bordered().title("Map");
        let inner = block.inner(area);
        f.render_widget(block, area);
        let size = self.model.size();
        if size.x <= 0 || size.y <= 0 {
            f.render_widget(Paragraph::new("No map loaded"), inner);
            return;
        }
        let player = self.model.player();
        let window = |centre: i32, len: i32, avail: u16| {
            let avail = i32::from(avail).min(len);
            let start = (centre - avail / 2).clamp(0, len - avail);
            (start, avail)
        };
        let (x0, w) = window(player.x, size.x, inner.width);
        let (y0, h) = window(player.y, size.y, inner.height);
        let lines = (y0..y0 + h)
            .map(|y| {
                let spans = (x0..x0 + w).map(|x| {
                    if (x, y) == (player.x, player.y) {
                        return Span::styled("@", Style::new().fg(Color::Yellow));
                    }
                    let cell = self.model.cell(Pos { x, y });
                    let style = Style::new().fg(color(cell.color));
                    let style = cell.bg.map_or(style, |bg| style.bg(color(bg)));
                    Span::styled(cell.glyph.to_string(), style)
                });
                Line::from(spans.collect::<Vec<_>>())
            })
            .collect::<Vec<_>>();
        f.render_widget(Paragraph::new(lines), inner);
    }

    fn draw_actions(&mut self, f: &mut Frame, area: Rect) {
        let lines = self
            .model
            .actions()
            .iter()
            .map(|a| match a.keys.is_empty() {
                true => Line::raw(a.name.clone()),
                false => Line::raw(format!("{} [{}]", a.name, a.keys.join(", "))),
            })
            .collect::<Vec<_>>();
        f.render_widget(
            Paragraph::new(lines).block(Block::bordered().title("Actions")),
            area,
        );
    }

    fn draw_items(&mut self, f: &mut Frame, area: Rect) {
        let section = |title: &str, items: &[String]| {
            let head = Line::styled(title.to_string(), Style::new().add_modifier(Modifier::BOLD));
            std::iter::once(head)
                .chain(items.iter().map(|i| Line::raw(i.clone())))
                .collect::<Vec<_>>()
        };
        let mut lines = section("Ground", self.model.ground());
        lines.extend(section("Inventory", self.model.inventory()));
        f.render_widget(Paragraph::new(lines).block(Block::bordered()), area);
    }

    fn draw_menu(&mut self, f: &mut Frame, menu: &Menu) {
        let mut lines = vec![];
        if !menu.message.is_empty() {
            lines.extend(menu.message.lines().map(|l| Line::raw(l.to_string())));
        }
        for (i, c) in menu.choices.iter().enumerate() {
            let mark = if i == self.selected { "> " } else { "  " };
            let denial = if c.enabled || c.denial.is_empty() {
                String::new()
            } else {
                format!(" ({})", c.denial)
            };
            let mut style = if c.enabled {
                Style::new()
            } else {
                Style::new().fg(Color::DarkGray)
            };
            if i == self.selected {
                style = style.add_modifier(Modifier::REVERSED);
            }
            lines.push(Line::styled(format!("{mark}{}{denial}", c.label), style));
        }
        let area = f.area();
        let height = (lines.len() as u16 + 2).min(area.height);
        let width = area.width.min(70);
        let rect = Rect::new(
            area.x + (area.width - width) / 2,
            area.y + (area.height - height) / 2,
            width,
            height,
        );
        f.render_widget(Clear, rect);
        f.render_widget(
            Paragraph::new(lines).block(Block::bordered().title(menu.title.clone())),
            rect,
        );
    }
}
