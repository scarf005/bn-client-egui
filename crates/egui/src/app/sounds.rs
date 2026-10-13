//! The sounds under the message log, drawn from the model's grouped, aged lines.

use bn_client_core::sounds::SoundLine;
use eframe::egui;
use std::time::Duration;

/// The lines under the message log; nothing at all while no sound is fresh.
pub(super) fn show(ui: &mut egui::Ui, lines: &[SoundLine]) {
    if lines.is_empty() {
        return;
    }
    ui.separator();
    for line in lines {
        let color = ui.visuals().text_color().gamma_multiply(line.alpha);
        ui.label(egui::RichText::new(&line.text).color(color));
    }
    // The fade is drawn by time, not by events.
    ui.ctx().request_repaint_after(Duration::from_millis(250));
}
