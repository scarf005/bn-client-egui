//! Game colour names (`c_light_gray`, `h_red`, `i_white`, `c_white_red`) to RGB, using the game's
//! own palette from `data/raw/colors.json` and its name table from `src/color.cpp`.

use crate::color_table::NAMED;
use eframe::egui::Color32;
use serde_json::Value;
use std::path::Path;

/// Palette keys of `colors.json` in curses colour order: indices 0..7, then their bold halves.
const KEYS: [&str; 16] = [
    "BLACK", "RED", "GREEN", "BROWN", "BLUE", "MAGENTA", "CYAN", "GRAY", "DGRAY", "LRED", "LGREEN",
    "YELLOW", "LBLUE", "LMAGENTA", "LCYAN", "WHITE",
];

#[derive(Clone, Debug)]
pub struct ColorMap {
    palette: [Color32; 16],
}

impl Default for ColorMap {
    fn default() -> Self {
        let rgb = |r, g, b| Color32::from_rgb(r, g, b);
        Self {
            palette: [
                rgb(60, 60, 60),
                rgb(170, 40, 40),
                rgb(40, 160, 60),
                rgb(150, 100, 50),
                rgb(60, 90, 200),
                rgb(170, 60, 170),
                rgb(40, 160, 170),
                rgb(180, 180, 180),
                rgb(110, 110, 110),
                rgb(240, 90, 90),
                rgb(110, 240, 110),
                rgb(240, 220, 80),
                rgb(110, 150, 255),
                rgb(240, 110, 240),
                rgb(110, 240, 240),
                rgb(250, 250, 250),
            ],
        }
    }
}

impl ColorMap {
    pub fn from_json(text: &str) -> Option<Self> {
        let json: Value = serde_json::from_str(text).ok()?;
        let def = json.as_array()?.iter().find(|e| e["type"] == "colordef")?;
        let mut palette = [Color32::BLACK; 16];
        for (slot, key) in palette.iter_mut().zip(KEYS) {
            let c = def[key].as_array()?;
            let ch = |i: usize| u8::try_from(c.get(i)?.as_u64()?).ok();
            *slot = Color32::from_rgb(ch(0)?, ch(1)?, ch(2)?);
        }
        Some(Self { palette })
    }

    /// The palette of the game installed at `root`, or the built-in one if it cannot be read.
    pub fn load(root: &Path) -> Self {
        std::fs::read_to_string(root.join("data/raw/colors.json"))
            .ok()
            .and_then(|t| Self::from_json(&t))
            .unwrap_or_default()
    }

    /// Foreground, and background when it is not black. Unknown names read as `c_light_gray`.
    pub fn resolve(&self, name: &str) -> (Color32, Option<Color32>) {
        let full = if name.contains('_') && name.as_bytes().get(1) == Some(&b'_') {
            name.to_string()
        } else {
            format!("c_{name}")
        };
        let find = |n: &str| NAMED.iter().find(|(k, ..)| *k == n);
        let &(_, fg, bg) = find(&full)
            .or_else(|| find("c_light_gray"))
            .expect("c_light_gray is in the table");
        let bg = (bg != 0).then(|| self.palette[usize::from(bg)]);
        (self.palette[usize::from(fg)], bg)
    }
}
