//! The affordances drawn as buttons: a component's own and the screen's footer.

use super::*;

/// The most buttons a screen's footer shows; the rest of its actions go in a menu.
const FOOTER_BUTTONS: usize = 6;

/// ` [key]` after an affordance's label, the first key the player has bound to it.
pub(super) fn key_hint(a: &Affordance) -> String {
    a.keys.first().map_or(String::new(), |k| format!(" [{k}]"))
}

impl<M: GameModel> App<M> {
    /// Buttons for the affordances: enabled as the affordance (and the row, when it overrides) says, with the
    /// engine's reason on hover when off. Selecting and navigating are the clicks and keys the client maps.
    pub(super) fn affordance_buttons(
        &self,
        ui: &mut egui::Ui,
        affordances: &[&Affordance],
        row: Option<&Row>,
        out: &mut Vec<Intent>,
    ) {
        if affordances.is_empty() {
            return;
        }
        ui.horizontal_wrapped(|ui| {
            for a in affordances.iter().filter(|a| drawn_button(a)) {
                self.affordance_button(ui, a, row, out);
            }
        });
    }

    /// One affordance as a button labelled with the engine's name and the key bound to it.
    fn affordance_button(
        &self,
        ui: &mut egui::Ui,
        a: &Affordance,
        row: Option<&Row>,
        out: &mut Vec<Intent>,
    ) {
        let availability = interaction::row_availability(a, row);
        let (enabled, denial) = (availability.enabled, availability.denial.map(|d| d.shown()));
        let label = format!("{}{}", markup::plain(&a.label.shown()), key_hint(a));
        let button = self.control(ui, label, &a.id, enabled, false);
        let button = match (enabled, denial) {
            (false, Some(why)) => button.on_disabled_hover_text(why),
            _ => button,
        };
        if button.clicked() {
            out.push(Intent::Run(a.id.clone()));
        }
    }

    /// The screen's own actions under its content: the confirm and the cancel first, at most `FOOTER_BUTTONS`
    /// buttons with the rest in a menu, and the help last. The navigation actions and a counting list's digits are
    /// the keys, the wheel and the clicks the client already maps, so they are not drawn; in a window with a
    /// `title_bar` the cancel is its close button and the help is there too.
    pub(super) fn footer(
        &self,
        ui: &mut egui::Ui,
        affordances: &[&Affordance],
        title_bar: bool,
        out: &mut Vec<Intent>,
    ) {
        let role = |a: &&Affordance, role| a.role == Some(role);
        let shown = affordances.iter().copied().filter(|a| match a.role {
            Some(AffordanceRole::Select | AffordanceRole::Navigate | AffordanceRole::Digit) => {
                false
            }
            Some(AffordanceRole::Cancel | AffordanceRole::Help) => !title_bar,
            _ => true,
        });
        let (help, actions): (Vec<_>, Vec<_>) = shown.partition(|a| role(a, AffordanceRole::Help));
        let help = help.into_iter().find(|a| a.enabled);
        let (first, rest): (Vec<_>, Vec<_>) = actions
            .into_iter()
            .partition(|a| role(a, AffordanceRole::Confirm) || role(a, AffordanceRole::Cancel));
        let direct = [first, rest].concat();
        let slots = FOOTER_BUTTONS - usize::from(help.is_some());
        let fits = direct.len() <= slots;
        let (direct, more) = direct.split_at(if fits { direct.len() } else { slots - 1 });
        if direct.is_empty() && help.is_none() {
            return;
        }
        ui.horizontal_wrapped(|ui| {
            for a in direct {
                self.affordance_button(ui, a, None, out);
            }
            if !more.is_empty() {
                ui.menu_button("⋯", |ui| {
                    for a in more {
                        let enabled = interaction::row_availability(a, None).enabled;
                        let label = format!("{}{}", markup::plain(&a.label.shown()), key_hint(a));
                        if ui.add_enabled(enabled, egui::Button::new(label)).clicked() {
                            out.push(Intent::Run(a.id.clone()));
                            ui.close();
                        }
                    }
                });
            }
            if let Some(a) = help {
                let label = markup::plain(&a.label.shown());
                let button = ui
                    .button("?")
                    .on_hover_text(format!("{label}{}", key_hint(a)));
                a11y::named_button(ui.ctx(), button.id, &label);
                if button.clicked() {
                    out.push(Intent::Run(a.id.clone()));
                }
            }
        });
    }

    /// The affordances a component names that its own widgets do not run.
    pub(super) fn component_buttons(
        &self,
        ui: &mut egui::Ui,
        screen: &Screen,
        component: &Component,
        row: Option<&Row>,
        out: &mut Vec<Intent>,
    ) {
        let own = component
            .head()
            .affordances
            .iter()
            .filter_map(|id| find(screen, id))
            // A list's rows run its `activate`, also where a panel around the list names it.
            .filter(|a| !driven(component, a) && !run_by_rows(screen, a))
            .collect::<Vec<_>>();
        self.affordance_buttons(ui, &own, row, out);
    }
}

/// Whether `a` is drawn as a button: picking, moving and digits run from their rows and keys instead.
pub(super) fn drawn_button(a: &Affordance) -> bool {
    !matches!(
        a.role,
        Some(AffordanceRole::Select | AffordanceRole::Navigate | AffordanceRole::Digit)
    )
}
