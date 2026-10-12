//! The engine's classic sidebar as coloured lines, in the native order: limbs beside the needs,
//! then stats, location, weather, time and weapon.

use crate::colors::ColorMap;
use crate::model::Rgb;
use crate::protocol::{Avatar, Sidebar, SidebarText};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Span {
    pub text: String,
    pub color: Rgb,
}

/// One sidebar row; its spans read left to right. A span that prints nothing (a need that is not
/// felt) stays as an empty one, so every other span keeps its place in the row.
pub type Line = Vec<Span>;

fn line(spans: Vec<Option<Span>>) -> Line {
    spans.into_iter().flatten().collect()
}

pub fn lines(avatar: &Avatar, colors: &ColorMap) -> Vec<Line> {
    let Some(side) = &avatar.sidebar else {
        return vec![];
    };
    let span = |text: String, color: &str| {
        Some(Span {
            text,
            color: colors.resolve(color).0,
        })
    };
    let said = |t: &SidebarText| span(t.text.clone(), &t.color);
    let plain = |text: &str| span(text.into(), "light_gray");
    let stat = |id: &str, name: &str| {
        let s = avatar.stats.iter().find(|s| s.id == id)?;
        let value = s
            .value
            .as_str()
            .map_or_else(|| s.value.to_string(), String::from);
        span(
            format!("{name} {value}"),
            s.color.as_deref().unwrap_or("light_gray"),
        )
    };
    let Sidebar { limbs, .. } = side;
    let limb = |i: usize| {
        let l = limbs.get(i)?;
        let mut parts = vec![span(l.label.text.clone(), &l.label.color)?];
        parts.push(span(format!("{}/{}", l.hp, l.hp_max), &l.color)?);
        Some(parts)
    };
    let mut rows: Vec<Line> = vec![];
    // Rows 0..5 hold one limb each, with the needs and stats beside them.
    let beside: [Vec<Option<Span>>; 6] = [
        vec![said(&side.pain), stat("strength", "Str")],
        vec![
            said(&side.hunger),
            span(side.morale.face.clone(), &side.morale.color),
            stat("dexterity", "Dex"),
        ],
        vec![said(&side.thirst), stat("intelligence", "Int")],
        vec![said(&side.fatigue), stat("perception", "Per")],
        vec![
            plain(&format!("Focus {}", side.focus)),
            span(
                if side.safe_mode.enabled {
                    "SAFE".into()
                } else {
                    String::new()
                },
                &side.safe_mode.color,
            ),
        ],
        vec![
            span(format!("Spd {}", side.speed.value), &side.speed.color),
            span(
                format!("{} {}", side.movement.counter, side.movement.mode),
                &side.movement.color,
            ),
            span(
                format!("Stm {}/{}", side.stamina.current, side.stamina.max),
                &side.stamina.color,
            ),
        ],
    ];
    for (i, rest) in beside.into_iter().enumerate() {
        let mut spans: Vec<Option<Span>> =
            limb(i).map_or(vec![], |p| p.into_iter().map(Some).collect());
        spans.extend(rest);
        rows.push(line(spans));
    }
    // Limbs beyond the six the classic panel has room for.
    for i in 6..limbs.len() {
        rows.push(line(
            limb(i).map_or(vec![], |p| p.into_iter().map(Some).collect()),
        ));
    }
    rows.push(line(vec![
        said(&side.temperature),
        span(format!("Power {}", side.power.text), &side.power.color),
    ]));
    rows.push(line(vec![plain("Location:"), said(&side.location)]));
    rows.push(line(vec![plain("Weather:"), said(&side.weather)]));
    let t = &side.time;
    rows.push(line(vec![
        span(format!("{}, Day {}", t.season, t.day), "white"),
        plain(&t.clock),
        side.ambient_temperature
            .as_ref()
            .and_then(|a| plain(&format!("Temp {a}"))),
    ]));
    rows.push(line(vec![
        plain("Weapon:"),
        plain(&side.weapon),
        side.style.as_ref().and_then(said),
    ]));
    rows.retain(|r| r.iter().any(|s| !s.text.is_empty()));
    rows
}
