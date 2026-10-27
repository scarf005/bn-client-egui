//! A `workspace` screen laid out as an editor: a menu bar from the affordances' groups, the tools on the left, the
//! canvas in the middle with the log under it, the palette, outline and inspector on the right and the status line
//! at the bottom, each part by the slot the engine gave it.

use super::*;
use bn_client_core::localise::tr;

/// The width the tools' column and the right column first have, which the player can change.
const TOOLS_WIDTH: f32 = 180.0;
const SIDE_WIDTH: f32 = 300.0;
/// The most the log's strip is tall before it scrolls.
const LOG_HEIGHT: f32 = 160.0;

/// The parts of `root` in `slot`; a part with none is the inspector.
fn parts(root: &Split, slot: Slot) -> Vec<&Component> {
    let slotted = |c: &&Component| c.head().slot.unwrap_or(Slot::Inspector) == slot;
    root.children.iter().filter(slotted).collect()
}

/// The split of a workspace screen that has a canvas part: the layout needs one, and any other is laid out as a
/// screen.
pub(super) fn laid_out(screen: &Screen) -> Option<&Split> {
    match (screen.role, &screen.root) {
        (ScreenRole::Workspace, Some(Component::Split(root)))
            if !parts(root, Slot::Canvas).is_empty() =>
        {
            Some(root)
        }
        _ => None,
    }
}

impl<M: GameModel> App<M> {
    /// One menu per group of the screen's affordances, in the order the groups first appear.
    pub(super) fn menu_bar(&self, ui: &mut egui::Ui, screen: &Screen, out: &mut Vec<Intent>) {
        let groups = screen
            .affordances
            .iter()
            .filter_map(|a| a.group.as_ref().map(Text::shown))
            .fold(vec![], |mut groups, group| {
                if !groups.contains(&group) {
                    groups.push(group);
                }
                groups
            });
        if groups.is_empty() {
            return;
        }
        egui::Panel::top(egui::Id::new(("menu bar", &screen.screen_id))).show(ui, |ui| {
            egui::MenuBar::new().ui(ui, |ui| {
                for group in &groups {
                    ui.menu_button(markup::plain(group), |ui| {
                        let items = screen
                            .affordances
                            .iter()
                            .filter(|a| a.group.as_ref().map(Text::shown).as_ref() == Some(group));
                        for a in items {
                            let availability = interaction::row_availability(a, None);
                            let mut item = egui::Button::new(markup::plain(&a.label.shown()));
                            if let Some(key) = a.keys.first() {
                                item = item.shortcut_text(key);
                            }
                            let button = ui.add_enabled(availability.enabled, item);
                            let button = described(ui.ctx(), button, a);
                            let button = match (availability.enabled, availability.denial) {
                                (false, Some(why)) => button.on_disabled_hover_text(why.shown()),
                                _ => button,
                            };
                            if button.clicked() {
                                out.push(Intent::Run(a.id.clone()));
                                ui.close();
                            }
                        }
                    });
                }
            });
        });
    }

    /// The parts of `root` by slot, around the canvas that takes the room left.
    pub(super) fn workspace(
        &self,
        ui: &mut egui::Ui,
        screen: &Screen,
        root: &Split,
        out: &mut Vec<Intent>,
    ) {
        let id = |name: &str| egui::Id::new((name, &screen.screen_id));
        // The columns' ids are strings, which `label_resize_handle` names the drag handles by.
        let (tools_id, side_id) = (
            format!("workspace-tools {}", screen.screen_id),
            format!("workspace-side {}", screen.screen_id),
        );
        egui::Panel::bottom(id("status"))
            .resizable(false)
            .show(ui, |ui| {
                parts(root, Slot::Status)
                    .into_iter()
                    .for_each(|c| self.status_line(ui, screen, c, out));
            });
        let tools = parts(root, Slot::Tools);
        if !tools.is_empty() {
            egui::Panel::left(egui::Id::new(&tools_id))
                .default_size(TOOLS_WIDTH)
                .show(ui, |ui| self.column(ui, screen, &tools, out));
        }
        let side = [Slot::Palette, Slot::Outline, Slot::Inspector]
            .into_iter()
            .flat_map(|slot| parts(root, slot))
            .collect::<Vec<_>>();
        if !side.is_empty() {
            egui::Panel::right(egui::Id::new(&side_id))
                .default_size(SIDE_WIDTH)
                .show(ui, |ui| self.column(ui, screen, &side, out));
        }
        for name in [&tools_id, &side_id] {
            crate::app::label_resize_handle(ui.ctx(), name, &tr("Resize workspace column"));
        }
        let log = parts(root, Slot::Log);
        if !log.is_empty() {
            egui::Panel::bottom(id("log"))
                .resizable(false)
                .show(ui, |ui| {
                    egui::ScrollArea::vertical()
                        .id_salt(id("log scroll"))
                        .max_height(LOG_HEIGHT)
                        .auto_shrink([false, true])
                        .show(ui, |ui| self.stack(ui, screen, &log, out));
                });
        }
        egui::CentralPanel::no_frame().show(ui, |ui| {
            parts(root, Slot::Canvas)
                .into_iter()
                .for_each(|c| self.component(ui, screen, c, out));
        });
    }

    /// A status part on one line: a panel's title and its facts side by side; any other part as it is drawn
    /// elsewhere.
    fn status_line(
        &self,
        ui: &mut egui::Ui,
        screen: &Screen,
        c: &Component,
        out: &mut Vec<Intent>,
    ) {
        let Component::Panel(panel) = c else {
            return self.component(ui, screen, c, out);
        };
        ui.horizontal_wrapped(|ui| {
            ui.label(egui::RichText::new(markup::plain(&panel.title.shown())).strong());
            for stat in panel.stats.iter().flatten() {
                ui.separator();
                ui.weak(markup::plain(&stat.label.shown()));
                let shown = stat_text(stat);
                if !shown.is_empty() {
                    let color = stat.tone.map_or(ui.visuals().text_color(), tone_color);
                    ui.label(egui::RichText::new(shown).color(color));
                }
            }
        });
        if let Some(child) = &panel.child {
            self.component(ui, screen, child, out);
        }
    }

    /// `parts` one above the other in a column that scrolls, whole lines in view.
    fn column(
        &self,
        ui: &mut egui::Ui,
        screen: &Screen,
        parts: &[&Component],
        out: &mut Vec<Intent>,
    ) {
        let scroll = egui::ScrollArea::vertical()
            .id_salt(ui.id().with("column"))
            .auto_shrink([false, false]);
        let room = ui.available_height();
        body_scroll(ui, scroll, room, |ui| self.stack(ui, screen, parts, out));
    }

    /// `parts` one above the other, each under its label as a heading that collapses it.
    fn stack(
        &self,
        ui: &mut egui::Ui,
        screen: &Screen,
        parts: &[&Component],
        out: &mut Vec<Intent>,
    ) {
        for c in parts {
            match &c.head().label {
                Some(label) => {
                    egui::CollapsingHeader::new(markup::plain(&label.shown()))
                        .id_salt(c.id())
                        .default_open(true)
                        .show(ui, |ui| self.component_body(ui, screen, c, out));
                }
                None => self.component(ui, screen, c, out),
            }
        }
    }
}
