//! Short overlays the native game plays on the map: shots, explosions and floating combat text.
//!
//! What is on screen at a moment is first worked out as plain `Op`s, so a frame can be dumped and
//! checked without a window; `paint` only draws them.

use bn_client_core::{Effect, Layer, Pos, Rgb, Shape};
use eframe::egui::{self, Color32, Rect};

/// The native step delay is a few milliseconds; a person needs at least this long to see a step.
const SHOT_STEP: f64 = 0.04;
const BURST_STEP: f64 = 0.06;
const BLAST: f64 = 0.12;
/// How long one step of floating text lasts; the native text moves for `MAX_STEPS` steps.
const TEXT_STEP: f64 = 0.08;
const MAX_STEPS: i64 = 8;
/// Sequential effects never queue further ahead than this, so a long fight does not replay for minutes.
const MAX_QUEUE: f64 = 1.5;

/// One thing to draw, in view squares.
#[derive(Clone, Debug, PartialEq)]
pub enum Op {
    Rect {
        at: Pos,
        color: Rgb,
        alpha: f32,
    },
    Glyph {
        at: Pos,
        ch: char,
        color: Rgb,
        alpha: f32,
    },
    /// A tileset sprite, or what stands in for it when the tileset has none.
    Tile {
        at: Pos,
        layer: Box<Layer>,
        or: Box<Op>,
    },
}

struct Playing {
    shape: Shape,
    start: f64,
    life: f64,
}

/// Floating text as the native `scrolling_combat_text` keeps it.
struct Text {
    at: Pos,
    segments: Vec<(String, Rgb)>,
    dir: Pos,
    hp: bool,
    start: f64,
    /// Steps this text waits so that texts moving the same way do not overlap.
    offset: i64,
}

/// The effects now on the map, in the order the game produced them.
#[derive(Default)]
pub struct Effects {
    playing: Vec<Playing>,
    texts: Vec<Text>,
    /// When the last shot or explosion ends; the next one starts then.
    tail: f64,
}

fn life(effect: &Effect) -> f64 {
    let native = effect.duration_ms as f64 / 1000.0;
    match &effect.shape {
        Shape::Projectile { path, .. } => native.max(SHOT_STEP * path.len() as f64),
        Shape::Burst { radius, .. } => native.max(BURST_STEP * (f64::from(*radius) + 1.0)),
        Shape::Blast { .. } => native.max(BLAST),
        Shape::Text { .. } => 0.0,
    }
}

/// The direction the native text really scrolls in: a hit-point readout stays to one side of its
/// creature, and text meant for a side moves diagonally up or down so that the sides stay free.
fn text_direction(scroll: Pos, hp: bool, seed: u32) -> Pos {
    let pick = |a: Pos, b: Pos| if seed.is_multiple_of(2) { a } else { b };
    match (hp, scroll.x, scroll.y) {
        (true, x, _) if x < 0 => Pos { x: -1, y: 0 },
        (true, _, _) => Pos { x: 1, y: 0 },
        (false, 1, 0) => pick(Pos { x: 1, y: -1 }, Pos { x: 1, y: 1 }),
        (false, -1, 0) => pick(Pos { x: -1, y: -1 }, Pos { x: -1, y: 1 }),
        (false, 0, 0) => Pos { x: -1, y: 0 },
        _ => scroll,
    }
}

impl Text {
    fn step(&self, now: f64) -> i64 {
        ((now - self.start) / TEXT_STEP).floor() as i64
    }

    /// The segments in reading order: text moving left puts its second part first.
    fn parts(&self) -> Vec<(String, Rgb)> {
        let [first, second] = match self.segments.as_slice() {
            [a] => return vec![a.clone()],
            [a, b] => [a, b],
            _ => return self.segments.clone(),
        };
        if self.dir.x < 0 {
            vec![(format!("{} ", second.0), second.1), first.clone()]
        } else {
            vec![first.clone(), (format!(" {}", second.0), second.1)]
        }
    }

    /// Where the text starts, in squares, and whether it is old enough to dim; none until it moves.
    fn place(&self, now: f64) -> Option<(Pos, bool)> {
        let step = self.step(now);
        if step <= 0 || step > MAX_STEPS {
            return None;
        }
        let width = self
            .parts()
            .iter()
            .map(|p| p.0.chars().count() as i32)
            .sum::<i32>();
        let (dx, dy) = (self.dir.x, self.dir.y);
        let mut x = match (dx, dy) {
            (1, 0) => 1,
            (-1, 0) => -1,
            (0, _) => -(width / 2),
            _ => 0,
        };
        if dx < 0 {
            x -= width - 1;
        }
        let y = match (dx, dy) {
            (0, 1) => 1,
            (0, -1) => -1,
            _ => 0,
        };
        let moved = if self.hp {
            self.offset + 1
        } else {
            self.offset + step
        };
        Some((
            Pos {
                x: self.at.x + x + dx * moved as i32,
                y: self.at.y + y + dy * (self.offset + step) as i32,
            },
            step >= MAX_STEPS / 2,
        ))
    }
}

/// The glyph the native curses explosion draws on a square, from which neighbours blast too.
fn curses_glyph(n: bool, s: bool, w: bool, e: bool) -> char {
    match (n, s, w, e) {
        (true, true, true, true) => ' ',
        (false, false, false, false) => '*',
        (_, _, true, true) if !(n && s) => '-',
        (true, true, _, _) => '|',
        (true, false, false, false) | (false, true, false, false) => '|',
        (false, false, true, false) | (false, false, false, true) => '-',
        (false, true, false, true) | (true, false, true, false) => '/',
        _ => '\\',
    }
}

/// The native tile selection of an explosion square from which of its four neighbours blast too.
fn tile_subtile(n: bool, s: bool, w: bool, e: bool) -> (&'static str, u8) {
    match (n, s, w, e) {
        (true, true, _, _) if !(w && e) => ("edge", 1),
        (_, _, true, true) if !(n && s) => ("edge", 0),
        (true, false, false, false) | (false, true, false, false) => ("edge", 1),
        (false, false, true, false) | (false, false, false, true) => ("edge", 0),
        (false, true, false, true) => ("corner", 0),
        (true, false, false, true) => ("corner", 1),
        (true, false, true, false) => ("corner", 2),
        (false, true, true, false) => ("corner", 3),
        _ => ("edge", 0),
    }
}

fn neighbours(at: Pos, cells: &[Pos]) -> (bool, bool, bool, bool) {
    let has = |dx: i32, dy: i32| {
        cells.contains(&Pos {
            x: at.x + dx,
            y: at.y + dy,
        })
    };
    (has(0, -1), has(0, 1), has(-1, 0), has(1, 0))
}

/// What stands in for an explosion square: the curses glyph in ASCII, a tinted square otherwise.
fn blast_fallback(at: Pos, cells: &[Pos], color: Rgb, alpha: f32, ascii: bool) -> Op {
    if ascii {
        let (n, s, w, e) = neighbours(at, cells);
        Op::Glyph {
            at,
            ch: curses_glyph(n, s, w, e),
            color,
            alpha,
        }
    } else {
        Op::Rect { at, color, alpha }
    }
}

fn explosion_layer(id: &str, at: Pos, cells: &[Pos]) -> Layer {
    let (n, s, w, e) = neighbours(at, cells);
    let (subtile, rotation) = tile_subtile(n, s, w, e);
    Layer {
        id: id.to_string(),
        subtile: Some(subtile.to_string()),
        rotation,
        glyph: '\0',
        ..Default::default()
    }
}

/// The squares of the native ring `r` squares from `at`.
fn ring(at: Pos, r: i32) -> Vec<Pos> {
    let mut out = vec![];
    for j in -r..=r {
        out.push(Pos {
            x: at.x + j,
            y: at.y - r,
        });
        out.push(Pos {
            x: at.x + j,
            y: at.y + r,
        });
        if j.abs() != r {
            out.push(Pos {
                x: at.x - r,
                y: at.y + j,
            });
            out.push(Pos {
                x: at.x + r,
                y: at.y + j,
            });
        }
    }
    out
}

/// Tileset sprites for the effects; ASCII mode has none.
pub trait Sprites {
    /// Whether the tileset has a sprite for the layer.
    fn has(&self, layer: &Layer) -> bool;
    /// Draws the tile into `square` when the tileset has it.
    fn tile(&mut self, painter: &egui::Painter, square: Rect, layer: &Layer) -> bool;
}

impl Effects {
    /// Takes the effects the game produced; with `reduced_motion` they are dropped unseen.
    pub fn add(&mut self, now: f64, produced: Vec<Effect>, reduced_motion: bool) {
        if reduced_motion {
            self.playing.clear();
            self.texts.clear();
            return;
        }
        self.tail = self.tail.max(now);
        for effect in produced {
            let life = life(&effect);
            if let Shape::Text {
                at,
                segments,
                scroll,
                hp,
            } = effect.shape
            {
                self.add_text(now, at, segments, scroll, hp);
                continue;
            }
            // Shots and blasts follow one another, none starting later than the queue bound.
            let start = self.tail.min(now + MAX_QUEUE);
            self.tail = start + life;
            self.playing.push(Playing {
                shape: effect.shape,
                start,
                life,
            });
        }
        self.playing.retain(|p| now < p.start + p.life);
        self.texts.retain(|t| t.step(now) <= MAX_STEPS);
    }

    /// The native `scrolling_combat_text::add`: texts that would move the same way at the same step
    /// make way for the new one by waiting a step more.
    fn add_text(&mut self, now: f64, at: Pos, segments: Vec<(String, Rgb)>, scroll: Pos, hp: bool) {
        let seed = (at.x.wrapping_mul(31) ^ at.y.wrapping_mul(17)) as u32
            ^ segments.first().map_or(0, |s| s.0.len() as u32);
        let dir = text_direction(scroll, hp, seed);
        if hp {
            self.texts.retain(|t| !t.hp);
        }
        let mut current = 0;
        for text in self.texts.iter_mut().rev() {
            if text.dir == dir && text.step(now) + text.offset == current {
                current += 1;
                text.offset += 1;
            }
        }
        self.texts.push(Text {
            at,
            segments,
            dir,
            hp,
            start: now,
            offset: 0,
        });
    }

    pub fn len(&self) -> usize {
        self.playing.len() + self.texts.len()
    }

    pub fn active(&self) -> bool {
        !self.playing.is_empty() || !self.texts.is_empty()
    }

    /// What is on screen at `now`. `ascii` picks the glyph stand-ins; `has` tells which tileset
    /// sprites exist.
    pub fn ops(&self, now: f64, ascii: bool, has: &dyn Fn(&Layer) -> bool) -> Vec<Op> {
        let mut out = vec![];
        for p in self.playing.iter().filter(|p| now >= p.start) {
            let t = ((now - p.start) / p.life).clamp(0.0, 1.0);
            let fade = (0.6 * (1.0 - t)) as f32;
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
                    out.push(Op::Tile {
                        at: *at,
                        layer: Box::new(Layer {
                            id: sprite
                                .clone()
                                .unwrap_or_else(|| "animation_bullet_normal".into()),
                            fallbacks: vec!["animation_bullet_normal".into()],
                            glyph: '\0',
                            ..Default::default()
                        }),
                        or: Box::new(Op::Glyph {
                            at: *at,
                            ch: *glyph,
                            color: *color,
                            alpha: 1.0,
                        }),
                    });
                }
                Shape::Burst {
                    at,
                    radius,
                    color,
                    tile,
                } => {
                    let reach = (f64::from(*radius) * t).ceil() as i32;
                    let probe = Layer {
                        id: tile.clone(),
                        subtile: Some("edge".into()),
                        glyph: '\0',
                        ..Default::default()
                    };
                    if reach > 0 && (ascii || has(&probe)) {
                        // The native ring after ring: sprites where the tileset has them, curses glyphs else.
                        for r in 1..=reach {
                            let cells = ring(*at, r);
                            for square in &cells {
                                let layer = explosion_layer(tile, *square, &cells);
                                out.push(Op::Tile {
                                    at: *square,
                                    layer: Box::new(layer),
                                    or: Box::new(blast_fallback(
                                        *square, &cells, *color, 1.0, ascii,
                                    )),
                                });
                            }
                        }
                    } else {
                        for dy in -reach..=reach {
                            for dx in -reach..=reach {
                                if dx * dx + dy * dy <= reach * reach + reach {
                                    out.push(Op::Rect {
                                        at: Pos {
                                            x: at.x + dx,
                                            y: at.y + dy,
                                        },
                                        color: *color,
                                        alpha: fade,
                                    });
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
                        let fallback = if *shrapnel {
                            Op::Rect {
                                at: *at,
                                color,
                                alpha: fade,
                            }
                        } else {
                            blast_fallback(*at, cells, color, 1.0, ascii)
                        };
                        out.push(if *shrapnel {
                            fallback
                        } else {
                            Op::Tile {
                                at: *at,
                                layer: Box::new(explosion_layer(tile, *at, cells)),
                                or: Box::new(fallback),
                            }
                        });
                    }
                }
                Shape::Text { .. } => {}
            }
        }
        for text in &self.texts {
            let Some((start, old)) = text.place(now) else {
                continue;
            };
            let mut at = start;
            for (part, color) in text.parts() {
                for ch in part.chars() {
                    let shade = if old { 0.6 } else { 1.0 };
                    out.push(Op::Glyph {
                        at,
                        ch,
                        color: Rgb(color.0, color.1, color.2),
                        alpha: shade,
                    });
                    at.x += 1;
                }
            }
        }
        out
    }

    /// Draws what is playing at `now`. `cell` is the screen square of a world square.
    pub fn paint(
        &self,
        painter: &egui::Painter,
        now: f64,
        cell_px: f32,
        cell: impl Fn(Pos) -> Rect,
        sprites: &mut dyn Sprites,
        ascii: bool,
    ) {
        let font = egui::FontId::monospace(cell_px - 2.0);
        let ops = self.ops(now, ascii, &|layer| sprites.has(layer));
        for op in &ops {
            draw(painter, op, &cell, &font, sprites);
        }
    }
}

fn tint(color: Rgb, alpha: f32) -> Color32 {
    Color32::from_rgba_unmultiplied(
        color.0,
        color.1,
        color.2,
        (alpha.clamp(0.0, 1.0) * 255.0) as u8,
    )
}

fn draw(
    painter: &egui::Painter,
    op: &Op,
    cell: &impl Fn(Pos) -> Rect,
    font: &egui::FontId,
    sprites: &mut dyn Sprites,
) {
    match op {
        Op::Rect { at, color, alpha } => {
            painter.rect_filled(cell(*at), 0.0, tint(*color, *alpha));
        }
        Op::Glyph {
            at,
            ch,
            color,
            alpha,
        } => {
            painter.text(
                cell(*at).center(),
                egui::Align2::CENTER_CENTER,
                *ch,
                font.clone(),
                tint(*color, *alpha),
            );
        }
        Op::Tile { at, layer, or } => {
            if !sprites.tile(painter, cell(*at), layer) {
                draw(painter, or, cell, font, sprites);
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

#[cfg(test)]
mod tests {
    use super::*;

    fn text(at: Pos, scroll: Pos, words: &[&str]) -> Effect {
        Effect {
            shape: Shape::Text {
                at,
                segments: words
                    .iter()
                    .map(|w| (w.to_string(), Rgb(255, 0, 0)))
                    .collect(),
                scroll,
                hp: false,
            },
            duration_ms: 80,
        }
    }

    fn none(_: &Layer) -> bool {
        false
    }

    fn glyph_at(op: &Op, at: Pos) -> Option<char> {
        match op {
            Op::Glyph { at: p, ch, .. } if *p == at => Some(*ch),
            Op::Tile { or, .. } => glyph_at(or, at),
            _ => None,
        }
    }

    /// The frame as the text a terminal would show, one line per row.
    fn dump(ops: &[Op], from: Pos, to: Pos) -> String {
        (from.y..=to.y)
            .map(|y| {
                (from.x..=to.x)
                    .map(|x| {
                        ops.iter()
                            .rev()
                            .find_map(|op| glyph_at(op, Pos { x, y }))
                            .unwrap_or('.')
                    })
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn squares(ops: &[Op]) -> Vec<Pos> {
        fn at(op: &Op) -> Option<Pos> {
            match op {
                Op::Glyph { at, .. } => Some(*at),
                Op::Tile { or, .. } => at(or),
                _ => None,
            }
        }
        ops.iter().filter_map(at).collect()
    }

    fn shot(len: i32) -> Effect {
        Effect {
            shape: Shape::Projectile {
                path: (0..len).map(|x| Pos { x, y: 0 }).collect(),
                glyph: '*',
                sprite: None,
                color: Rgb(255, 0, 0),
            },
            duration_ms: 20,
        }
    }

    #[test]
    fn many_shots_in_one_turn_play_out_within_the_queue_bound() {
        let mut e = Effects::default();
        let shots = (0..100).map(|_| shot(20)).collect::<Vec<_>>();
        let longest = shots.iter().map(life).fold(0.0, f64::max);
        e.add(0.0, shots, false);
        let end = MAX_QUEUE + longest + 0.001;
        e.add(end, vec![], false);
        assert!(
            !e.active(),
            "every shot ended by the queue bound plus one shot"
        );
    }

    #[test]
    fn floating_text_moves_one_square_per_step_in_its_direction_and_is_gone_after_eight() {
        let mut e = Effects::default();
        let up = Pos { x: 0, y: -1 };
        e.add(0.0, vec![text(Pos { x: 5, y: 5 }, up, &["-3"])], false);
        assert!(
            e.ops(0.0, true, &none).is_empty(),
            "native text starts moving at step one"
        );
        let row = |t: f64| squares(&e.ops(t, true, &none)).first().map(|p| p.y);
        // The native text sits one square above its origin and then moves a square per step.
        assert_eq!(row(TEXT_STEP * 1.5), Some(3));
        assert_eq!(row(TEXT_STEP * 3.5), Some(1));
        assert_eq!(row(TEXT_STEP * 9.5), None);
        e.add(TEXT_STEP * 9.5, vec![], false);
        assert!(!e.active(), "a finished text is dropped");
    }

    #[test]
    fn texts_at_one_square_moving_the_same_way_do_not_overlap() {
        let mut e = Effects::default();
        let (at, up) = (Pos { x: 5, y: 5 }, Pos { x: 0, y: -1 });
        e.add(
            0.0,
            vec![
                text(at, up, &["-3"]),
                text(at, up, &["-7"]),
                text(at, up, &["-9"]),
            ],
            false,
        );
        let ops = e.ops(TEXT_STEP * 2.5, true, &none);
        let all = squares(&ops);
        let mut unique = all.clone();
        unique.sort_by_key(|p| (p.x, p.y));
        unique.dedup();
        assert_eq!(unique.len(), all.len(), "{all:?}");
        // The native layout pushes the older texts further along their way.
        let row = |last: char| {
            ops.iter()
                .find_map(|op| match op {
                    Op::Glyph { at, ch, .. } if *ch == last => Some(at.y),
                    _ => None,
                })
                .unwrap()
        };
        assert!(row('3') < row('7') && row('7') < row('9'));
    }

    #[test]
    fn a_hit_point_readout_stays_beside_the_creature_and_replaces_the_last_one() {
        let mut e = Effects::default();
        let hp = |x| Effect {
            shape: Shape::Text {
                at: Pos { x: 5, y: 5 },
                segments: vec![("12/20".into(), Rgb(0, 255, 0))],
                scroll: Pos { x, y: 0 },
                hp: true,
            },
            duration_ms: 80,
        };
        e.add(0.0, vec![hp(1)], false);
        let first = |t: f64| squares(&e.ops(t, true, &none)).first().copied();
        assert_eq!(
            first(TEXT_STEP * 1.5),
            first(TEXT_STEP * 5.5),
            "it does not scroll"
        );
        assert_eq!(first(TEXT_STEP * 1.5), Some(Pos { x: 7, y: 5 }));
        e.add(TEXT_STEP, vec![hp(-1)], false);
        assert_eq!(e.len(), 1, "only one readout at a time");
        let left = squares(&e.ops(TEXT_STEP * 2.5, true, &none));
        assert_eq!(left.last(), Some(&Pos { x: 3, y: 5 }), "right-aligned");
    }

    #[test]
    fn text_moving_left_reads_its_second_part_first() {
        let mut e = Effects::default();
        e.add(
            0.0,
            vec![text(
                Pos { x: 10, y: 5 },
                Pos { x: -1, y: -1 },
                &["-5", "crit"],
            )],
            false,
        );
        let line: String = e
            .ops(TEXT_STEP * 1.5, true, &none)
            .iter()
            .filter_map(|op| match op {
                Op::Glyph { ch, .. } => Some(*ch),
                _ => None,
            })
            .collect();
        assert_eq!(line, "crit -5");
    }

    fn blast(cells: Vec<Pos>) -> Effect {
        Effect {
            shape: Shape::Blast {
                cells,
                shrapnel: false,
                tile: "explosion".into(),
            },
            duration_ms: 10,
        }
    }

    #[test]
    fn an_ascii_blast_frame_draws_the_curses_shell_glyphs() {
        let mut e = Effects::default();
        let cells = (0..3)
            .flat_map(|y| (0..3).map(move |x| Pos { x, y }))
            .collect::<Vec<_>>();
        e.add(0.0, vec![blast(cells)], false);
        let ops = e.ops(0.01, true, &none);
        // The native custom explosion keeps a shell: slanted corners, '-' and '|' sides, hollow inside.
        assert_eq!(
            dump(&ops, Pos { x: 0, y: 0 }, Pos { x: 2, y: 2 }),
            "/-\\\n| |\n\\-/"
        );
    }

    #[test]
    fn a_tiles_blast_frame_asks_for_the_tileset_explosion_with_its_native_subtile() {
        let mut e = Effects::default();
        e.add(
            0.0,
            vec![blast(vec![Pos { x: 0, y: 0 }, Pos { x: 1, y: 0 }])],
            false,
        );
        let layers = e
            .ops(0.01, false, &none)
            .iter()
            .filter_map(|op| match op {
                Op::Tile { layer, or, .. } => Some((
                    layer.subtile.clone(),
                    layer.rotation,
                    matches!(**or, Op::Rect { .. }),
                )),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            layers,
            [
                (Some("edge".into()), 0, true),
                (Some("edge".into()), 0, true)
            ]
        );
    }

    #[test]
    fn an_explosion_ring_grows_with_time() {
        let mut e = Effects::default();
        e.add(
            0.0,
            vec![Effect {
                shape: Shape::Burst {
                    at: Pos { x: 0, y: 0 },
                    radius: 2,
                    color: Rgb(255, 0, 0),
                    tile: "explosion".into(),
                },
                duration_ms: 100,
            }],
            false,
        );
        let early = squares(&e.ops(0.01, true, &none)).len();
        let late = squares(&e.ops(0.17, true, &none)).len();
        assert!(0 < early && early < late, "{early} < {late}");
    }

    /// A shot along row `y` from x 0 over `len` squares.
    fn shot_on(y: i32, len: i32) -> Effect {
        let mut e = shot(len);
        if let Shape::Projectile { path, .. } = &mut e.shape {
            path.iter_mut().for_each(|p| p.y = y);
        }
        e
    }

    fn volley(pellets: Vec<Pellet>) -> Effect {
        Effect {
            shape: Shape::Volley { pellets },
            duration_ms: 0,
        }
    }

    #[test]
    fn a_volleys_pellets_take_each_step_together_and_each_ends_where_its_flight_does() {
        let mut e = Effects::default();
        let pellet = |y, len| Pellet {
            flight: shot_on(y, len),
            then: vec![],
        };
        e.add(0.0, vec![volley(vec![pellet(0, 3), pellet(1, 2)])], false);
        let at = |step: f64| squares(&e.ops(SHOT_STEP * step, true, &none));
        assert_eq!(at(0.5), [Pos { x: 0, y: 0 }, Pos { x: 0, y: 1 }]);
        assert_eq!(at(1.5), [Pos { x: 1, y: 0 }, Pos { x: 1, y: 1 }]);
        assert_eq!(at(2.5), [Pos { x: 2, y: 0 }], "the shorter pellet ended");
        e.add(SHOT_STEP * 3.0 + 0.001, vec![shot_on(5, 1)], false);
        assert_eq!(
            at(3.5),
            [Pos { x: 0, y: 5 }],
            "a shot after the volley starts once all its pellets flew"
        );
    }

    #[test]
    fn a_pellets_hit_text_rises_once_the_pellet_reached_the_square_it_hit() {
        let mut e = Effects::default();
        let up = Pos { x: 0, y: -1 };
        let hit = text(Pos { x: 2, y: 9 }, up, &["-3"]);
        e.add(
            0.0,
            vec![volley(vec![Pellet {
                flight: shot_on(0, 4),
                then: vec![Follow {
                    after: 3,
                    effect: hit,
                }],
            }])],
            false,
        );
        let rows = |t: f64| {
            squares(&e.ops(t, true, &none))
                .iter()
                .filter(|p| p.y != 0)
                .count()
        };
        assert_eq!(rows(TEXT_STEP * 1.5), 0, "a text on its own would rise now");
        assert!(rows(SHOT_STEP * 3.0 + TEXT_STEP * 1.5) > 0);
    }
}
