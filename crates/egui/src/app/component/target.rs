//! The aim screen's panel: the ways of aiming the engine publishes as stats become buttons beside their chances,
//! and the creatures it offers the cursor become rows that move it.

use super::*;
use bn_client_core::protocol::MapComponent;

/// One way of aiming: its stat, the affordance that fires with it, and the stats of its chances.
pub(super) struct Level<'a> {
    stat: &'a PanelStat,
    action: &'a Affordance,
    ratings: Vec<&'a PanelStat>,
}

/// The stats of a panel that are ways of aiming, and the others. A way of aiming is the stat `aim:<id>` of an
/// affordance `<id>` of the screen; the stats `aim:<id>:<rating>` after it are its chances.
pub(super) fn split_levels<'a>(
    screen: &'a Screen,
    stats: &'a [PanelStat],
) -> (Vec<Level<'a>>, Vec<&'a PanelStat>) {
    let parts = |stat: &'a PanelStat| stat.key.split(':').collect::<Vec<_>>();
    let mut levels = stats
        .iter()
        .filter_map(|stat| match parts(stat)[..] {
            ["aim", id] => find(screen, id).map(|action| Level {
                stat,
                action,
                ratings: vec![],
            }),
            _ => None,
        })
        .collect::<Vec<_>>();
    let mut others = vec![];
    for stat in stats {
        let level = match parts(stat)[..] {
            ["aim", id] => levels.iter().any(|l| l.action.id == id).then_some(None),
            ["aim", id, _] => levels.iter().position(|l| l.action.id == id).map(Some),
            _ => None,
        };
        match level {
            Some(Some(at)) => levels[at].ratings.push(stat),
            Some(None) => {}
            None => others.push(stat),
        }
    }
    (levels, others)
}

impl<M: GameModel> App<M> {
    /// The ways of aiming as buttons that fire with them, each over the chances of its ratings.
    pub(super) fn aim_levels(&self, ui: &mut egui::Ui, levels: &[Level], out: &mut Vec<Intent>) {
        egui::Grid::new("aim levels").num_columns(2).show(ui, |ui| {
            for level in levels {
                let label = markup::plain(&level.stat.label.shown());
                let button = ui.add_enabled(level.action.enabled, egui::Button::new(label));
                if button.clicked() {
                    out.push(Intent::Run(level.action.id.clone()));
                }
                ui.label(stat_text(level.stat));
                ui.end_row();
                let chances = level.ratings.iter().map(|rating| {
                    let label = markup::plain(&rating.label.shown());
                    format!("{label} {}", stat_text(rating))
                });
                ui.label("");
                ui.weak(chances.collect::<Vec<_>>().join("  "));
                ui.end_row();
            }
        });
    }

    /// The creatures the cursor can jump to, the one it is on selected; a click moves it there.
    pub(super) fn target_candidates(
        &self,
        ui: &mut egui::Ui,
        screen: &Screen,
        map: &MapComponent,
        out: &mut Vec<Intent>,
    ) {
        let Some(mover) = screen
            .affordances
            .iter()
            .find(|a| a.args.as_ref().is_some_and(|args| args.pos.is_some()))
        else {
            return;
        };
        let cursor = map.cursor();
        // The engine lists the one the cursor is on first; by identity the rows keep their places as it moves.
        let mut candidates = map.candidates();
        candidates.sort_by(|a, b| a.id.cmp(&b.id));
        for candidate in candidates.into_iter().filter(|c| c.creature) {
            let label = markup::plain(&candidate.label.shown());
            let here = cursor.as_ref() == Some(&candidate.position);
            if ui.selectable_label(here, label).clicked() && !here {
                out.push(Intent::Move {
                    affordance: mover.id.clone(),
                    at: candidate.position,
                });
            }
        }
    }
}
