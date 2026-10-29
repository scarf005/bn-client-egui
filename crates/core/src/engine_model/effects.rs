//! The animations the engine tells of: shots, explosions and combat text, and the pellets of a multishot
//! flying together.

use super::*;

/// A multishot being told: each pellet's flight by its projectile, and where what the last hit brought shows.
pub(super) struct Volley {
    group: String,
    ids: Vec<String>,
    pellets: Vec<Pellet>,
    /// The pellet that hit last and the squares it flew up to the square it hit.
    hit: Option<(usize, usize)>,
}

impl Volley {
    fn new(group: &str) -> Self {
        Self {
            group: group.to_string(),
            ids: vec![],
            pellets: vec![],
            hit: None,
        }
    }

    /// A move of the pellet `id`: the first starts its flight, a later one lengthens it.
    fn flew(&mut self, id: &str, flight: Effect) {
        let Some(i) = self.ids.iter().position(|known| known == id) else {
            self.ids.push(id.to_string());
            self.pellets.push(Pellet {
                flight,
                then: vec![],
            });
            return;
        };
        let mine = &mut self.pellets[i].flight;
        mine.duration_ms += flight.duration_ms;
        if let (Shape::Projectile { path, .. }, Shape::Projectile { path: more, .. }) =
            (&mut mine.shape, flight.shape)
        {
            path.extend(more);
        }
    }

    /// The pellet `id` hit or landed at `at` (none on another level); what follows shows once it got there.
    fn hit(&mut self, id: &str, at: Option<Pos>) {
        let Some(i) = self.ids.iter().position(|known| known == id) else {
            return;
        };
        let Shape::Projectile { path, .. } = &self.pellets[i].flight.shape else {
            return;
        };
        let reached = at.and_then(|at| path.iter().position(|p| *p == at));
        self.hit = Some((i, reached.map_or(path.len(), |k| k + 1)));
    }

    /// An effect told after the last hit, such as its damage text; given back when no pellet flew yet.
    fn follow(&mut self, effect: Effect) -> Option<Effect> {
        let last = self.pellets.len().checked_sub(1).map(|i| (i, 0));
        let Some((i, after)) = self.hit.or(last) else {
            return Some(effect);
        };
        self.pellets[i].then.push(Follow { after, effect });
        None
    }

    /// The pellets flying together; none when none flew on the shown level.
    fn effect(self) -> Option<Effect> {
        let duration_ms = self.pellets.iter().map(|p| p.flight.duration_ms).max()?;
        Some(Effect {
            shape: Shape::Volley {
                pellets: self.pellets,
            },
            duration_ms,
        })
    }
}

impl EngineModel {
    /// The world square of a fact on the shown level; the view places it when it draws it.
    fn level_square(&self, square: &Value) -> Option<Pos> {
        let at = |k: &str| square[k].as_i64().map(|v| v as i32);
        (at("z")? == self.shown_z()?).then(|| Pos {
            x: at("x").unwrap_or_default(),
            y: at("y").unwrap_or_default(),
        })
    }

    fn effect_of(&self, fact: &crate::replica::Fact) -> Option<Effect> {
        let d = &fact.data;
        let squares = |key: &str| -> Vec<Pos> {
            d[key]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|s| self.level_square(s))
                .collect()
        };
        let color = |name: &Value| self.colors.resolve(name.as_str().unwrap_or("white")).0;
        let shape = match fact.kind.as_str() {
            "projectile.moved" => Shape::Projectile {
                path: squares("path"),
                glyph: d["look"]["glyph"].as_str()?.chars().next().unwrap_or('*'),
                sprite: d["look"]["id"].as_str().map(str::to_string),
                color: color(&d["look"]["color"]),
            },
            "explosion.started" => Shape::Burst {
                at: self.level_square(&d["at"])?,
                radius: d["radius"].as_u64().unwrap_or(0) as u32,
                color: color(&d["color"]),
                tile: d["tile"].as_str().unwrap_or("explosion").to_string(),
            },
            "explosion.blast" | "explosion.shrapnel" => Shape::Blast {
                cells: squares("cells"),
                shrapnel: fact.kind == "explosion.shrapnel",
                tile: self
                    .explosion_tiles
                    .get(d["explosion"].as_str().unwrap_or_default())
                    .cloned()
                    .unwrap_or_else(|| "explosion".into()),
            },
            "combat_text.shown" => Shape::Text {
                at: self.level_square(&d["at"])?,
                segments: d["segments"]
                    .as_array()?
                    .iter()
                    .map(|s| {
                        (
                            s["text"].as_str().unwrap_or_default().to_string(),
                            color(&s["color"]),
                        )
                    })
                    .collect(),
                scroll: Pos {
                    x: d["scroll"]["x"].as_i64().unwrap_or(0) as i32,
                    y: d["scroll"]["y"].as_i64().unwrap_or(-1) as i32,
                },
                hp: d["kind"] == "hp",
            },
            _ => return None,
        };
        Some(Effect {
            shape,
            duration_ms: fact.duration_ms,
        })
    }

    /// The animations of the facts the engine told since the last call.
    pub(super) fn told_effects(&mut self) -> Vec<Effect> {
        let facts = self.replica.take_facts();
        // A multishot is held while more of its pellets may come with the next notification; a poll that brings
        // nothing ends it.
        let quiet = facts.is_empty();
        let mut effects = vec![];
        for fact in facts {
            let id = fact.data["explosion"]
                .as_str()
                .unwrap_or_default()
                .to_string();
            match fact.kind.as_str() {
                "explosion.started" => {
                    let tile = fact.data["tile"].as_str().unwrap_or("explosion");
                    self.explosion_tiles.insert(id, tile.to_string());
                }
                "explosion.ended" => {
                    self.explosion_tiles.remove(&id);
                }
                _ => {}
            }
            let group = fact.data["group"].as_str();
            // Another multishot, or a shot outside one, ends the multishot told so far.
            let ends = self.volley.as_ref().is_some_and(|v| match group {
                Some(g) => g != v.group,
                None => fact.kind == "projectile.moved",
            });
            if ends {
                effects.extend(self.volley.take().and_then(Volley::effect));
            }
            let effect = self.effect_of(&fact);
            let at = self.level_square(&fact.data["at"]);
            let id = fact.data["projectile"].as_str().unwrap_or_default();
            match (group, &mut self.volley) {
                (Some(g), volley) => {
                    let volley = volley.get_or_insert_with(|| Volley::new(g));
                    match fact.kind.as_str() {
                        "projectile.moved" => effect.into_iter().for_each(|e| volley.flew(id, e)),
                        "projectile.hit" | "item.landed" => volley.hit(id, at),
                        _ => effects.extend(effect.and_then(|e| volley.follow(e))),
                    }
                }
                (None, Some(volley)) => effects.extend(effect.and_then(|e| volley.follow(e))),
                (None, None) => effects.extend(effect),
            }
        }
        if quiet {
            effects.extend(self.volley.take().and_then(Volley::effect));
        }
        effects
    }
}
