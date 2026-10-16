//! Terminal client over the same `GameModel` as the egui client.

mod component;
mod settings;

use bn_client_core::markup::plain;
use bn_client_core::protocol::SidebarSide;
use bn_client_core::sidebar::{self as native, Body};
use bn_client_core::{ActionEntry, Failure, GameModel, Loading, Menu, MenuChoice, Pos, Rgb, keys};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Position, Rect};
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
    /// Where the engine's own cursor was in the menu, and the row Up or Down moved the highlight to when the
    /// engine binds neither; the engine's cursor moving takes the highlight back.
    engine_at: Option<usize>,
    moved: Option<usize>,
    /// Identity of the menu `selected` refers to, its schema id and selected tab; a different menu resets the
    /// selection.
    menu_sig: Option<(String, Option<String>)>,
    quit: bool,
    /// Where the map cells were last drawn, for mouse clicks.
    map_cells: Rect,
    /// View coordinates of the first drawn cell.
    map_first: Pos,
    /// The action list popup is open, with its highlighted row.
    actions_open: Option<usize>,
    /// Message log lines scrolled back from the newest.
    log_back: usize,
    /// The map square the mouse last hovered, in absolute coordinates.
    hovered: Option<Pos>,
    /// A settings value being typed.
    edit: Option<settings::Edit>,
    /// Where the open menu's rows and tabs were last drawn, for mouse clicks.
    menu_hits: MenuHits,
    /// The action row of the open menu the keyboard focus is on (Alt+Down, Alt+Up), where Enter sends it.
    action_focus: Option<usize>,
    footer: Footer,
    /// The state of the engine's 0.2 screens, which the components draw.
    components: component::View,
}

/// What a click on the open menu can reach: the row ids from the first drawn one, and the tab ids.
#[derive(Default)]
struct MenuHits {
    list: Rect,
    rows: Vec<String>,
    bar: Rect,
    tabs: Vec<(String, u16, u16)>,
    /// The description under the rows.
    footer: Rect,
}

/// The description under a menu's rows: the text it was last drawn for, how far it is scrolled, and
/// how many lines it has and shows.
#[derive(Default)]
struct Footer {
    text: String,
    scroll: usize,
    total: usize,
    shown: usize,
}

impl Footer {
    /// Scrolls by `lines` (negative is up), staying within the text.
    fn scroll_by(&mut self, lines: isize) {
        let last = self.total.saturating_sub(self.shown);
        self.scroll = self.scroll.saturating_add_signed(lines).min(last);
    }

    /// Shows `text`, from its top when it is not the text already shown.
    fn show(&mut self, text: &str) {
        if self.text != text {
            *self = Self {
                text: text.to_string(),
                ..Self::default()
            };
        }
    }

    /// Takes the room and scroll `draw_popup` gave the text.
    fn fit(&mut self, layout: &PopupLayout) {
        self.total = layout.footer_total;
        self.shown = usize::from(layout.footer.height);
        self.scroll = layout.footer_scroll;
    }
}

/// The sidebar's width in columns and whether it sits on the left: the game's panel options, else a default.
pub(crate) fn side_of(layout: Option<&native::Layout>) -> (u16, bool) {
    layout.map_or((SIDE_WIDTH, false), |l| {
        (
            l.width.min(u16::MAX as u32) as u16,
            l.side == SidebarSide::Left,
        )
    })
}

/// The rows a panel's contents take.
fn panel_lines(panel: &native::Panel) -> usize {
    match &panel.body {
        Body::Status(rows) => rows.len(),
        Body::Compass(directions) => directions.iter().map(|d| 1 + d.creatures.len()).sum(),
        Body::Log => 0,
    }
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

impl<M: GameModel> TuiApp<M> {
    pub fn new(model: M) -> Self {
        Self {
            model,
            selected: 0,
            engine_at: None,
            moved: None,
            menu_sig: None,
            quit: false,
            map_cells: Rect::default(),
            map_first: Pos { x: 0, y: 0 },
            actions_open: None,
            log_back: 0,
            edit: None,
            hovered: None,
            menu_hits: Default::default(),
            action_focus: None,
            footer: Footer::default(),
            components: Default::default(),
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
            (m.schema_id.clone(), tab)
        });
        // Typing a filter changes the rows but not the field it is typed into.
        let field = menu.as_ref().and_then(|m| m.field.as_ref()).map(|f| &f.id);
        if self.edit.as_ref().is_some_and(|e| Some(&e.id) != field) {
            self.edit = None;
        }
        if sig != self.menu_sig {
            self.selected = 0;
            self.moved = None;
            self.action_focus = None;
            self.menu_sig = sig;
        }
        // The engine's own cursor is the highlight; its navigation keys move it.
        if let Some(menu) = menu.as_ref() {
            let entries = menu.entries();
            // A cursor on a tab header, or none, leaves the row marked selected as the one it is on.
            let at = entries
                .iter()
                .position(|c| menu.focus.as_deref() == Some(c.id.as_str()))
                .or_else(|| entries.iter().position(|c| c.selected));
            if at != self.engine_at {
                self.moved = None;
            }
            self.engine_at = at;
            self.selected = self.moved.or(at).unwrap_or(self.selected);
            // The rest of a long list comes in pages as the cursor nears the end of what is listed.
            if menu.total > menu.choices.len() && self.selected + PAGE >= entries.len() {
                self.model.more_choices();
            }
        }
        menu
    }

    /// Sends the filter a list is waiting to send; call between frames.
    pub fn tick(&mut self) {
        self.send_filter();
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
        if component::placement(self.model.screens()).is_some() {
            return self.components.key(&mut self.model, key);
        }
        if key.code == KeyCode::Esc {
            // Esc stops a running activity, else closes a menu.
            let stopped = self.model.interrupt();
            if stopped {
                return;
            }
        }
        if let Some(row) = self.actions_open {
            return self.actions_key(row, key.code);
        }
        // F2 always opens the client's action list, which Tab does only where the engine leaves Tab unbound.
        if key.code == KeyCode::F(2) && !self.model.key_bound("F2") {
            self.actions_open = Some(0);
            return;
        }
        if let Some(menu) = self.menu() {
            let taken = self.footer_key(key)
                || (!menu.screen
                    && (self.action_row_key(&menu, key) || self.settings_key(&menu, key)));
            if !taken {
                self.menu_key(&menu, key);
            }
            return;
        }
        let token = token(key);
        if token.as_ref().is_some_and(|t| self.model.press(t)) {
            return;
        }
        match key.code {
            // Tab is the engine's when it binds it, even while a command keeps it from running now.
            KeyCode::Tab if !token.is_some_and(|t| self.model.key_bound(&t)) => {
                self.actions_open = Some(0)
            }
            KeyCode::PageUp => self.log_back += LOG_LINES,
            KeyCode::PageDown => self.log_back = self.log_back.saturating_sub(LOG_LINES),
            _ => {}
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

    /// The rows of the menu that stand for an engine action (`action:NAME`), which the engine's own cursor
    /// never reaches. Alt+Down and Alt+Up move the focus among them; Enter sends the one in focus, and
    /// any other key leaves them (the action's own key, shown beside the row, works throughout).
    /// Alt+PageUp and Alt+PageDown page the description under the rows, which the engine has no key for.
    fn footer_key(&mut self, key: KeyEvent) -> bool {
        let page = isize::try_from(self.footer.shown).unwrap_or(isize::MAX);
        match (key.code, key.modifiers.contains(KeyModifiers::ALT)) {
            (KeyCode::PageUp, true) => self.footer.scroll_by(-page),
            (KeyCode::PageDown, true) => self.footer.scroll_by(page),
            _ => return false,
        }
        true
    }

    fn action_row_key(&mut self, menu: &Menu, key: KeyEvent) -> bool {
        let rows = menu
            .entries()
            .into_iter()
            .filter(|c| c.id.starts_with("action:") && c.enabled)
            .map(|c| c.id.clone())
            .collect::<Vec<_>>();
        let alt = key.modifiers.contains(KeyModifiers::ALT);
        match (key.code, alt, self.action_focus) {
            (KeyCode::Down, true, at) if !rows.is_empty() => {
                self.action_focus = Some(at.map_or(0, |i| (i + 1).min(rows.len() - 1)));
                true
            }
            (KeyCode::Up, true, Some(0)) => {
                self.action_focus = None;
                true
            }
            (KeyCode::Up, true, Some(i)) => {
                self.action_focus = Some(i - 1);
                true
            }
            (KeyCode::Enter, _, Some(i)) => {
                self.action_focus = None;
                if let Some(id) = rows.get(i) {
                    self.model.choose(id);
                }
                true
            }
            (KeyCode::Esc, _, Some(_)) => {
                self.action_focus = None;
                true
            }
            (_, _, Some(_)) => {
                self.action_focus = None;
                false
            }
            _ => false,
        }
    }

    /// A menu is an engine input context: its keys (navigation, paging, confirm, cancel, tab switch)
    /// are whatever the player bound in it, and the engine moves its own cursor.
    fn menu_key(&mut self, menu: &Menu, key: KeyEvent) {
        let step = match key.code {
            KeyCode::Up => -1,
            KeyCode::Down => 1,
            _ => 0,
        };
        // Up and Down the engine does not bind in this menu move the highlight among its rows, and Return
        // then chooses the row the highlight is on; the engine's own cursor stays where it was.
        if step != 0 && !token(key).is_some_and(|t| self.model.key_bound(&t)) {
            let rows = menu
                .entries()
                .iter()
                .enumerate()
                .filter(|(_, c)| c.enabled)
                .map(|(i, _)| i)
                .collect::<Vec<_>>();
            let at = rows.iter().position(|i| *i == self.selected);
            let to = match at {
                Some(at) => at
                    .saturating_add_signed(step)
                    .min(rows.len().saturating_sub(1)),
                None if step > 0 => 0,
                None => rows.len().saturating_sub(1),
            };
            self.moved = rows.get(to).copied();
            return;
        }
        if key.code == KeyCode::Enter
            && let Some(row) = self
                .moved
                .filter(|moved| Some(*moved) != self.engine_at)
                .and_then(|moved| menu.entries().get(moved).filter(|c| c.enabled).copied())
        {
            self.model.choose(&row.id);
            return;
        }
        if token(key).is_some_and(|t| self.model.press(&t)) {
            return;
        }
        // A menu the engine binds no confirm key in still acts on the row under its cursor.
        if key.code == KeyCode::Enter
            && let Some(row) = menu.entries().get(self.selected).filter(|c| c.enabled)
        {
            self.model.choose(&row.id);
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

    /// The mouse as the game reads it: the wheel steps an engine menu, hovering a map cell asks for its
    /// native description (or moves the engine's own cursor), the left click travels there (the first
    /// click shows the route), the right click is the engine's context click.
    pub fn handle_mouse(&mut self, ev: MouseEvent) {
        if component::placement(self.model.screens()).is_some() {
            if ev.kind == MouseEventKind::Down(MouseButton::Left) {
                self.components.click(&mut self.model, ev.column, ev.row);
            }
            return;
        }
        if self.model.loading().is_some() {
            return;
        }
        if ev.kind == MouseEventKind::Down(MouseButton::Left) && self.menu_click(ev) {
            return;
        }
        let wheel = match ev.kind {
            MouseEventKind::ScrollUp => Some(true),
            MouseEventKind::ScrollDown => Some(false),
            _ => None,
        };
        let over_footer = self
            .menu_hits
            .footer
            .contains(Position::new(ev.column, ev.row));
        if let Some(up) = wheel.filter(|_| over_footer) {
            self.footer.scroll_by(if up { -1 } else { 1 });
            return;
        }
        if let Some(up) = wheel {
            // Only a list the engine scrolls natively (it registers SCROLL_UP/SCROLL_DOWN) takes the
            // wheel; the highlight is the engine's own cursor, so any other menu ignores it, as native.
            self.model.scroll_menu(up);
            return;
        }
        let area = self.map_cells;
        let inside = (area.x..area.x + area.width).contains(&ev.column)
            && (area.y..area.y + area.height).contains(&ev.row);
        if !inside || self.model.menu().is_some() {
            // Off the map, or with a menu over it, the pointer is on no square.
            self.hovered = None;
            return;
        }
        let view = Pos {
            x: self.map_first.x + i32::from(ev.column - area.x),
            y: self.map_first.y + i32::from(ev.row - area.y),
        };
        match ev.kind {
            MouseEventKind::Moved => {
                self.hovered = Some(self.absolute(view));
                if !self.model.aim(view, false) {
                    self.model.hover(view);
                }
            }
            MouseEventKind::Down(MouseButton::Left) => {
                if !self.model.aim(view, true) {
                    // The engine plans the route; a repeat click walks it.
                    self.model.travel(view);
                }
            }
            MouseEventKind::Down(MouseButton::Right) => {
                self.model.context_click(view);
            }
            _ => {}
        }
    }

    /// A left click on a row or tab of the open menu chooses it, as the engine's own choose does.
    fn menu_click(&mut self, ev: MouseEvent) -> bool {
        let Some(menu) = self.model.menu() else {
            return false;
        };
        if menu.screen {
            return false;
        }
        let within = |r: Rect| {
            (r.x..r.x + r.width).contains(&ev.column) && (r.y..r.y + r.height).contains(&ev.row)
        };
        let hits = &self.menu_hits;
        let id = if within(hits.bar) {
            hits.tabs
                .iter()
                .find(|(_, from, to)| (*from..*to).contains(&(ev.column - hits.bar.x)))
                .map(|(id, ..)| id.clone())
        } else if within(hits.list) {
            hits.rows
                .get(usize::from(ev.row - hits.list.y))
                .filter(|id| !id.is_empty())
                .cloned()
        } else {
            None
        };
        match id {
            Some(id) => {
                self.model.choose(&id);
                true
            }
            None => false,
        }
    }

    pub fn draw(&mut self, f: &mut Frame) {
        if let Some(failure) = self.model.failure() {
            return Self::draw_failure(f, &failure);
        }
        let placement = component::placement(self.model.screens());
        if placement == Some(component::Placement::Full) {
            return self
                .components
                .draw(f, &mut self.model, component::Placement::Full);
        }
        if let Some(loading) = self.model.loading() {
            return Self::draw_loading(f, &loading);
        }
        let layout = self.model.sidebar_layout();
        let log_shown = layout.as_ref().is_none_or(native::Layout::has_log);
        let [main, status, log] = Layout::vertical([
            Constraint::Min(5),
            Constraint::Length(1),
            Constraint::Length(if log_shown { LOG_LINES as u16 + 2 } else { 0 }),
        ])
        .areas(f.area());
        let (width, left) = side_of(layout.as_ref());
        let width = width.min(main.width / 2);
        let [map, side] = match left {
            true => {
                let [side, map] =
                    Layout::horizontal([Constraint::Length(width), Constraint::Min(10)])
                        .areas(main);
                [map, side]
            }
            false => {
                Layout::horizontal([Constraint::Min(10), Constraint::Length(width)]).areas(main)
            }
        };
        self.components.set_sidebar(side, left);
        // The game's panels, else the status rows alone.
        let panels = match &layout {
            Some(l) => l.panels.clone(),
            None => vec![native::Panel {
                id: String::new(),
                label: "Status".into(),
                height: None,
                body: Body::Status(self.model.sidebar()),
            }],
        };
        let panels = panels
            .into_iter()
            .filter(|p| p.body != Body::Log && p.body != Body::Status(vec![]))
            .collect::<Vec<_>>();
        let heights = panels
            .iter()
            .map(|p| Constraint::Max(panel_lines(p) as u16 + 2));
        let areas = Layout::vertical(heights.chain([Constraint::Min(3), Constraint::Length(12)]))
            .split(side);
        let (actions, items) = (areas[panels.len()], areas[panels.len() + 1]);
        self.draw_map(f, map);
        for (panel, area) in panels.iter().zip(areas.iter()) {
            Self::draw_panel(f, *area, panel);
        }
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
        // Sounds the game did not log fade from light to dark gray as they age.
        let sounds = self.model.sounds().into_iter().map(|s| {
            let shade = (190.0 * s.alpha) as u8;
            Line::styled(s.text, Style::new().fg(Color::Rgb(shade, shade, shade)))
        });
        let all = native.chain(commands).chain(sounds).collect::<Vec<_>>();
        let end = all.len().saturating_sub(self.log_back.min(all.len()));
        let lines = all[end.saturating_sub(LOG_LINES)..end].to_vec();
        if log_shown {
            f.render_widget(
                Paragraph::new(lines).block(Block::bordered().title("Message log")),
                log,
            );
        }
        if let Some(menu) = self.menu() {
            self.draw_menu(f, &menu);
        }
        if let Some(row) = self.actions_open {
            self.draw_action_list(f, row);
        }
        if placement == Some(component::Placement::Over) {
            self.components
                .draw(f, &mut self.model, component::Placement::Over);
        }
    }

    /// The pressed command and its stage, then what the engine says of the hovered square and of the
    /// square under its own cursor.
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
        // What a target screen of the engine says: a zone's area step, the map editor's square and keys,
        // the look window's description of the square under the cursor.
        let note = self
            .model
            .aiming()
            .map(|note| format!(" | {note}"))
            .unwrap_or_default();
        // With the log panel off, the client's own results and errors are said here instead.
        let said = match self.model.sidebar_layout().is_some_and(|l| !l.has_log()) {
            true => self.model.messages().last().map(|m| format!(" | {m}")),
            false => None,
        };
        format!("{command}{hover}{note}{}", said.unwrap_or_default())
    }

    /// The engine is gone or silent: its last stderr lines and how to restart.
    fn draw_failure(f: &mut Frame, failure: &Failure) {
        let mut lines = vec![Line::from(failure.message.clone()), Line::from("")];
        if let Some(engine) = &failure.engine {
            lines.push(Line::from(format!("Engine: {}", engine.display())));
            lines.push(Line::from(""));
        }
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
        // The engine's own cursor (look, aiming, a zone's area) when it owns one.
        let cursor = self.model.focus();
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

    /// One panel of the sidebar under the engine's label.
    fn draw_panel(f: &mut Frame, area: Rect, panel: &native::Panel) {
        match &panel.body {
            Body::Status(rows) => Self::draw_status(f, area, &panel.label, rows),
            Body::Compass(directions) => {
                let lines = directions.iter().flat_map(|d| {
                    let creatures = d.creatures.iter().map(|c| {
                        let glyph = Span::styled(
                            c.cell.glyph.to_string(),
                            Style::new().fg(color(c.cell.color)),
                        );
                        Line::from(vec![
                            glyph,
                            Span::raw(format!(" {} {}", c.name, c.distance)),
                        ])
                    });
                    std::iter::once(Line::raw(d.label.as_str())).chain(creatures)
                });
                f.render_widget(
                    Paragraph::new(lines.collect::<Vec<_>>())
                        .block(Block::bordered().title(panel.label.as_str())),
                    area,
                );
            }
            Body::Log => {}
        }
    }

    /// The engine's sidebar rows, each span in its native colour.
    fn draw_status(f: &mut Frame, area: Rect, title: &str, rows: &[bn_client_core::sidebar::Line]) {
        let lines = rows.iter().map(|row| {
            let spans = row.iter().filter(|s| !s.text.is_empty());
            let spans = spans.flat_map(|s| {
                let runs = s.runs().into_iter();
                let runs = runs.map(|r| Span::styled(r.text, Style::new().fg(color(r.color))));
                std::iter::once(Span::raw(" "))
                    .chain(runs)
                    .collect::<Vec<_>>()
            });
            Line::from(spans.skip(1).collect::<Vec<_>>())
        });
        f.render_widget(
            Paragraph::new(lines.collect::<Vec<_>>()).block(Block::bordered().title(title)),
            area,
        );
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
            let row = |item: &String| {
                let runs = self.model.styled(item).into_iter();
                Line::from(
                    runs.map(|run| match run.color {
                        Some(c) => Span::styled(run.text, Style::new().fg(color(c))),
                        None => Span::raw(run.text),
                    })
                    .collect::<Vec<_>>(),
                )
            };
            std::iter::once(head)
                .chain(items.iter().map(row))
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
        let entries = menu.entries();
        // The focused action row, when the keyboard is on one, is the highlighted row.
        let selected = self
            .action_focus
            .and_then(|i| {
                entries
                    .iter()
                    .enumerate()
                    .filter(|(_, c)| c.id.starts_with("action:") && c.enabled)
                    .nth(i)
                    .map(|(row, _)| row)
            })
            .unwrap_or(self.selected);
        let rows = self.menu_rows(menu, &entries, selected);
        let sections = Sections::of(menu, &entries, rows);
        // What the engine says about the row its cursor is on.
        let described = entries
            .get(selected)
            .map(|c| plain(&c.description))
            .unwrap_or_default();
        self.footer.show(&described);
        let popup = Popup {
            title: &menu.title,
            header: plain(&menu.message)
                .lines()
                .map(str::to_string)
                .chain(self.settings_hint(menu))
                .collect(),
            tabs: menu
                .tabs()
                .iter()
                .map(|t| (plain(&t.label), t.selected))
                .collect(),
            selected: sections.at.get(selected).copied().unwrap_or(selected),
            rows: sections.rows,
            footer: described.lines().map(str::to_string).collect(),
            footer_scroll: self.footer.scroll,
        };
        let layout = Self::draw_popup(f, &popup);
        self.footer.fit(&layout);
        self.menu_hits = MenuHits {
            list: layout.list,
            rows: sections.ids.into_iter().skip(layout.offset).collect(),
            bar: layout.bar,
            footer: layout.footer,
            tabs: tab_hits(menu, &popup.tabs),
        };
    }

    /// Each entry's key column, text and style; the cursor row `selected` is plain text.
    fn menu_rows(&self, menu: &Menu, entries: &[&MenuChoice], selected: usize) -> Vec<PopupRow> {
        let actions = self.model.actions();
        let any_hotkey = entries.iter().any(|c| !c.hotkey.is_empty());
        entries
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
                // The row's values beside it, as the native lists print them.
                let columns = c.values_markup();
                // Chosen rows (an enabled panel or zone, an active mutation or mod) are lit white; the
                // native cursor row drops its colours.
                let style = match c.selected && menu.focus.is_some() {
                    true => style.fg(Color::White),
                    false => style,
                };
                // The key the game binds to an action row, as the native screens print their keys.
                let hint = bn_client_core::action_hint(&actions, &c.id)
                    .filter(|_| c.hotkey.is_empty())
                    .map(|key| format!("  [{key}]"))
                    .unwrap_or_default();
                let key = match (c.hotkey.as_str(), any_hotkey) {
                    ("", true) => "  ".to_string(),
                    ("", false) => String::new(),
                    (k, _) => format!("{k} "),
                };
                // An inventory row is drawn in its item's color, charges and all.
                let name = match c.color.as_str() {
                    "" => c.label.clone(),
                    color if c.enabled => format!(
                        "<color_{color}>{}</color>",
                        Some(c.markup.as_str())
                            .filter(|m| !m.is_empty())
                            .unwrap_or(&c.label)
                    ),
                    _ => c.label.clone(),
                };
                let text = format!("{name}{columns}{hint}{denial}");
                // The cursor row is drawn whole in one colour, so it loses the markup.
                let spans = match i == selected {
                    true => vec![Span::raw(plain(&text))],
                    false => self
                        .model
                        .styled(&text)
                        .into_iter()
                        .map(|run| {
                            let fg = run.color.map(|c| Style::new().fg(color(c)));
                            Span::styled(run.text, fg.unwrap_or_default())
                        })
                        .collect(),
                };
                (key, spans, style)
            })
            .collect()
    }

    fn draw_action_list(&mut self, f: &mut Frame, row: usize) {
        let rows = self
            .model
            .actions()
            .iter()
            .map(|a| {
                (
                    String::new(),
                    vec![Span::raw(action_label(a))],
                    Style::new(),
                )
            })
            .collect();
        let popup = Popup {
            title: "Actions",
            header: vec![],
            tabs: vec![],
            rows,
            selected: row,
            footer: vec![],
            footer_scroll: 0,
        };
        Self::draw_popup(f, &popup);
    }

    /// A centred box: message lines, a tab bar, then the rows scrolled to keep the highlighted one visible.
    fn draw_popup(f: &mut Frame, popup: &Popup) -> PopupLayout {
        let area = f.area();
        let tab_bar = bar(&popup.tabs);
        let widest = popup
            .header
            .iter()
            .map(|l| l.width())
            .chain(
                popup
                    .rows
                    .iter()
                    .map(|(k, l, _)| k.width() + Line::from(l.clone()).width() + 2),
            )
            .chain(std::iter::once(tab_bar.width()))
            .max()
            .unwrap_or(0);
        let width = (widest as u16 + 2).clamp(30, 100).min(area.width);
        // The text inside the border; none in a terminal too narrow for the border alone.
        let inside = usize::from(width.saturating_sub(2));
        let header = popup
            .header
            .iter()
            .flat_map(|l| wrap(l, inside))
            .collect::<Vec<_>>();
        let head = (header.len() as u16).min(area.height.saturating_sub(2) / 2);
        let bar = u16::from(!popup.tabs.is_empty());
        // The native text window draws a line of two dashes as a rule across the window.
        let footer = popup
            .footer
            .iter()
            .flat_map(|l| match l.as_str() {
                "--" => vec!["─".repeat(inside)],
                _ => wrap(l, inside),
            })
            .collect::<Vec<_>>();
        // The description gets the room it needs, down to a few rows of the list; the rest scrolls.
        let room = area.height.saturating_sub(2 + head + bar);
        let keep = (popup.rows.len() as u16).min(3);
        let foot = (footer.len() as u16).min(room.saturating_sub(keep));
        let want = head + bar + popup.rows.len() as u16 + foot + 2;
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
        let [header_area, bar_area, list, footer_area] = Layout::vertical([
            Constraint::Length(head),
            Constraint::Length(bar),
            Constraint::Min(0),
            Constraint::Length(foot),
        ])
        .areas(inner);
        let header_lines = header;
        let text = |s: &String| Line::raw(s.clone());
        f.render_widget(
            Paragraph::new(header_lines.iter().map(text).collect::<Vec<_>>()),
            header_area,
        );
        f.render_widget(Paragraph::new(tab_bar), bar_area);
        let footer_scroll = popup
            .footer_scroll
            .min(footer.len().saturating_sub(usize::from(foot)));
        let offset = (popup.selected + 1).saturating_sub(usize::from(list.height));
        let lines = popup
            .rows
            .iter()
            .enumerate()
            .skip(offset)
            .map(|(i, (key, label, style))| {
                // The native key column in light green; the cursor row is reversed whole.
                let on = i == popup.selected;
                let green = match on {
                    true => Style::new(),
                    false => Style::new().fg(Color::LightGreen),
                };
                let line = Line::from(
                    [
                        Span::raw(if on { "> " } else { "  " }),
                        Span::styled(key.clone(), green),
                    ]
                    .into_iter()
                    .chain(label.clone())
                    .collect::<Vec<_>>(),
                );
                match on {
                    true => line.style(style.add_modifier(Modifier::REVERSED)),
                    false => line.style(*style),
                }
            })
            .collect::<Vec<_>>();
        f.render_widget(Paragraph::new(lines), list);
        f.render_widget(
            Paragraph::new(
                footer
                    .iter()
                    .skip(footer_scroll)
                    .map(|l| Line::styled(l.clone(), Style::new().fg(Color::Gray)))
                    .collect::<Vec<_>>(),
            ),
            footer_area,
        );
        PopupLayout {
            list,
            offset,
            bar: bar_area,
            footer: footer_area,
            footer_total: footer.len(),
            footer_scroll,
        }
    }
}

/// A popup row: its native key (empty or padded), label and style.
type PopupRow = (String, Vec<Span<'static>>, Style);

/// A menu's rows with a heading over each named pane it lists in one list: `ids` keeps a click on a drawn
/// row mapped to its entry, and `at` is the drawn row of each entry.
struct Sections {
    rows: Vec<PopupRow>,
    ids: Vec<String>,
    at: Vec<usize>,
}

impl Sections {
    fn of(menu: &Menu, entries: &[&MenuChoice], rows: Vec<PopupRow>) -> Self {
        let mut s = Self {
            rows: vec![],
            ids: vec![],
            at: vec![],
        };
        for (i, (row, entry)) in rows.into_iter().zip(entries).enumerate() {
            if let Some(heading) = menu.heading(i.checked_sub(1).map(|p| entries[p]), entry) {
                let bold = Style::new().add_modifier(Modifier::BOLD);
                s.rows.push((
                    String::new(),
                    vec![Span::styled(heading.to_string(), bold)],
                    bold,
                ));
                s.ids.push(String::new());
            }
            s.at.push(s.rows.len());
            s.rows.push(row);
            s.ids.push(entry.id.clone());
        }
        s
    }
}

/// Where each tab of a menu's bar is drawn: its id and the columns it spans from the bar's left edge.
fn tab_hits(menu: &Menu, labels: &[(String, bool)]) -> Vec<(String, u16, u16)> {
    let widths = labels.iter().map(|(label, _)| label.width() as u16 + 3);
    let mut from = 0;
    menu.tabs()
        .into_iter()
        .zip(widths)
        .map(|(t, w)| {
            from += w;
            (t.id.clone(), from - w, from - 1)
        })
        .collect()
}

/// Where `draw_popup` put the rows and the tab bar.
struct PopupLayout {
    list: Rect,
    /// Index of the first row drawn.
    offset: usize,
    bar: Rect,
    footer: Rect,
    /// Lines the whole description wraps to, and how many of them are scrolled off the top.
    footer_total: usize,
    footer_scroll: usize,
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
    /// Each row's native key (empty or padded), label and style.
    rows: Vec<PopupRow>,
    selected: usize,
    /// Detail text under the rows, such as the highlighted row's description.
    footer: Vec<String>,
    /// How many lines of the footer are scrolled off the top.
    footer_scroll: usize,
}

fn action_label(a: &ActionEntry) -> String {
    match a.keys.is_empty() {
        true => a.name.clone(),
        false => format!("{} [{}]", a.name, a.keys.join(", ")),
    }
}
