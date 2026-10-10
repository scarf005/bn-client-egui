//! Game key settings: which keys trigger which registered action in the main game context.

use serde_json::Value;
use std::collections::BTreeMap;
use std::path::Path;

/// Directional actions have no entry in `keybindings.json`; the game registers arrows and
/// vi-keys for them (see the tutorial text).
const MOVES: [(&str, &[&str]); 8] = [
    ("UP", &["UP", "k"]),
    ("DOWN", &["DOWN", "j"]),
    ("LEFT", &["LEFT", "h"]),
    ("RIGHT", &["RIGHT", "l"]),
    ("LEFTUP", &["HOME", "y"]),
    ("RIGHTUP", &["PAGE_UP", "u"]),
    ("LEFTDOWN", &["END", "b"]),
    ("RIGHTDOWN", &["PAGE_DOWN", "n"]),
];

#[derive(Clone, Debug)]
pub struct KeyMap {
    /// Action id to keyboard keys, in file order.
    keys: BTreeMap<String, Vec<String>>,
}

impl Default for KeyMap {
    fn default() -> Self {
        let keys = MOVES
            .iter()
            .map(|(id, ks)| (id.to_string(), ks.iter().map(|k| k.to_string()).collect()))
            .collect();
        Self { keys }
    }
}

impl KeyMap {
    pub fn from_json(text: &str, context: &str) -> Option<Self> {
        let json: Value = serde_json::from_str(text).ok()?;
        let mut map = Self::default();
        for entry in json.as_array()?.iter().filter(|e| e["category"] == context) {
            let (Some(id), Some(bindings)) = (entry["id"].as_str(), entry["bindings"].as_array())
            else {
                continue;
            };
            let keys = bindings
                .iter()
                .filter(|b| b["input_method"] == "keyboard")
                .filter_map(|b| b["key"].as_str())
                .map(String::from)
                .collect::<Vec<_>>();
            if !keys.is_empty() {
                map.keys.insert(id.to_string(), keys);
            }
        }
        Some(map)
    }

    /// The main-game bindings of the game installed at `root`; movement keys only if unreadable.
    pub fn load(root: &Path) -> Self {
        std::fs::read_to_string(root.join("data/raw/keybindings/keybindings.json"))
            .ok()
            .and_then(|t| Self::from_json(&t, "DEFAULTMODE"))
            .unwrap_or_default()
    }

    pub fn keys(&self, action: &str) -> &[String] {
        self.keys.get(action).map_or(&[], Vec::as_slice)
    }

    pub fn actions_for(&self, key: &str) -> Vec<&str> {
        self.keys
            .iter()
            .filter(|(_, ks)| ks.iter().any(|k| k == key))
            .map(|(id, _)| id.as_str())
            .collect()
    }
}
