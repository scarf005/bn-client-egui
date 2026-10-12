//! The text windows the game draws whole: a computer terminal (green text) and the extended
//! description of a square. The text fills the screen and scrolls with the arrow and page keys; the
//! answers or the targets to describe are listed under it, and every other key is the engine's.

use super::{PAGE, TuiApp, color, wrap};
use bn_client_core::{GameModel, Menu};
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph};

#[derive(Default)]
pub(super) struct Terminal {
    /// The first text line shown, and the text it was counted in.
    scroll: usize,
    shown: String,
}

impl<M: GameModel> TuiApp<M> {
    /// Scrolls the text; true when the key was one of the scroll keys.
    pub(super) fn terminal_key(&mut self, key: KeyEvent) -> bool {
        let scroll = &mut self.terminal.scroll;
        match key.code {
            KeyCode::Up => *scroll = scroll.saturating_sub(1),
            KeyCode::Down => *scroll += 1,
            KeyCode::PageUp => *scroll = scroll.saturating_sub(PAGE),
            KeyCode::PageDown => *scroll += PAGE,
            _ => return false,
        }
        true
    }

    pub(super) fn draw_terminal(&mut self, f: &mut Frame, menu: &Menu) {
        let screen = f.area();
        if self.terminal.shown != menu.message {
            self.terminal.shown.clone_from(&menu.message);
            self.terminal.scroll = 0;
        }
        let ink = match menu.terminal() {
            true => Color::Green,
            false => Color::Gray,
        };
        let width = usize::from(screen.width.saturating_sub(2));
        let lines = menu
            .message
            .lines()
            .flat_map(|line| {
                let runs = self.model.styled(line);
                let text = runs.iter().map(|r| r.text.as_str()).collect::<String>();
                let fold = wrap(&text, width.max(1));
                // A folded line keeps the color of the run it starts in.
                let first = runs.first().and_then(|r| r.color).map_or(ink, color);
                fold.into_iter()
                    .map(move |l| Line::styled(l, Style::new().fg(first)))
            })
            .collect::<Vec<_>>();
        let answers = Rect {
            height: 1,
            y: screen.bottom().saturating_sub(1),
            ..screen
        };
        let body = Rect {
            height: screen.height.saturating_sub(1),
            ..screen
        };
        let room = usize::from(body.height.saturating_sub(2));
        self.terminal.scroll = self.terminal.scroll.min(lines.len().saturating_sub(room));
        let shown = lines
            .into_iter()
            .skip(self.terminal.scroll)
            .take(room)
            .collect::<Vec<_>>();
        f.render_widget(Clear, screen);
        f.render_widget(
            Paragraph::new(shown).block(Block::bordered().title(menu.title.as_str())),
            body,
        );
        let row = menu
            .choices
            .iter()
            .flat_map(|c| {
                let style = match c.selected {
                    true => Style::new().add_modifier(Modifier::REVERSED),
                    false => Style::new(),
                };
                [
                    Span::styled(format!(" {} ", c.label), style),
                    Span::raw(" "),
                ]
            })
            .collect::<Vec<_>>();
        f.render_widget(Paragraph::new(Line::from(row)), answers);
    }
}
