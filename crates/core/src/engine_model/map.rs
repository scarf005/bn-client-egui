//! The squares of the map as the client draws them: the layers of a square, the creature on it, and their colours.

use super::*;

pub(super) fn look_name(l: &Look) -> String {
    (l.name.clone().or_else(|| l.id.clone())).unwrap_or_else(|| l.glyph.clone())
}

fn glyph(l: &Look) -> char {
    l.glyph.chars().next().unwrap_or('?')
}

/// The topmost thing drawn in a cell: item, field, furniture, terrain, then what memory holds
/// (its furniture or other overlay over its terrain).
fn top_look(c: &WorldCell) -> Option<&Look> {
    c.vehicle
        .as_ref()
        .or(c.items.last())
        .or(c.fields.last().map(|f| &f.look))
        .or(c.furniture.as_ref())
        .or(c.terrain.as_ref())
        .or(c
            .memory
            .as_ref()
            .map(|m| m.overlay.as_ref().unwrap_or(&m.terrain)))
}

/// The topmost thing on a level the game draws without its ground: vehicle part, item, field.
fn on_top(c: &WorldCell) -> Option<&Look> {
    c.vehicle
        .as_ref()
        .or(c.items.last())
        .or(c.fields.last().map(|f| &f.look))
}

/// One channel of a look's paint.
fn paint(look: &Look, pick: impl Fn(&protocol::Tint) -> Option<&str>) -> Option<Rgb> {
    let [r, g, b] = parse_hex(pick(look.tint.as_ref()?)?)?;
    Some(Rgb(r, g, b))
}

/// The coloured light on a square in sight.
fn glow(c: &WorldCell) -> Option<Glow> {
    let glow = c.glow.as_ref()?;
    let [r, g, b] = parse_hex(&glow.color)?;
    Some(Glow {
        color: Rgb(r, g, b),
        strength: glow.strength,
    })
}

/// `#rrggbb` to its channels.
fn parse_hex(text: &str) -> Option<[u8; 3]> {
    let digits = text.strip_prefix('#').filter(|d| d.len() == 6)?;
    let channel = |i: usize| u8::from_str_radix(digits.get(i..i + 2)?, 16).ok();
    Some([channel(0)?, channel(2)?, channel(4)?])
}

/// The game's per-tile seed for weighted sprite variants: the square's coordinates mixed into an integer.
pub(super) fn square_seed(p: &WorldPos) -> u32 {
    (p.x as u32).wrapping_add((p.y as u32).wrapping_mul(65536))
}

/// A stable seed for a creature; the game seeds monsters by address and characters by name or id.
fn id_seed(id: &str) -> u32 {
    id.bytes()
        .fold(2166136261, |h, b| (h ^ u32::from(b)).wrapping_mul(16777619))
}

impl EngineModel {
    pub(super) fn styled(&self, look: &Look, desc: String) -> Cell {
        let (color, bg) = self.colors.resolve(&look.color);
        Cell {
            glyph: glyph(look),
            color,
            bg,
            desc,
            id: look.id.clone().unwrap_or_default(),
            ..Cell::default()
        }
    }

    /// The sprite request for one look; `seed` is the game's choice among weighted variants.
    pub(super) fn layer(&self, look: &Look, seed: u32) -> Layer {
        let (color, bg) = self.colors.resolve(&look.color);
        Layer {
            id: look
                .tile
                .clone()
                .or_else(|| look.id.clone())
                .unwrap_or_default(),
            fallbacks: look.looks_like.clone(),
            subtile: look.subtile.clone(),
            rotation: look.rotation.unwrap_or(0),
            mirrored: look.facing.as_deref() == Some("left"),
            creature: matches!(look.kind.as_str(), "monster" | "npc" | "avatar"),
            terrain: look.kind == "terrain",
            wall: look.wall,
            window: look.window,
            coverage: look.coverage,
            form: look.form,
            vehicle: look.kind == "vehicle_part",
            item: look.kind == "item",
            seed,
            tint_bg: paint(look, |t| t.bg.as_deref()),
            tint_fg: paint(look, |t| t.fg.as_deref()),
            mutation: None,
            tint_keys: look.tint_keys.clone(),
            tint_vars: look.tint_vars.clone(),
            glyph: glyph(look),
            color,
            color_index: ColorMap::curses(&look.color),
            bg,
        }
    }

    /// A character's overlays, which face as it does; an overlay without a sprite draws nothing.
    fn overlays(&self, body: &Look, overlays: &[Look], traits: &Traits, seed: u32) -> Vec<Layer> {
        let traits = std::sync::Arc::new(traits.clone());
        overlays
            .iter()
            .map(|overlay| Layer {
                mirrored: body.facing.as_deref() == Some("left"),
                creature: true,
                glyph: '\0',
                mutation: overlay.mutation.as_ref().map(|m| Mutation {
                    id: m.id.clone(),
                    types: m.types.clone(),
                    flags: m.flags.clone(),
                    traits: traits.clone(),
                }),
                ..self.layer(overlay, seed)
            })
            .collect()
    }

    /// The cell without its square seed. The avatar's square shows the avatar's own glyph and colour over
    /// what it stands on, as the native view draws it.
    pub(super) fn cell_at(&self, at: Pos) -> Cell {
        let cell = self.square_at(at);
        let here = self.abs(at).is_some_and(|p| self.avatar_pos() == Some(&p));
        match self.replica.avatar().and_then(|a| a.look.as_ref()) {
            Some(look) if here => {
                let shown = self.styled(look, String::new());
                Cell {
                    glyph: shown.glyph,
                    color: shown.color,
                    bg: shown.bg,
                    ..cell
                }
            }
            _ => cell,
        }
    }

    /// What is on a square, without the avatar's glyph.
    fn square_at(&self, at: Pos) -> Cell {
        let unknown = Cell {
            glyph: ' ',
            color: Rgb(128, 128, 128),
            ..Cell::default()
        };
        let Some(p) = self.abs(at) else {
            return unknown;
        };
        let world = self.replica.cell(&p.dim, p.x, p.y, p.z);
        // A square without a floor, through which the game draws the square the engine names below.
        let below = world.and_then(|c| c.below);
        let level = |z: i32| self.replica.cell(&p.dim, p.x, p.y, z);
        let lower = below.and_then(|d| level(p.z - i32::from(d)));
        // The levels from above the drawn square up to this one draw only what is on them, no ground.
        let over = below.map_or(vec![], |d| {
            (p.z - i32::from(d) + 1..=p.z).filter_map(level).collect()
        });
        let mut layers = match below {
            Some(_) => lower.map_or(vec![], |c| self.layers(c, true)),
            None => world.map_or(vec![], |c| self.layers(c, true)),
        };
        layers.extend(over.iter().flat_map(|c| self.layers(c, false)));
        // The square drawn there: the level below through open air.
        let shown = lower.or(world);
        if self.avatar_pos() == Some(&p)
            && let Some(avatar) = self.replica.avatar()
            && let Some(look) = &avatar.look
        {
            let seed = u32::from(avatar.name.bytes().next().unwrap_or(0));
            layers.push(self.layer(look, seed));
            layers.extend(self.overlays(look, &avatar.overlays, &avatar.traits, seed));
        }
        // A creature here, else the nearest one on the levels seen through.
        let on = |at: &WorldPos| self.replica.entity_at(at);
        let entity = on(&p).or_else(|| {
            over.iter()
                .rev()
                .copied()
                .chain(lower)
                .find_map(|c| on(&c.at))
        });
        if let Some(e) = entity {
            let look = e.look.as_ref().expect("filtered on look");
            let desc = e.name.clone().unwrap_or_else(|| look_name(look));
            layers.push(self.layer(look, id_seed(&e.id)));
            layers.extend(self.overlays(look, &e.overlays, &e.traits, id_seed(&e.id)));
            layers.extend(self.attitude(e));
            layers.extend(
                lower
                    .iter()
                    .chain(&over)
                    .filter_map(|c| self.revival(Some(*c))),
            );
            layers.extend(self.revival(world.filter(|_| below.is_none())));
            return Cell {
                layers,
                remembered: false,
                light: shown
                    .and_then(|c| c.light.as_ref()?.as_u64())
                    .map(|l| l as u8),
                glow: shown.and_then(glow),
                health: e.health.as_ref().map(|h| Health {
                    fifths: h.fifths,
                    color: self.colors.resolve(&h.color).0,
                }),
                hostile: e.attitude.as_deref() == Some("hostile"),
                depth: below.unwrap_or(0),
                ..self.styled(look, desc)
            };
        }
        layers.extend(
            lower
                .iter()
                .chain(&over)
                .filter_map(|c| self.revival(Some(*c))),
        );
        layers.extend(self.revival(world.filter(|_| below.is_none())));
        match world.filter(|_| below.is_none()) {
            Some(c) => match top_look(c) {
                Some(l) => Cell {
                    layers,
                    remembered: c.known == Known::Remembered,
                    light: c.light.as_ref().and_then(Value::as_u64).map(|l| l as u8),
                    glow: glow(c),
                    ..self.styled(l, look_name(l))
                },
                None => Cell {
                    glyph: '?',
                    color: Rgb(128, 128, 128),
                    ..Cell::default()
                },
            },
            None => {
                // What is on the levels seen through, else the square drawn below, describes it.
                let look = over
                    .iter()
                    .rev()
                    .find_map(|c| on_top(c))
                    .or_else(|| lower.and_then(top_look));
                Cell {
                    layers,
                    remembered: lower.is_some_and(|c| c.known == Known::Remembered),
                    depth: below.unwrap_or(0),
                    light: lower.and_then(|c| c.light.as_ref()?.as_u64().map(|l| l as u8)),
                    glow: lower.and_then(glow),
                    ..look.map_or(unknown, |l| self.styled(l, look_name(l)))
                }
            }
        }
    }

    /// What a cell draws, bottom first, in the order of the native view: terrain, furniture, trap,
    /// field, item, vehicle part; a cell only remembered draws its memorized terrain and overlay.
    fn layers(&self, c: &WorldCell, ground: bool) -> Vec<Layer> {
        let seed = square_seed(&c.at);
        // The displayed field may hide the items under it, and so their highlight.
        let hidden = c.fields.last().is_some_and(|f| f.hides_items);
        let live = [
            c.terrain.as_ref().filter(|_| ground),
            c.furniture.as_ref().filter(|_| ground),
            c.traps.last().filter(|_| ground),
            c.fields.last().map(|f| &f.look),
            c.items.last().filter(|_| !hidden),
            c.vehicle.as_ref(),
        ];
        // A level drawn without its ground draws no memory either.
        let looks = match live.iter().any(Option::is_some) || !ground {
            true => live.into_iter().flatten().collect::<Vec<_>>(),
            false => c
                .memory
                .iter()
                .flat_map(|m| std::iter::once(&m.terrain).chain(m.overlay.as_ref()))
                .collect(),
        };
        looks
            .into_iter()
            .flat_map(|l| {
                let seeded = !matches!(l.kind.as_str(), "item" | "trap");
                // A loaded cargo part is highlighted over the part, a pile over its displayed item.
                let loaded = c.cargo && l.kind == "vehicle_part";
                let stacked = loaded || l.stack.is_some_and(|n| n > 1);
                let mark = stacked.then(|| self.marker("highlight_item"));
                [Some(self.layer(l, if seeded { seed } else { 0 })), mark]
            })
            .flatten()
            .collect()
    }

    /// A sprite the tileset may have and that stands for nothing else: it draws nothing without one.
    fn marker(&self, id: &str) -> Layer {
        Layer {
            id: id.into(),
            ..Layer::default()
        }
    }

    /// The mark over a square holding a corpse that can rise: the last thing the native view draws.
    fn revival(&self, world: Option<&WorldCell>) -> Option<Layer> {
        world
            .is_some_and(|c| c.reviving)
            .then(|| self.marker("zombie_revival_indicator"))
    }

    /// The overlay the native view puts over a monster or NPC for how it regards the avatar.
    fn attitude(&self, e: &Entity) -> Option<Layer> {
        let attitude = e.attitude.as_ref()?;
        let aware = if e.aware { "_sees_player" } else { "" };
        Some(self.marker(&format!("overlay_{attitude}{aware}")))
    }
}
