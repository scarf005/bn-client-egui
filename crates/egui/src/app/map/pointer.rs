//! The pointer and the keyboard on the map: the cursor a moving pointer takes, clicks and the keyboard focus, the
//! tooltip of the square under the pointer and its right-click menu.

use super::*;

/// The widest a square's tooltip text gets before its lines wrap; the tooltip's frame adds its margins.
const TOOLTIP_WIDTH: f32 = 420.0;

impl<M: GameModel> App<M> {
    /// The pointer's hover, clicks and right click on the map, and the map's keyboard focus.
    pub(super) fn map_pointer(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
        canvas: &Canvas,
        scene: Option<&Scene>,
        mut cursor: Pos,
    ) -> MapInput {
        let origin = self.model.origin();
        let to_abs = |view: Pos| Pos {
            x: view.x + origin.x,
            y: view.y + origin.y,
        };
        let cell_at = |p: egui::Pos2| canvas.square_at(p, scene);
        // Only a moving pointer moves the cursor, so it cannot undo keyboard navigation, nor take the cursor
        // to where it rests when a window over the map closes.
        let hover = response.hover_pos();
        let pointer = ui.input(|i| i.pointer.latest_pos());
        let moved = pointer != self.last_pointer;
        self.last_pointer = pointer;
        if let Some(p) = hover.filter(|_| moved) {
            cursor = cell_at(p);
            // The engine's own cursor follows the mouse while it is looking or aiming; otherwise the
            // native mouse view describes the square.
            match self.model.focus() {
                Some(_) => {
                    self.aim = Some(Aim {
                        at: to_abs(cursor),
                        sent: false,
                    })
                }
                None => self.model.hover(cursor),
            }
        }
        self.last_hover = hover;
        // One move is in flight at a time: the pointer's latest square goes once the engine is free, and the
        // square stands until the engine has answered it, wherever the engine then put its cursor.
        if let Some(aim) = self.aim.as_mut().filter(|_| !self.model.busy()) {
            match aim.sent {
                true => self.aim = None,
                false => {
                    aim.sent = true;
                    let at = Pos {
                        x: aim.at.x - origin.x,
                        y: aim.at.y - origin.y,
                    };
                    self.model.aim(at, false);
                }
            }
        }
        let mut menu_key = false;
        // A click goes to the square it landed on, not to where the pointer was last seen hovering.
        let clicked = response.interact_pointer_pos().map_or(cursor, cell_at);
        if response.clicked() {
            // A click while the engine owns the cursor moves or confirms it; otherwise it travels.
            if !self.model.aim(clicked, true) {
                self.go_to(clicked);
            }
            ui.ctx().request_repaint();
        }
        if response.secondary_clicked() {
            self.menu_cell = to_abs(clicked);
        }
        // With nothing focused, an arrow key would move egui's focus onto a toolbar widget and a later
        // Space would press it, so the map holds the focus.
        if (self.refocus_map || ui.memory(|m| m.focused()).is_none()) && self.model.menu().is_none()
        {
            self.refocus_map = false;
            response.request_focus();
        }
        if response.has_focus() {
            // Keep arrow keys from moving egui's focus away from the map.
            ui.memory_mut(|m| {
                m.set_focus_lock_filter(
                    response.id,
                    egui::EventFilter {
                        // Tab is the engine's here (auto-fight); it must not walk the focus over the sidebar.
                        tab: true,
                        horizontal_arrows: true,
                        vertical_arrows: true,
                        ..Default::default()
                    },
                )
            });
            // A keyboard cursor gets the same native description a hovering pointer does. The arrow keys
            // only walk: outside look and aim mode the cursor follows the mouse, else it is the player's square.
            if self.model.focus().is_none() && !response.context_menu_opened() {
                self.model.hover(cursor);
            }
            // Shift+F10, the keyboard's context-menu key, is the right click on the cursor square.
            if ui.input(|i| i.modifiers.shift && i.key_pressed(egui::Key::F10)) {
                self.menu_cell = to_abs(cursor);
                menu_key = true;
            }
        }
        MapInput { cursor, menu_key }
    }

    /// The engine's description of the square under the pointer; keyboard moves of the cursor leave it alone.
    pub(super) fn map_tooltip(
        &mut self,
        response: &egui::Response,
        canvas: &Canvas,
        scene: Option<&Scene>,
        cursor: Pos,
    ) {
        // A dialog on top owns the pointer; the tooltip would hang over it.
        let tip = self.model.menu().is_none()
            && self.palette.is_none()
            && !response.context_menu_opened();
        let under = response
            .hover_pos()
            .map_or(cursor, |p| canvas.square_at(p, scene));
        // A square nothing is known about says nothing.
        let cell = self.model.cell(under);
        let known = !(cell.layers.is_empty() && cell.desc.is_empty());
        let season = self.model.season();
        response.clone().on_hover_ui_at_pointer(|ui| {
            if !tip || !known {
                return;
            }
            // The engine's own mouse-view facts when it has answered for this square. A creature's name heads
            // them beside its sprite, its health bar drawn as a bar.
            let parts = self.model.hover_parts(under).unwrap_or_default();
            let creature = parts.iter().find(|p| p.kind == "creature");
            let hp = parts
                .iter()
                .find(|p| p.kind == "hp")
                .filter(|_| creature.is_some());
            // Under a heading, the creature's name and bar are drawn as such; what more it says stays a line.
            let lines = match parts.is_empty() {
                true => vec![cell.desc.clone()],
                false => parts
                    .iter()
                    .flat_map(|p| match (creature.is_some(), p.kind.as_str()) {
                        (true, "hp") => vec![],
                        (true, "creature") => p.detail.iter().collect(),
                        _ => std::iter::once(&p.text).chain(&p.detail).collect(),
                    })
                    .filter(|text| !markup::plain(text).trim().is_empty())
                    .cloned()
                    .collect(),
            };
            // A tooltip at the pointer gets almost no width to wrap in: its text takes its widest line's width, up
            // to `TOOLTIP_WIDTH`, and wraps the longer lines there.
            let font = egui::TextStyle::Body.resolve(ui.style());
            let name = creature.map(|c| markup::plain(&c.text));
            let widest = lines
                .iter()
                .map(|l| markup::plain(l))
                .chain(name.clone())
                .map(|l| crate::app::component::text_width(ui, &l, &font))
                .fold(0.0, f32::max);
            ui.set_width(widest.ceil().min(TOOLTIP_WIDTH));
            if let Some(name) = name {
                ui.horizontal(|ui| {
                    if self.display.tiles() {
                        let (rect, _) = ui.allocate_exact_size(
                            egui::vec2(CELL, CELL) * 2.0,
                            egui::Sense::hover(),
                        );
                        let body = Cell {
                            layers: cell
                                .layers
                                .iter()
                                .skip_while(|l| !l.creature)
                                .cloned()
                                .collect(),
                            ..Cell::default()
                        };
                        self.display
                            .paint_icon(ui.painter(), rect, &body, season.as_deref());
                    }
                    ui.label(egui::RichText::new(name).strong());
                });
            }
            if let Some(hp) = hp {
                let value = |key: &str| hp.values.get(key).map(|&v| v as f32);
                let done = match (value("hp"), value("max"), value("percent")) {
                    (Some(hp), Some(max), _) if max > 0.0 => hp / max,
                    (_, _, Some(percent)) => percent / 100.0,
                    _ => 0.0,
                };
                let color = markup::runs(&hp.text, self.model.colors())
                    .into_iter()
                    .find_map(|r| r.color)
                    .unwrap_or(Rgb(255, 255, 255));
                let bar = egui::ProgressBar::new(done.clamp(0.0, 1.0))
                    .desired_height(6.0)
                    .fill(rgb(color))
                    .corner_radius(0);
                ui.add(bar);
            }
            for line in lines {
                let line = self.rich(ui, &line);
                ui.add(egui::Label::new(line).wrap_mode(egui::TextWrapMode::Wrap));
            }
        });
    }

    /// The right click lists what the engine says the avatar can do with the square; the choice is sent as
    /// that action. A keyboard cursor opens the same menu on its square (`menu_key`). The menu sits at its
    /// square, which a keyboard has no pointer for.
    pub(super) fn map_menu(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
        canvas: &Canvas,
        menu_key: bool,
    ) {
        // The menu's square is absolute: a view that scrolls under the open menu does not change it.
        let origin = self.model.origin();
        let at = Pos {
            x: self.menu_cell.x - origin.x,
            y: self.menu_cell.y - origin.y,
        };
        let mut popup = egui::Popup::context_menu(response).anchor(egui::Rect::from_min_size(
            match canvas.view {
                // The 3D scene has no square grid to place the menu on; it opens at the map's centre.
                Some(_) => response.rect.center(),
                None if canvas.diamond.is_some() => canvas.rect(at).left_top(),
                None => canvas.flat_rect(at).min,
            },
            egui::Vec2::splat(canvas.cell_px),
        ));
        if menu_key {
            popup = popup.open_memory(egui::SetOpenCommand::Bool(true));
        }
        let menu = popup.show(|ui| {
            let Some(actions) = self.model.tile_actions(at) else {
                // The engine has not answered for this square yet.
                ui.add_enabled(false, egui::Label::new("…"));
                ui.ctx().request_repaint();
                return;
            };
            if actions.is_empty() {
                ui.close();
            }
            for action in actions {
                let key = action
                    .keys
                    .first()
                    .map_or(String::new(), |k| format!(" [{k}]"));
                let label = format!(
                    "{}{key}",
                    bn_client_core::markup::plain(&action.label.shown())
                );
                if menu_item(ui, &label) {
                    self.model.tile_action(at, &action.id);
                    ui.ctx().request_repaint();
                    ui.close();
                }
            }
        });
        if let Some(menu) = menu {
            ui.ctx().accesskit_node_builder(menu.response.id, |b| {
                b.set_role(accesskit::Role::Menu);
                b.set_label(tr("Map actions"));
            });
        }
    }
}
