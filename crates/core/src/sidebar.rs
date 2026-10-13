//! The engine's classic sidebar rows as coloured lines.

use crate::colors::ColorMap;
use crate::model::Rgb;
use crate::protocol::Avatar;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Span {
    pub text: String,
    pub color: Rgb,
}

/// One sidebar row as the engine words it; its spans read left to right. A span that prints nothing
/// stays as an empty one, so every other span keeps its place in the row.
pub type Line = Vec<Span>;

pub fn lines(avatar: &Avatar, colors: &ColorMap) -> Vec<Line> {
    let Some(side) = &avatar.sidebar else {
        return vec![];
    };
    side.rows
        .iter()
        .map(|row| {
            row.iter()
                .map(|t| Span {
                    text: t.text.clone(),
                    color: colors.resolve(&t.color).0,
                })
                .collect()
        })
        .collect()
}
