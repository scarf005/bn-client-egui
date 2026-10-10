//! Needs the game's data directory: set `BN_ROOT`, or keep the default sibling worktree.
use bn_client_egui::fonts;
use eframe::egui::{self, FontId};
use std::path::PathBuf;

fn root() -> PathBuf {
    std::env::var_os("BN_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| "/home/scarf/repo/cata/Cataclysm-BN-worktrees/magnum-opus".into())
}

fn covers(ctx: &egui::Context, text: &str) -> bool {
    ctx.fonts_mut(|f| {
        f.has_glyphs(&FontId::proportional(14.0), text)
            && f.has_glyphs(&FontId::monospace(14.0), text)
    })
}

fn frame(ctx: &egui::Context) {
    ctx.run_ui(egui::RawInput::default(), |_| {})
        .textures_delta
        .clear();
}

#[test]
fn unifont_from_the_game_root_covers_hangul_in_both_font_families() {
    let ctx = egui::Context::default();
    frame(&ctx);
    assert!(!covers(&ctx, "한글"), "egui's default fonts have no Hangul");
    assert!(fonts::install(&ctx, &root()));
    frame(&ctx);
    assert!(covers(&ctx, "한글인벤토리걷기"));
    let latin = ctx.fonts_mut(|f| f.has_glyphs(&FontId::proportional(14.0), "abc"));
    assert!(latin, "the default fonts are kept");
}

#[test]
fn missing_font_file_leaves_the_defaults_and_reports_false() {
    let ctx = egui::Context::default();
    assert!(!fonts::install(&ctx, std::path::Path::new("/nonexistent")));
    frame(&ctx);
    assert!(!covers(&ctx, "한글"));
}
