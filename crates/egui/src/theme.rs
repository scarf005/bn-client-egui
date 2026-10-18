//! Flat, square, motionless dark theme with game-sized text.

use bn_client_core::Rgb;
use eframe::egui::{self, Color32, FontId, Stroke, TextStyle};

/// Side panels and the top bar.
pub const PANEL: Color32 = Color32::from_rgb(0x12, 0x12, 0x12);
/// Windows, dialogs and menus.
pub const DIALOG: Color32 = Color32::from_rgb(0x0c, 0x0c, 0x0c);
/// The frame of a window.
pub const BORDER: Color32 = Color32::from_rgb(0x3a, 0x3a, 0x3a);
/// The selected row or chip.
pub const SELECTION: Color32 = Color32::from_rgb(0x12, 0x40, 0x7a);
/// The outline of what the pointer is over.
pub const HOVER: Color32 = Color32::from_rgb(0x5a, 0x5a, 0x5a);
/// The empty part of a gauge.
pub const GAUGE: Color32 = Color32::from_rgb(0x30, 0x30, 0x30);

/// The least contrast game text keeps against what it is drawn on (WCAG AA for body text).
const MIN_CONTRAST: f32 = 4.5;

pub fn apply(ctx: &egui::Context) {
    ctx.all_styles_mut(|style| {
        style.animation_time = 0.0;
        // Solid scroll bars always show that a list goes on, instead of fading in when hovered.
        style.spacing.scroll = egui::style::ScrollStyle::solid();
        style.spacing.item_spacing = egui::vec2(6.0, 4.0);
        style.spacing.button_padding = egui::vec2(8.0, 4.0);
        style.visuals = egui::Visuals::dark();
        let visuals = &mut style.visuals;
        visuals.panel_fill = PANEL;
        visuals.window_fill = DIALOG;
        visuals.window_stroke = Stroke::new(1.0, BORDER);
        visuals.selection.bg_fill = SELECTION;
        visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, HOVER);
        visuals.window_corner_radius = egui::CornerRadius::ZERO;
        visuals.menu_corner_radius = egui::CornerRadius::ZERO;
        visuals.window_shadow = egui::Shadow::NONE;
        visuals.popup_shadow = egui::Shadow::NONE;
        for widget in [
            &mut visuals.widgets.noninteractive,
            &mut visuals.widgets.inactive,
            &mut visuals.widgets.hovered,
            &mut visuals.widgets.active,
            &mut visuals.widgets.open,
        ] {
            widget.corner_radius = egui::CornerRadius::ZERO;
        }
        for (text_style, size) in [
            (TextStyle::Body, 15.0),
            (TextStyle::Button, 15.0),
            (TextStyle::Heading, 15.0),
            (TextStyle::Small, 13.0),
        ] {
            style
                .text_styles
                .insert(text_style, FontId::proportional(size));
        }
        style
            .text_styles
            .insert(TextStyle::Monospace, FontId::monospace(15.0));
    });
}

/// WCAG relative luminance.
fn luminance(c: Color32) -> f32 {
    let linear = |v: u8| {
        let v = f32::from(v) / 255.0;
        if v <= 0.040_45 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * linear(c.r()) + 0.7152 * linear(c.g()) + 0.0722 * linear(c.b())
}

fn contrast(a: Color32, b: Color32) -> f32 {
    let (a, b) = (luminance(a), luminance(b));
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}

/// A game text colour on `bg` (the panel when none): itself when it reads, else mixed toward white (or black on a
/// light ground) just enough to keep `MIN_CONTRAST`, so its hue stays recognisable.
pub fn readable(c: Rgb, bg: Option<Rgb>) -> Color32 {
    let fg = Color32::from_rgb(c.0, c.1, c.2);
    let bg = bg.map_or(PANEL, |b| Color32::from_rgb(b.0, b.1, b.2));
    let toward = match contrast(Color32::WHITE, bg) >= contrast(Color32::BLACK, bg) {
        true => Color32::WHITE,
        false => Color32::BLACK,
    };
    (0..=32)
        .map(|step| fg.lerp_to_gamma(toward, step as f32 / 32.0))
        .find(|mixed| contrast(*mixed, bg) >= MIN_CONTRAST)
        .unwrap_or(toward)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn game_text_reads_on_its_ground_and_keeps_its_hue() {
        let blue = readable(Rgb(10, 10, 220), None);
        assert!(contrast(blue, PANEL) >= MIN_CONTRAST, "{blue:?}");
        assert!(blue.b() > blue.r() && blue.b() > blue.g(), "{blue:?}");
        let black = readable(Rgb(0, 0, 0), None);
        assert!(contrast(black, PANEL) >= MIN_CONTRAST, "{black:?}");
        // What already reads is untouched, on dark and light grounds alike.
        assert_eq!(
            readable(Rgb(250, 250, 250), None),
            Color32::from_rgb(250, 250, 250)
        );
        let on_white = Some(Rgb(250, 250, 250));
        assert_eq!(readable(Rgb(0, 0, 0), on_white), Color32::BLACK);
        let yellow = readable(Rgb(240, 220, 80), on_white);
        assert!(
            contrast(yellow, Color32::from_rgb(250, 250, 250)) >= MIN_CONTRAST,
            "{yellow:?}"
        );
    }
}
