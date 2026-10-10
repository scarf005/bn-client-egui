//! The engine's sidebar as a self-contained panel: draw it into any `Ui` area.

use bn_client_core::sidebar::Line;
use eframe::egui::{self, Color32, FontId, TextFormat, text::LayoutJob};

/// One label per sidebar line, each span in its native colour; a screen reader reads the line as one text.
pub fn show(ui: &mut egui::Ui, lines: &[Line]) {
    ui.heading("Status");
    for line in lines {
        let mut job = LayoutJob::default();
        for (i, span) in line.iter().enumerate() {
            let text = if i == 0 {
                span.text.clone()
            } else {
                format!("  {}", span.text)
            };
            let format = TextFormat {
                font_id: FontId::monospace(14.0),
                color: Color32::from_rgb(span.color.0, span.color.1, span.color.2),
                ..Default::default()
            };
            job.append(&text, 0.0, format);
        }
        ui.label(job);
    }
}
