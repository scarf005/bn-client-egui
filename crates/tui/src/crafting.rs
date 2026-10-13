//! The native crafting menu: category tabs, sub-tabs, search, the recipe list and the selected
//! recipe's detail text. The engine owns the lists; the client keeps the highlighted recipe.
use super::{PAGE, TuiApp, bar, wrap};
use bn_client_core::{GameModel, Menu, MenuChoice};
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Clear, Paragraph};

/// A text box being edited: the search filter or a batch count.
pub enum Input {
    Search(String),
    Count(String),
}

fn recipes(menu: &Menu) -> Vec<&MenuChoice> {
    let all = menu.choices.iter();
    all.filter(|c| c.id.starts_with("recipe:")).collect()
}

fn labels<'a>(menu: &'a Menu, prefix: &str) -> Vec<&'a MenuChoice> {
    let all = menu.choices.iter();
    all.filter(|c| c.id.starts_with(prefix)).collect()
}

/// The row `step` away from the selected one, wrapping around the ends.
fn neighbour<'a>(rows: &[&'a MenuChoice], step: isize) -> Option<&'a MenuChoice> {
    let here = rows.iter().position(|c| c.selected).unwrap_or(0) as isize;
    let len = rows.len() as isize;
    (len > 0).then(|| rows[(here + step).rem_euclid(len) as usize])
}

impl<M: GameModel> TuiApp<M> {
    pub(super) fn crafting_key(&mut self, menu: &Menu, key: KeyEvent) {
        let rows = recipes(menu);
        let last = rows.len().saturating_sub(1);
        let current = rows.get(self.selected.min(last)).copied();
        if let Some(input) = self.input.take() {
            return self.edit_input(menu, input, current, key.code);
        }
        match key.code {
            KeyCode::Up => self.selected = self.selected.saturating_sub(1),
            KeyCode::Down => self.selected = (self.selected + 1).min(last),
            KeyCode::PageUp => self.selected = self.selected.saturating_sub(PAGE),
            KeyCode::PageDown => self.selected = (self.selected + PAGE).min(last),
            KeyCode::Home => self.selected = 0,
            KeyCode::End => self.selected = last,
            KeyCode::Left | KeyCode::Right => {
                let step = if key.code == KeyCode::Left { -1 } else { 1 };
                if let Some(tab) = neighbour(&labels(menu, "tab:"), step) {
                    self.model.choose(&tab.id);
                }
            }
            KeyCode::Char(c @ ('[' | ']')) => {
                let step = if c == '[' { -1 } else { 1 };
                if let Some(sub) = neighbour(&labels(menu, "subtab:"), step) {
                    self.model.choose(&sub.id);
                }
            }
            KeyCode::Char('/') => {
                if let Some(field) = &menu.field {
                    self.input = Some(Input::Search(field.value.clone()));
                }
            }
            KeyCode::Char('b') if menu.can_set_count => {
                self.input = Some(Input::Count(String::new()))
            }
            KeyCode::Enter => {
                if let Some(recipe) = current {
                    self.model.choose(&recipe.id);
                }
            }
            KeyCode::Esc => self.model.cancel_menu(),
            _ => {}
        }
    }

    fn edit_input(
        &mut self,
        menu: &Menu,
        mut input: Input,
        current: Option<&MenuChoice>,
        code: KeyCode,
    ) {
        let (Input::Search(text) | Input::Count(text)) = &mut input;
        match code {
            KeyCode::Esc => return,
            KeyCode::Backspace => drop(text.pop()),
            KeyCode::Char(c) if matches!(input, Input::Search(_)) || c.is_ascii_digit() => {
                let (Input::Search(text) | Input::Count(text)) = &mut input;
                text.push(c);
            }
            KeyCode::Enter => {
                match (&input, &menu.field, current) {
                    (Input::Search(value), Some(field), _) => {
                        self.model.fill(&field.id, value, true)
                    }
                    (Input::Count(n), _, Some(recipe)) => {
                        if let Some(n) = n.parse::<u64>().ok().filter(|n| *n > 0) {
                            self.model.set_count(&recipe.id, n);
                        }
                    }
                    _ => {}
                }
                return;
            }
            _ => {}
        }
        self.input = Some(input);
    }

    pub(super) fn draw_crafting(&mut self, f: &mut Frame, menu: &Menu) {
        let area = f.area();
        let (width, height) = (area.width.min(110), area.height.min(32));
        let rect = Rect::new(
            area.x + (area.width - width) / 2,
            area.y + (area.height - height) / 2,
            width,
            height,
        );
        f.render_widget(Clear, rect);
        let block = Block::bordered().title(menu.title.clone());
        let inner = block.inner(rect);
        f.render_widget(block, rect);
        let [tabs, subtabs, search, body, hint] = Layout::vertical([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(1),
        ])
        .areas(inner);
        let bar_of = |prefix: &str| {
            let rows = labels(menu, prefix);
            bar(&rows
                .iter()
                .map(|c| (c.label.clone(), c.selected))
                .collect::<Vec<_>>())
        };
        f.render_widget(Paragraph::new(bar_of("tab:")), tabs);
        f.render_widget(Paragraph::new(bar_of("subtab:")), subtabs);
        let query = match (&self.input, &menu.field) {
            (Some(Input::Search(text)), Some(field)) => Some(format!("{} {text}_", field.label)),
            (_, Some(field)) => Some(format!("{} {}", field.label, field.value)),
            _ => None,
        };
        f.render_widget(Paragraph::new(query.unwrap_or_default()), search);
        let hint_text = match &self.input {
            Some(Input::Search(_)) => "Enter search  Esc cancel".to_string(),
            Some(Input::Count(n)) => format!("Count: {n}_  Enter craft batch  Esc cancel"),
            None => "Enter craft  / search  b batch  [ ] subtab  Left/Right tab  Esc close".into(),
        };
        f.render_widget(
            Paragraph::new(hint_text).style(Style::new().fg(Color::DarkGray)),
            hint,
        );
        let [list, detail] =
            Layout::horizontal([Constraint::Percentage(40), Constraint::Percentage(60)])
                .areas(body);
        let rows = recipes(menu);
        let at = self.selected.min(rows.len().saturating_sub(1));
        self.selected = at;
        let offset = (at + 1).saturating_sub(usize::from(list.height));
        let lines = rows
            .iter()
            .enumerate()
            .skip(offset)
            .map(|(i, row)| {
                let style = if row.enabled {
                    Style::new()
                } else {
                    Style::new().fg(Color::DarkGray)
                };
                match i == at {
                    true => Line::styled(
                        format!("> {}", row.label),
                        style.add_modifier(Modifier::REVERSED),
                    ),
                    false => Line::styled(format!("  {}", row.label), style),
                }
            })
            .collect::<Vec<_>>();
        f.render_widget(Paragraph::new(lines), list);
        let Some(row) = rows.get(at) else {
            return;
        };
        let width = usize::from(detail.width).saturating_sub(2);
        let denial = (!row.denial.is_empty())
            .then(|| Line::styled(row.denial.clone(), Style::new().fg(Color::Red)));
        let columns = row
            .columns
            .iter()
            .map(|c| Line::raw(format!("{}: {}", c.label, c.value)));
        let text = row
            .description
            .lines()
            .flat_map(|l| wrap(l, width))
            .map(Line::raw);
        let lines = denial
            .into_iter()
            .chain(columns)
            .chain(text)
            .collect::<Vec<_>>();
        f.render_widget(
            Paragraph::new(lines).block(Block::bordered().borders(ratatui::widgets::Borders::LEFT)),
            detail,
        );
    }
}
