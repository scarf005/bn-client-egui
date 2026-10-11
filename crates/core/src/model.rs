use crate::colors::ColorMap;
use crate::markup::{self, Run};

use crate::protocol::{Editor, ModTileset};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Pos {
    pub x: i32,
    pub y: i32,
}

/// 8-bit sRGB colour.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Rgb(pub u8, pub u8, pub u8);

/// One thing drawn in a cell, with what a tileset needs to pick its sprite the way the game does and
/// the glyph that stands in for it when the tileset has none.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Layer {
    /// Tile id to look up first.
    pub id: String,
    /// The game data's fallback ids, nearest first.
    pub fallbacks: Vec<String>,
    /// Multitile key the engine's tile selection chose.
    pub subtile: Option<String>,
    /// Quarter turns of the engine's tile selection: 0 north, 1 west, 2 south, 3 east.
    pub rotation: u8,
    /// A creature faces left; its sprite is mirrored.
    pub mirrored: bool,
    /// Whether the thing is a character or monster, whose sprites keep their facing.
    pub creature: bool,
    /// Picks between the weighted variants of a sprite, like the game's per-tile seed.
    pub seed: u32,
    /// Paint the tileset blends into the background and the foreground sprite.
    pub tint_bg: Option<Rgb>,
    pub tint_fg: Option<Rgb>,
    /// Set on the overlay of a mutation, whose paint the tileset picks from the character's mutations.
    pub mutation: Option<Mutation>,
    /// The ids a tileset's tints for an item, bionic or effect overlay are keyed by, first match first.
    pub tint_keys: Vec<String>,
    /// The tint an item's own variables ask for, over what its keys give.
    pub tint_vars: Option<crate::protocol::TintVars>,
    /// What stands in for the sprite when the tileset has none; NUL for an overlay, which then draws nothing.
    pub glyph: char,
    pub color: Rgb,
    pub bg: Option<Rgb>,
}

/// What a tileset needs to paint the sprite of a mutation with the character's hair, skin or eye color.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Mutation {
    pub id: String,
    pub types: Vec<String>,
    pub flags: Vec<String>,
    /// The ids of the character's mutations by mutation type.
    pub traits: std::sync::Arc<std::collections::BTreeMap<String, Vec<String>>>,
}

#[derive(Default)]
pub struct Cell {
    pub glyph: char,
    pub color: Rgb,
    /// Background behind the glyph, when the game colour has one.
    pub bg: Option<Rgb>,
    /// What a reader or a tooltip says: the display name when the engine sends one, else the id.
    pub desc: String,
    /// The data id of the topmost look (`t_wall`), stable under translation; empty when nothing is known.
    /// Code that decides by what is on a square matches this, never `desc`.
    pub id: String,
    /// What is drawn here from the bottom up; empty when the model offers no sprites.
    pub layers: Vec<Layer>,
    /// Only remembered, not in sight.
    pub remembered: bool,
    /// The game's light level of a square in sight: 0 dark, 1 low, 2 bright only, 3 lit, 4 bright.
    pub light: Option<u8>,
    /// How many levels down the layers lie, seen through open air; 0 on the avatar's own level.
    pub depth: u8,
    /// The game's per-square seed for weighted sprite variants, also for squares nothing is known about.
    pub square: u32,
}

/// One short animation the native game would play on the map, in view coordinates.
#[derive(Clone, Debug, PartialEq)]
pub struct Effect {
    pub shape: Shape,
    /// How long the native animation holds it.
    pub duration_ms: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Shape {
    /// A shot flying along `path`; a tiles client looks `sprite` up first.
    Projectile {
        path: Vec<Pos>,
        glyph: char,
        sprite: Option<String>,
        color: Rgb,
    },
    /// An explosion starting at `at` and reaching `radius` squares.
    Burst {
        at: Pos,
        radius: u32,
        color: Rgb,
        /// The tile id the native tiles draw it with.
        tile: String,
    },
    /// The squares one step of an explosion blasts, or its shrapnel lands on.
    Blast {
        cells: Vec<Pos>,
        shrapnel: bool,
        tile: String,
    },
    /// Floating text at `at`, in colored segments, moving by `scroll` each animation step as the
    /// native text does; `hp` marks a creature's hit-point readout, which stays beside it.
    Text {
        at: Pos,
        segments: Vec<(String, Rgb)>,
        scroll: Pos,
        hp: bool,
    },
}

/// One line of the game's own message log. A repeat of the newest line keeps its `id` and raises
/// `count`; the client shows it as "text x2" in place of the earlier line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LogLine {
    /// The engine's identity of the line; empty when the engine sent none.
    pub id: String,
    pub text: String,
    /// The game's colour for the message type.
    pub color: Option<Rgb>,
    pub count: u32,
}

impl LogLine {
    /// The line as the game prints it, with the repeat count.
    pub fn shown(&self) -> String {
        if self.count > 1 {
            format!("{} x{}", self.text, self.count)
        } else {
            self.text.clone()
        }
    }
}

/// One selectable row of the engine's current interaction menu.
#[derive(Clone, Debug, PartialEq)]
pub struct MenuChoice {
    pub id: String,
    pub label: String,
    pub enabled: bool,
    /// Why the row cannot be chosen; empty when enabled.
    pub denial: String,
    /// The key that chooses the row natively, printed before the label; empty when it has none.
    pub hotkey: String,
    /// The engine's committed selection, such as the current tab.
    pub selected: bool,
    /// The tab this entry belongs to; tab rows and plain list rows have none.
    pub group: Option<String>,
    /// A tab header of the menu rather than one of its entries.
    pub tab: bool,
    /// The native cursor is on this row.
    pub highlighted: bool,
    /// How many of the row are chosen, and how many there are, for rows that take a count.
    pub selected_count: Option<u64>,
    pub available_count: Option<u64>,
    /// The engine's detail text for the row; empty when it has none.
    pub description: String,
    /// Label and value pairs shown beside the row.
    pub columns: Vec<(String, String)>,
    /// How the row's value can be set directly, as an option of the options menu.
    pub editor: Option<Editor>,
}

/// The text field of a menu, such as the crafting search.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MenuField {
    pub id: String,
    pub label: String,
    pub value: String,
    /// `text`, `integer`, or `key` for a prompt that takes the next key press.
    pub kind: String,
}

/// One pane of a two-pane menu such as the advanced inventory: where it looks and how it is filtered.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MenuPane {
    pub id: String,
    pub label: String,
    /// `source` for the pane the engine acts on, `destination` for the other.
    pub role: String,
    /// The name of the area the pane shows, as the native screen prints it.
    pub area: String,
    pub storage: String,
    /// The line under the pane's title, such as a trade pane's free capacity; empty when none.
    pub description: String,
    pub filter: String,
    /// The native sort name, such as `name`; empty when the engine sent none.
    pub sort: String,
    /// The header totals the native pane prints; absent from engines that predate them.
    pub totals: Option<PaneTotals>,
    /// Actions of the areas this pane cannot put items in; the native picker draws them red.
    pub blocked: Vec<String>,
    /// The native page indicator as (number, count) from 1; absent for a list that does not page.
    pub page: Option<(u64, u64)>,
}

/// The weight and volume line of a pane header: `weight`/`volume` against an optional maximum.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PaneTotals {
    pub weight: String,
    /// Empty where the native header shows no weight capacity.
    pub weight_max: String,
    pub weight_unit: String,
    pub volume: String,
    /// Empty where the native header shows no volume capacity.
    pub volume_max: String,
    pub volume_unit: String,
    pub over_weight: bool,
    pub over_volume: bool,
}

impl PaneTotals {
    /// `weight[/max] unit`, as the native header prints it.
    pub fn weight_text(&self) -> String {
        Self::text(&self.weight, &self.weight_max, &self.weight_unit)
    }

    /// `volume[/max] unit`, as the native header prints it.
    pub fn volume_text(&self) -> String {
        Self::text(&self.volume, &self.volume_max, &self.volume_unit)
    }

    fn text(value: &str, max: &str, unit: &str) -> String {
        match max.is_empty() {
            true => format!("{value} {unit}"),
            false => format!("{value}/{max} {unit}"),
        }
    }
}

/// The engine's current interaction, rendered generically.
#[derive(Clone, Debug, PartialEq)]
pub struct Menu {
    /// The engine's name for the kind of menu, such as `CRAFTING`.
    pub context: String,
    pub title: String,
    pub message: String,
    /// `good` or `bad` where the native screen colors the message line; empty otherwise.
    pub message_tone: String,
    /// Whether the native category-selection mode is on; absent where the screen has none.
    pub category_mode: Option<bool>,
    /// Lines drawn above the menu in a fixed-width font, with colour tags.
    pub banner: Vec<String>,
    pub choices: Vec<MenuChoice>,
    /// The panes of a two-pane menu; empty for other menus.
    pub panes: Vec<MenuPane>,
    /// How many choices the engine has in all; more than `choices.len()` until the rest is fetched.
    pub total: usize,
    pub can_cancel: bool,
    pub can_set_count: bool,
    pub field: Option<MenuField>,
    /// The pane of the selected tab, when the menu has tabs.
    pub pane: Option<String>,
    /// The row the engine's own cursor is on, which its navigation keys move.
    pub focus: Option<String>,
    /// The engine's composed screen of a view it has no structured interaction for: only text to
    /// show, driven by the keys of the view's actions.
    pub screen: bool,
}

impl Menu {
    /// The tab headers of a tabbed menu; empty when the menu has none.
    pub fn tabs(&self) -> Vec<&MenuChoice> {
        self.choices.iter().filter(|c| c.tab).collect()
    }

    /// The rows below the tabs: the selected tab's entries, or every row when there are no tabs.
    pub fn entries(&self) -> Vec<&MenuChoice> {
        self.choices
            .iter()
            .filter(|c| !c.tab && (self.pane.is_none() || c.group == self.pane))
            .collect()
    }

    /// The only way out is the engine's own acknowledge row, so a separate Cancel would duplicate it.
    pub fn acknowledges(&self) -> bool {
        self.choices.iter().any(|c| c.id == "acknowledge")
    }
}

/// A registered action the engine currently accepts, with the keys the game binds to it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActionEntry {
    pub id: String,
    pub name: String,
    pub keys: Vec<String>,
}

/// The key the game binds to the action a menu row stands for (`action:NEXT_TAB`), shown as a hint
/// beside the row as the native screens print their keys; `None` for any other row or an unbound action.
pub fn action_hint(actions: &[ActionEntry], row_id: &str) -> Option<String> {
    let id = row_id.strip_prefix("action:")?;
    actions
        .iter()
        .find(|a| a.id == id)
        .and_then(|a| a.keys.first().cloned())
}

/// The command in flight, or the last one's outcome, for the status line and the pressed control.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommandStatus {
    /// What the user asked for: a choice or action name, `Travel`, `Cancel`.
    pub label: String,
    /// Id of the choice or action that was pressed, to mark it pending.
    pub target: Option<String>,
    /// `sent`, `accepted`, `executing`, `done`, or the failure in words.
    pub stage: String,
    /// The command reached a terminal stage.
    pub finished: bool,
    /// Every stage the command went through, oldest first, so ones that pass within a frame are kept.
    pub history: Vec<String>,
}

impl CommandStatus {
    pub fn set_stage(&mut self, stage: String, finished: bool) {
        self.history.push(stage.clone());
        self.stage = stage;
        self.finished = finished;
    }
}

/// Engine loading screen: a title, the steps seen so far and the one in progress.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Loading {
    pub title: String,
    pub entries: Vec<String>,
    pub index: usize,
    /// Splash image, relative to the game root.
    pub image: Option<String>,
}

/// One overmap square as the native screen draws it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OvermapCell {
    pub glyph: String,
    pub color: Rgb,
    pub bg: Option<Rgb>,
}

/// What the tiles view draws on an overmap square.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OvermapTile {
    pub id: String,
    pub rotation: u8,
    /// The multitile key, empty for a plain tile.
    pub subtile: String,
    /// Tile ids to try when the tileset has no sprite for `id`, nearest first.
    pub fallbacks: Vec<String>,
}

/// The overmap screen (`m`): the native window's cells and sidebar, with the cursor.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Overmap {
    pub cols: i32,
    pub rows: i32,
    /// Row-major, `cols * rows` cells.
    pub cells: Vec<OvermapCell>,
    /// Absolute overmap square of cell (0, 0), the cursor and the player, as `(x, y, z)`.
    pub origin: (i32, i32, i32),
    pub cursor: (i32, i32, i32),
    pub player: (i32, i32, i32),
    pub legend: Vec<String>,
    pub notes: Vec<((i32, i32, i32), String)>,
    /// The planned travel route, nearest square first.
    pub path: Vec<(i32, i32, i32)>,
    /// Row-major like `cells`; empty unless the client said it draws tiles.
    pub tiles: Vec<OvermapTile>,
}

impl Overmap {
    pub fn cell(&self, col: i32, row: i32) -> Option<&OvermapCell> {
        let inside = (0..self.cols).contains(&col) && (0..self.rows).contains(&row);
        inside.then(|| &self.cells[(row * self.cols + col) as usize])
    }

    /// The absolute overmap square drawn in view cell `(col, row)`.
    pub fn square(&self, col: i32, row: i32) -> (i32, i32, i32) {
        (self.origin.0 + col, self.origin.1 + row, self.origin.2)
    }

    /// The cursor square as the native sidebar describes it: its first paragraph, without the window
    /// frame and the leading glyph of the square.
    pub fn here(&self) -> Option<String> {
        let lines = self
            .legend
            .iter()
            .map(|l| l.trim_start_matches(['│', '|']).trim())
            .skip_while(|l| l.is_empty())
            .take_while(|l| !l.is_empty())
            .collect::<Vec<_>>()
            .join(" ");
        let text = match lines.split_once(' ') {
            Some((glyph, rest))
                if glyph.chars().count() == 1 && !glyph.chars().all(char::is_alphanumeric) =>
            {
                rest
            }
            _ => &lines,
        };
        (!text.is_empty()).then(|| text.to_string())
    }

    /// Every note in view, for a reader that cannot see the markers.
    pub fn notes_text(&self) -> Option<String> {
        let notes = self
            .notes
            .iter()
            .map(|(p, t)| format!("({}, {}, {}) {t}", p.0, p.1, p.2))
            .collect::<Vec<_>>();
        (!notes.is_empty()).then(|| format!("Notes: {}.", notes.join("; ")))
    }

    /// The tile of view cell `(col, row)`, when the engine sent tile ids.
    pub fn tile(&self, col: i32, row: i32) -> Option<&OvermapTile> {
        let inside = (0..self.cols).contains(&col) && (0..self.rows).contains(&row);
        self.tiles.get(
            usize::try_from(row * self.cols + col)
                .ok()
                .filter(|_| inside)?,
        )
    }

    /// The note on this square, if any.
    pub fn note(&self, at: (i32, i32, i32)) -> Option<&str> {
        self.notes
            .iter()
            .find(|(p, _)| *p == at)
            .map(|(_, t)| t.as_str())
    }
}

/// Why the engine cannot be played right now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FailureKind {
    /// The engine process ended or closed its output.
    Exited,
    /// A request has been outstanding longer than the watchdog allows.
    Stalled,
}

/// The engine's failure, with its last stderr lines, for an error screen with a Restart button.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Failure {
    pub kind: FailureKind,
    pub message: String,
    pub stderr: Vec<String>,
}

/// What the UI needs from the game: `FakeModel` in tests, `EngineModel` against a real engine.
pub trait GameModel {
    /// Called once per frame: drain engine messages. No-op for synchronous models.
    fn poll(&mut self) {}
    /// Registers a callback the model runs (from any thread) when new engine data has arrived.
    fn set_waker(&mut self, _waker: crate::transport::Waker) {}
    /// The map size, in cells, the UI has room for; models that can show more or less adapt.
    fn set_viewport(&mut self, _cells: Pos) {}
    /// Absolute map position of view cell (0, 0); lets a walk target survive the view scrolling.
    fn origin(&self) -> Pos {
        Pos { x: 0, y: 0 }
    }
    /// True while the model keeps changing on its own and needs periodic repaints.
    fn live(&self) -> bool {
        false
    }
    /// True while a submitted command has not finished; input is ignored until then.
    fn busy(&self) -> bool {
        false
    }
    /// The command in flight or the last one's outcome; none before the first submit.
    fn command(&self) -> Option<CommandStatus> {
        None
    }
    /// Actions the engine accepts right now.
    fn actions(&self) -> Vec<ActionEntry> {
        vec![]
    }
    /// Runs the action with this id; false if it is not offered or the model is busy.
    fn invoke_action(&mut self, _id: &str) -> bool {
        false
    }
    /// Asks the engine to interrupt the activity a command is running (wait, craft, ...), as the
    /// native interrupt key does. False when no command is in flight.
    fn interrupt(&mut self) -> bool {
        false
    }
    /// The game key would run an action if the model were not busy; the UI queues such keys meanwhile.
    fn key_bound(&self, _key: &str) -> bool {
        false
    }
    /// A game key token (`g`, `SPACE`, `UP`); true if it ran an action.
    fn press(&mut self, _key: &str) -> bool {
        false
    }
    /// Left click on view cell `at`: the model's own travel (plan a route, repeat to walk it).
    /// False when the model has none, so the UI walks by itself.
    fn travel(&mut self, _at: Pos) -> bool {
        false
    }
    /// The planned route in view coordinates, nearest first; empty when none.
    fn route(&self) -> Vec<Pos> {
        vec![]
    }
    /// The engine's `USE_TILES` and `TILES` options, when it publishes them: the map mode a new profile starts in.
    fn display_default(&self) -> Option<(bool, String)> {
        None
    }
    /// Says whether the client draws tiles now, so the engine sends tile ids where it has them.
    fn set_tiles(&mut self, _tiles: bool) {}
    /// Gives the ids of the tints the client's tileset has, so the engine publishes the `tint_keys` among them.
    fn set_tint_ids(&mut self, _ids: &[String]) {}
    /// The overmap screen, while it is open.
    fn overmap(&self) -> Option<Overmap> {
        None
    }
    /// Moves the overmap cursor to an absolute overmap square.
    fn overmap_cursor_to(&mut self, _at: (i32, i32, i32)) {}
    /// Leaves the overmap screen.
    fn close_overmap(&mut self) {}
    /// Set while the engine has exited or stopped answering; the UI shows it instead of the map.
    fn failure(&self) -> Option<Failure> {
        None
    }
    /// Starts the engine again on the same profile; false when the model cannot.
    fn restart(&mut self) -> bool {
        false
    }
    /// The engine's loading screen, while it is up.
    fn loading(&self) -> Option<Loading> {
        None
    }
    fn menu(&self) -> Option<Menu> {
        None
    }
    /// Where the engine's own cursor is (look mode), in view coordinates; none when it has no cursor.
    fn focus(&self) -> Option<Pos> {
        None
    }
    /// What the engine's aiming screen shows beside the cursor: its title, the cursor's distance
    /// against the range, and whether a shot can be taken; none outside aiming.
    fn aiming(&self) -> Option<String> {
        None
    }
    /// `text` split at its colour tags, coloured with the game's palette.
    fn styled(&self, text: &str) -> Vec<Run> {
        markup::runs(text, &ColorMap::default())
    }
    fn choose(&mut self, _choice_id: &str) {}
    /// Asks for the native description of the hovered view square; it arrives with later polls.
    fn hover(&mut self, _at: Pos) {}
    /// The native description lines of view square `at`, once the engine has answered for it.
    fn hover_text(&self, _at: Pos) -> Option<Vec<String>> {
        None
    }
    /// The model answers a right click itself, so the client offers no menu of its own.
    fn native_context(&self) -> bool {
        false
    }
    /// The right click on a view square, as the engine's own SEC_SELECT; false when the model has none.
    fn context_click(&mut self, _at: Pos) -> bool {
        false
    }
    /// The engine's cursor (look mode, aiming) moves to view square `at`, or confirms it when already there.
    fn aim(&mut self, _at: Pos) -> bool {
        false
    }
    /// Whether the open menu scrolls by the mouse wheel in the engine, as a native list does.
    fn can_scroll_menu(&self) -> bool {
        false
    }
    /// One wheel step in the open menu; `up` moves the engine's cursor three rows up.
    fn scroll_menu(&mut self, _up: bool) -> bool {
        false
    }
    /// Fetches the next page of a menu that has more choices than it lists.
    fn more_choices(&mut self) {}
    fn cancel_menu(&mut self) {}
    /// The files of game data that add sprites to the tileset the game draws, in the engine's load order.
    fn mod_tilesets(&self) -> &[ModTileset] {
        &[]
    }
    /// Season of the game calendar (`spring`, `summer`, `autumn`, `winter`), which picks seasonal sprites.
    fn season(&self) -> Option<String> {
        None
    }
    /// Fills the menu's text field; `submit` applies it (search) rather than only editing it.
    fn fill(&mut self, _field_id: &str, _value: &str, _submit: bool) {}
    /// Chooses a row with a count, such as a batch craft.
    fn set_count(&mut self, _choice_id: &str, _count: u64) {}
    fn size(&self) -> Pos;
    fn cell(&self, at: Pos) -> Cell;
    fn player(&self) -> Pos;
    /// Moves the player one step toward `target` (diagonals allowed); false if blocked.
    fn step_toward(&mut self, target: Pos) -> bool;
    fn ground(&self) -> &[String];
    fn inventory(&self) -> &[String];
    fn pick_up(&mut self, index: usize);
    fn drop_item(&mut self, index: usize);
    /// Client-side lines: command results and errors.
    fn messages(&self) -> &[String];
    /// The avatar's sidebar as coloured lines; empty before the engine reports one.
    fn sidebar(&self) -> Vec<crate::sidebar::Line> {
        vec![]
    }
    /// The game's own message log, oldest first.
    fn native_log(&self) -> &[LogLine] {
        &[]
    }
    fn say(&mut self, text: String);
    /// The map animations produced since the last call; models without a game have none.
    fn take_effects(&mut self) -> Vec<Effect> {
        vec![]
    }
}

pub struct FakeModel {
    /// Map size; tests shrink it to emulate loading or a replaced map.
    pub size: Pos,
    player: Pos,
    ground: Vec<String>,
    inventory: Vec<String>,
    messages: Vec<String>,
    effects: Vec<Effect>,
}

impl FakeModel {
    /// An animation the next `take_effects` hands out.
    pub fn queue_effect(&mut self, effect: Effect) {
        self.effects.push(effect);
    }
}

impl Default for FakeModel {
    fn default() -> Self {
        Self {
            size: Pos { x: 40, y: 25 },
            player: Pos { x: 5, y: 5 },
            ground: vec!["Rusty knife".into(), "Tin can".into()],
            inventory: vec!["Flashlight".into()],
            messages: vec!["Welcome to the prototype.".into()],
            effects: vec![],
        }
    }
}

impl GameModel for FakeModel {
    fn actions(&self) -> Vec<ActionEntry> {
        let action = |id: &str, name: &str, keys: &[&str]| ActionEntry {
            id: id.into(),
            name: name.into(),
            keys: keys.iter().map(|k| k.to_string()).collect(),
        };
        vec![
            action("look", "Look around", &[";", "x"]),
            action("UP", "Move up", &["UP", "k"]),
            action("DOWN", "Move down", &["DOWN", "j"]),
            action("LEFT", "Move left", &["LEFT", "h"]),
            action("RIGHT", "Move right", &["RIGHT", "l"]),
            action("LEFTUP", "Move up left", &["HOME", "y"]),
            action("RIGHTUP", "Move up right", &["PPAGE", "u"]),
            action("LEFTDOWN", "Move down left", &["END", "b"]),
            action("RIGHTDOWN", "Move down right", &["NPAGE", "n"]),
        ]
    }

    fn take_effects(&mut self) -> Vec<Effect> {
        std::mem::take(&mut self.effects)
    }

    fn size(&self) -> Pos {
        self.size
    }

    fn cell(&self, at: Pos) -> Cell {
        if at.x == 20 && at.y != 12 {
            return Cell {
                glyph: '#',
                color: Rgb(180, 180, 180),
                desc: "a wall".into(),
                ..Cell::default()
            };
        }
        let (glyph, color, desc) = match (at.x * 7 + at.y * 13) % 11 {
            0 => ('T', Rgb(40, 160, 60), "a tree"),
            1 => ('~', Rgb(70, 120, 220), "shallow water"),
            _ => ('.', Rgb(130, 130, 110), "open grass"),
        };
        Cell {
            glyph,
            color,
            desc: desc.into(),
            ..Cell::default()
        }
    }

    fn player(&self) -> Pos {
        self.player
    }

    fn step_toward(&mut self, target: Pos) -> bool {
        let next = Pos {
            x: self.player.x + (target.x - self.player.x).signum(),
            y: self.player.y + (target.y - self.player.y).signum(),
        };
        let blocked = self.cell(next).glyph == '#';
        if !blocked {
            self.player = next;
        }
        !blocked
    }

    fn ground(&self) -> &[String] {
        &self.ground
    }

    fn inventory(&self) -> &[String] {
        &self.inventory
    }

    fn pick_up(&mut self, index: usize) {
        if index < self.ground.len() {
            self.inventory.push(self.ground.remove(index));
        }
    }

    fn drop_item(&mut self, index: usize) {
        if index < self.inventory.len() {
            self.ground.push(self.inventory.remove(index));
        }
    }

    fn messages(&self) -> &[String] {
        &self.messages
    }

    fn say(&mut self, text: String) {
        self.messages.push(text);
    }
}
