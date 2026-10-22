//! The keyboard of a frame: the keys the client keeps for its own view and focus, the keys that go to the engine,
//! and the held walk.

use super::component::PANELS;
use super::{App, menu_forwards_keys, tab_is_engines};
use crate::iso;
use bn_client_core::{GameModel, Menu, interaction, keys};
use eframe::egui;

/// A key pressed this frame, named like the engine's keys (`SPACE`, `ESC`, `RETURN`, `CTRL+S`); a typed
/// character is its text.
pub(super) struct FrameKey {
    pub token: String,
    /// The key pressed and its modifiers; none for typed text.
    pub key: Option<(egui::Key, egui::Modifiers)>,
}

/// The keys pressed this frame, in the order they came.
pub(super) fn frame_keys(ctx: &egui::Context) -> Vec<FrameKey> {
    ctx.input(|i| {
        i.events
            .iter()
            .filter_map(|e| match e {
                egui::Event::Text(t) => Some(FrameKey {
                    token: if t == " " { "SPACE".into() } else { t.clone() },
                    key: None,
                }),
                egui::Event::Key {
                    key,
                    pressed: true,
                    modifiers,
                    ..
                } => key_name(*key, *modifiers).map(|token| FrameKey {
                    token,
                    key: Some((*key, *modifiers)),
                }),
                _ => None,
            })
            .collect()
    })
}

impl<M: GameModel> App<M> {
    /// Hands the frame's keys to whoever has the keyboard.
    pub(super) fn keys(&mut self, ctx: &egui::Context) {
        // Esc interrupts a running activity, and then does nothing else; with none running it goes on like any key
        // (leaves look mode, acknowledges a popup, closes a screen, else opens the main menu).
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) && self.model.interrupt() {
            ctx.input_mut(|i| {
                i.events.retain(|e| {
                    !matches!(
                        e,
                        egui::Event::Key {
                            key: egui::Key::Escape,
                            ..
                        }
                    )
                })
            });
        }
        self.palette_keys(ctx);
        // The palette and an open menu of the client (the map's right-click menu) have the keyboard; Escape closes
        // them.
        let client = self.palette.is_some() || egui::Popup::is_any_open(ctx);
        match interaction::described(self.model.screens()) {
            _ if client => {}
            Some(_) => self.component_keys(ctx),
            None => self.game_keys(ctx),
        }
        // Whichever screen took the keys, the walk ends with its key.
        self.release_hold_when_up(ctx);
        let open = self.model.menu();
        if menu_forwards_keys(open.as_ref()) {
            // Arrow keys belong to the game, whichever widget was clicked last; Tab still moves focus,
            // except on a tabbed screen where it is the engine's tab key.
            let tab = open.as_ref().is_some_and(tab_is_engines);
            if let Some(id) = ctx.memory(|m| m.focused()) {
                ctx.memory_mut(|m| {
                    m.set_focus_lock_filter(
                        id,
                        egui::EventFilter {
                            tab,
                            horizontal_arrows: true,
                            vertical_arrows: true,
                            ..Default::default()
                        },
                    )
                });
            }
        }
    }

    /// Typed characters and special keys are game keys, after the client took its own.
    fn game_keys(&mut self, ctx: &egui::Context) {
        let menu = self.model.menu();
        let keys = self.client_keys(frame_keys(ctx), menu.as_ref());
        let keys = self.engine_keys(ctx, keys, menu.as_ref());
        self.keys_consumed = false;
        self.focus_action_row(ctx);
        self.dispatch(ctx, keys);
    }

    /// Runs the keys that work the client's own view, and returns the rest. Zoom is the client's: the keys the
    /// engine binds to its zoom actions never reach it. In the 3D map Alt+Left and Alt+Right turn the camera,
    /// unless the engine binds those keys, and steps turn with the camera. With a menu open, every key is the menu's.
    fn client_keys(&mut self, keys: Vec<FrameKey>, menu: Option<&Menu>) -> Vec<FrameKey> {
        if menu.is_some() {
            return keys;
        }
        let actions = self.model.actions();
        let zoom = |k: &FrameKey| {
            let bound = actions.iter().find(|a| a.keys.contains(&k.token));
            bound
                .filter(|_| k.key.is_none())
                .and_then(|a| match a.id.as_str() {
                    "zoom_in" => Some(1),
                    "zoom_out" => Some(-1),
                    _ => None,
                })
        };
        let turning = self.iso.active(&self.display);
        let turn = |k: &FrameKey| {
            let (key, mods) = k.key?;
            let (name, way) = match key {
                egui::Key::ArrowLeft => ("LEFT", 1.0),
                egui::Key::ArrowRight => ("RIGHT", -1.0),
                _ => return None,
            };
            (turning && mods.alt && !self.model.key_bound(&format!("ALT+{name}"))).then_some(way)
        };
        // The engine's map, character sheet and inventory keys open the client's own windows, which leave the game
        // taking keys.
        let window = |k: &FrameKey| {
            let bound = |id: &&str| {
                k.key.is_none()
                    && actions
                        .iter()
                        .any(|a| a.id == *id && a.keys.contains(&k.token))
            };
            ["map"]
                .into_iter()
                .chain(PANELS.map(|(_, id)| id))
                .find(bound)
        };
        let opened = keys.iter().filter_map(window).collect::<Vec<_>>();
        let zooms = keys.iter().filter_map(zoom).sum();
        let turns = keys.iter().filter_map(turn).sum::<f32>();
        // In the 3D map a step goes the way its key points on the screen, and in first person W, A, S and D step
        // ahead, left, back and right of the look; each is sent as a key of the step it becomes.
        let first = self.display.first_person();
        let camera = self.iso.camera;
        let bound = |token: &str| {
            let action = |token: &str| actions.iter().find(|a| a.keys.iter().any(|k| k == token));
            action(token).or_else(|| action(&keys::numpad(token)?))
        };
        let screen_step = |k: &FrameKey| {
            let id = match (k.token.as_str(), k.key) {
                ("w", None) if first => "UP",
                ("a", None) if first => "LEFT",
                ("s", None) if first => "DOWN",
                ("d", None) if first => "RIGHT",
                (token, _) => bound(token)?.id.as_str(),
            };
            let step = camera.screen_step(id)?;
            actions.iter().find(|a| a.id == step)?.keys.first().cloned()
        };
        let rest = keys
            .into_iter()
            .filter(|k| zoom(k).is_none() && turn(k).is_none() && window(k).is_none())
            .map(|k| match turning.then(|| screen_step(&k)).flatten() {
                Some(token) => FrameKey { token, ..k },
                None => k,
            })
            .collect();
        for id in opened {
            if !self.toggle_panel(id) {
                self.world_map = match self.world_map {
                    Some(_) => None,
                    None => Some((bn_client_core::Pos { x: 0, y: 0 }, egui::Vec2::ZERO)),
                };
            }
        }
        self.display.zoom_by(zooms);
        self.iso.camera.orbit(turns * iso::TURN, 0.0);
        rest
    }

    /// The engine's tokens of the keys a menu or the map forwards, typed text first.
    ///
    /// A menu is an engine input context: its navigation, paging, confirm and cancel keys are the player's
    /// bindings, and the engine moves its own cursor. The exceptions are the toolkit's accessibility keys: Tab and
    /// Shift+Tab move focus, and Space and Enter act on a focused widget. Menus with a text field keep the keyboard
    /// for typing. Only a focused control of the menu itself takes Space and Enter; with the focus left on the
    /// map, they are the engine's (they close a popup).
    fn engine_keys(
        &self,
        ctx: &egui::Context,
        keys: Vec<FrameKey>,
        menu: Option<&Menu>,
    ) -> Vec<String> {
        use egui::Key;
        if !menu_forwards_keys(menu) {
            return vec![];
        }
        let focused = ctx
            .memory(|m| m.focused())
            .is_some_and(|id| self.menu_ids.contains(&id));
        let toolkit = |token: &str| match menu {
            // A composed screen has no widgets: every key, Tab included, is the engine's.
            None => false,
            Some(m) if m.screen => false,
            Some(m) => {
                // Tab switches a tabbed screen's tab, unless a text box has the keyboard.
                (matches!(token, "TAB" | "BACKTAB") && !tab_is_engines(m))
                    || (focused && matches!(token, "SPACE" | "RETURN"))
            }
        };
        // Esc closes a menu through its cancel button, which also covers menus the engine binds no key to.
        let esc_cancels = menu.is_some_and(|m| m.can_cancel && !m.acknowledges());
        let engines = |k: &FrameKey| match k.key {
            None => true,
            Some((key, modifiers)) => {
                let cancels = key == Key::Escape && esc_cancels;
                // Alt with an arrow moves the focus among the action rows, not the engine's cursor.
                let moves_focus = modifiers.alt && matches!(key, Key::ArrowUp | Key::ArrowDown);
                !(cancels || moves_focus)
            }
        };
        let (typed, named): (Vec<_>, Vec<_>) = keys
            .into_iter()
            .filter(|k| engines(k) && !toolkit(&k.token))
            .partition(|k| k.key.is_none());
        let mut keys = typed
            .into_iter()
            .chain(named)
            .map(|k| k.token)
            .collect::<Vec<_>>();
        // A popup takes one key per frame; the rest of a burst would land on the game it covers.
        if menu.is_some_and(Menu::acknowledges) {
            keys.truncate(1);
        }
        keys
    }

    /// Sends the keys. A key press is one step, as in the native game. Once the OS repeats the key, the step is
    /// held, not repeated: the engine walks until key up. Any other key is a screen-scoped affordance, which the
    /// engine queues behind the command in flight.
    fn dispatch(&mut self, ctx: &egui::Context, keys: Vec<String>) {
        let down = ctx.input(|i| {
            i.events.iter().rev().find_map(|e| match e {
                egui::Event::Key {
                    key,
                    pressed: true,
                    repeat,
                    ..
                } => Some((*key, *repeat)),
                _ => None,
            })
        });
        let repeated = down.filter(|(_, repeat)| *repeat).map(|(key, _)| key);
        for key in keys {
            if repeated.is_some() && self.model.hold(&key) {
                self.walk_key = repeated;
                self.keys_consumed = true;
            } else {
                self.keys_consumed |= self.model.press(&key) || self.model.key_bound(&key);
            }
        }
    }

    /// The walk ends when the key that started it is up, or the window lost the keyboard (a key-up would never come).
    fn release_hold_when_up(&mut self, ctx: &egui::Context) {
        let walk_key = self.walk_key;
        if ctx.input(|i| !i.focused || walk_key.is_none_or(|k| !i.keys_down.contains(&k))) {
            self.walk_key = None;
            self.model.release_hold();
        }
    }

    /// Alt+Down and Alt+Up put the keyboard focus on the next and previous action row of the open menu,
    /// where Enter sends that action; the engine's own keys never reach those rows.
    fn focus_action_row(&mut self, ctx: &egui::Context) {
        let step = ctx.input(|i| {
            i.events.iter().fold(0_isize, |step, e| match e {
                egui::Event::Key {
                    key: egui::Key::ArrowDown,
                    pressed: true,
                    modifiers,
                    ..
                } if modifiers.alt => step + 1,
                egui::Event::Key {
                    key: egui::Key::ArrowUp,
                    pressed: true,
                    modifiers,
                    ..
                } if modifiers.alt => step - 1,
                _ => step,
            })
        });
        if step == 0 || self.action_ids.is_empty() {
            return;
        }
        let at = ctx
            .memory(|m| m.focused())
            .and_then(|id| self.action_ids.iter().position(|a| *a == id));
        let last = self.action_ids.len() as isize - 1;
        let to = match at {
            Some(i) => (i as isize + step).clamp(0, last),
            None if step > 0 => 0,
            None => last,
        };
        ctx.memory_mut(|m| m.request_focus(self.action_ids[to as usize]));
    }
}

/// The engine's name for a pressed key, with Ctrl and Shift folded in (`CTRL+S`, `BACKTAB`); typed characters
/// arrive as text instead, so a character key counts only with Ctrl.
pub(crate) fn key_name(key: egui::Key, mods: egui::Modifiers) -> Option<String> {
    use egui::Key as K;
    use keys::Key;
    const FUNCTION: [K; 15] = [
        K::F1,
        K::F2,
        K::F3,
        K::F4,
        K::F5,
        K::F6,
        K::F7,
        K::F8,
        K::F9,
        K::F10,
        K::F11,
        K::F12,
        K::F13,
        K::F14,
        K::F15,
    ];
    let base = match key {
        K::ArrowUp => Key::Up,
        K::ArrowDown => Key::Down,
        K::ArrowLeft => Key::Left,
        K::ArrowRight => Key::Right,
        K::Escape => Key::Escape,
        K::Enter => Key::Enter,
        K::Tab => Key::Tab,
        K::Backspace => Key::Backspace,
        K::Delete => Key::Delete,
        K::Home => Key::Home,
        K::End => Key::End,
        K::PageUp => Key::PageUp,
        K::PageDown => Key::PageDown,
        _ => match FUNCTION.iter().position(|k| *k == key) {
            Some(n) => Key::F(n as u8 + 1),
            None if mods.ctrl => {
                let mut chars = key.name().chars();
                match (chars.next(), chars.next()) {
                    (Some(only), None) => Key::Char(only.to_ascii_lowercase()),
                    _ => return None,
                }
            }
            None => return None,
        },
    };
    keys::name(
        base,
        keys::Mods {
            ctrl: mods.ctrl,
            shift: mods.shift,
        },
    )
}
