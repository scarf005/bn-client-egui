//! The client's own windows of the character sheet and the inventory: the engine's screens read while the game
//! goes on, so the player keeps moving with them open. Their tabs and rows only move the window's own focus,
//! which the engine shows as the screen's cursor; the engine's screen, where stats and items are acted on, opens
//! from them.

use super::*;
use bn_client_core::PanelWatch;
use bn_client_core::interaction::Interaction;
use bn_client_core::protocol::PanelKind;

/// Each panel window and the engine action whose screen it shows.
pub(in crate::app) const PANELS: [(PanelKind, &str); 2] = [
    (PanelKind::Character, "player_data"),
    (PanelKind::Inventory, "inventory"),
];

impl<M: GameModel> App<M> {
    /// Opens the panel window of `action`'s screen, or closes it when it is open; false for an action that has
    /// none.
    pub(in crate::app) fn toggle_panel(&mut self, action: &str) -> bool {
        let Some((panel, _)) = PANELS.iter().find(|(_, id)| *id == action) else {
            return false;
        };
        if self.panels.remove(panel).is_none() {
            self.panels.insert(*panel, Interaction::default());
        }
        true
    }

    pub(in crate::app) fn panel_windows(&mut self, ctx: &egui::Context) {
        for (panel, action) in PANELS {
            self.panel_window(ctx, panel, action);
        }
    }

    /// One panel window: its screen drawn as the top screen is, with the window's focus standing in for the top
    /// screen's. The character sheet first opens at the map's top left, the inventory at its top right, and
    /// neither leaves the map's room.
    fn panel_window(&mut self, ctx: &egui::Context, panel: PanelKind, action: &str) {
        let Some(mut focus) = self.panels.remove(&panel) else {
            return self.model.watch_panel(panel, None);
        };
        let row = focus.focus().row_id.clone();
        self.model.watch_panel(panel, Some(PanelWatch { row }));
        let Some(screen) = self.model.watched_panel(panel) else {
            self.panels.insert(panel, focus);
            return;
        };
        focus.sync(std::slice::from_ref(screen));
        std::mem::swap(&mut self.screens.interaction, &mut focus);
        self.screens.passive = true;
        let kind = format!("panel:{action}");
        let title = markup::plain(&screen.title.as_ref().map(Text::shown).unwrap_or_default());
        let region = self.region.unwrap_or_else(|| ctx.content_rect());
        let place = side(region, panel == PanelKind::Character, 1.0);
        let placement = Placement {
            class: Class::Side,
            place,
            room: place.room().x,
            table: false,
            panes: false,
        };
        // As wide as its content, within the share of the room a window beside the map takes.
        let width = screen.root.as_ref().and_then(|root| {
            let builder = egui::UiBuilder::new().max_rect(region);
            let ui = egui::Ui::new(ctx.clone(), egui::Id::new("measure"), builder);
            let margin = ui.spacing().window_margin.sum().x;
            let content = self.extent(&ui, root, placement.widest() - margin);
            let content = Extent {
                min: content.min + margin,
                natural: content.natural + margin,
                ..content
            };
            placement.width(content, 0.0)
        });
        let spec = WindowSpec {
            closable: true,
            place: Some(place),
            movable: true,
            width,
            ..self.window_spec(ctx, Some(&kind), &title)
        };
        let offered = self.model.actions().into_iter().find(|a| a.id == action);
        let mut out = vec![];
        let mut full = false;
        let shown = windows::show(ctx, spec, |ui| {
            if let Some(a) = &offered {
                full = ui.button(markup::plain(&a.name)).clicked();
            }
            // A sheet taller than the room scrolls rather than leaving it.
            if let Some(root) = &screen.root {
                egui::ScrollArea::vertical()
                    .id_salt(("body", action))
                    .auto_shrink([false, true])
                    .show(ui, |ui| self.component(ui, screen, root, &mut out));
            }
        });
        self.screens.passive = false;
        std::mem::swap(&mut self.screens.interaction, &mut focus);
        self.paint_icons();
        let closed = self.keep_window(ctx, Some(&kind), &shown);
        for intent in out {
            match intent {
                Intent::Focus { component, row } => focus.focus_row(&component, &row),
                Intent::Open => full |= offered.is_some(),
                _ => {}
            }
        }
        if !closed {
            self.panels.insert(panel, focus);
        }
        if full {
            self.model.invoke_action(action);
        }
    }
}
