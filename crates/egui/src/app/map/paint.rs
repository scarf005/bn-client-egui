//! The map's paint: the 3D scene or the flat squares, and what the client draws over them.

use super::*;

/// The share of the shorter side of the first-person view the wielded item takes.
const HELD: f32 = 0.3;

/// How large a shot is beside its square on the 3D map.
const SHOT_3D: f32 = 0.35;

/// The wielded item `held` in the right hand of the first-person view in `view`, its sprite pushed out as a block game
/// holds an item; none while the sheet it is cut from loads.
fn held_mesh(
    display: &crate::display::Display,
    pages: &Pages,
    held: &Cell,
    view: egui::Rect,
    season: Option<&str>,
) -> Option<egui::Mesh> {
    let (tileset, _) = display.tileset()?;
    let (layer, _) = crate::display::resolve_layer(tileset, held.layers.last()?);
    let chosen = tileset.choose(&crate::tileset::Query {
        frame: display.frame(),
        ..crate::display::query(&layer, season)
    })?;
    let sprite = chosen.fg.or(chosen.bg)?.sprite;
    let source = iso::scene::source(tileset, sprite)?;
    let outline = pages.outline(&source)?;
    let (texture, uv) = display.sprite_texture(sprite)?;
    let held = iso::held::Held {
        texture,
        uv,
        size: source.src,
        outline: &outline,
    };
    Some(iso::held::mesh(&held, view))
}

/// The 3D scene of the map and the pages it samples.
type Scene3d = (Arc<Scene>, Arc<Pages>);

impl<M: GameModel> App<M> {
    /// The squares: the 3D scene with the glyphs it has no sprite for, or the flat map's sprites and glyphs.
    pub(super) fn paint_map(
        &mut self,
        painter: &egui::Painter,
        canvas: &Canvas,
        shown: Option<Scene3d>,
        season: Option<&str>,
    ) {
        let tiles = self.display.tiles();
        if tiles {
            painter.rect_filled(canvas.rect, 0.0, egui::Color32::BLACK);
        }
        profiling::scope!("map cells");
        if let (Some(v), Some(target)) = (&canvas.view, self.iso.target) {
            if let Some(shown) = shown {
                let stats = self.iso.stats.clone();
                let (scene, pages) = (shown.0.clone(), shown.1.clone());
                // The first-person view looks into the sky the engine describes; looking down, there is none.
                let sky = self.model.sky().filter(|_| v.camera.first_person);
                painter.add(iso::callback(
                    canvas.rect,
                    v,
                    target,
                    shown,
                    (stats, sky.as_ref()),
                ));
                iso::paint_glyphs(painter, canvas.rect, v, &scene, canvas.cell_px);
                if let Some(sky) = sky.as_ref().filter(|_| !iso::sheltered(&self.model)) {
                    // With reduced motion what falls stands still.
                    let still = self.display.reduced_motion();
                    let time = if still {
                        0.0
                    } else {
                        painter.ctx().input(|i| i.time)
                    };
                    iso::paint_weather(painter, canvas.rect, sky, time);
                    if sky.precipitation != Precipitation::None && !still {
                        painter
                            .ctx()
                            .request_repaint_after(std::time::Duration::from_millis(33));
                    }
                }
                iso::paint_bearings(painter, canvas.rect, v, &scene);
                if v.camera.first_person
                    && let Some(held) = self.model.wielded_cell()
                {
                    match held_mesh(&self.display, &pages, &held, canvas.rect, season) {
                        Some(mesh) => {
                            painter.add(mesh);
                        }
                        // The flat picture while the sheet it is cut from loads: the right hand holds it low, so
                        // the bottom edge cuts it.
                        None => {
                            let side = canvas.rect.size().min_elem() * HELD;
                            let corner =
                                canvas.rect.right_bottom() + egui::vec2(-1.5 * side, -0.8 * side);
                            let rect = egui::Rect::from_min_size(corner, egui::Vec2::splat(side));
                            crate::app::sidebar::paint_cell(
                                painter,
                                rect,
                                &mut self.display,
                                &held,
                                season,
                            );
                        }
                    }
                }
            }
            return;
        }
        let squares = match &canvas.diamond {
            // The game's order: row by row down the screen, so what stands lower covers what stands behind it.
            // Sprites reach up above their square, so squares just below the canvas are drawn too.
            Some(d) => {
                let reach = egui::Rect::from_min_max(
                    canvas.rect.min - egui::vec2(canvas.cell_px, 0.0),
                    canvas.rect.max + egui::vec2(canvas.cell_px, 2.0 * canvas.cell_px),
                );
                d.order(canvas.size)
                    .filter(|p| reach.intersects(d.tile(*p)))
                    .collect::<Vec<_>>()
            }
            None => (0..canvas.size.y)
                .flat_map(|y| (0..canvas.size.x).map(move |x| Pos { x, y }))
                .collect(),
        };
        let font = egui::FontId::monospace(match canvas.diamond {
            Some(_) => canvas.cell_px / 2.0 - 2.0,
            None => canvas.cell_px - 2.0,
        });
        for p in squares {
            let cell = self.model.cell(p);
            let square = canvas.rect(p);
            if !(tiles && self.display.paint(painter, square, &cell, season)) {
                if let Some(bg) = cell.bg {
                    painter.rect_filled(square, 0.0, rgb(bg));
                }
                painter.text(
                    square.center(),
                    egui::Align2::CENTER_CENTER,
                    cell.glyph,
                    font.clone(),
                    rgb(cell.color),
                );
            }
            if let Some(health) = cell.health {
                let track = egui::Rect::from_min_max(
                    egui::pos2(square.left(), square.bottom() - 2.0),
                    square.right_bottom(),
                );
                health_bar(painter, track, health);
            }
        }
    }

    /// What the client draws over the squares: the planned route, the clicked square, the avatar where nothing
    /// else draws it, the `cursor` square and the playing animations.
    pub(super) fn paint_overlays(
        &mut self,
        ui: &egui::Ui,
        painter: &egui::Painter,
        canvas: &Canvas,
        route: &[Pos],
        cursor: Option<Pos>,
        now: f64,
    ) {
        let size = canvas.size;
        for (i, p) in route.iter().enumerate() {
            let in_view = (0..size.x).contains(&p.x) && (0..size.y).contains(&p.y);
            if in_view {
                let alpha = if i + 1 == route.len() { 140 } else { 70 };
                canvas.fill(
                    painter,
                    *p,
                    egui::Color32::from_rgba_unmultiplied(0, 200, 255, alpha),
                );
            }
        }
        // The line of fire the engine publishes while aiming, from the avatar to the cursor in the map's own space.
        let origin = self.model.origin();
        let aimed = self.model.screens().last().and_then(|s| s.root.as_ref());
        if let Some(map) = aimed.and_then(Component::cursor_map)
            && let Some(source) = map.source()
        {
            let level = self
                .display
                .tileset()
                .map_or(3.0, |(t, _)| iso::scene::metrics(t).level);
            let square = |p: &WorldPos| FireSquare {
                at: Pos {
                    x: p.x - origin.x,
                    y: p.y - origin.y,
                },
                up: p.z - source.z,
            };
            let squares = std::iter::once(&source)
                .chain(&map.trajectory())
                .map(square)
                .collect::<Vec<_>>();
            let line = canvas.fire_line(&squares, level);
            if line.len() > 1 && squares.len() > 1 {
                let shade = egui::Stroke::new(4.0, egui::Color32::from_black_alpha(170));
                painter.add(egui::Shape::line(line.clone(), shade));
                let orange = egui::Stroke::new(2.0, egui::Color32::from_rgb(255, 170, 0));
                painter.add(egui::Shape::line(line, orange));
            }
            let frame = map.cursor().map(|p| canvas.cursor_frame(square(&p), level));
            let shade = egui::Stroke::new(3.5, egui::Color32::from_black_alpha(170));
            let white = egui::Stroke::new(1.5, egui::Color32::WHITE);
            let frame = frame.into_iter().flatten().collect::<Vec<_>>();
            frame.iter().for_each(|e| {
                painter.line_segment(*e, shade);
            });
            frame.iter().for_each(|e| {
                painter.line_segment(*e, white);
            });
        }
        for mark in self.model.marks() {
            canvas.mark(painter, &mark);
        }
        if let Some(at) = self.marker() {
            canvas.outline(
                painter,
                at,
                egui::Stroke::new(2.0, egui::Color32::from_rgb(0, 200, 255)),
            );
        }
        if let Some(cursor) = cursor {
            canvas.outline(
                painter,
                cursor,
                egui::Stroke::new(1.0, egui::Color32::WHITE),
            );
        }
        // How much of the avatar's activity is done, in a thin bar just over its square.
        if let Some(done) = self.model.activity().and_then(|a| a.done) {
            let square = canvas.rect(self.model.player());
            let track = egui::Rect::from_min_size(
                square.left_top() - egui::vec2(0.0, 6.0),
                egui::vec2(square.width(), 4.0),
            );
            painter.rect_filled(track, 0.0, egui::Color32::from_black_alpha(180));
            let mut filled = track;
            filled.set_width(track.width() * f32::from(done) / 1000.0);
            painter.rect_filled(filled, 0.0, ui.visuals().selection.bg_fill);
        }
        // Effects stand on world squares: placed by the origin of this frame, not of when they arrived.
        let tiles = self.display.tiles();
        // On the 3D map a shot is a small thing flying through the square, not a sprite as large as it.
        let shot = if canvas.view.is_some() && canvas.diamond.is_none() {
            SHOT_3D
        } else {
            1.0
        };
        self.effects.paint(
            painter,
            (now, canvas.cell_px, shot),
            |p| {
                canvas.standing(Pos {
                    x: p.x - origin.x,
                    y: p.y - origin.y,
                })
            },
            &mut self.display,
            !tiles,
        );
        if self.effects.active() {
            ui.ctx().request_repaint();
        }
    }
}
