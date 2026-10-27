//! The client's sizes, type scale and colour roles, and the flat, square, motionless dark theme made of them.
//! Every size is a multiple of `U`.

use bn_client_core::Rgb;
use eframe::egui::{self, Color32, FontId, Stroke, TextStyle};

/// The unit every size is a multiple of.
pub const U: f32 = 4.0;
/// Room around a window's content.
pub const WINDOW_MARGIN: i8 = 12;
/// How far a nested row is indented.
pub const INDENT: f32 = 4.0 * U;
/// The height of a control and of a list row without an item.
pub const ROW: f32 = 6.0 * U;
/// The height of a list row that draws its item in a square.
pub const ROW_ITEM: f32 = 7.0 * U;
/// Side of the square a row's item is drawn in.
pub const ITEM: f32 = 6.0 * U;

/// Column captions, keys and units.
pub const CAPTION: f32 = 13.0;
/// Rows and prose.
pub const BODY: f32 = 15.0;
/// Section headings, bright on `BAND`.
pub const SECTION: f32 = 15.0;
/// Window titles.
pub const TITLE: f32 = 18.0;
/// The sidebar, grids, terminal text and banners.
pub const MONO: f32 = 14.0;

/// Side panels and the top bar.
pub const PANEL: Color32 = Color32::from_rgb(0x12, 0x12, 0x12);
/// Windows, dialogs and menus.
pub const DIALOG: Color32 = Color32::from_rgb(0x0c, 0x0c, 0x0c);
/// The band behind a section heading.
pub const BAND: Color32 = Color32::from_rgb(0x1c, 0x1c, 0x1c);
/// The ground of a text box.
pub const FIELD: Color32 = Color32::from_rgb(0x20, 0x20, 0x20);
/// The frame of a window.
pub const BORDER: Color32 = Color32::from_rgb(0x3a, 0x3a, 0x3a);
/// The selected row or chip.
pub const SELECTION: Color32 = Color32::from_rgb(0x12, 0x40, 0x7a);
/// The outline of what the pointer is over.
pub const HOVER: Color32 = Color32::from_rgb(0x5a, 0x5a, 0x5a);
/// The empty part of a gauge.
pub const GAUGE: Color32 = Color32::from_rgb(0x30, 0x30, 0x30);
/// Text without a colour of its own.
pub const TEXT: Color32 = Color32::from_rgb(0xd4, 0xd4, 0xd4);
/// Captions, keys and what is not available.
pub const WEAK: Color32 = Color32::from_rgb(0x8f, 0x8f, 0x8f);
/// The outline of a region drawn over a canvas, and of the picked one.
pub const REGION: Color32 = Color32::from_rgb(0xe8, 0xe8, 0xe8);
pub const REGION_PICKED: Color32 = Color32::from_rgb(0xff, 0xb6, 0x2e);
pub const GOOD: Color32 = Color32::from_rgb(110, 210, 110);
pub const BAD: Color32 = Color32::from_rgb(235, 95, 95);

/// The least contrast game text keeps against what it is drawn on (WCAG AA for body text).
const MIN_CONTRAST: f32 = 4.5;

pub fn apply(ctx: &egui::Context) {
    ctx.all_styles_mut(|style| {
        style.animation_time = 0.0;
        // Solid scroll bars always show that a list goes on, instead of fading in when hovered.
        style.spacing.scroll = egui::style::ScrollStyle::solid();
        style.spacing.item_spacing = egui::vec2(2.0 * U, U);
        style.spacing.button_padding = egui::vec2(2.0 * U, U);
        style.spacing.window_margin = egui::Margin::same(WINDOW_MARGIN);
        style.spacing.indent = INDENT;
        style.spacing.interact_size.y = ROW;
        style.visuals = egui::Visuals::dark();
        let visuals = &mut style.visuals;
        visuals.panel_fill = PANEL;
        visuals.window_fill = DIALOG;
        visuals.window_stroke = Stroke::new(1.0, BORDER);
        visuals.text_edit_bg_color = Some(FIELD);
        visuals.selection.bg_fill = SELECTION;
        visuals.weak_text_color = Some(WEAK);
        visuals.widgets.noninteractive.fg_stroke.color = TEXT;
        visuals.widgets.inactive.fg_stroke.color = TEXT;
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
            (TextStyle::Body, BODY),
            (TextStyle::Button, BODY),
            (TextStyle::Heading, TITLE),
            (TextStyle::Small, CAPTION),
        ] {
            style
                .text_styles
                .insert(text_style, FontId::proportional(size));
        }
        style
            .text_styles
            .insert(TextStyle::Monospace, FontId::monospace(MONO));
    });
}

/// What `ui` draws on: the fill of the nearest frame around it, a window's or a panel's.
pub fn ground(ui: &egui::Ui) -> Color32 {
    ui.stack()
        .iter()
        .map(|s| s.info.frame.fill)
        .find(|fill| fill.is_opaque())
        .unwrap_or(ui.visuals().panel_fill)
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

/// A game text colour on `ground`, what it is drawn on: itself when it reads, else mixed toward white (or black on
/// a light ground) just enough to keep `MIN_CONTRAST`, so its hue stays recognisable.
pub fn readable(c: Rgb, ground: Color32) -> Color32 {
    let fg = Color32::from_rgb(c.0, c.1, c.2);
    let toward = match contrast(Color32::WHITE, ground) >= contrast(Color32::BLACK, ground) {
        true => Color32::WHITE,
        false => Color32::BLACK,
    };
    (0..=32)
        .map(|step| fg.lerp_to_gamma(toward, step as f32 / 32.0))
        .find(|mixed| contrast(*mixed, ground) >= MIN_CONTRAST)
        .unwrap_or(toward)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn game_text_reads_on_its_ground_and_keeps_its_hue() {
        let blue = readable(Rgb(10, 10, 220), PANEL);
        assert!(contrast(blue, PANEL) >= MIN_CONTRAST, "{blue:?}");
        assert!(blue.b() > blue.r() && blue.b() > blue.g(), "{blue:?}");
        let black = readable(Rgb(0, 0, 0), PANEL);
        assert!(contrast(black, PANEL) >= MIN_CONTRAST, "{black:?}");
        // What already reads is untouched, on dark and light grounds alike.
        assert_eq!(
            readable(Rgb(250, 250, 250), PANEL),
            Color32::from_rgb(250, 250, 250)
        );
        let white = Color32::from_rgb(250, 250, 250);
        assert_eq!(readable(Rgb(0, 0, 0), white), Color32::BLACK);
        let yellow = readable(Rgb(240, 220, 80), white);
        assert!(contrast(yellow, white) >= MIN_CONTRAST, "{yellow:?}");
    }

    #[test]
    fn game_text_reads_on_the_selection_bar_it_is_drawn_on() {
        // Dark blue reads on a dialog once lightened, yet that is still too dark on the blue selection bar.
        let navy = Rgb(0, 0, 160);
        let on_dialog = readable(navy, DIALOG);
        assert!(
            contrast(on_dialog, SELECTION) < MIN_CONTRAST,
            "{on_dialog:?}"
        );
        let on_selection = readable(navy, SELECTION);
        assert!(
            contrast(on_selection, SELECTION) >= MIN_CONTRAST,
            "{on_selection:?}"
        );
    }

    #[test]
    fn every_text_role_reads_on_every_ground() {
        for fg in [TEXT, WEAK, GOOD, BAD] {
            for ground in [PANEL, DIALOG, BAND, FIELD] {
                assert!(
                    contrast(fg, ground) >= MIN_CONTRAST,
                    "{fg:?} on {ground:?}: {}",
                    contrast(fg, ground)
                );
            }
        }
    }
}
