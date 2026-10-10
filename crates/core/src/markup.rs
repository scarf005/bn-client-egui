//! The game's inline colour markup (`<color_c_light_green>text</color>`), parsed into styled runs.

use crate::colors::ColorMap;
use crate::model::Rgb;

const OPEN: &str = "<color_";
const CLOSE: &str = "</color>";

/// A stretch of text in one colour; `None` is the surrounding text colour.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Run {
    pub text: String,
    pub color: Option<Rgb>,
}

/// Splits `text` at its colour tags. Tags nest; a closer without an opener and any other `<` stay as written.
pub fn runs(text: &str, colors: &ColorMap) -> Vec<Run> {
    let mut out = Vec::new();
    let mut stack: Vec<Rgb> = Vec::new();
    let mut buf = String::new();
    let mut rest = text;
    let flush = |buf: &mut String, stack: &[Rgb], out: &mut Vec<Run>| {
        if !buf.is_empty() {
            out.push(Run {
                text: std::mem::take(buf),
                color: stack.last().copied(),
            });
        }
    };
    while let Some(at) = rest.find('<') {
        buf.push_str(&rest[..at]);
        rest = &rest[at..];
        if let Some(name_and_rest) = rest.strip_prefix(OPEN)
            && let Some(end) = name_and_rest.find('>')
        {
            flush(&mut buf, &stack, &mut out);
            stack.push(colors.resolve(&name_and_rest[..end]).0);
            rest = &name_and_rest[end + 1..];
        } else if let Some(after) = rest.strip_prefix(CLOSE) {
            if !stack.is_empty() {
                flush(&mut buf, &stack, &mut out);
                stack.pop();
            }
            rest = after;
        } else {
            buf.push('<');
            rest = &rest[1..];
        }
    }
    buf.push_str(rest);
    flush(&mut buf, &stack, &mut out);
    out
}

/// `text` without its colour tags, for names read aloud and compared.
pub fn plain(text: &str) -> String {
    runs(text, &ColorMap::default())
        .into_iter()
        .map(|r| r.text)
        .collect()
}
