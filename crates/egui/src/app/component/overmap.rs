//! The overmap component and the map's place in a screen.

use super::*;
use crate::app::{CELL, rgb};

/// One thing the overmap draws, in the order it is drawn.
pub(super) enum OvermapPaint {
    /// A square: its terrain sprite when the tileset has one, else its glyph.
    Square {
        painter: egui::Painter,
        rect: egui::Rect,
        tile: Option<OwnedTile>,
        glyph: String,
        color: Rgb,
        bg: Option<Rgb>,
        /// Something lies over the terrain (the player, a note, the route): the glyph is drawn over a sprite.
        overlay: bool,
        font: egui::FontId,
    },
    Fill {
        painter: egui::Painter,
        rect: egui::Rect,
        color: egui::Color32,
    },
    Outline {
        painter: egui::Painter,
        rect: egui::Rect,
    },
    /// A name or marker drawn over the squares, centred on `rect`.
    Text {
        painter: egui::Painter,
        rect: egui::Rect,
        text: String,
        color: Rgb,
        font: egui::FontId,
    },
}

/// An overmap tile that outlives the frame's data.
pub(super) struct OwnedTile {
    id: String,
    rotation: u8,
    subtile: String,
    fallbacks: Vec<String>,
}

impl<M: GameModel> App<M> {
    /// The overmap squares around the cursor, cut to the room the window has: hovering names a square, a
    /// click moves the cursor there, and a click on the cursor runs the map's other affordance (travel). The
    /// engine's lines for the cursor square and its notes stand beside them.
    pub(super) fn overmap(
        &self,
        ui: &mut egui::Ui,
        screen: &Screen,
        map: &MapComponent,
        om: &OvermapWire,
        out: &mut Vec<Intent>,
    ) {
        let cursor = map.cursor().unwrap_or_else(|| om.origin.clone());
        let ids = &map.head.affordances;
        let by_args = |pos: bool| {
            ids.iter()
                .filter_map(|id| find(screen, id))
                .find(|a| a.args.as_ref().is_some_and(|args| args.pos.is_some()) == pos)
        };
        let (mover, runner) = (by_args(true), by_args(false));
        // What the engine says of the cursor square, a line each.
        let status = om.status();
        for line in &status {
            let label = ui.label(*line);
            ui.ctx()
                .accesskit_node_builder(label.id, |b| b.set_live(egui::accesskit::Live::Polite));
        }
        let status = status.join(" ");

        let cell_px = self.display.cell(CELL);
        let font = egui::FontId::monospace(cell_px - 2.0);
        // The squares that fit, kept around the cursor.
        let room = ui.available_size();
        let fit = |space: f32| ((space / cell_px) as usize).max(1);
        let Some(window) = om.window(&cursor, (fit(room.x * 0.7), fit(ui.available_height())))
        else {
            return;
        };
        let (cursor_col, cursor_row) = om.cell_of(&cursor).unwrap_or((om.cols / 2, om.rows / 2));
        let (left, top, cols, rows) = (window.left, window.top, window.cols, window.rows);
        let colors = self.model.colors();
        let square_status = |at: &WorldPos| {
            if *at == cursor {
                return status.clone();
            }
            [
                Some(format!("({}, {}, {})", at.x, at.y, at.z)),
                om.note(at).map(str::to_owned),
            ]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(" ")
        };
        ui.horizontal_top(|ui| {
            let (response, painter) = ui.allocate_painter(
                egui::vec2(cols as f32 * cell_px, rows as f32 * cell_px),
                egui::Sense::click(),
            );
            let title = screen
                .title
                .as_ref()
                .map(|t| markup::plain(&t.shown()))
                .unwrap_or_default();
            response.widget_info(|| WidgetInfo::labeled(WidgetType::Other, true, &title));
            ui.ctx().accesskit_node_builder(response.id, |b| {
                b.set_role(egui::accesskit::Role::Canvas);
                let notes = (!om.notes.is_empty())
                    .then(|| [om.notes_title.clone()].into_iter().chain(om.note_lines()))
                    .into_iter()
                    .flatten();
                b.set_description(
                    [status.clone()]
                        .into_iter()
                        .chain(notes)
                        .filter(|t| !t.is_empty())
                        .collect::<Vec<_>>()
                        .join(" "),
                );
            });
            let rect_of = |col: usize, row: usize| {
                egui::Rect::from_min_size(
                    response.rect.min
                        + egui::vec2((col - left) as f32, (row - top) as f32) * cell_px,
                    egui::Vec2::splat(cell_px),
                )
            };
            painter.rect_filled(response.rect, 0.0, egui::Color32::BLACK);
            let mut paints = self.screens.overmap.borrow_mut();
            for row in top..top + rows {
                for col in left..left + cols {
                    let Some((glyph, fg, bg)) = om.cells.get(row).and_then(|r| r.get(col)) else {
                        continue;
                    };
                    let at = om.square(col, row);
                    let bg = colors.index(*bg);
                    paints.push(OvermapPaint::Square {
                        painter: painter.clone(),
                        rect: rect_of(col, row),
                        tile: om.tile(col, row).map(|t| OwnedTile {
                            id: t.id.into(),
                            rotation: t.rotation,
                            subtile: t.subtile.into(),
                            fallbacks: t.fallbacks.to_vec(),
                        }),
                        glyph: glyph.clone(),
                        color: colors.index(*fg),
                        bg: (bg != Rgb(0, 0, 0)).then_some(bg),
                        overlay: at == om.player || om.note(&at).is_some() || om.path.contains(&at),
                        font: font.clone(),
                    });
                }
            }
            for (i, step) in om.path.iter().enumerate() {
                let Some((col, row)) = om.cell_of(step).filter(|(c, r)| {
                    (left..left + cols).contains(c) && (top..top + rows).contains(r)
                }) else {
                    continue;
                };
                let alpha = if i + 1 == om.path.len() { 140 } else { 70 };
                paints.push(OvermapPaint::Fill {
                    painter: painter.clone(),
                    rect: rect_of(col, row),
                    color: egui::Color32::from_rgba_unmultiplied(0, 200, 255, alpha),
                });
            }
            for label in &om.labels {
                let Some((col, row)) = om.cell_of(&label.pos).filter(|(c, r)| {
                    (left..left + cols).contains(c) && (top..top + rows).contains(r)
                }) else {
                    continue;
                };
                paints.push(OvermapPaint::Text {
                    painter: painter.clone(),
                    rect: rect_of(col, row),
                    text: label.text.clone(),
                    color: colors.index(label.foreground),
                    font: font.clone(),
                });
            }
            // The mission's target, marked at the edge of the squares shown when it lies beyond them.
            if let Some(mark) = om.target_mark(&window, &cursor) {
                paints.push(OvermapPaint::Text {
                    painter: painter.clone(),
                    rect: rect_of(mark.col, mark.row),
                    text: mark.arrow.into(),
                    color: colors.index(1),
                    font: font.clone(),
                });
            }
            paints.push(OvermapPaint::Outline {
                painter: painter.clone(),
                rect: rect_of(cursor_col, cursor_row),
            });
            drop(paints);
            let square_at = |p: egui::Pos2| {
                let v = (p - response.rect.min) / cell_px;
                om.square(
                    left + (v.x as usize).min(cols - 1),
                    top + (v.y as usize).min(rows - 1),
                )
            };
            response.clone().on_hover_ui_at_pointer(|ui| {
                if let Some(p) = response.hover_pos() {
                    ui.add(
                        egui::Label::new(square_status(&square_at(p)))
                            .wrap_mode(egui::TextWrapMode::Extend),
                    );
                }
            });
            if let Some(p) = response
                .interact_pointer_pos()
                .filter(|_| response.clicked())
            {
                let at = square_at(p);
                match (at == cursor, &runner, &mover) {
                    (true, Some(run), _) => out.push(Intent::Run(run.id.clone())),
                    (_, _, Some(mover)) => out.push(Intent::Move {
                        affordance: mover.id.clone(),
                        at,
                    }),
                    _ => {}
                }
                ui.ctx().request_repaint();
            }
            ui.vertical(|ui| {
                for line in &om.legend {
                    ui.add(
                        egui::Label::new(egui::RichText::new(line).monospace())
                            .wrap_mode(egui::TextWrapMode::Extend),
                    );
                }
                if !om.notes.is_empty() {
                    ui.separator();
                    ui.heading(&om.notes_title);
                }
                for note in &om.notes {
                    let link = ui.link(format!("({}, {}) {}", note.pos.x, note.pos.y, note.text));
                    if let (true, Some(mover)) = (link.clicked(), &mover) {
                        out.push(Intent::Move {
                            affordance: mover.id.clone(),
                            at: note.pos.clone(),
                        });
                    }
                }
            });
        });
    }

    /// Paints what the overmap collected, now that the window is drawn.
    pub(super) fn paint_overmap(&mut self, ctx: &egui::Context) {
        let paints = self.screens.overmap.take();
        if paints.is_empty() {
            return;
        }
        self.display.update(ctx);
        self.display.begin();
        for paint in paints {
            match paint {
                OvermapPaint::Square {
                    painter,
                    rect,
                    tile,
                    glyph,
                    color,
                    bg,
                    overlay,
                    font,
                } => {
                    // With a tileset the terrain is its sprite; what lies over it stays a glyph.
                    let sprite = tile.is_some_and(|t| {
                        self.display.paint_overmap(
                            &painter,
                            rect,
                            &OvermapTile {
                                id: &t.id,
                                rotation: t.rotation,
                                subtile: &t.subtile,
                                fallbacks: &t.fallbacks,
                            },
                        )
                    });
                    if sprite && !overlay {
                        continue;
                    }
                    if let Some(bg) = bg.filter(|_| !sprite) {
                        painter.rect_filled(rect, 0.0, rgb(bg));
                    }
                    if !glyph.trim().is_empty() {
                        painter.text(
                            rect.center(),
                            egui::Align2::CENTER_CENTER,
                            glyph,
                            font,
                            rgb(color),
                        );
                    }
                }
                OvermapPaint::Fill {
                    painter,
                    rect,
                    color,
                } => {
                    painter.rect_filled(rect, 0.0, color);
                }
                OvermapPaint::Text {
                    painter,
                    rect,
                    text,
                    color,
                    font,
                } => {
                    painter.text(
                        rect.center(),
                        egui::Align2::CENTER_CENTER,
                        text,
                        font,
                        rgb(color),
                    );
                }
                OvermapPaint::Outline { painter, rect } => {
                    painter.rect_stroke(
                        rect,
                        0.0,
                        egui::Stroke::new(1.0, egui::Color32::WHITE),
                        egui::StrokeKind::Inside,
                    );
                }
            }
        }
    }

    /// The map is the map view's; here it is the place it would take, or the words the engine says of the
    /// cursor's square when the screen puts a cursor on it.
    pub(super) fn map_slot(
        &self,
        ui: &mut egui::Ui,
        screen: &Screen,
        map: &MapComponent,
        out: &mut Vec<Intent>,
    ) {
        if let (MapMode::Overmap, Some(om)) = (map.mode, &map.overmap) {
            return self.overmap(ui, screen, map, om, out);
        }
        let lines = map.cursor_lines();
        if !lines.is_empty() {
            let font = egui::TextStyle::Body.resolve(ui.style());
            let job = text::job(&self.model.styled(&lines.join("\n")), &font);
            ui.add(egui::Label::new(job).wrap_mode(egui::TextWrapMode::Wrap));
            return;
        }
        let name = match (&map.head.label, map.mode) {
            (Some(label), _) => markup::plain(&label.shown()),
            (None, MapMode::World) => "Map".into(),
            (None, MapMode::Overmap) => screen
                .title
                .as_ref()
                .map(|t| markup::plain(&t.shown()))
                .unwrap_or_default(),
        };
        let size = egui::vec2(ui.available_width().min(480.0), 120.0);
        let (rect, response) = ui.allocate_exact_size(size, egui::Sense::hover());
        let color = ui.visuals().weak_text_color();
        ui.painter().rect_stroke(
            rect,
            0,
            egui::Stroke::new(1.0, color),
            egui::StrokeKind::Inside,
        );
        ui.painter().text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            &name,
            egui::TextStyle::Body.resolve(ui.style()),
            color,
        );
        response.widget_info(|| WidgetInfo::labeled(WidgetType::Label, true, &name));
    }
}
