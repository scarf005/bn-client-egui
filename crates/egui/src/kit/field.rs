//! A text box, outlined so that it reads as a place to type rather than a line of text.

use super::tokens::BORDER;
use eframe::egui;

/// Adds `edit`, outlined in the border colour while neither the pointer nor the keyboard is on it.
pub fn add(ui: &mut egui::Ui, edit: egui::TextEdit) -> egui::Response {
    let idle = ui.visuals().widgets.inactive.bg_stroke;
    ui.visuals_mut().widgets.inactive.bg_stroke = egui::Stroke::new(1.0, BORDER);
    let response = ui.add(edit);
    ui.visuals_mut().widgets.inactive.bg_stroke = idle;
    response
}
