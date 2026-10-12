//! The engine's sidebar as a self-contained panel: draw it into any `Ui` area.

use bn_client_core::sidebar::{Line, Span};
use eframe::egui::{self, Color32, FontId, TextFormat, text::LayoutJob};

/// The least width in characters of the span at each index of a row: the widest thing the classic panel
/// puts there, so a number gaining a digit never moves what follows it.
const MIN_COLUMNS: [usize; 5] = [5, 7, 7, 8, 15];

/// The indent of a line continued after a break, in characters.
const INDENT: usize = 2;

/// One label per sidebar line, each span in its native colour and at a fixed column, so a value changing
/// never moves another one; a screen reader reads the line as one text.
/// A line too wide for the panel loses its padding first, then breaks between spans, never inside one,
/// so a label stays with its value.
pub fn show(ui: &mut egui::Ui, lines: &[Line]) {
    ui.heading("Status");
    let width = ui.available_width();
    let font = FontId::monospace(14.0);
    let char_width = ui
        .painter()
        .layout_no_wrap("0".into(), font, Color32::WHITE)
        .size()
        .x;
    // Columns only widen, so a value that once needed more room keeps it.
    let key = egui::Id::new("sidebar_columns");
    let mut columns: Vec<usize> = ui.data(|d| d.get_temp(key)).unwrap_or_default();
    for line in lines {
        columns.resize(columns.len().max(line.len()), 0);
        for (i, span) in line.iter().enumerate() {
            let floor = MIN_COLUMNS.get(i).copied().unwrap_or(0);
            columns[i] = columns[i].max(floor).max(span.text.chars().count());
        }
    }
    ui.data_mut(|d| d.insert_temp(key, columns.clone()));
    let grid = Grid {
        columns: &columns,
        char_width,
    };
    for line in lines {
        let job = fitted(line, width, &grid);
        ui.add(egui::Label::new(job).wrap_mode(egui::TextWrapMode::Extend));
    }
}

/// Where the spans of a row sit: every index has a column of fixed character width.
struct Grid<'a> {
    columns: &'a [usize],
    char_width: f32,
}

fn format_of(span: &Span) -> TextFormat {
    TextFormat {
        font_id: FontId::monospace(14.0),
        color: Color32::from_rgb(span.color.0, span.color.1, span.color.2),
        ..Default::default()
    }
}

/// The spans on one row: `gap` between spans and each at the start of its column. A run that
/// breaks (`breaks` holds the span indices that start one) continues on a new line, indented, with the
/// columns counted from the indent.
fn job_of(spans: &[&Span], gap: &str, breaks: &[usize], grid: &Grid) -> LayoutJob {
    let mut job = LayoutJob::default();
    // Characters on the current line, the start of the next column, whether the line has a span yet.
    let (mut at, mut column, mut has) = (0, 0, false);
    for (i, span) in spans.iter().enumerate() {
        if breaks.contains(&i) {
            job.append("\n  ", 0.0, format_of(span));
            (at, column, has) = (INDENT, INDENT, false);
        }
        let start = column;
        column += grid.columns.get(i).copied().unwrap_or(0) + gap.len();
        if span.text.is_empty() {
            continue;
        }
        let lead = if has { gap } else { "" };
        let pad = start.saturating_sub(at + lead.len());
        job.append(
            &format!("{lead}{}", span.text),
            pad as f32 * grid.char_width,
            format_of(span),
        );
        at += pad + lead.len() + span.text.chars().count();
        has = true;
    }
    job
}

fn fitted(line: &Line, width: f32, grid: &Grid) -> LayoutJob {
    let spans = line.iter().collect::<Vec<_>>();
    let room = (width / grid.char_width) as usize;
    let column = |i: usize| grid.columns.get(i).copied().unwrap_or(0);
    // Whether the gap fits is decided by the row's columns, not by how long its values are right now.
    let last = spans.iter().rposition(|s| !s.text.is_empty()).unwrap_or(0);
    for gap in ["  ", " "] {
        if (0..=last).map(column).sum::<usize>() + last * gap.len() <= room {
            return job_of(&spans, gap, &[], grid);
        }
    }
    // Greedy: start a new line at the first span that no longer fits the current one.
    let mut breaks = vec![];
    let mut used = 0;
    for i in 0..=last {
        if used > 0 && used + 1 + column(i) > room {
            breaks.push(i);
            used = INDENT;
        }
        used += column(i) + usize::from(used > 0);
    }
    job_of(&spans, " ", &breaks, grid)
}
