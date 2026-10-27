//! The width rule of split panes: each pane is as wide as its content, never narrower than the content's
//! minimum, so a word never breaks inside a pane; what is left over goes to the panes that can use it.

/// What a pane needs, measured from its content.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Extent {
    /// The width below which the content would break inside a word or lose a column.
    pub min: f32,
    /// The width of the content laid out without wrapping.
    pub natural: f32,
    /// Takes the room left once every pane has its natural width (a list, a long text).
    pub grows: bool,
    /// Takes a line of its own: a pane that wraps to a further line does not share it (a terminal's output).
    pub alone: bool,
}

/// The width of each pane in `room`, with `gap` between panes. Never below a pane's minimum: when the minimums
/// do not fit, they are the widths and the caller scrolls sideways.
pub fn split_widths(parts: &[Extent], room: f32, gap: f32) -> Vec<f32> {
    let spare = room - gaps(parts.len(), gap) - parts.iter().map(|p| p.min).sum::<f32>();
    let want = parts
        .iter()
        .map(|p| (p.natural - p.min).max(0.0))
        .collect::<Vec<_>>();
    let wanted = want.iter().sum::<f32>();
    let growers = parts.iter().filter(|p| p.grows).count();
    let left = (spare - wanted).max(0.0);
    parts
        .iter()
        .zip(want)
        .map(|(p, want)| {
            let toward_natural = match wanted > spare {
                true => spare.max(0.0) * want / wanted.max(f32::EPSILON),
                false => want,
            };
            let share = match (p.grows, growers) {
                (true, n) => left / n as f32,
                (false, 0) => left / parts.len() as f32,
                _ => 0.0,
            };
            p.min + toward_natural + share
        })
        .collect()
}

/// The panes cut into lines when their minimums do not fit one or one of them stands alone: a line takes panes
/// while their natural widths fit `room`, and a pane wider than the room, or one that stands alone, has a line of
/// its own. Panes that fit stay on one line.
pub fn wrap_lines(parts: &[Extent], room: f32, gap: f32) -> Vec<std::ops::Range<usize>> {
    let mins = parts.iter().map(|p| p.min).collect::<Vec<_>>();
    if !overflows(&mins, room, gap) && !parts.iter().any(|p| p.alone) {
        return std::iter::once(0..parts.len()).collect();
    }
    let mut lines = Vec::new();
    let (mut start, mut used) = (0, 0.0);
    for (i, part) in parts.iter().enumerate() {
        let need = part.natural.min(room);
        if i > start && (part.alone || parts[i - 1].alone || used + gap + need > room) {
            lines.push(start..i);
            (start, used) = (i, need);
        } else {
            used += if i > start { gap } else { 0.0 } + need;
        }
    }
    lines.push(start..parts.len());
    lines
}

/// The room `count` panes' gaps take.
pub fn gaps(count: usize, gap: f32) -> f32 {
    count.saturating_sub(1) as f32 * gap
}

/// Whether the panes, at these widths, are wider than `room`.
pub fn overflows(widths: &[f32], room: f32, gap: f32) -> bool {
    widths.iter().sum::<f32>() + gaps(widths.len(), gap) > room + 0.5
}

/// The share of the room the first pane of a detail split, the list the others describe, takes at least.
const MASTER_SHARE: f32 = 0.4;

/// The widths of a detail split: the first pane, the list, takes `MASTER_SHARE` of the room and at least its
/// natural width; the panes that describe it share the remainder. Never below a pane's minimum; a list with
/// nothing to describe it is a split of one.
pub fn detail_widths(parts: &[Extent], room: f32, gap: f32) -> Vec<f32> {
    let Some((master, rest @ [_, ..])) = parts.split_first() else {
        return split_widths(parts, room, gap);
    };
    let rest_min = rest.iter().map(|p| p.min).sum::<f32>() + gaps(parts.len(), gap);
    let width = (room * MASTER_SHARE)
        .max(master.natural)
        .min(room - rest_min)
        .max(master.min);
    std::iter::once(width)
        .chain(split_widths(rest, room - width - gap, gap))
        .collect()
}

/// The widths of peers of which `prose` is the only running text. A text no longer than an equal share of the room
/// takes its natural width, or wraps to leave the others theirs, and the others share the rest as `split_widths`
/// does. A longer text takes the room the others leave of their natural widths, never less than an equal share.
pub fn prose_widths(parts: &[Extent], prose: usize, room: f32, gap: f32) -> Vec<f32> {
    let inner = room - gaps(parts.len(), gap);
    let text = parts[prose];
    let fair = inner / parts.len() as f32;
    let others = |grows: bool| {
        parts
            .iter()
            .enumerate()
            .filter(|(i, _)| *i != prose)
            .map(|(_, p)| Extent {
                grows: p.grows && grows,
                ..*p
            })
            .collect::<Vec<_>>()
    };
    let keeps = others(false);
    let spare = inner - keeps.iter().map(|p| p.natural).sum::<f32>();
    let short = text.natural <= fair && spare >= text.min;
    let wanted = match short {
        true => spare.min(text.natural),
        false => spare.max(fair.min(text.natural)),
    };
    let width = wanted
        .min(inner - keeps.iter().map(|p| p.min).sum::<f32>())
        .max(text.min);
    let mut widths = split_widths(&others(short), room - width - gap, gap);
    widths.insert(prose, width);
    widths
}

/// The height, at most `room`, of a viewport from the top of content whose lines of text are `rows`, as
/// (top, bottom) from the content's top: `room`, or the top of the line it would cut across, until it cuts none or
/// has moved `MOVES` times: lines of past views, left behind by a list that scrolled since, chain without end.
pub fn whole_rows(rows: &[(f32, f32)], room: f32) -> f32 {
    const MOVES: usize = 4;
    (0..MOVES).fold(room, |at, _| {
        rows.iter()
            .filter(|(top, bottom)| *top + 0.5 < at && at < *bottom - 0.5)
            .map(|(top, _)| *top)
            .reduce(f32::min)
            .unwrap_or(at)
    })
}
