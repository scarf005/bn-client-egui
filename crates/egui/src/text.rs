//! Game text with colour tags, as egui text.

use bn_client_core::markup::Run;
use eframe::egui::{self, text::LayoutJob};

/// Runs as one layout job; uncoloured runs take the colour of the widget showing them.
pub fn job(runs: &[Run], font: &egui::FontId) -> LayoutJob {
    let mut job = LayoutJob::default();
    for run in runs {
        let color = run.color.map_or(egui::Color32::PLACEHOLDER, |c| {
            crate::theme::readable(c, None)
        });
        job.append(
            &run.text,
            0.0,
            egui::TextFormat {
                color,
                font_id: font.clone(),
                ..Default::default()
            },
        );
    }
    job
}
