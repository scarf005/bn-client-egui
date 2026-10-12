//! A window the engine draws itself, such as a software mini-game, in the colors it drew it with.

use super::color;
use bn_client_core::Canvas;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph};

/// Draws the window centered, as much of it as the terminal holds.
pub(super) fn draw_canvas(f: &mut Frame, canvas: &Canvas) {
    let screen = f.area();
    let width = u16::try_from(canvas.cols).map_or(screen.width, |w| w.min(screen.width));
    let height = u16::try_from(canvas.rows.len()).map_or(screen.height, |h| h.min(screen.height));
    let area = Rect::new(
        screen.x + (screen.width - width) / 2,
        screen.y + (screen.height - height) / 2,
        width,
        height,
    );
    let lines = canvas.rows.iter().map(|row| {
        Line::from(
            row.iter()
                .map(|run| {
                    let style = run
                        .bg
                        .iter()
                        .fold(Style::new().fg(color(run.color)), |s, bg| s.bg(color(*bg)));
                    Span::styled(run.text.clone(), style)
                })
                .collect::<Vec<_>>(),
        )
    });
    f.render_widget(Clear, area);
    f.render_widget(Paragraph::new(lines.collect::<Vec<_>>()), area);
}
