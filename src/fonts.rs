//! The game's own fallback font (`data/font/unifont.ttf`), so Hangul and other CJK text draws.

use eframe::egui::{self, FontData, FontDefinitions, FontFamily};
use std::path::Path;
use std::sync::Arc;

/// Registers unifont as the last fallback of both font families; false if the file is unreadable.
pub fn install(ctx: &egui::Context, root: &Path) -> bool {
    let Ok(bytes) = std::fs::read(root.join("data/font/unifont.ttf")) else {
        return false;
    };
    let mut fonts = FontDefinitions::default();
    fonts
        .font_data
        .insert("unifont".into(), Arc::new(FontData::from_owned(bytes)));
    for family in [FontFamily::Proportional, FontFamily::Monospace] {
        fonts
            .families
            .entry(family)
            .or_default()
            .push("unifont".into());
    }
    ctx.set_fonts(fonts);
    true
}
