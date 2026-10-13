//! The engine's classic sidebar values, laid out by the client as coloured lines.

use crate::colors::ColorMap;
use crate::model::Rgb;
use crate::protocol::{Avatar, SidebarText};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Span {
    pub text: String,
    pub color: Rgb,
}

/// One sidebar row; its spans read left to right. A value with nothing to print stays as an empty
/// span, so every other span keeps its place in the row.
pub type Line = Vec<Span>;

/// Cells of a health bar.
const BAR: i64 = 5;

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

/// The classic layout: a limb beside a need and a stat on the first six rows, then temperature and
/// power, location, weather, date and time, and the weapon. A row with nothing to print is left out.
pub fn lines(avatar: &Avatar, colors: &ColorMap) -> Vec<Line> {
    let Some(side) = &avatar.sidebar else {
        return vec![];
    };
    let span = |text: String, color: &str| Span {
        text,
        color: colors
            .resolve(if color.is_empty() { PLAIN } else { color })
            .0,
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
    let beside: [Line; 6] = [
        vec![value(&side.pain), stat("strength")],
        vec![
            value(&side.hunger),
            span(side.morale.face.clone(), &side.morale.color),
            stat("dexterity"),
        ],
        vec![value(&side.thirst), stat("intelligence")],
        vec![value(&side.fatigue), stat("perception")],
        vec![
            labelled(&side.focus.label, &side.focus.value.to_string(), "white"),
            span(
                safe.enabled.then(|| safe.label.clone()).unwrap_or_default(),
                &safe.color,
            ),
        ],
        vec![
            labelled(
                &side.speed.label,
                &side.speed.value.to_string(),
                &side.speed.color,
            ),
            span(
                format!("{} {}", movement.counter, movement.mode_text),
                &movement.color,
            ),
            labelled(
                &stamina.label,
                &bar(stamina.current, stamina.max),
                &stamina.color,
            ),
        ],
    ];
    let rows = side.limbs.len().max(beside.len());
    let mut result = (0..rows)
        .map(|i| {
            let mut row = side.limbs.get(i).map_or_else(Vec::new, |limb| {
                vec![
                    value(&limb.label),
                    span(bar(limb.hp, limb.hp_max), &limb.color),
                ]
            });
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
    result.extend([
        vec![
            value(&side.temperature),
            labelled(
                side.power.label.as_deref().unwrap_or_default(),
                &side.power.text,
                &side.power.color,
            ),
        ],
        vec![label(&side.location), value(&side.location)],
        vec![label(&side.weather), value(&side.weather)],
        time,
        weapon,
    ]);
    result.retain(|row| row.iter().any(|s| !s.text.is_empty()));
    result
}
