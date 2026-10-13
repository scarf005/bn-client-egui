//! Sounds the avatar heard that the game's message log has no line for: each distinct sound once with
//! how often it came, fading with its age until it is gone.

use crate::model::Heard;
use std::time::Duration;

/// How long a sound stays in the list.
const LIFETIME: Duration = Duration::from_secs(12);
/// The opacity of a sound about to disappear.
const LEAST: f32 = 0.2;

/// One shown sound: its text with the count when it came more than once, and its opacity.
#[derive(Clone, Debug, PartialEq)]
pub struct SoundLine {
    pub text: String,
    pub alpha: f32,
}

/// The sounds younger than `LIFETIME` at `now`, one line per distinct text in the order each last
/// came, faded by the age of its newest occurrence.
pub fn grouped(sounds: &[Heard], now: Duration) -> Vec<SoundLine> {
    let mut lines: Vec<(&str, usize, Duration)> = vec![];
    for sound in sounds
        .iter()
        .filter(|s| now.saturating_sub(s.at) < LIFETIME)
    {
        let (count, newest) = match lines.iter().position(|l| l.0 == sound.text) {
            Some(i) => {
                let old = lines.remove(i);
                (old.1 + 1, sound.at)
            }
            None => (1, sound.at),
        };
        lines.push((&sound.text, count, newest));
    }
    lines
        .into_iter()
        .map(|(text, count, at)| {
            let age = now.saturating_sub(at).as_secs_f32() / LIFETIME.as_secs_f32();
            SoundLine {
                text: match count {
                    1 => text.to_string(),
                    n => format!("{text} ×{n}"),
                },
                alpha: 1.0 - age * (1.0 - LEAST),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn heard(text: &str, secs: u64) -> Heard {
        Heard {
            text: text.into(),
            at: Duration::from_secs(secs),
        }
    }

    #[test]
    fn a_repeated_sound_is_one_line_with_its_count_at_the_place_of_its_newest() {
        let sounds = [heard("a", 1), heard("b", 2), heard("a", 3)];
        let lines = grouped(&sounds, Duration::from_secs(3));
        let texts = lines.iter().map(|l| l.text.as_str()).collect::<Vec<_>>();
        assert_eq!(texts, ["b", "a ×2"]);
    }

    #[test]
    fn a_sound_fades_with_age_and_is_gone_after_its_lifetime() {
        let sounds = [heard("a", 0)];
        assert_eq!(grouped(&sounds, Duration::ZERO)[0].alpha, 1.0);
        let half = grouped(&sounds, LIFETIME / 2)[0].alpha;
        assert!(LEAST < half && half < 1.0, "{half}");
        assert!(grouped(&sounds, LIFETIME).is_empty());
    }
}
