//! Drawing the map's squares on the flat map: each layer's sprites under its filter, coloured light and haze,
//! and the glyph of what the tileset has no sprite for.

use super::*;

impl Display {
    /// Draws an overmap square's tile into `rect`; false when no tileset is in use or it has no sprite
    /// for the tile or any of its fallbacks.
    pub fn paint_overmap(
        &mut self,
        painter: &egui::Painter,
        rect: Rect,
        tile: &OvermapTile<'_>,
    ) -> bool {
        let Some(atlas) = &mut self.atlas else {
            return false;
        };
        atlas.request(Fx::Plain);
        let query = Query {
            id: tile.id,
            fallbacks: tile.fallbacks,
            subtile: (!tile.subtile.is_empty()).then_some(tile.subtile),
            rotation: tile.rotation,
            mirrored: false,
            creature: false,
            seed: 0,
            season: None,
            frame: self.frame,
        };
        let drawn = draw_plain(painter, atlas, rect, &query);
        self.drawn += usize::from(drawn);
        drawn
    }

    /// Whether the tileset has a sprite for the layer.
    pub fn has_sprite(&self, layer: &Layer) -> bool {
        self.atlas
            .as_ref()
            .is_some_and(|a| a.tileset.choose(&query(layer, None)).is_some())
    }

    /// Draws one tile into `rect` when the tileset has a sprite for it; false draws nothing.
    pub fn paint_sprite(&mut self, painter: &egui::Painter, rect: Rect, layer: &Layer) -> bool {
        let Some(atlas) = &mut self.atlas else {
            return false;
        };
        atlas.request(Fx::Plain);
        let wanted = Query {
            frame: self.frame,
            ..query(layer, None)
        };
        draw_plain(painter, atlas, rect, &wanted)
    }

    /// Draws a list row's item into `rect` as `paint` draws a map square, without counting it in what the map
    /// paint reports.
    pub fn paint_icon(
        &mut self,
        painter: &egui::Painter,
        rect: Rect,
        cell: &Cell,
        season: Option<&str>,
    ) -> bool {
        let counts = (self.drawn, self.hidden, self.dimmed);
        let painted = self.paint(painter, rect, cell, season);
        (self.drawn, self.hidden, self.dimmed) = counts;
        painted
    }

    /// Draws a cell's layers into `rect`, bottom first, the way the game's tile view does: sprites where
    /// the tileset has them and the glyph of the thing where it has none. A square nothing is known about is only the hidden
    /// backdrop; false when even that has no sprite.
    pub fn paint(
        &mut self,
        painter: &egui::Painter,
        rect: Rect,
        cell: &Cell,
        season: Option<&str>,
    ) -> bool {
        profiling::scope!("Display::paint");
        let Some(atlas) = &mut self.atlas else {
            return false;
        };
        let fx = Fx::of(cell);
        atlas.request(fx);
        let lighting = |id| Query {
            id,
            fallbacks: &[],
            subtile: None,
            rotation: 0,
            mirrored: false,
            creature: false,
            seed: cell.square,
            season,
            frame: self.frame,
        };
        // What the game does not show plainly lies on its "hidden" lighting sprite.
        let backdrop = (cell.layers.is_empty() || cell.remembered)
            && draw_plain(painter, atlas, rect, &lighting("lighting_hidden"));
        self.hidden += usize::from(backdrop);
        if cell.layers.is_empty() {
            return backdrop;
        }
        // The game cannot see a dark square at all and can only tell a bright-only one is bright: it draws
        // the lowlight overlay in place of the square's terrain.
        let overlay = match (cell.remembered, cell.light) {
            (false, Some(0)) => Some("lighting_lowlight_dark"),
            (false, Some(2)) => Some("lighting_lowlight_light"),
            _ => None,
        };
        if let Some(id) = overlay
            && draw_plain(painter, atlas, rect, &lighting(id))
        {
            self.dimmed += 1;
            return true;
        }
        let glow = glow_on(cell, fx);
        let scale = atlas.scale(rect);
        let mut drawn = 0;
        for layer in &cell.layers {
            let (layer, paints) = resolve_layer(&atlas.tileset, layer);
            let layer = layer.as_ref();
            let wanted = Query {
                frame: self.frame,
                ..query(layer, season)
            };
            let chosen = atlas
                .tileset
                .choose_or_stand_in(&wanted, layer.glyph, layer.color_index);
            animate(painter, chosen.as_ref());
            let sprites = chosen.iter().flat_map(|c| {
                [(c.bg, paints.0), (c.fg, paints.1)]
                    .into_iter()
                    .filter_map(|(placed, paint)| Some((placed?, paint)))
            });
            let mut any = false;
            for (placed, paint) in sprites {
                let tone = Tone::Alpha(255);
                any |= draw_sprite(painter, atlas, rect, scale, (placed, paint), (fx, tone));
                if let Some(light) = glow {
                    // The game adds the light's colour over the sprite it falls on.
                    let tone = Tone::Light(light);
                    draw_sprite(painter, atlas, rect, scale, (placed, paint), (fx, tone));
                }
                if cell.depth > 0 {
                    let haze = haze_at(cell.depth);
                    atlas.request(Fx::ZOverlay);
                    let sprite = (placed, None);
                    let tone = Tone::Alpha(haze);
                    draw_sprite(painter, atlas, rect, scale, sprite, (Fx::ZOverlay, tone));
                }
            }
            drawn += usize::from(any);
            if !any && layer.glyph != '\0' {
                glyph(painter, rect, layer, fx.stand_in());
            }
        }
        self.drawn += drawn;
        true
    }
}

/// The layer as the game draws it with this tileset, and its paint for the background and the foreground: a
/// mutation's, an item's, a bionic's or an effect's from the tileset, else the part's own. A hair style
/// goes by the sprite of the hair color where the tileset paints no hair.
pub(crate) fn resolve_layer<'a>(tileset: &Tileset, layer: &'a Layer) -> (Cow<'a, Layer>, Paints) {
    if let Some(m) = &layer.mutation {
        let q = PaintQuery {
            id: &m.id,
            types: &m.types,
            flags: &m.flags,
            traits: &m.traits,
        };
        let male = layer.id.starts_with("overlay_male_");
        return match tileset.colored_hair(&q, male) {
            Some([id, plain]) => {
                let shown = Layer {
                    id,
                    fallbacks: vec![plain],
                    ..layer.clone()
                };
                (Cow::Owned(shown), (None, None))
            }
            None => (Cow::Borrowed(layer), tileset.mutation_paints(&q)),
        };
    }
    (Cow::Borrowed(layer), tileset.layer_paints(layer))
}

pub fn query<'a>(layer: &'a Layer, season: Option<&'a str>) -> Query<'a> {
    Query {
        id: &layer.id,
        fallbacks: &layer.fallbacks,
        subtile: layer.subtile.as_deref(),
        rotation: layer.rotation,
        mirrored: layer.mirrored,
        creature: layer.creature,
        seed: layer.seed,
        season,
        frame: None,
    }
}

/// Draws the sprites `query` chooses into `rect` unfiltered, background first; false when none is drawn.
fn draw_plain(painter: &egui::Painter, atlas: &Atlas, rect: Rect, query: &Query) -> bool {
    let chosen = atlas.tileset.choose(query);
    animate(painter, chosen.as_ref());
    let (scale, plain) = (atlas.scale(rect), (Fx::Plain, Tone::Alpha(255)));
    let mut any = false;
    for placed in chosen.iter().flat_map(|c| [c.bg, c.fg]).flatten() {
        any |= draw_sprite(painter, atlas, rect, scale, (placed, None), plain);
    }
    any
}

/// Asks for a paint of the next animation frame when `chosen` changes with it.
fn animate(painter: &egui::Painter, chosen: Option<&tileset::Chosen>) {
    if chosen.is_some_and(|c| c.animated) {
        painter.ctx().request_repaint_after(tileset::FRAME);
    }
}

/// A glyph's or its background's colour, darkened by `tint` as the filter of its square asks.
pub(crate) fn glyph_color(c: Rgb, tint: Color32) -> Color32 {
    Color32::from_rgb(c.0, c.1, c.2).gamma_multiply(f32::from(tint.r()) / 255.0)
}

/// A hurt creature's health as a dark `track` filled for its fifths in the game's colour.
pub(crate) fn health_bar(painter: &egui::Painter, track: Rect, health: Health) {
    painter.rect_filled(track, 0.0, Color32::from_black_alpha(200));
    let mut filled = track;
    filled.set_width(track.width() * f32::from(health.fifths) / 5.0);
    let Rgb(r, g, b) = health.color;
    painter.rect_filled(filled, 0.0, Color32::from_rgb(r, g, b));
}

/// The ASCII stand-in of a thing the tileset has no sprite for.
fn glyph(painter: &egui::Painter, rect: Rect, layer: &Layer, tint: Color32) {
    if let Some(bg) = layer.bg {
        painter.rect_filled(rect, 0.0, glyph_color(bg, tint));
    }
    painter.text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        layer.glyph,
        egui::FontId::monospace(rect.height() - 2.0),
        glyph_color(layer.color, tint),
    );
}

/// How a sprite is laid on what is under it.
#[derive(Clone, Copy)]
enum Tone {
    /// Blended at this opacity.
    Alpha(u8),
    /// Added, multiplied by this colour, as the game adds coloured light.
    Light(Color32),
}

/// The colour coloured light adds to a sprite: the game's tint, its alpha 40 to 176 with the square root of
/// the light's strength, with no alpha of its own so that egui adds it.
pub fn light(glow: Glow) -> Color32 {
    let alpha = (40.0 + 136.0 * (f32::from(glow.strength) / 255.0).sqrt()).round() / 255.0;
    let Rgb(r, g, b) = glow.color;
    let add = |c: u8| (f32::from(c) * alpha).round() as u8;
    Color32::from_rgba_premultiplied(add(r), add(g), add(b), 0)
}

/// The coloured light that falls on `cell` drawn under `fx`: only on what is plainly or dimly in sight.
pub(crate) fn glow_on(cell: &Cell, fx: Fx) -> Option<Color32> {
    cell.glow
        .filter(|_| fx == Fx::Plain || fx == Fx::Shadow)
        .map(light)
}

/// The haze over a level seen through open air `depth` levels down, more the deeper it is; none on the shown one.
pub(crate) fn haze_at(depth: u8) -> u8 {
    match depth {
        0 => 0,
        depth => (24 * (u32::from(depth) + 1)).min(192) as u8,
    }
}

/// One sprite at its place in the cell, turned as the tileset asks; false when its sheet is not loaded.
fn draw_sprite(
    painter: &egui::Painter,
    atlas: &Atlas,
    cell: Rect,
    scale: (f32, f32),
    (placed, paint): (Placed, Option<Paint>),
    (fx, tone): (Fx, Tone),
) -> bool {
    // A painted sprite is made in the background; the plain one stands in until it is.
    let painted = paint.and_then(|p| atlas.painted(painter.ctx(), (placed, p), fx));
    let Some(Source {
        texture,
        uv,
        offset,
        fx: shown,
    }) = painted.or_else(|| atlas.source(placed.sprite, fx))
    else {
        return false;
    };
    // A sprite whose filtered sheet is still being decoded is darkened by colour meanwhile.
    let tint = match tone {
        Tone::Light(light) => light,
        Tone::Alpha(opacity) if shown == fx => Color32::from_white_alpha(opacity),
        Tone::Alpha(_) => fx.stand_in(),
    };
    let Some((n, _)) = atlas.tileset.locate(placed.sprite) else {
        return false;
    };
    let sheet = &atlas.tileset.sheets[n];
    let size = egui::vec2(
        sheet.sprite.0 as f32 * sheet.pixelscale * scale.0,
        sheet.sprite.1 as f32 * sheet.pixelscale * scale.1,
    );
    let min = cell.min + egui::vec2(offset.0 * scale.0, offset.1 * scale.1);
    let dest = Rect::from_min_size(min, size);
    // A sample on the edge of a sprite can land in the sprite next to it in the sheet, at a zoom where the
    // renderer draws the rectangle off the pixel grid: the source stops half a texel short of each edge.
    let texel = egui::vec2(
        uv.width() / sheet.sprite.0 as f32,
        uv.height() / sheet.sprite.1 as f32,
    );
    let uv = uv.shrink2(texel / 2.0);
    // Source corners for the destination's top left, top right, bottom right and bottom left.
    let [tl, tr, br, bl] = [
        uv.left_top(),
        uv.right_top(),
        uv.right_bottom(),
        uv.left_bottom(),
    ];
    let corners = match placed.turn {
        Turn::None => [tl, tr, br, bl],
        Turn::Left => [tr, br, bl, tl],
        Turn::Right => [bl, tl, tr, br],
        Turn::Half => [br, bl, tl, tr],
        Turn::Mirror => [tr, tl, bl, br],
    };
    let mut mesh = Mesh::with_texture(texture);
    for (pos, uv) in [
        dest.left_top(),
        dest.right_top(),
        dest.right_bottom(),
        dest.left_bottom(),
    ]
    .into_iter()
    .zip(corners)
    {
        mesh.vertices.push(egui::epaint::Vertex {
            pos,
            uv,
            color: tint,
        });
    }
    mesh.indices.extend([0, 1, 2, 0, 2, 3]);
    painter.add(Shape::mesh(mesh));
    true
}
