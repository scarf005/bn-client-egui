//! The engine's sidebar as a self-contained panel: draw it into any `Ui` area.

use bn_client_core::sidebar::{Line, Span};
use eframe::egui::{self, Color32, FontId, TextFormat, text::LayoutJob};

/// One label per sidebar line, each span in its native colour; a screen reader reads the line as one text.
/// A line too wide for the panel loses its padding first, then breaks between spans, never inside one,
/// so a label stays with its value.
pub fn show(ui: &mut egui::Ui, lines: &[Line]) {
    ui.heading("Status");
    let width = ui.available_width();
    for line in lines {
        let job = fitted(ui, line, width);
        ui.add(egui::Label::new(job).wrap_mode(egui::TextWrapMode::Extend));
    }
}

fn format_of(span: &Span) -> TextFormat {
    TextFormat {
        font_id: FontId::monospace(14.0),
        color: Color32::from_rgb(span.color.0, span.color.1, span.color.2),
        ..Default::default()
    }
}

/// The spans on one run of lines: `gap` between spans, `\n` plus an indent where a break is needed.
fn job_of(spans: &[&Span], gap: &str, breaks: &[usize]) -> LayoutJob {
    let mut job = LayoutJob::default();
    for (i, span) in spans.iter().enumerate() {
        let lead = match (i, breaks.contains(&i)) {
            (0, _) => "",
            (_, true) => "\n  ",
            _ => gap,
        };
        job.append(&format!("{lead}{}", span.text), 0.0, format_of(span));
    }
    job
}

fn fitted(ui: &egui::Ui, line: &Line, width: f32) -> LayoutJob {
    let spans = line.iter().collect::<Vec<_>>();
    let wide = |job: LayoutJob| ui.painter().layout_job(job).size().x > width;
    for gap in ["  ", " "] {
        if !wide(job_of(&spans, gap, &[])) {
            return job_of(&spans, gap, &[]);
        }
    }
    // Greedy: start a new line at the first span that no longer fits the current one.
    let mut breaks = vec![];
    let mut start = 0;
    for end in 1..=spans.len() {
        if end - start > 1 && wide(job_of(&spans[start..end], " ", &[])) {
            breaks.push(end - 1);
            start = end - 1;
        }
    }
    job_of(&spans, " ", &breaks)
}
