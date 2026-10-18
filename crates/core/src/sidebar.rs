//! The engine's classic sidebar values, laid out by the client as coloured lines.

use crate::colors::ColorMap;
use crate::markup;
use crate::model::{Cell, Rgb};
use crate::protocol::{Avatar, Look, SidebarSide, SidebarText};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Span {
    pub text: String,
    pub color: Rgb,
    /// The engine's numbers behind a bar that ends the text in `BAR` cells, for a client that draws the bar.
    pub gauge: Option<Gauge>,
}

/// `current` of `max`, such as a limb's health.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Gauge {
    pub current: i64,
    pub max: i64,
}

impl Span {
    /// The text as one span per colour run of its inline markup, the span's own colour where the markup names none.
    pub fn runs(&self) -> Vec<Span> {
        markup::runs(&self.text, &ColorMap::default())
            .into_iter()
            .map(|run| Span {
                text: run.text,
                color: run.color.unwrap_or(self.color),
                gauge: None,
            })
            .collect()
    }

    /// The characters the text takes once printed, its colour tags not counted.
    pub fn width(&self) -> usize {
        markup::plain(&self.text).chars().count()
    }
}

/// One sidebar row; its spans read left to right. A value with nothing to print stays as an empty
/// span, so every other span keeps its place in the row.
pub type Line = Vec<Span>;

/// Cells of a health bar.
pub const BAR: i64 = 5;

/// The colour of labels and of values whose colour means nothing.
const PLAIN: &str = "light_gray";

/// `current` of `max` as a bar of `BAR` cells, the rest dotted.
fn bar(current: i64, max: i64) -> String {
    let filled = if max > 0 {
        (current.clamp(0, max) * BAR + max - 1) / max
    } else {
        0
    };
    "|".repeat(filled as usize) + &".".repeat((BAR - filled) as usize)
}

/// The rows of the panels the client draws. The classic health panel puts a limb beside a need and a stat on
/// its first six rows, then temperature and power; the other layouts split it into limbs, needs, stats and
/// movement. A row with nothing to print is left out.
struct Panels {
    health: Vec<Line>,
    limbs: Vec<Line>,
    needs: Vec<Line>,
    stats: Vec<Line>,
    movement: Vec<Line>,
    location: Vec<Line>,
    weather: Vec<Line>,
    time: Vec<Line>,
    weapon: Vec<Line>,
}

impl Panels {
    /// The rows of the panel the engine's panel manager calls `id`, taken out of `self`.
    fn take(&mut self, id: &str) -> Option<Vec<Line>> {
        let rows = match id {
            "Health" => &mut self.health,
            "Limbs" => &mut self.limbs,
            "Needs" => &mut self.needs,
            "Stats" => &mut self.stats,
            "Movement" => &mut self.movement,
            "Location" => &mut self.location,
            "Weather" => &mut self.weather,
            "Time" => &mut self.time,
            "Weapon" => &mut self.weapon,
            _ => return None,
        };
        Some(std::mem::take(rows))
    }
}

fn panels(avatar: &Avatar, colors: &ColorMap) -> Option<Panels> {
    let side = avatar.sidebar.as_ref()?;
    let span = |text: String, color: &str| Span {
        text,
        color: colors
            .resolve(if color.is_empty() { PLAIN } else { color })
            .0,
        gauge: None,
    };
    let gauged = |span: Span, current: i64, max: i64| Span {
        gauge: Some(Gauge { current, max }),
        ..span
    };
    let value = |t: &SidebarText| span(t.text.clone(), &t.color);
    let label = |t: &SidebarText| span(t.label.clone().unwrap_or_default(), PLAIN);
    // A label and its value as one span, so a narrow panel never breaks them apart.
    let labelled = |label: &str, text: &str, color: &str| {
        let text = match (label.is_empty(), text.is_empty()) {
            (_, true) => String::new(),
            (true, false) => text.to_string(),
            (false, false) => format!("{label} {text}"),
        };
        span(text, color)
    };
    let stat = |id: &str| {
        avatar.stats.iter().find(|s| s.id == id).map_or_else(
            || span(String::new(), PLAIN),
            |s| {
                let value = s
                    .value
                    .as_str()
                    .map_or_else(|| s.value.to_string(), str::to_string);
                labelled(
                    s.short_label.as_deref().unwrap_or(&s.label),
                    &value,
                    s.color.as_deref().unwrap_or_default(),
                )
            },
        )
    };
    let (stamina, movement, safe) = (&side.stamina, &side.movement, &side.safe_mode);
    let face = span(side.morale.face.clone(), &side.morale.color);
    let focus = labelled(&side.focus.label, &side.focus.value.to_string(), "white");
    let safe = span(
        if safe.enabled {
            safe.label.clone()
        } else {
            Default::default()
        },
        &safe.color,
    );
    let speed = labelled(
        &side.speed.label,
        &side.speed.value.to_string(),
        &side.speed.color,
    );
    let moving = span(
        format!("{} {}", movement.counter, movement.mode_text),
        &movement.color,
    );
    let stamina = gauged(
        labelled(
            &stamina.label,
            &bar(stamina.current, stamina.max),
            &stamina.color,
        ),
        stamina.current,
        stamina.max,
    );
    let power = labelled(
        side.power.label.as_deref().unwrap_or_default(),
        &side.power.text,
        &side.power.color,
    );
    let beside: [Line; 6] = [
        vec![value(&side.pain), stat("strength")],
        vec![value(&side.hunger), face.clone(), stat("dexterity")],
        vec![value(&side.thirst), stat("intelligence")],
        vec![value(&side.fatigue), stat("perception")],
        vec![focus.clone(), safe.clone()],
        vec![speed.clone(), moving.clone(), stamina.clone()],
    ];
    let limbs = side
        .limbs
        .iter()
        .map(|limb| {
            vec![
                value(&limb.label),
                gauged(
                    span(bar(limb.hp, limb.hp_max), &limb.color),
                    limb.hp,
                    limb.hp_max,
                ),
            ]
        })
        .collect::<Vec<_>>();
    let rows = limbs.len().max(beside.len());
    let mut health = (0..rows)
        .map(|i| {
            let mut row = limbs.get(i).cloned().unwrap_or_default();
            row.extend(beside.get(i).cloned().unwrap_or_default());
            row
        })
        .collect::<Vec<_>>();
    let mut time = vec![
        span(
            format!(
                "{}, {} {}",
                side.time.season, side.time.day_label, side.time.day
            ),
            "white",
        ),
        span(side.time.clock.clone(), PLAIN),
    ];
    time.extend(
        side.ambient_temperature
            .as_ref()
            .map(|t| labelled(t.label.as_deref().unwrap_or_default(), &t.text, &t.color)),
    );
    let mut weapon = vec![label(&side.weapon), value(&side.weapon)];
    weapon.extend(side.style.as_ref().map(value));
    health.push(vec![value(&side.temperature), power.clone()]);
    let one = |row: Line| vec![row];
    let mut result = Panels {
        health,
        limbs,
        needs: vec![
            vec![value(&side.pain), value(&side.thirst)],
            vec![value(&side.fatigue), value(&side.hunger)],
            vec![value(&side.temperature)],
        ],
        stats: vec![
            vec![stat("strength"), stat("dexterity"), power],
            vec![stat("intelligence"), stat("perception"), safe],
        ],
        movement: vec![vec![face, focus], vec![stamina, speed, moving]],
        location: one(vec![label(&side.location), value(&side.location)]),
        weather: one(vec![label(&side.weather), value(&side.weather)]),
        time: one(time),
        weapon: one(weapon),
    };
    [
        &mut result.health,
        &mut result.limbs,
        &mut result.needs,
        &mut result.stats,
        &mut result.movement,
        &mut result.location,
        &mut result.weather,
        &mut result.time,
        &mut result.weapon,
    ]
    .into_iter()
    .for_each(|rows| rows.retain(|row| row.iter().any(|s| !s.text.is_empty())));
    Some(result)
}

/// All panels' rows in the classic order; empty before the engine reports a sidebar.
pub fn lines(avatar: &Avatar, colors: &ColorMap) -> Vec<Line> {
    panels(avatar, colors).map_or(vec![], |p| {
        [p.health, p.location, p.weather, p.time, p.weapon].concat()
    })
}

/// What a panel of the sidebar shows.
#[derive(Clone, Debug, PartialEq)]
pub enum Body {
    /// Coloured rows.
    Status(Vec<Line>),
    /// The message log.
    Log,
    /// The creatures in sight, by direction.
    Compass(Vec<Direction>),
}

/// The creatures toward one direction, nearest first.
#[derive(Clone, Debug, PartialEq)]
pub struct Direction {
    /// The heading the game prints (`North:`), else the direction's name.
    pub label: String,
    pub creatures: Vec<Creature>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Creature {
    pub name: String,
    /// Squares from the avatar.
    pub distance: u32,
    /// Its sprite, or its glyph in the colour of its look.
    pub cell: Cell,
}

/// One enabled panel the client can draw.
#[derive(Clone, Debug, PartialEq)]
pub struct Panel {
    /// The panel manager's name for it (`Health`).
    pub id: String,
    pub label: String,
    /// Rows the native panel takes; absent for the one that takes the rest of the column.
    pub height: Option<u32>,
    pub body: Body,
}

/// The sidebar as the game's panel options arrange it.
#[derive(Clone, Debug, PartialEq)]
pub struct Layout {
    pub side: SidebarSide,
    /// Columns the native sidebar takes.
    pub width: u32,
    /// The enabled panels the client has a renderer for, top to bottom, labelled by the engine.
    pub panels: Vec<Panel>,
}

impl Layout {
    /// Whether the message log panel is on.
    pub fn has_log(&self) -> bool {
        self.panels.iter().any(|p| p.body == Body::Log)
    }
}

/// The avatar's sidebar laid out by its `layout`, or `None` from an engine that sends none. Panels the
/// player switched off or the client cannot draw are left out. `cell` draws a creature's look.
pub fn layout(avatar: &Avatar, colors: &ColorMap, cell: impl Fn(&Look) -> Cell) -> Option<Layout> {
    let arrangement = avatar.sidebar.as_ref()?.layout.as_ref()?;
    let mut rows = panels(avatar, colors)?;
    let compass = || {
        avatar.compass.iter().flatten().map(|d| Direction {
            label: d.label.clone().unwrap_or_else(|| d.toward.name.clone()),
            creatures: d
                .creatures
                .iter()
                .map(|c| Creature {
                    name: c.name.clone(),
                    distance: c.distance,
                    cell: cell(&c.look),
                })
                .collect(),
        })
    };
    let panels = arrangement
        .panels
        .iter()
        .filter(|p| p.enabled)
        .filter_map(|p| {
            let body = match p.id.as_str() {
                "Log" => Body::Log,
                "Compass" | "Comp.Compass" => Body::Compass(compass().collect()),
                id => Body::Status(rows.take(id)?),
            };
            Some(Panel {
                id: p.id.clone(),
                label: p.label.clone(),
                height: p.height,
                body,
            })
        })
        .collect();
    Some(Layout {
        side: arrangement.side,
        width: arrangement.width,
        panels,
    })
}
