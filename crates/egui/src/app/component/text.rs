//! The text components: a block of the engine's text, and the engine's own window drawn as rows of coloured
//! cells (`canvas`).

use super::*;

impl<M: GameModel> App<M> {
    pub(super) fn text_block(&self, ui: &mut egui::Ui, block: &TextBlock) {
        let (font, wrap) = match block.role {
            Some(TextRole::TerminalOutput | TextRole::Banner) => (
                egui::TextStyle::Monospace.resolve(ui.style()),
                egui::TextWrapMode::Extend,
            ),
            Some(TextRole::Prompt) | None => (
                egui::TextStyle::Body.resolve(ui.style()),
                egui::TextWrapMode::Wrap,
            ),
        };
        let ground = tokens::ground(ui);
        let job = self.styled_job(&block.markup.shown(), &font, ground);
        let show = |ui: &mut egui::Ui| {
            ui.scope(|ui| {
                if let Some(tone) = block.tone {
                    ui.visuals_mut().override_text_color = Some(tone_color(tone));
                }
                ui.add(egui::Label::new(job).wrap_mode(wrap));
            });
        };
        // A banner is the screen's heading and keeps its height; any other text scrolls past a window's.
        if block.role == Some(TextRole::Banner) {
            ui.vertical_centered(show);
        } else {
            // The viewport is a whole number of lines tall, so none is cut across.
            let line = ui.fonts_mut(|f| f.row_height(&font));
            let lines = (ui.available_height() / line).floor().max(1.0);
            egui::ScrollArea::vertical()
                .max_height(lines * line)
                .auto_shrink(true)
                .show(ui, show);
        }
    }

    /// The engine's own window, a row of colored cells at a time, in a fixed-width font as large as
    /// lets the whole grid fit the window, so every cell keeps its column and row at any size.
    pub(super) fn canvas(&self, ui: &mut egui::Ui, canvas: &CanvasComponent) {
        // The room is what the dialog offers its list; it does not depend on the dialog's own size,
        // which follows the canvas.
        // A row's height rounds up to a whole pixel, which a row of slack each pays for.
        let room = egui::vec2(
            ui.ctx().content_rect().width() * 0.9,
            ui.available_height() - canvas.rows.len() as f32,
        );
        let unit = 100.0;
        let cell = ui
            .painter()
            .layout_no_wrap(
                "0".into(),
                egui::FontId::monospace(unit),
                egui::Color32::WHITE,
            )
            .size()
            / unit;
        let fit = (room.x / (canvas.cols.max(1) as f32 * cell.x))
            .min(room.y / (canvas.rows.len().max(1) as f32 * cell.y));
        let font = egui::FontId::monospace(fit.clamp(10.0, 40.0));
        let colors = self.model.colors();
        let paint = |index: u8| {
            let c = colors.index(index);
            egui::Color32::from_rgb(c.0, c.1, c.2)
        };
        let window = tokens::ground(ui);
        let ink = |fg: u8, bg: u8| {
            let ground = if bg == 0 { window } else { paint(bg) };
            tokens::readable(colors.index(fg), ground)
        };
        // The rows touch; what follows the canvas keeps the theme's spacing.
        ui.scope(|ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            for row in &canvas.rows {
                let mut job = egui::text::LayoutJob::default();
                for (text, fg, bg) in row {
                    job.append(
                        text,
                        0.0,
                        egui::TextFormat {
                            color: ink(*fg, *bg),
                            background: if *bg == 0 {
                                egui::Color32::TRANSPARENT
                            } else {
                                paint(*bg)
                            },
                            font_id: font.clone(),
                            ..Default::default()
                        },
                    );
                }
                // An empty row keeps its height.
                if row.is_empty() {
                    job.append(
                        " ",
                        0.0,
                        egui::TextFormat::simple(font.clone(), egui::Color32::BLACK),
                    );
                }
                ui.add(egui::Label::new(job).wrap_mode(egui::TextWrapMode::Extend));
            }
        });
    }
}
