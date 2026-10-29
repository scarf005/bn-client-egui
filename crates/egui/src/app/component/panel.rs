//! A panel component: a section under its title with its subtitle, its stats and its child; the aim screen's
//! ways of aiming and cursor targets are drawn in it (`target`).

use super::*;
use bn_client_core::protocol::Panel;

/// A stat's value with its maximum and unit, as the panel shows it.
pub(super) fn stat_text(stat: &PanelStat) -> String {
    let max = stat
        .max
        .as_ref()
        .map_or(String::new(), |m| format!("/{}", m.shown()));
    let unit = stat
        .unit
        .as_ref()
        .map_or(String::new(), |u| format!(" {}", u.shown()));
    format!("{}{max}{unit}", markup::plain(&stat.value.shown()))
}

impl<M: GameModel> App<M> {
    /// `panel` as a section under its title: its subtitle, its stats as facts and ways of aiming, its filter, the
    /// creatures its map's cursor can jump to, and its child.
    pub(super) fn panel_section(
        &self,
        ui: &mut egui::Ui,
        screen: &Screen,
        panel: &Panel,
        out: &mut Vec<Intent>,
    ) {
        // A panel the screen's own title already names has none.
        let title = markup::plain(&panel.title.shown());
        section::show(ui, &title, |ui| {
            if let Some(subtitle) = &panel.subtitle {
                ui.label(self.line(ui, &subtitle.shown()));
            }
            if let Some(stats) = &panel.stats {
                let (levels, others) = split_levels(screen, stats);
                Self::facts(ui, &panel.head.id, &others);
                self.aim_levels(ui, &levels, out);
            }
            if let Some(filter) = &panel.filter {
                ui.weak(format!("/{filter}"));
            }
            if let Some(map) = panel.child.as_deref().and_then(Component::cursor_map) {
                self.target_candidates(ui, screen, map, out);
            }
            if let Some(child) = &panel.child {
                self.component(ui, screen, child, out);
            }
        });
    }

    /// The panel's stats as facts: label and value, or a stat without a value as its label in its tone.
    fn facts(ui: &mut egui::Ui, id: &str, stats: &[&PanelStat]) {
        let facts = stats
            .iter()
            .map(|stat| {
                let shown = stat_text(stat);
                let label = egui::RichText::new(markup::plain(&stat.label.shown()));
                let color = stat.tone.map_or(ui.visuals().text_color(), tone_color);
                match shown.is_empty() {
                    true => Fact {
                        label: label.color(color),
                        value: None,
                    },
                    // The engine's text carries any punctuation.
                    false => Fact {
                        label,
                        value: Some(egui::RichText::new(shown).color(color)),
                    },
                }
            })
            .collect::<Vec<_>>();
        section::facts(ui, egui::Id::new(("facts", id)), &facts);
    }
}
