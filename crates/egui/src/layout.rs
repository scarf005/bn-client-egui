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

/// The room `count` panes' gaps take.
pub fn gaps(count: usize, gap: f32) -> f32 {
    count.saturating_sub(1) as f32 * gap
}

/// Whether the panes, at these widths, are wider than `room`.
pub fn overflows(widths: &[f32], room: f32, gap: f32) -> bool {
    widths.iter().sum::<f32>() + gaps(widths.len(), gap) > room + 0.5
}
