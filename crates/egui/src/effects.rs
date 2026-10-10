//! Short overlays the native game plays on the map: shots, explosions and floating combat text.

use bn_client_core::{Effect, Layer, Pos, Rgb, Shape};
use eframe::egui::{self, Color32, Rect};

/// The native step delay is a few milliseconds; a person needs at least this long to see a step.
const SHOT_STEP: f64 = 0.04;
const BURST_STEP: f64 = 0.06;
const BLAST: f64 = 0.12;
const TEXT: f64 = 0.7;
/// Sequential effects never queue further ahead than this, so a long fight does not replay for minutes.
const MAX_QUEUE: f64 = 1.5;

struct Playing {
    shape: Shape,
    start: f64,
    life: f64,
}

/// The effects now on the map, in the order the game produced them.
#[derive(Default)]
pub struct Effects {
    playing: Vec<Playing>,
    /// When the last shot or explosion ends; the next one starts then.
    tail: f64,
}

fn life(effect: &Effect) -> f64 {
    let native = effect.duration_ms as f64 / 1000.0;
    match &effect.shape {
        Shape::Projectile { path, .. } => native.max(SHOT_STEP * path.len() as f64),
        Shape::Burst { radius, .. } => native.max(BURST_STEP * (f64::from(*radius) + 1.0)),
        Shape::Blast { .. } => native.max(BLAST),
        Shape::Text { .. } => native.max(TEXT),
    }
}

fn tint(color: Rgb, alpha: f64) -> Color32 {
    Color32::from_rgba_unmultiplied(
        color.0,
        color.1,
        color.2,
        (alpha.clamp(0.0, 1.0) * 255.0) as u8,
    )
}

/// Tileset sprites for the effects; ASCII mode has none.
pub trait Sprites {
    /// Whether the tileset has a sprite for the layer.
    fn has(&self, layer: &Layer) -> bool;
    /// Draws the tile into `square` when the tileset has it.
    fn tile(&mut self, painter: &egui::Painter, square: Rect, layer: &Layer) -> bool;
}

/// The native tile selection of an explosion square from which of its four neighbours blast too.
fn explosion_layer(id: &str, at: Pos, cells: &[Pos]) -> Layer {
    let has = |dx: i32, dy: i32| {
        cells.contains(&Pos {
            x: at.x + dx,
            y: at.y + dy,
        })
    };
    let (n, s, w, e) = (has(0, -1), has(0, 1), has(-1, 0), has(1, 0));
    let (subtile, rotation) = match (n, s, w, e) {
        (true, true, _, _) if !(w && e) => ("edge", 1),
        (_, _, true, true) if !(n && s) => ("edge", 0),
        (true, false, false, false) | (false, true, false, false) => ("edge", 1),
        (false, false, true, false) | (false, false, false, true) => ("edge", 0),
        (false, true, false, true) => ("corner", 0),
        (true, false, false, true) => ("corner", 1),
        (true, false, true, false) => ("corner", 2),
        (false, true, true, false) => ("corner", 3),
        _ => ("edge", 0),
    };
    Layer {
        id: id.to_string(),
        subtile: Some(subtile.to_string()),
        rotation,
        glyph: '\0',
        ..Default::default()
    }
}

impl Effects {
    /// Takes the effects the game produced; with `reduced_motion` they are dropped unseen.
    pub fn add(&mut self, now: f64, produced: Vec<Effect>, reduced_motion: bool) {
        if reduced_motion {
            self.playing.clear();
            return;
        }
        self.tail = self.tail.max(now).min(now + MAX_QUEUE);
        for effect in produced {
            let life = life(&effect);
            // Text rises where it is told to; shots and blasts follow one another.
            let start = if matches!(effect.shape, Shape::Text { .. }) {
                now
            } else {
                self.tail
            };
            if !matches!(effect.shape, Shape::Text { .. }) {
                self.tail = start + life;
            }
            self.playing.push(Playing {
                shape: effect.shape,
                start,
                life,
            });
        }
        self.playing.retain(|p| now < p.start + p.life);
    }

    pub fn len(&self) -> usize {
        self.playing.len()
    }

    pub fn active(&self) -> bool {
        !self.playing.is_empty()
    }

    /// Draws what is playing at `now`. `cell` is the screen square of a view square;
    /// `sprite` paints a projectile with the tileset and tells whether it could.
    pub fn paint(
        &self,
        painter: &egui::Painter,
        now: f64,
        cell_px: f32,
        cell: impl Fn(Pos) -> Rect,
        sprites: &mut dyn Sprites,
    ) {
        let font = egui::FontId::monospace(cell_px - 2.0);
        for p in self.playing.iter().filter(|p| now >= p.start) {
            let t = ((now - p.start) / p.life).clamp(0.0, 1.0);
            match &p.shape {
                Shape::Projectile {
                    path,
                    glyph,
                    sprite,
                    color,
                } => {
                    let Some(at) = path
                        .get(((t * path.len() as f64) as usize).min(path.len().saturating_sub(1)))
                    else {
                        continue;
                    };
                    let square = cell(*at);
                    let layer = Layer {
                        id: sprite
                            .clone()
                            .unwrap_or_else(|| "animation_bullet_normal".into()),
                        fallbacks: vec!["animation_bullet_normal".into()],
                        glyph: '\0',
                        ..Default::default()
                    };
                    if !sprites.tile(painter, square, &layer) {
                        painter.text(
                            square.center(),
                            egui::Align2::CENTER_CENTER,
                            *glyph,
                            font.clone(),
                            tint(*color, 1.0),
                        );
                    }
                }
                Shape::Burst {
                    at,
                    radius,
                    color,
                    tile,
                } => {
                    let reach = (f64::from(*radius) * t).ceil() as i32;
                    // The native ring: corners and edges of each square ring, as the tileset draws it.
                    let ring = |r: i32| -> Vec<(Pos, &'static str, u8)> {
                        let mut out = vec![
                            (
                                Pos {
                                    x: at.x - r,
                                    y: at.y - r,
                                },
                                "corner",
                                0,
                            ),
                            (
                                Pos {
                                    x: at.x - r,
                                    y: at.y + r,
                                },
                                "corner",
                                1,
                            ),
                            (
                                Pos {
                                    x: at.x + r,
                                    y: at.y + r,
                                },
                                "corner",
                                2,
                            ),
                            (
                                Pos {
                                    x: at.x + r,
                                    y: at.y - r,
                                },
                                "corner",
                                3,
                            ),
                        ];
                        for j in 1 - r..r {
                            out.push((
                                Pos {
                                    x: at.x + j,
                                    y: at.y - r,
                                },
                                "edge",
                                0,
                            ));
                            out.push((
                                Pos {
                                    x: at.x + j,
                                    y: at.y + r,
                                },
                                "edge",
                                0,
                            ));
                            out.push((
                                Pos {
                                    x: at.x - r,
                                    y: at.y + j,
                                },
                                "edge",
                                1,
                            ));
                            out.push((
                                Pos {
                                    x: at.x + r,
                                    y: at.y + j,
                                },
                                "edge",
                                1,
                            ));
                        }
                        out
                    };
                    let sprite_layer = |subtile: &str, rotation: u8| Layer {
                        id: tile.clone(),
                        subtile: Some(subtile.to_string()),
                        rotation,
                        glyph: '\0',
                        ..Default::default()
                    };
                    if reach > 0 && sprites.has(&sprite_layer("edge", 0)) {
                        for r in 1..=reach {
                            for (square, subtile, rotation) in ring(r) {
                                sprites.tile(
                                    painter,
                                    cell(square),
                                    &sprite_layer(subtile, rotation),
                                );
                            }
                        }
                    } else {
                        for dy in -reach..=reach {
                            for dx in -reach..=reach {
                                if dx * dx + dy * dy <= reach * reach + reach {
                                    let square = cell(Pos {
                                        x: at.x + dx,
                                        y: at.y + dy,
                                    });
                                    painter.rect_filled(square, 0.0, tint(*color, 0.6 * (1.0 - t)));
                                }
                            }
                        }
                    }
                }
                Shape::Blast {
                    cells,
                    shrapnel,
                    tile,
                } => {
                    let color = if *shrapnel {
                        Rgb(240, 240, 240)
                    } else {
                        Rgb(255, 160, 40)
                    };
                    for at in cells {
                        let square = cell(*at);
                        if !*shrapnel
                            && sprites.tile(painter, square, &explosion_layer(tile, *at, cells))
                        {
                            continue;
                        }
                        painter.rect_filled(square, 0.0, tint(color, 0.6 * (1.0 - t)));
                    }
                }
                Shape::Text { at, segments } => {
                    let mut square = cell(*at);
                    square = square.translate(egui::vec2(0.0, -cell_px * 1.5 * t as f32));
                    let mut x = square.center().x
                        - cell_px
                            * 0.3
                            * segments.iter().map(|s| s.0.chars().count()).sum::<usize>() as f32;
                    for (text, color) in segments {
                        let rect = painter.text(
                            egui::pos2(x, square.min.y),
                            egui::Align2::LEFT_BOTTOM,
                            text,
                            font.clone(),
                            tint(*color, 1.0 - t * t),
                        );
                        x = rect.max.x;
                    }
                }
            }
        }
    }
}

impl Sprites for crate::display::Display {
    fn has(&self, layer: &Layer) -> bool {
        self.has_sprite(layer)
    }

    fn tile(&mut self, painter: &egui::Painter, square: Rect, layer: &Layer) -> bool {
        self.paint_sprite(painter, square, layer)
    }
}
