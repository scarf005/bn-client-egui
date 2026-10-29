//! The player's display choices kept between runs, and the controls that change them.

use super::*;

/// The choice that survives a restart.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Settings {
    /// Draw sprites (true) or ASCII glyphs (false); absent until the player chooses, so the engine's
    /// `USE_TILES` option decides.
    #[serde(default)]
    pub tiles: Option<bool>,
    /// The `TILES` name of the tileset; the game's default when absent.
    pub tileset: Option<String>,
    /// Play no shot, explosion or floating-text overlays on the map, and keep animated tiles on their first frame.
    #[serde(default)]
    pub reduced_motion: bool,
    /// Draw the map as a 3D scene a camera can turn, or flat; an isometric tileset is drawn in 3D unless this says
    /// otherwise.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub three_d: Option<bool>,
    /// The 3D map is seen through the avatar's eyes.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub first_person: bool,
    /// The map zoom in steps from the tileset's own size, which the zoom keys and the wheel move.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub zoom: i8,
    /// The size of the whole interface in percent of its natural size, one of `SCALES`; 100 when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interface_scale: Option<u16>,
    /// Where each kind of window sits, how large it is and whether it is collapsed.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub windows: BTreeMap<String, WindowState>,
}

/// The interface sizes the player can choose, in percent.
pub(super) const SCALES: [u16; 6] = [75, 100, 125, 150, 175, 200];

fn is_zero(n: &i8) -> bool {
    *n == 0
}

/// `$XDG_CONFIG_HOME/bn-client-egui/settings.json`, or the same under `~/.config`.
pub fn settings_path() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(base.join("bn-client-egui/settings.json"))
}

/// Whether the game at `root` would be shown with sprites: sprites are chosen in the saved settings
/// at `path` (on by default) and the game has a tileset.
pub fn tiles_wanted(root: Option<&Path>, path: Option<&Path>) -> bool {
    let settings = path
        .and_then(|p| std::fs::read(p).ok())
        .and_then(|b| serde_json::from_slice::<Settings>(&b).ok())
        .unwrap_or_default();
    settings.tiles.unwrap_or(true) && root.is_some_and(|r| !tileset::available(r).is_empty())
}

impl Display {
    /// Mode, tileset and motion choice, kept as soon as they change; `take_focus` puts the keyboard focus on the
    /// first of them.
    pub fn controls(&mut self, ui: &mut egui::Ui, take_focus: bool) {
        let before = self.settings.clone();
        ui.vertical(|ui| {
            ui.horizontal(|ui| {
                for (tiles, label) in [(true, tr("Tiles")), (false, "ASCII".into())] {
                    let choice = ui.selectable_label(self.wants_tiles() == tiles, label);
                    if take_focus && tiles {
                        choice.request_focus();
                    }
                    if choice.clicked() {
                        self.settings.tiles = Some(tiles);
                    }
                }
                ui.add_enabled_ui(self.wants_tiles(), |ui| {
                    let first = tr("First person");
                    for (three_d, first_person, label) in [
                        (false, false, "2D"),
                        (true, false, "3D"),
                        (true, true, first.as_str()),
                    ] {
                        let on = self.three_d() == three_d && self.first_person() == first_person;
                        if ui.selectable_label(on, label).clicked() {
                            self.settings.three_d = Some(three_d);
                            self.settings.first_person = first_person;
                        }
                    }
                });
            });
            let shown = self.chosen().map_or_else(|| tr("none"), |t| t.view.clone());
            let mut pick = None;
            ui.add_enabled_ui(self.wants_tiles(), |ui| {
                egui::ComboBox::from_label(tr("Tileset"))
                    .selected_text(shown)
                    .show_ui(ui, |ui| {
                        for t in &self.tilesets {
                            let on = self.chosen().is_some_and(|c| c.name == t.name);
                            if ui.selectable_label(on, &t.view).clicked() {
                                pick = Some(t.name.clone());
                            }
                        }
                    });
            });
            if pick.is_some() {
                self.settings.tileset = pick;
            }
            if let Some(problem) = &self.problem {
                ui.label(tr_format(
                    "Tileset unavailable: %s",
                    &[Arg::Text(problem.clone())],
                ));
            }
            ui.checkbox(&mut self.settings.reduced_motion, tr("Reduced motion"));
            let scale = self.interface_scale();
            egui::ComboBox::from_label(tr("Interface scale"))
                .selected_text(format!("{scale} %"))
                .show_ui(ui, |ui| {
                    for choice in SCALES {
                        let label = format!("{choice} %");
                        if ui.selectable_label(scale == choice, label).clicked() {
                            self.settings.interface_scale = Some(choice);
                        }
                    }
                });
        });
        if self.settings != before {
            self.save();
            // Another view of the same tileset keeps it.
            if (&self.settings.tiles, &self.settings.tileset) != (&before.tiles, &before.tileset) {
                self.open();
            }
        }
    }
}
