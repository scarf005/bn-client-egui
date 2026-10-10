//! Terminal client over the same `GameModel` as the egui client.

mod aim;
mod crafting;
mod settings;

use bn_client_core::{ActionEntry, Failure, GameModel, Loading, Menu, Overmap, Pos, Rgb, keys};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Gauge, Paragraph, Wrap};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

const LOG_LINES: usize = 6;
const SIDE_WIDTH: u16 = 36;
const PAGE: usize = 10;

pub struct TuiApp<M: GameModel> {
    pub model: M,
    selected: usize,
    /// Identity of the menu `selected` refers to; a different menu resets the selection.
    menu_sig: Option<(String, usize, Option<String>)>,
    quit: bool,
    /// The look cursor in absolute map coordinates; `Some` while looking around.
    cursor: Option<Pos>,
    /// Absolute destination of our own walk, for models without engine travel.
    destination: Option<Pos>,
    /// Where the map cells were last drawn, for mouse clicks.
    map_cells: Rect,
    /// Where the overmap's cells were last drawn, and the overmap they show.
    overmap_cells: Rect,
    /// Where the overmap's note list was last drawn, one note per row.
    overmap_notes: Rect,
    overmap_shown: Option<Overmap>,
    /// View coordinates of the first drawn cell.
    map_first: Pos,
    /// The action list popup is open, with its highlighted row.
    actions_open: Option<usize>,
    /// Message log lines scrolled back from the newest.
    log_back: usize,
    /// The crafting search or batch count being typed.
    input: Option<crafting::Input>,
    /// The map square the mouse last hovered, in absolute coordinates.
    hovered: Option<Pos>,
    /// A settings value being typed.
    edit: Option<settings::Edit>,
    /// The advanced inventory's drawn rows and queued inputs.
    aim: aim::Aim,
}

fn color(c: Rgb) -> Color {
    Color::Rgb(c.0, c.1, c.2)
}

/// The game's key token for a terminal key, or `None` for keys the game has no name for.
fn token(key: KeyEvent) -> Option<String> {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let base = match key.code {
        KeyCode::Char(' ') => "SPACE".into(),
        KeyCode::Char(c) => c.to_string(),
        KeyCode::Up => "UP".into(),
        KeyCode::Down => "DOWN".into(),
        KeyCode::Left => "LEFT".into(),
        KeyCode::Right => "RIGHT".into(),
        KeyCode::Home => "HOME".into(),
        KeyCode::End => "END".into(),
        KeyCode::PageUp => "PPAGE".into(),
        KeyCode::PageDown => "NPAGE".into(),
        KeyCode::Enter => "RETURN".into(),
        KeyCode::Tab => "TAB".into(),
        KeyCode::BackTab => "BACKTAB".into(),
        KeyCode::Backspace => "BACKSPACE".into(),
        KeyCode::Delete => "DELETE".into(),
        KeyCode::Esc => "ESC".into(),
        KeyCode::F(n @ 1..=12) => keys::function(n),
        _ => return None,
    };
    Some(keys::chord(
        &base,
        ctrl,
        key.modifiers.contains(KeyModifiers::SHIFT),
    ))
}

fn arrow(code: KeyCode) -> Option<(i32, i32)> {
    match code {
        KeyCode::Up => Some((0, -1)),
        KeyCode::Down => Some((0, 1)),
        KeyCode::Left => Some((-1, 0)),
        KeyCode::Right => Some((1, 0)),
        _ => None,
    }
}

impl<M: GameModel> TuiApp<M> {
    pub fn new(model: M) -> Self {
        Self {
            model,
            selected: 0,
            menu_sig: None,
            quit: false,
            cursor: None,
            destination: None,
            map_cells: Rect::default(),
            overmap_cells: Rect::default(),
            overmap_notes: Rect::default(),
            overmap_shown: None,
            map_first: Pos { x: 0, y: 0 },
            actions_open: None,
            log_back: 0,
            input: None,
            edit: None,
            hovered: None,
            aim: Default::default(),
        }
    }

    pub fn quit(&self) -> bool {
        self.quit
    }

    /// Label of the highlighted menu row.
    pub fn selected_label(&mut self) -> Option<String> {
        let menu = self.menu()?;
        menu.entries().get(self.selected).map(|c| c.label.clone())
    }

    /// The current menu, resetting the highlight when the engine shows a different one.
    fn menu(&mut self) -> Option<Menu> {
        let menu = self.model.menu();
        let sig = menu.as_ref().map(|m| {
            let tab = m.tabs().iter().find(|t| t.selected).map(|t| t.id.clone());
            (m.title.clone(), m.entries().len(), tab)
        });
        if sig != self.menu_sig {
            // Typing a filter changes the rows but not the screen.
            if sig.as_ref().map(|s| &s.0) != self.menu_sig.as_ref().map(|s| &s.0) {
                self.edit = None;
            }
            self.selected = 0;
            self.menu_sig = sig;
        }
        // The engine's own cursor is the highlight; its navigation keys move it. The crafting
        // screen keeps its own list.
        if let Some(menu) = menu.as_ref().filter(|m| m.context != "CRAFTING") {
            let entries = menu.entries();
            // A cursor on a tab header, or none, leaves the row marked selected as the one it is on.
            let at = entries
                .iter()
                .position(|c| menu.focus.as_deref() == Some(c.id.as_str()))
                .or_else(|| entries.iter().position(|c| c.selected));
            self.selected = at.unwrap_or(self.selected);
            // The rest of a long list comes in pages as the cursor nears the end of what is listed.
            if menu.total > menu.choices.len() && self.selected + PAGE >= entries.len() {
                self.model.more_choices();
            }
        }
        menu
    }

    /// Whether our own walk is under way and needs `tick`s.
    pub fn walking(&self) -> bool {
        self.destination.is_some()
    }

    /// One step of our own walk (models without engine travel); call between frames.
    pub fn tick(&mut self) {
        self.run_aim_steps();
        let Some(dest) = self.destination else {
            return;
        };
        let origin = self.model.origin();
        let target = Pos {
            x: dest.x - origin.x,
            y: dest.y - origin.y,
        };
        if self.model.busy() {
            return;
        }
        if !self.model.step_toward(target) {
            self.model.say("Something blocks the way.".into());
            self.destination = None;
        } else if self.model.player() == target {
            self.destination = None;
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            self.quit = true;
            return;
        }
        if self.model.failure().is_some() {
            if matches!(key.code, KeyCode::Char('r' | 'R')) {
                self.model.restart();
            }
            return;
        }
        if self.model.loading().is_some() {
            return;
        }
        if key.code == KeyCode::Esc {
            // Esc stops a running activity and leaves look mode, whichever applies.
            let stopped = self.model.interrupt();
            if self.cursor.take().is_some() {
                self.destination = None;
                return;
            }
            if stopped {
                return;
            }
            if self.model.overmap().is_some() {
                self.model.close_overmap();
                return;
            }
        }
        if let Some(row) = self.actions_open {
            return self.actions_key(row, key.code);
        }
        if let Some(menu) = self.menu() {
            return match menu.context.as_str() {
                "CRAFTING" => self.crafting_key(&menu, key),
                // Every key of the advanced inventory is the engine's, Tab included (it swaps panes).
                "ADVANCED_INVENTORY" => self.menu_key(&menu, key),
                _ if !menu.screen && self.settings_key(&menu, key) => {}
                _ => self.menu_key(&menu, key),
            };
        }
        if self.cursor.is_some() {
            return self.look_key(key);
        }
        self.destination = None;
        // Look mode starts with whatever keys the engine binds to its look action.
        if self.is_look_key(key) {
            self.cursor = Some(self.absolute(self.model.player()));
            return;
        }
        if token(key).is_some_and(|t| self.model.press(&t)) {
            return;
        }
        match key.code {
            KeyCode::Tab => self.actions_open = Some(0),
            KeyCode::PageUp => self.log_back += LOG_LINES,
            KeyCode::PageDown => self.log_back = self.log_back.saturating_sub(LOG_LINES),
            code => {
                if let Some((dx, dy)) = arrow(code) {
                    let me = self.model.player();
                    self.model.step_toward(Pos {
                        x: me.x + dx,
                        y: me.y + dy,
                    });
                }
            }
        }
    }

    fn absolute(&self, view: Pos) -> Pos {
        let origin = self.model.origin();
        Pos {
            x: view.x + origin.x,
            y: view.y + origin.y,
        }
    }

    fn view(&self, absolute: Pos) -> Pos {
        let origin = self.model.origin();
        Pos {
            x: absolute.x - origin.x,
            y: absolute.y - origin.y,
        }
    }

    /// A menu is an engine input context: its keys (navigation, paging, confirm, cancel, tab switch)
    /// are whatever the player bound in it, and the engine moves its own cursor.
    fn menu_key(&mut self, menu: &Menu, key: KeyEvent) {
        if token(key).is_some_and(|t| self.model.press(&t)) {
            return;
        }
        // A menu the engine gives no cancel key still closes with Esc.
        if key.code == KeyCode::Esc && menu.can_cancel {
            self.model.cancel_menu();
        }
    }

    fn actions_key(&mut self, row: usize, code: KeyCode) {
        let actions = self.model.actions();
        let last = actions.len().saturating_sub(1);
        match code {
            KeyCode::Up => self.actions_open = Some(row.saturating_sub(1)),
            KeyCode::Down => self.actions_open = Some((row + 1).min(last)),
            KeyCode::PageUp => self.actions_open = Some(row.saturating_sub(PAGE)),
            KeyCode::PageDown => self.actions_open = Some((row + PAGE).min(last)),
            KeyCode::Enter => {
                self.actions_open = None;
                if let Some(a) = actions.get(row) {
                    self.model.invoke_action(&a.id);
                }
            }
            KeyCode::Esc | KeyCode::Tab => self.actions_open = None,
            _ => {}
        }
    }

    /// The one-square step of the movement action the engine binds to `token`.
    fn direction(&self, token: &str) -> Option<(i32, i32)> {
        let action = self
            .model
            .actions()
            .into_iter()
            .find(|a| a.keys.iter().any(|k| k == token))?;
        Some(match action.id.as_str() {
            "UP" => (0, -1),
            "DOWN" => (0, 1),
            "LEFT" => (-1, 0),
            "RIGHT" => (1, 0),
            "LEFTUP" => (-1, -1),
            "RIGHTUP" => (1, -1),
            "LEFTDOWN" => (-1, 1),
            "RIGHTDOWN" => (1, 1),
            _ => return None,
        })
    }

    /// Whether the engine binds `key` to its look action.
    fn is_look_key(&self, key: KeyEvent) -> bool {
        token(key).is_some_and(|t| {
            self.model
                .actions()
                .iter()
                .any(|a| a.id == "look" && a.keys.contains(&t))
        })
    }

    /// The look cursor moves with arrows and vi keys (five cells with Shift); Enter plans a route to it
    /// and, pressed again, walks it; Esc leaves.
    fn look_key(&mut self, key: KeyEvent) {
        let Some(at) = self.cursor else {
            return;
        };
        let shifted = key.modifiers.contains(KeyModifiers::SHIFT)
            || matches!(key.code, KeyCode::Char(c) if c.is_uppercase());
        let step = if shifted { 5 } else { 1 };
        // The directions are whatever keys the engine binds to its movement actions; an upper case
        // letter without a binding of its own is the five-cell move of its lower case twin.
        let named = token(key);
        let delta = named
            .as_deref()
            .and_then(|t| self.direction(t))
            .or_else(|| named.and_then(|t| self.direction(&t.to_lowercase())));
        if let Some((dx, dy)) = delta {
            let size = self.model.size();
            let view = self.view(at);
            self.cursor = Some(self.absolute(Pos {
                x: (view.x + dx * step).clamp(0, (size.x - 1).max(0)),
                y: (view.y + dy * step).clamp(0, (size.y - 1).max(0)),
            }));
            return;
        }
        match key.code {
            KeyCode::Enter => {
                if self.view(at) == self.model.player() {
                    self.cursor = None;
                } else {
                    self.go_to(self.view(at));
                }
            }
            _ if key.code == KeyCode::Esc || self.is_look_key(key) => {
                self.cursor = None;
                self.destination = None;
            }
            _ => {}
        }
    }

    /// The model's own travel if it has one (engine: first call plans, second walks), else our walk.
    fn go_to(&mut self, view: Pos) {
        if !self.model.travel(view) {
            self.destination = Some(self.absolute(view));
        }
    }

    /// The mouse as the game reads it: the wheel steps an engine menu, hovering a map cell asks for its
    /// native description (or moves the engine's own cursor), the left click moves the look cursor there
    /// and travels (the first click shows the route), the right click is the engine's context click.
    pub fn handle_mouse(&mut self, ev: MouseEvent) {
        if self.model.loading().is_some() {
            return;
        }
        if let Some(menu) = self
            .model
            .menu()
            .filter(|m| m.context == "ADVANCED_INVENTORY")
        {
            return self.aim_mouse(&menu, ev);
        }
        let wheel = match ev.kind {
            MouseEventKind::ScrollUp => Some(true),
            MouseEventKind::ScrollDown => Some(false),
            _ => None,
        };
        if let Some(up) = wheel {
            // Only a list the engine scrolls natively (it registers SCROLL_UP/SCROLL_DOWN) takes the
            // wheel; the highlight is the engine's own cursor, so any other menu ignores it, as native.
            self.model.scroll_menu(up);
            return;
        }
        if let Some(om) = self
            .overmap_shown
            .clone()
            .filter(|_| self.model.overmap().is_some())
        {
            return self.overmap_mouse(&om, ev);
        }
        let area = self.map_cells;
        let inside = (area.x..area.x + area.width).contains(&ev.column)
            && (area.y..area.y + area.height).contains(&ev.row);
        if !inside || self.model.menu().is_some() {
            return;
        }
        let view = Pos {
            x: self.map_first.x + i32::from(ev.column - area.x),
            y: self.map_first.y + i32::from(ev.row - area.y),
        };
        match ev.kind {
            MouseEventKind::Moved => {
                self.hovered = Some(self.absolute(view));
                if !self.model.aim(view) {
                    self.model.hover(view);
                }
            }
            MouseEventKind::Down(MouseButton::Left) => {
                self.cursor = Some(self.absolute(view));
                if !self.model.aim(view) {
                    self.go_to(view);
                }
            }
            MouseEventKind::Down(MouseButton::Right) => {
                self.model.context_click(view);
            }
            _ => {}
        }
    }

    /// A click moves the overmap cursor to the square; a click on the cursor travels there.
    fn overmap_mouse(&mut self, om: &Overmap, ev: MouseEvent) {
        let notes = self.overmap_notes;
        let on_notes = (notes.x..notes.x + notes.width).contains(&ev.column)
            && (notes.y..notes.y + notes.height).contains(&ev.row);
        if ev.kind == MouseEventKind::Down(MouseButton::Left) && on_notes {
            if let Some((at, _)) = om.notes.get(usize::from(ev.row - notes.y)) {
                self.model.overmap_cursor_to(*at);
            }
            return;
        }
        let area = self.overmap_cells;
        let inside = (area.x..area.x + area.width).contains(&ev.column)
            && (area.y..area.y + area.height).contains(&ev.row);
        if ev.kind != MouseEventKind::Down(MouseButton::Left) || !inside {
            return;
        }
        let at = om.square(i32::from(ev.column - area.x), i32::from(ev.row - area.y));
        if at == om.cursor {
            self.model.invoke_action("CHOOSE_DESTINATION");
        } else {
            self.model.overmap_cursor_to(at);
        }
    }

    /// The overmap window as the game drew it, the cursor reversed, with the sidebar beside it.
    fn draw_overmap(&mut self, f: &mut Frame, om: &Overmap) {
        let [grid, legend] = Layout::horizontal([
            Constraint::Length(om.cols.clamp(0, i32::from(u16::MAX)) as u16),
            Constraint::Min(10),
        ])
        .areas(f.area());
        let cells = Rect::new(
            grid.x,
            grid.y,
            grid.width,
            (om.rows.clamp(0, i32::from(u16::MAX)) as u16).min(grid.height),
        );
        self.overmap_cells = cells;
        self.overmap_shown = Some(om.clone());
        let cursor = (om.cursor.0 - om.origin.0, om.cursor.1 - om.origin.1);
        let lines = (0..i32::from(cells.height)).map(|row| {
            let spans = (0..i32::from(cells.width)).map(|col| {
                let Some(cell) = om.cell(col, row) else {
                    return Span::raw(" ");
                };
                let mut style = Style::new().fg(color(cell.color));
                if let Some(bg) = cell.bg {
                    style = style.bg(color(bg));
                }
                if om.path.contains(&om.square(col, row)) {
                    style = style.bg(Color::Cyan);
                }
                if (col, row) == cursor {
                    style = style.add_modifier(Modifier::REVERSED);
                }
                let glyph = if cell.glyph.is_empty() {
                    " "
                } else {
                    &cell.glyph
                };
                Span::styled(glyph.to_string(), style)
            });
            Line::from(spans.collect::<Vec<_>>())
        });
        f.render_widget(Paragraph::new(lines.collect::<Vec<_>>()), cells);
        let mut sidebar = vec![Line::from(format!(
            "Overmap ({}, {}, {})",
            om.cursor.0, om.cursor.1, om.cursor.2
        ))];
        if let Some(note) = om.note(om.cursor) {
            sidebar.push(Line::from(format!("Note: {note}")));
        }
        if !om.path.is_empty() {
            sidebar.push(Line::from(format!(
                "Route of {} squares: click the cursor to go",
                om.path.len()
            )));
        }
        sidebar.extend(om.legend.iter().map(|l| Line::from(l.clone())));
        let listed = om.notes.len().min(usize::from(legend.height) / 3);
        let [text, notes] =
            Layout::vertical([Constraint::Min(0), Constraint::Length(listed as u16)]).areas(legend);
        f.render_widget(Paragraph::new(sidebar), text);
        // One row per note, nearest the foot of the sidebar; a click on a row moves the cursor to it.
        self.overmap_notes = notes;
        let rows = om.notes.iter().take(listed).map(|((x, y, _), note)| {
            Line::styled(
                format!("({x}, {y}) {note}"),
                Style::new()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::UNDERLINED),
            )
        });
        f.render_widget(Paragraph::new(rows.collect::<Vec<_>>()), notes);
    }

    pub fn draw(&mut self, f: &mut Frame) {
        if let Some(failure) = self.model.failure() {
            return Self::draw_failure(f, &failure);
        }
        if let Some(om) = self.model.overmap() {
            return self.draw_overmap(f, &om);
        }
        if let Some(loading) = self.model.loading() {
            return Self::draw_loading(f, &loading);
        }
        let [main, status, log] = Layout::vertical([
            Constraint::Min(5),
            Constraint::Length(1),
            Constraint::Length(LOG_LINES as u16 + 2),
        ])
        .areas(f.area());
        let [map, side] =
            Layout::horizontal([Constraint::Min(10), Constraint::Length(SIDE_WIDTH)]).areas(main);
        let [actions, items] =
            Layout::vertical([Constraint::Min(3), Constraint::Length(12)]).areas(side);
        self.draw_map(f, map);
        self.draw_actions(f, actions);
        self.draw_items(f, items);
        f.render_widget(Paragraph::new(self.status_line()), status);
        // The game's own lines in their colours with repeat counts, then the client's command results.
        let native = self.model.native_log().iter().map(|l| {
            let style = l.color.map_or(Style::new(), |c| Style::new().fg(color(c)));
            Line::styled(l.shown(), style)
        });
        let commands = self
            .model
            .messages()
            .iter()
            .map(|m| Line::styled(m.as_str(), Style::new().fg(Color::DarkGray)));
        let all = native.chain(commands).collect::<Vec<_>>();
        let end = all.len().saturating_sub(self.log_back.min(all.len()));
        let lines = all[end.saturating_sub(LOG_LINES)..end].to_vec();
        f.render_widget(
            Paragraph::new(lines).block(Block::bordered().title("Message log")),
            log,
        );
        if let Some(menu) = self.menu() {
            match menu.context.as_str() {
                "CRAFTING" => self.draw_crafting(f, &menu),
                "ADVANCED_INVENTORY" => self.draw_aim(f, &menu),
                _ => self.draw_menu(f, &menu),
            }
        }
        if let Some(row) = self.actions_open {
            self.draw_action_list(f, row);
        }
    }

    /// The pressed command and its stage, then what the look cursor points at.
    fn status_line(&mut self) -> String {
        let command = self
            .model
            .command()
            .map_or_else(|| "Ready".into(), |s| format!("{}: {}", s.label, s.stage));
        let command = match self.model.busy() {
            true => format!("{command} [Esc: interrupt]"),
            false => command,
        };
        // What the engine's mouse view prints for the hovered square, once it has answered.
        let hover = self
            .hovered
            .and_then(|at| self.model.hover_text(self.view(at)))
            .map(|lines| format!(" | {}", lines.join(" / ")))
            .unwrap_or_default();
        let command = format!("{command}{hover}");
        let Some(at) = self.cursor else {
            return command;
        };
        let view = self.view(at);
        let route = self.model.route();
        let hint = match route.is_empty() {
            true => "Enter: route",
            false => "Enter: walk",
        };
        let desc = self.model.cell(view).desc;
        format!(
            "{command} | Look ({}, {}): {desc} [{hint}, Esc]",
            view.x, view.y
        )
    }

    /// The engine is gone or silent: its last stderr lines and how to restart.
    fn draw_failure(f: &mut Frame, failure: &Failure) {
        let mut lines = vec![Line::from(failure.message.clone()), Line::from("")];
        if !failure.stderr.is_empty() {
            lines.push(Line::from("Last engine output:"));
            lines.extend(failure.stderr.iter().map(|l| Line::from(l.clone())));
            lines.push(Line::from(""));
        }
        lines.push(Line::from("R: restart the engine    Ctrl-C: quit"));
        f.render_widget(Paragraph::new(lines), f.area());
    }

    /// Title, a progress bar over the steps, and the step list (the splash image is skipped).
    fn draw_loading(f: &mut Frame, loading: &Loading) {
        let total = loading.entries.len();
        let step = (loading.index + 1).min(total);
        let [title, bar, list] = Layout::vertical([
            Constraint::Length(1),
            Constraint::Length(3),
            Constraint::Min(1),
        ])
        .areas(f.area());
        f.render_widget(
            Paragraph::new(Line::styled(
                loading.title.clone(),
                Style::new().add_modifier(Modifier::BOLD),
            )),
            title,
        );
        let ratio = if total == 0 {
            0.0
        } else {
            step as f64 / total as f64
        };
        f.render_widget(
            Gauge::default()
                .block(Block::bordered())
                .ratio(ratio)
                .label(format!("{step}/{total}")),
            bar,
        );
        let lines = loading
            .entries
            .iter()
            .enumerate()
            .map(|(i, e)| match i == loading.index {
                true => Line::styled(format!("> {e}"), Style::new().add_modifier(Modifier::BOLD)),
                false => Line::raw(format!("  {e}")),
            });
        let offset = (loading.index + 1).saturating_sub(usize::from(list.height));
        f.render_widget(Paragraph::new(lines.skip(offset).collect::<Vec<_>>()), list);
    }

    fn draw_map(&mut self, f: &mut Frame, area: Rect) {
        let block = Block::bordered().title("Map");
        let inner = block.inner(area);
        f.render_widget(block, area);
        self.model.set_viewport(Pos {
            x: i32::from(inner.width),
            y: i32::from(inner.height),
        });
        let size = self.model.size();
        if size.x <= 0 || size.y <= 0 {
            self.map_cells = Rect::default();
            f.render_widget(Paragraph::new("No map loaded"), inner);
            return;
        }
        let player = self.model.player();
        // The part of the view that fits, centred on the player when the view is larger than the room.
        let window = |centre: i32, len: i32, avail: u16| {
            let avail = i32::from(avail).min(len);
            let start = (centre - avail / 2).clamp(0, len - avail);
            (start, avail)
        };
        let (x0, w) = window(player.x, size.x, inner.width);
        let (y0, h) = window(player.y, size.y, inner.height);
        // A view smaller than the room sits in the middle of it.
        let cells = Rect::new(
            inner.x + (inner.width - w as u16) / 2,
            inner.y + (inner.height - h as u16) / 2,
            w as u16,
            h as u16,
        );
        self.map_cells = cells;
        self.map_first = Pos { x: x0, y: y0 };
        let route = self.model.route();
        let cursor = self.cursor.map(|c| self.view(c));
        let lines = (y0..y0 + h)
            .map(|y| {
                let mut spans = vec![];
                let mut skip = 0;
                for x in x0..x0 + w {
                    if skip > 0 {
                        skip -= 1;
                        continue;
                    }
                    let at = Pos { x, y };
                    let cell = self.model.cell(at);
                    let (mut glyph, mut style) = (cell.glyph, Style::new().fg(color(cell.color)));
                    if let Some(bg) = cell.bg {
                        style = style.bg(color(bg));
                    }
                    if at == player {
                        (glyph, style) = ('@', Style::new().fg(Color::Yellow));
                    } else if route.contains(&at) {
                        (glyph, style) = ('*', Style::new().fg(Color::Yellow));
                    }
                    if cursor == Some(at) {
                        style = style.add_modifier(Modifier::REVERSED);
                    }
                    // A wide glyph covers the next cell; one with no width would shift the row.
                    match glyph.width() {
                        Some(2) => skip = 1,
                        Some(1) => {}
                        _ => glyph = ' ',
                    }
                    spans.push(Span::styled(glyph.to_string(), style));
                }
                Line::from(spans)
            })
            .collect::<Vec<_>>();
        f.render_widget(Paragraph::new(lines), cells);
    }

    fn draw_actions(&mut self, f: &mut Frame, area: Rect) {
        let lines = self
            .model
            .actions()
            .iter()
            .map(|a| Line::raw(action_label(a)))
            .collect::<Vec<_>>();
        f.render_widget(
            Paragraph::new(lines).block(Block::bordered().title("Actions")),
            area,
        );
    }

    fn draw_items(&mut self, f: &mut Frame, area: Rect) {
        let section = |title: &str, items: &[String]| {
            let head = Line::styled(title.to_string(), Style::new().add_modifier(Modifier::BOLD));
            std::iter::once(head)
                .chain(items.iter().map(|i| Line::raw(i.clone())))
                .collect::<Vec<_>>()
        };
        let mut lines = section("Ground", self.model.ground());
        lines.extend(section("Inventory", self.model.inventory()));
        f.render_widget(
            Paragraph::new(lines)
                .wrap(Wrap { trim: false })
                .block(Block::bordered()),
            area,
        );
    }

    fn draw_menu(&mut self, f: &mut Frame, menu: &Menu) {
        let rows = menu
            .entries()
            .iter()
            .enumerate()
            .map(|(i, c)| {
                let denial = match c.enabled || c.denial.is_empty() {
                    true => String::new(),
                    false => format!(" ({})", c.denial),
                };
                let style = match c.enabled {
                    true => Style::new(),
                    false => Style::new().fg(Color::DarkGray),
                };
                let value = match (&c.editor, &self.edit) {
                    (Some(_), Some(edit)) if i == self.selected => format!(": {}", edit.shown()),
                    (Some(e), _) => format!(": {}", settings::shown_value(e)),
                    _ => String::new(),
                };
                (format!("{}{value}{denial}", c.label), style)
            })
            .collect::<Vec<_>>();
        let tabs = menu
            .tabs()
            .iter()
            .map(|t| (t.label.clone(), t.selected))
            .collect::<Vec<_>>();
        let popup = Popup {
            title: &menu.title,
            header: menu
                .message
                .lines()
                .map(str::to_string)
                .chain(self.settings_hint(menu))
                .collect(),
            tabs,
            rows,
            selected: self.selected,
        };
        Self::draw_popup(f, &popup);
    }

    fn draw_action_list(&mut self, f: &mut Frame, row: usize) {
        let rows = self
            .model
            .actions()
            .iter()
            .map(|a| (action_label(a), Style::new()))
            .collect();
        let popup = Popup {
            title: "Actions",
            header: vec![],
            tabs: vec![],
            rows,
            selected: row,
        };
        Self::draw_popup(f, &popup);
    }

    /// A centred box: message lines, a tab bar, then the rows scrolled to keep the highlighted one visible.
    fn draw_popup(f: &mut Frame, popup: &Popup) {
        let area = f.area();
        let tab_bar = bar(&popup.tabs);
        let widest = popup
            .header
            .iter()
            .map(|l| l.width())
            .chain(popup.rows.iter().map(|(l, _)| l.width() + 2))
            .chain(std::iter::once(tab_bar.width()))
            .max()
            .unwrap_or(0);
        let width = (widest as u16 + 2).clamp(30, 100).min(area.width);
        let header = popup
            .header
            .iter()
            .flat_map(|l| wrap(l, usize::from(width) - 2))
            .collect::<Vec<_>>();
        let head = (header.len() as u16).min(area.height.saturating_sub(2) / 2);
        let bar = u16::from(!popup.tabs.is_empty());
        let want = head + bar + popup.rows.len() as u16 + 2;
        let height = want.min(area.height);
        let rect = Rect::new(
            area.x + (area.width - width) / 2,
            area.y + (area.height - height) / 2,
            width,
            height,
        );
        f.render_widget(Clear, rect);
        let block = Block::bordered().title(popup.title.to_string());
        let inner = block.inner(rect);
        f.render_widget(block, rect);
        let [header_area, bar_area, list] = Layout::vertical([
            Constraint::Length(head),
            Constraint::Length(bar),
            Constraint::Min(0),
        ])
        .areas(inner);
        let header_lines = header;
        let text = |s: &String| Line::raw(s.clone());
        f.render_widget(
            Paragraph::new(header_lines.iter().map(text).collect::<Vec<_>>()),
            header_area,
        );
        f.render_widget(Paragraph::new(tab_bar), bar_area);
        let offset = (popup.selected + 1).saturating_sub(usize::from(list.height));
        let lines = popup
            .rows
            .iter()
            .enumerate()
            .skip(offset)
            .map(|(i, (label, style))| match i == popup.selected {
                true => Line::styled(format!("> {label}"), style.add_modifier(Modifier::REVERSED)),
                false => Line::styled(format!("  {label}"), *style),
            })
            .collect::<Vec<_>>();
        f.render_widget(Paragraph::new(lines), list);
    }
}

/// Tab labels in a row, the selected one reversed.
fn bar(tabs: &[(String, bool)]) -> Line<'static> {
    Line::from(
        tabs.iter()
            .flat_map(|(label, on)| {
                let style = match on {
                    true => Style::new().add_modifier(Modifier::REVERSED),
                    false => Style::new(),
                };
                [Span::styled(format!(" {label} "), style), Span::raw(" ")]
            })
            .collect::<Vec<_>>(),
    )
}

/// Breaks `text` into lines of at most `width` terminal columns, at spaces where it can.
fn wrap(text: &str, width: usize) -> Vec<String> {
    let mut lines = vec![String::new()];
    for word in text.split(' ') {
        let line = lines.last_mut().expect("starts with one line");
        let sep = usize::from(!line.is_empty());
        if line.width() + sep + word.width() <= width {
            line.push_str(&" ".repeat(sep));
            line.push_str(word);
            continue;
        }
        lines.push(String::new());
        // A word wider than a line (Korean runs, paths) breaks between characters.
        for c in word.chars() {
            let line = lines.last_mut().expect("just pushed");
            if line.width() + c.width().unwrap_or(0) > width {
                lines.push(String::new());
            }
            lines.last_mut().expect("just pushed").push(c);
        }
    }
    lines
}

/// What `draw_popup` shows.
struct Popup<'a> {
    title: &'a str,
    header: Vec<String>,
    /// Tab labels and whether each is the current one.
    tabs: Vec<(String, bool)>,
    rows: Vec<(String, Style)>,
    selected: usize,
}

fn action_label(a: &ActionEntry) -> String {
    match a.keys.is_empty() {
        true => a.name.clone(),
        false => format!("{} [{}]", a.name, a.keys.join(", ")),
    }
}
