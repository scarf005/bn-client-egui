//! The affordances drawn as buttons: a component's own and the screen's footer.

use super::*;

/// The button of the menu that holds the actions, or the tabs, a row has no room for.
pub(super) const MORE: &str = "⋯";
/// The footer's help button, in a window without a title bar.
const HELP: &str = "?";

/// The footer's buttons on one row: the back first, the confirm last with the help before it, and the other
/// actions between in the engine's order, as many as the row has room for and the rest in a menu. The navigation
/// actions and a counting list's digits are the keys, the wheel and the clicks the client already maps, and the
/// native window's layout toggles change nothing here, so they are not drawn; in a window with a `title_bar` the
/// cancel is its close button and the help is there too.
struct FooterButtons<'a> {
    back: Vec<&'a Affordance>,
    rest: Vec<&'a Affordance>,
    help: Option<&'a Affordance>,
    confirm: Vec<&'a Affordance>,
}

fn footer_buttons<'a>(affordances: &[&'a Affordance], title_bar: bool) -> FooterButtons<'a> {
    let role = |a: &&Affordance, role| a.role == Some(role);
    let shown = affordances.iter().copied().filter(|a| match a.role {
        Some(AffordanceRole::Cancel | AffordanceRole::Help) => !title_bar,
        _ => drawn_button(a),
    });
    let (help, actions): (Vec<_>, Vec<_>) = shown.partition(|a| role(a, AffordanceRole::Help));
    let (back, actions): (Vec<_>, Vec<_>) = actions
        .into_iter()
        .partition(|a| role(a, AffordanceRole::Cancel));
    let (confirm, rest) = actions
        .into_iter()
        .partition(|a| role(a, AffordanceRole::Confirm));
    FooterButtons {
        back,
        rest,
        help: help.into_iter().find(|a| a.enabled),
        confirm,
    }
}

impl FooterButtons<'_> {
    /// The width of the row with the first `n` of the other actions and the menu button when some are left.
    fn width(&self, ui: &egui::Ui, n: usize) -> f32 {
        let glyph = |text: &str| glyph_width(ui, text);
        let widths = self
            .back
            .iter()
            .chain(&self.rest[..n])
            .chain(&self.confirm)
            .map(|a| button_width(ui, a))
            .chain((n < self.rest.len()).then(|| glyph(MORE)))
            .chain(self.help.map(|_| glyph(HELP)))
            .collect::<Vec<_>>();
        let gaps = widths.len().saturating_sub(1) as f32 * ui.spacing().item_spacing.x;
        widths.iter().sum::<f32>() + gaps
    }

    /// The most of the other actions shown in the row that `fits`, none when not even the fixed buttons fit.
    fn shown(&self, fits: impl Fn(usize) -> bool) -> usize {
        (0..=self.rest.len()).rev().find(|n| fits(*n)).unwrap_or(0)
    }
}

/// The width of a button holding only `text`.
fn glyph_width(ui: &egui::Ui, text: &str) -> f32 {
    let font = egui::TextStyle::Button.resolve(ui.style());
    text_width(ui, text, &font) + 2.0 * ui.spacing().button_padding.x
}

/// The width of `a`'s button as `keyed` lays it out.
pub(super) fn button_width(ui: &egui::Ui, a: &Affordance) -> f32 {
    let caption = egui::TextStyle::Small.resolve(ui.style());
    let key = a.keys.first().map_or(0.0, |k| {
        ui.spacing().icon_spacing + text_width(ui, k, &caption)
    });
    glyph_width(ui, &markup::plain(&a.label.shown())) + key
}

/// An affordance's button text: the engine's label, then the first key the player has bound to it as a dim
/// caption.
fn keyed(a: &Affordance) -> egui::Atoms<'static> {
    let label = markup::plain(&a.label.shown());
    match a.keys.first() {
        Some(key) => egui::Atoms::new((label, egui::RichText::new(key).small().weak())),
        None => egui::Atoms::new(label),
    }
}

/// Every key the player has bound to `a`, for its button's hover text.
fn key_list(a: &Affordance) -> String {
    a.keys.join(", ")
}

/// The hover text of a button drawn as a glyph: the engine's label over every key bound to `a`.
pub(super) fn glyph_tip(a: &Affordance) -> String {
    let label = markup::plain(&a.label.shown());
    match a.keys.is_empty() {
        true => label,
        false => format!("{label}\n{}", key_list(a)),
    }
}

/// Names `a`'s button `button` by the engine's label alone, with its first key as the shortcut, and lists every
/// key on hover.
fn described(ctx: &egui::Context, button: egui::Response, a: &Affordance) -> egui::Response {
    a11y::keyed_button(
        ctx,
        button.id,
        &markup::plain(&a.label.shown()),
        a.keys.first().map(String::as_str),
    );
    match a.keys.is_empty() {
        true => button,
        false => button.on_hover_text(key_list(a)),
    }
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
        let button = self.control(ui, keyed(a), &a.id, enabled, false);
        let button = described(ui.ctx(), button, a);
        let button = match (enabled, denial) {
            (false, Some(why)) => button.on_disabled_hover_text(why),
            _ => button,
        };
        if button.clicked() {
            out.push(Intent::Run(a.id.clone()));
        }
    }

    /// The screen's own actions under its content, as `footer_buttons` picks them, on one row as wide as the
    /// room left to it.
    pub(super) fn footer(
        &self,
        ui: &mut egui::Ui,
        affordances: &[&Affordance],
        title_bar: bool,
        out: &mut Vec<Intent>,
    ) {
        let row = footer_buttons(affordances, title_bar);
        if row.back.is_empty()
            && row.rest.is_empty()
            && row.confirm.is_empty()
            && row.help.is_none()
        {
            return;
        }
        let room = ui.available_width();
        let n = row.shown(|n| row.width(ui, n) <= room + 0.5);
        let (direct, more) = row.rest.split_at(n);
        let mut confirmed = vec![];
        egui::Sides::new().show(
            ui,
            |ui| {
                for a in row.back.iter().chain(direct) {
                    self.affordance_button(ui, a, None, out);
                }
                if !more.is_empty() {
                    ui.menu_button(MORE, |ui| {
                        for a in more {
                            let enabled = interaction::row_availability(a, None).enabled;
                            let button = ui.add_enabled(enabled, egui::Button::new(keyed(a)));
                            if described(ui.ctx(), button, a).clicked() {
                                out.push(Intent::Run(a.id.clone()));
                                ui.close();
                            }
                        }
                    });
                }
            },
            // Right to left: the confirm is the last button of the row.
            |ui| {
                for a in row.confirm.iter().rev() {
                    self.affordance_button(ui, a, None, &mut confirmed);
                }
                if let Some(a) = row.help {
                    let label = markup::plain(&a.label.shown());
                    let button = ui.button(HELP).on_hover_text(glyph_tip(a));
                    a11y::named_button(ui.ctx(), button.id, &label);
                    if button.clicked() {
                        confirmed.push(Intent::Run(a.id.clone()));
                    }
                }
            },
        );
        out.extend(confirmed);
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
        self.affordance_buttons(ui, &own_buttons(screen, component), row, out);
    }
}

/// The buttons a component draws for the affordances it names that its own widgets do not run.
pub(super) fn own_buttons<'a>(screen: &'a Screen, component: &Component) -> Vec<&'a Affordance> {
    component
        .head()
        .affordances
        .iter()
        .filter_map(|id| find(screen, id))
        // A list's rows run its `activate`, also where a panel around the list names it.
        .filter(|a| !driven(component, a) && !run_by_rows(screen, a) && drawn_button(a))
        .collect()
}

/// Whether `a` is drawn as a button: picking, moving, digits and text edits run from their rows and keys instead, and what
/// only rearranges the native window changes nothing here.
pub(super) fn drawn_button(a: &Affordance) -> bool {
    !matches!(
        a.role,
        Some(
            AffordanceRole::Select
                | AffordanceRole::Navigate
                | AffordanceRole::Digit
                | AffordanceRole::Layout
                | AffordanceRole::Edit
        )
    )
}
