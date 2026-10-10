use crate::colors::ColorMap;
use crate::markup::{self, Run};

use crate::protocol::Editor;
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
#[derive(Clone, Debug, Default, PartialEq, Eq)]
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
    /// What stands in for the sprite when the tileset has none; NUL for an overlay, which then draws nothing.
    pub glyph: char,
    pub color: Rgb,
    pub bg: Option<Rgb>,
}

#[derive(Default)]
pub struct Cell {
    pub glyph: char,
    pub color: Rgb,
    /// Background behind the glyph, when the game colour has one.
    pub bg: Option<Rgb>,
    pub desc: String,
    /// What is drawn here from the bottom up; empty when the model offers no sprites.
    pub layers: Vec<Layer>,
    /// Only remembered, not in sight.
    pub remembered: bool,
    /// The game's light level of a square in sight: 0 dark, 1 low, 2 bright only, 3 lit, 4 bright.
    pub light: Option<u8>,
    /// The game's per-square seed for weighted sprite variants, also for squares nothing is known about.
    pub square: u32,
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

/// The engine's current interaction, rendered generically.
#[derive(Clone, Debug, PartialEq)]
pub struct Menu {
    /// The engine's name for the kind of menu, such as `CRAFTING`.
    pub context: String,
    pub title: String,
    pub message: String,
    pub choices: Vec<MenuChoice>,
    pub can_cancel: bool,
    pub can_set_count: bool,
    pub field: Option<MenuField>,
    /// The pane of the selected tab, when the menu has tabs.
    pub pane: Option<String>,
    /// The row the engine's own cursor is on, which its navigation keys move.
    pub focus: Option<String>,
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
    /// `text` split at its colour tags, coloured with the game's palette.
    fn styled(&self, text: &str) -> Vec<Run> {
        markup::runs(text, &ColorMap::default())
    }
    fn choose(&mut self, _choice_id: &str) {}
    fn cancel_menu(&mut self) {}
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
    /// The game's own message log, oldest first.
    fn native_log(&self) -> &[LogLine] {
        &[]
    }
    fn say(&mut self, text: String);
}

pub struct FakeModel {
    /// Map size; tests shrink it to emulate loading or a replaced map.
    pub size: Pos,
    player: Pos,
    ground: Vec<String>,
    inventory: Vec<String>,
    messages: Vec<String>,
}

impl Default for FakeModel {
    fn default() -> Self {
        Self {
            size: Pos { x: 40, y: 25 },
            player: Pos { x: 5, y: 5 },
            ground: vec!["Rusty knife".into(), "Tin can".into()],
            inventory: vec!["Flashlight".into()],
            messages: vec!["Welcome to the prototype.".into()],
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
