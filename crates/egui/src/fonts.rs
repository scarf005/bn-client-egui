//! The client's own faces, Noto Sans CJK KR and Noto Sans Mono CJK KR (`assets/fonts`, OFL, cut by
//! `tools/subset_fonts.sh`), so Latin, Hangul and kana share one face; the game's fallback font
//! (`data/font/unifont.ttf`) draws what they lack.

use eframe::egui::{self, FontData, FontDefinitions, FontFamily};
use std::path::Path;
use std::sync::Arc;

const SANS: &[u8] = include_bytes!("../assets/fonts/sans.otf");
const MONO: &[u8] = include_bytes!("../assets/fonts/mono.otf");

/// The client's faces first in each family, egui's own after them, the proportional face behind the monospace one
/// for its hanja and kanji, and `unifont` last when given.
fn definitions(unifont: Option<Vec<u8>>) -> FontDefinitions {
    let mut fonts = FontDefinitions::default();
    for (name, bytes) in [("noto-sans-cjk", SANS), ("noto-sans-mono-cjk", MONO)] {
        fonts
            .font_data
            .insert(name.into(), Arc::new(FontData::from_static(bytes)));
    }
    for (family, face) in [
        (FontFamily::Proportional, "noto-sans-cjk"),
        (FontFamily::Monospace, "noto-sans-mono-cjk"),
    ] {
        fonts
            .families
            .entry(family)
            .or_default()
            .insert(0, face.into());
    }
    fonts
        .families
        .entry(FontFamily::Monospace)
        .or_default()
        .push("noto-sans-cjk".into());
    if let Some(bytes) = unifont {
        fonts
            .font_data
            .insert("unifont".into(), Arc::new(FontData::from_owned(bytes)));
        for list in fonts.families.values_mut() {
            list.push("unifont".into());
        }
    }
    fonts
}

/// Installs the client's faces alone, until a game root is known.
pub fn install_bundled(ctx: &egui::Context) {
    ctx.set_fonts(definitions(None));
}

/// Installs the client's faces with unifont from the game at `root`; false, changing nothing, if it is unreadable.
pub fn install(ctx: &egui::Context, root: &Path) -> bool {
    let Ok(bytes) = std::fs::read(root.join("data/font/unifont.ttf")) else {
        return false;
    };
    ctx.set_fonts(definitions(Some(bytes)));
    true
}
