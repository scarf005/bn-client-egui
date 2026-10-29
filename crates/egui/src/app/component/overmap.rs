//! The overmap component and the map's place in a screen.

use super::*;
use crate::app::{CELL, rgb};
use bn_client_core::OvermapWatch;
use bn_client_core::localise::{Value as Arg, tr, tr_format};
use bn_client_core::protocol::{OvermapNote, OvermapWindow};

/// The client's own map window while it is open.
#[derive(Clone, Copy, Debug)]
pub(in crate::app) struct WorldMap {
    /// How far it is panned off the player, in squares.
    pub pan: Pos,
    /// The part of a square dragged past.
    pub drag: egui::Vec2,
    /// The levels it shows above the player's, below when negative.
    pub level: i32,
}

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

/// Where the overmap's shown squares sit on the screen.
struct OvermapGrid {
    origin: egui::Pos2,
    cell: f32,
    window: OvermapWindow,
}

impl OvermapGrid {
    fn rect_of(&self, (col, row): (usize, usize)) -> egui::Rect {
        let offset = egui::vec2(
            (col - self.window.left) as f32,
            (row - self.window.top) as f32,
        );
        egui::Rect::from_min_size(
            self.origin + offset * self.cell,
            egui::Vec2::splat(self.cell),
        )
    }

    /// The shown cell under `p`, the nearest one at the edges.
    fn cell_at(&self, p: egui::Pos2) -> (usize, usize) {
        let v = (p - self.origin) / self.cell;
        let w = &self.window;
        (
            w.left + (v.x as usize).min(w.cols - 1),
            w.top + (v.y as usize).min(w.rows - 1),
        )
    }

    fn shows(&self, (col, row): (usize, usize)) -> bool {
        let w = &self.window;
        (w.left..w.left + w.cols).contains(&col) && (w.top..w.top + w.rows).contains(&row)
    }
}

/// What the hover says of a square away from the cursor: where it is, and its note.
fn square_status(om: &OvermapWire, at: &WorldPos) -> String {
    [Some(format!("({}, {}, {})", at.x, at.y, at.z)), om.note(at)]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" ")
}

/// The overmap as a canvas named by the screen's title and described by the cursor's square and the notes.
fn overmap_a11y(
    ui: &egui::Ui,
    response: &egui::Response,
    screen: &Screen,
    om: &OvermapWire,
    status: &str,
) {
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
            [status.to_owned()]
                .into_iter()
                .chain(notes)
                .filter(|t| !t.is_empty())
                .collect::<Vec<_>>()
                .join(" "),
        );
    });
}

/// A note as the side panel lists it.
fn note_line(note: &OvermapNote) -> String {
    format!("({}, {}) {}", note.pos.x, note.pos.y, note.text)
}

impl<M: GameModel> App<M> {
    /// The width of `overmap_notes` with the gap before them: their widest line, none without notes.
    fn overmap_notes_width(&self, ui: &egui::Ui, om: &OvermapWire) -> f32 {
        if om.notes.is_empty() {
            return 0.0;
        }
        let body = egui::TextStyle::Body.resolve(ui.style());
        let heading = egui::TextStyle::Heading.resolve(ui.style());
        let notes = om
            .notes
            .iter()
            .map(|note| text_width(ui, &markup::plain(&note_line(note)), &body));
        let widest = notes.fold(text_width(ui, &om.notes_title, &heading), f32::max);
        widest + ui.spacing().item_spacing.x
    }

    /// The notes beside the overmap, each in its colour; a click on a note moves the cursor to it.
    fn overmap_notes(
        &self,
        ui: &mut egui::Ui,
        om: &OvermapWire,
        mover: Option<&Affordance>,
        out: &mut Vec<Intent>,
    ) {
        if om.notes.is_empty() {
            return;
        }
        ui.vertical(|ui| {
            ui.heading(&om.notes_title);
            for note in &om.notes {
                let link = ui.link(self.line(ui, &note_line(note)));
                if let (true, Some(mover)) = (link.clicked(), mover) {
                    out.push(Intent::Move {
                        affordance: mover.id.clone(),
                        at: note.pos.clone(),
                    });
                }
            }
        });
    }
}

impl<M: GameModel> App<M> {
    /// The overmap squares around the cursor, cut to the room the window has: hovering names a square, a
    /// click moves the cursor there, and a click on the cursor runs the map's other affordance (travel). The
    /// engine's lines for the cursor square and its legend stand above them, its notes beside them.
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
            let label = ui.label(line);
            ui.ctx()
                .accesskit_node_builder(label.id, |b| b.set_live(egui::accesskit::Live::Polite));
        }
        let status = status.join(" ");
        for line in &om.legend {
            ui.add(
                egui::Label::new(egui::RichText::new(line).monospace())
                    .wrap_mode(egui::TextWrapMode::Extend),
            );
        }
        // The squares take the room the notes beside them and the map's button under them leave.
        let spacing = ui.spacing();
        let button = (ui.text_style_height(&egui::TextStyle::Button)
            + 2.0 * spacing.button_padding.y)
            .max(spacing.interact_size.y);
        let under = match runner.is_some_and(drawn_button) {
            true => button + spacing.item_spacing.y,
            false => 0.0,
        };
        let space = egui::vec2(
            ui.available_width() - self.overmap_notes_width(ui, om),
            ui.available_height() - under,
        );
        // Fewer squares than the room holds grow, up to twice their size, until they fill it; those that then
        // pass its other side are cut, around the cursor.
        let base = self.display.cell(CELL);
        let cell = (space.x / om.cols.max(1) as f32)
            .max(space.y / om.rows.max(1) as f32)
            .clamp(base, 2.0 * base);
        // The squares that fit, kept around the cursor.
        let fit = |space: f32| ((space / cell) as usize).max(1);
        let room = (fit(space.x), fit(space.y));
        let Some(window) = om.window(&cursor, room) else {
            return;
        };
        ui.horizontal_top(|ui| {
            let size = egui::vec2(window.cols as f32, window.rows as f32) * cell;
            let (response, painter) = ui.allocate_painter(size, egui::Sense::click());
            overmap_a11y(ui, &response, screen, om, &status);
            painter.rect_filled(response.rect, 0.0, egui::Color32::BLACK);
            let grid = OvermapGrid {
                origin: response.rect.min,
                cell,
                window,
            };
            let paints = self.overmap_paints(om, &grid, &cursor, &painter);
            self.screens.overmap.borrow_mut().extend(paints);
            let square_at = |p: egui::Pos2| {
                let (col, row) = grid.cell_at(p);
                om.square(col, row)
            };
            response.clone().on_hover_ui_at_pointer(|ui| {
                if let Some(p) = response.hover_pos() {
                    let at = square_at(p);
                    let text = match at == cursor {
                        true => status.clone(),
                        false => square_status(om, &at),
                    };
                    ui.add(egui::Label::new(text).wrap_mode(egui::TextWrapMode::Extend));
                }
            });
            if let Some(p) = response
                .interact_pointer_pos()
                .filter(|_| response.clicked())
            {
                let at = square_at(p);
                match (at == cursor, runner, mover) {
                    (true, Some(run), _) => out.push(Intent::Run(run.id.clone())),
                    (_, _, Some(mover)) => out.push(Intent::Move {
                        affordance: mover.id.clone(),
                        at,
                    }),
                    _ => {}
                }
                ui.ctx().request_repaint();
            }
            self.overmap_notes(ui, om, mover, out);
        });
    }

    /// What the shown squares draw, in order: the terrain, the route, the names, the mission target's mark and
    /// the cursor.
    fn overmap_paints(
        &self,
        om: &OvermapWire,
        grid: &OvermapGrid,
        cursor: &WorldPos,
        painter: &egui::Painter,
    ) -> Vec<OvermapPaint> {
        let colors = self.model.colors();
        let font = egui::FontId::monospace(grid.cell - 2.0);
        let w = &grid.window;
        let shown = |at: &WorldPos| om.cell_of(at).filter(|&cell| grid.shows(cell));
        let squares = (w.top..w.top + w.rows)
            .flat_map(|row| (w.left..w.left + w.cols).map(move |col| (col, row)))
            .filter_map(|(col, row)| {
                let (glyph, fg, bg) = om.cells.get(row)?.get(col)?;
                let at = om.square(col, row);
                let bg = colors.index(*bg);
                Some(OvermapPaint::Square {
                    painter: painter.clone(),
                    rect: grid.rect_of((col, row)),
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
                })
            });
        let route = om.path.iter().enumerate().filter_map(|(i, step)| {
            let alpha = if i + 1 == om.path.len() { 140 } else { 70 };
            Some(OvermapPaint::Fill {
                painter: painter.clone(),
                rect: grid.rect_of(shown(step)?),
                color: egui::Color32::from_rgba_unmultiplied(0, 200, 255, alpha),
            })
        });
        let names = om.labels.iter().filter_map(|label| {
            Some(OvermapPaint::Text {
                painter: painter.clone(),
                rect: grid.rect_of(shown(&label.pos)?),
                text: label.text.clone(),
                color: colors.index(label.foreground),
                font: font.clone(),
            })
        });
        // The mission's target, marked at the edge of the squares shown when it lies beyond them.
        let target = om.target_mark(w, cursor).map(|mark| OvermapPaint::Text {
            painter: painter.clone(),
            rect: grid.rect_of((mark.col, mark.row)),
            text: mark.arrow.into(),
            color: colors.index(1),
            font: font.clone(),
        });
        let at_cursor = om.cell_of(cursor).unwrap_or((om.cols / 2, om.rows / 2));
        let outline = OvermapPaint::Outline {
            painter: painter.clone(),
            rect: grid.rect_of(at_cursor),
        };
        squares
            .chain(route)
            .chain(names)
            .chain(target)
            .chain([outline])
            .collect()
    }

    /// The client's own map window: the overmap squares around the player, read while the game goes on, so
    /// the player keeps moving with it open. Dragging pans it; the engine's map screen, with travel and notes,
    /// opens from it.
    pub(in crate::app) fn world_map(&mut self, ctx: &egui::Context) {
        let Some(WorldMap {
            mut pan,
            mut drag,
            level,
        }) = self.world_map
        else {
            return self.model.watch_overmap(None);
        };
        let kind = KeyWindow::WorldMap.kind();
        let kind = Some(kind.as_str());
        let spec = WindowSpec {
            closable: true,
            place: Some(self.map_room(ctx)),
            movable: true,
            ..self.window_spec(ctx, kind, &tr("Map"))
        };
        let cell = self.display.cell(CELL);
        let mut screen = false;
        let shown = windows::show(ctx, spec, |ui| {
            ui.horizontal(|ui| {
                if ui.button(tr("Centre")).clicked() {
                    pan = Pos { x: 0, y: 0 };
                }
                let offered = self.model.actions().iter().any(|a| a.id == "map");
                screen = ui
                    .add_enabled(offered, egui::Button::new(tr("Travel and notes")))
                    .clicked();
                if let Some(om) = self.model.watched_overmap() {
                    // TRANSLATORS: the overmap level the map window shows; %d is its z, 0 at the ground.
                    ui.label(tr_format("Level %d", &[Arg::Int(om.origin.z.into())]));
                }
            });
            let room = ui.available_size().max(egui::vec2(12.0, 8.0) * cell);
            // The engine reads at most 64 by 64 squares at once.
            let fit = |space: f32| ((space / cell) as i32).clamp(1, 64);
            self.model.watch_overmap(Some(OvermapWatch {
                cols: fit(room.x),
                rows: fit(room.y),
                pan,
                level,
            }));
            let Some(om) = self.model.watched_overmap() else {
                ui.allocate_space(room);
                return;
            };
            let window = OvermapWindow {
                left: 0,
                top: 0,
                cols: om.cols,
                rows: om.rows,
            };
            let size = egui::vec2(om.cols as f32, om.rows as f32) * cell;
            let (response, painter) = ui.allocate_painter(size, egui::Sense::drag());
            painter.rect_filled(response.rect, 0.0, egui::Color32::BLACK);
            let grid = OvermapGrid {
                origin: response.rect.min,
                cell,
                window,
            };
            let paints = self.overmap_paints(om, &grid, &om.player, &painter);
            self.screens.overmap.borrow_mut().extend(paints);
            // Squares dragged past move the view; what is short of a square is kept for the next frame.
            drag -= response.drag_delta() / cell;
            let whole = egui::vec2(drag.x.trunc(), drag.y.trunc());
            drag -= whole;
            pan.x += whole.x as i32;
            pan.y += whole.y as i32;
        });
        let closed = self.keep_window(ctx, kind, &shown);
        self.world_map =
            self.world_map
                .filter(|_| !closed)
                .map(|map| WorldMap { pan, drag, ..map });
        if closed {
            self.model.watch_overmap(None);
        }
        if screen {
            self.model.invoke_action("map");
        }
        self.paint_overmap();
    }

    /// Paints what the overmap collected, now that the window is drawn.
    pub(super) fn paint_overmap(&mut self) {
        let paints = self.screens.overmap.take();
        if paints.is_empty() {
            return;
        }
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
            let job = crate::text::job(
                &self.model.styled(&lines.join("\n")),
                &font,
                tokens::ground(ui),
            );
            ui.add(egui::Label::new(job).wrap_mode(egui::TextWrapMode::Wrap));
            return;
        }
        let name = match (&map.head.label, map.mode) {
            (Some(label), _) => markup::plain(&label.shown()),
            (None, MapMode::World) => tr("Map"),
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
