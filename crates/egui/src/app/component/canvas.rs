//! The view of a canvas grid: how far it is zoomed and where it is scrolled, kept while its screen stays.

use eframe::egui;

/// How far one point of wheel travel zooms, as the exponent of the scale.
const WHEEL_ZOOM: f32 = 1.0 / 200.0;

/// A canvas as it was drawn last frame.
#[derive(Clone, Copy)]
pub(super) struct View {
    /// Columns and rows drawn: a grid of another size is another document, shown fitted anew.
    pub span: (usize, usize),
    /// The cells' scale from the size that fills the room.
    pub zoom: f32,
    pub cell: f32,
    /// The part of the screen the canvas shows its squares in.
    pub rect: egui::Rect,
    pub offset: egui::Vec2,
}

/// The zoom and scroll offset after this frame's wheel with the pointer over `view`: the point of the grid under
/// the pointer stays under it. `None` when the pointer is elsewhere or the zoom does not change.
pub(super) fn zoomed(
    ui: &egui::Ui,
    view: View,
    base: f32,
    limits: (f32, f32),
) -> Option<(f32, egui::Vec2)> {
    let pointer = ui
        .input(|i| i.pointer.hover_pos())
        .filter(|p| view.rect.contains(*p))?;
    let wheel = ui.input_mut(|i| std::mem::take(&mut i.smooth_scroll_delta).y);
    let factor = ui.input(|i| i.zoom_delta()) * (wheel * WHEEL_ZOOM).exp();
    let zoom = (view.zoom * factor).clamp(limits.0, limits.1);
    if zoom == view.zoom {
        return None;
    }
    let under = (pointer - view.rect.min + view.offset) / view.cell;
    Some((zoom, under * base * zoom - (pointer - view.rect.min)))
}

/// Whether the primary button drags the canvas about instead of painting: Space is held.
pub(super) fn panning(ui: &egui::Ui) -> bool {
    ui.input(|i| i.key_down(egui::Key::Space))
}
