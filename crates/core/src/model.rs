use crate::colors::ColorMap;
use crate::interaction::InvokeError;
use crate::markup::{self, Run};

use crate::protocol::{
    Column, Component, Editor, InvokeArgs, ItemList, List, Look, Mark, ModTileset, Row, Screen,
    ScreenRole, Shortcut, TileAction,
};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Pos {
    pub x: i32,
    pub y: i32,
}

/// 8-bit sRGB colour.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Rgb(pub u8, pub u8, pub u8);

/// Coloured light the game tints a square in sight with: the light's colour and its strength, 1 to 255.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Glow {
    pub color: Rgb,
    pub strength: u8,
}

/// How hurt the creature on a square is: the fifths of the game's health bar and its colour.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Health {
    pub fifths: u8,
    pub color: Rgb,
}

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
    /// Whether this is the terrain of its square, which a 3D view draws as ground or as a wall.
    pub terrain: bool,
    /// Terrain that stands as a wall (a wall, a closed door or a window), which a 3D view raises.
    pub wall: bool,
    /// Terrain that is a window, which a 3D view opens between a sill and a lintel.
    pub window: bool,
    /// How much of a person the thing covers, in percent, which a 3D view raises it by; 0 when it lies flat.
    pub coverage: u8,
    /// Whether the thing is a part of a vehicle.
    pub vehicle: bool,
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

#[derive(Clone, Debug, Default, PartialEq)]
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
    pub glow: Option<Glow>,
    /// The creature drawn here is hurt.
    pub health: Option<Health>,
    /// How many levels down the layers lie, seen through open air; 0 on the avatar's own level.
    pub depth: u8,
    /// The game's per-square seed for weighted sprite variants, also for squares nothing is known about.
    pub square: u32,
}

/// One short animation the native game would play on the map, on world squares (view cell = square - origin).
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
/// `count`; the client shows its `label` in place of the earlier line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LogLine {
    /// The engine's identity of the line; empty when the engine sent none.
    pub id: String,
    pub text: String,
    /// The game's colour for the message type.
    pub color: Option<Rgb>,
    pub count: u32,
    /// The game turn the line was logged on, or last repeated on.
    pub turn: Option<String>,
    /// The line as the game prints it, with the repeat count.
    pub label: String,
}

/// A sound the avatar heard that the game did not write to its message log.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Heard {
    /// What the game says it heard, localized.
    pub text: String,
    /// The game's name for where it came from, when it names one.
    pub direction: Option<String>,
    /// Squares between the avatar and its source.
    pub distance: Option<u32>,
    /// When the client received it, on the model's clock.
    pub at: std::time::Duration,
}

impl Heard {
    /// The text with where it came from: `text (direction, distance)`.
    pub fn shown(&self) -> String {
        let place = [self.direction.clone(), self.distance.map(|n| n.to_string())]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>();
        match place.is_empty() {
            true => self.text.clone(),
            false => format!("{} ({})", self.text, place.join(", ")),
        }
    }
}

impl LogLine {
    pub fn shown(&self) -> String {
        self.label.clone()
    }
}

impl MenuChoice {
    /// The value of the column named `key`; empty when the row has none.
    pub fn column(&self, key: &str) -> &str {
        self.columns
            .iter()
            .find(|c| c.key == key)
            .map_or("", |c| c.value.as_str())
    }

    /// The row's values as markup after its label: `  caption: value`, a value tinted by its column's
    /// color; the selector bookkeeping columns are left out.
    pub fn values_markup(&self) -> String {
        let shown =
            |c: &&Column| !matches!(c.key.as_str(), "source" | "category") && !c.value.is_empty();
        self.columns
            .iter()
            .filter(shown)
            .map(|c| {
                let value = match c.color.as_deref() {
                    Some(color) => format!("<color_{color}>{}</color>", c.value),
                    None => c.value.clone(),
                };
                match c.label.is_empty() {
                    // A value that only repeats the row's own text adds nothing.
                    true if markup::plain(&value) == markup::plain(&self.label) => String::new(),
                    true => format!("  {value}"),
                    false => format!("  {}: {value}", c.label.trim_end_matches(':')),
                }
            })
            .collect()
    }

    /// The engine's caption of the column named `key`; empty when the row has none.
    pub fn column_label(&self, key: &str) -> &str {
        self.columns
            .iter()
            .find(|c| c.key == key)
            .map_or("", |c| c.label.as_str())
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
    /// The values shown beside the row.
    pub columns: Vec<Column>,
    pub favorite: bool,
    /// Game color names of the row's text and of its hotkey; empty for the default.
    pub color: String,
    pub key_color: String,
    /// The row's name with the native color tags; empty when it is the label.
    pub markup: String,
    /// The registered action the row runs; empty when it runs none.
    pub action: String,
    /// The color of the nesting slash drawn before a nested row; empty for a plain row.
    pub nest_color: String,
    /// The selection mark the native list prints; none when it prints none.
    pub mark: Option<Mark>,
    /// How the row's value can be set directly, as an option of the options menu.
    pub editor: Option<Editor>,
}

/// The text field of a menu, such as a search box.
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
    /// The line under the pane's title, such as a pane's free capacity; empty when none.
    pub description: String,
    pub filter: String,
    /// The native sort name, such as `name`; empty when the engine sent none.
    pub sort: String,
    /// The header totals the native pane prints; absent from engines that predate them.
    pub totals: Option<PaneTotals>,
    /// The native page indicator as (number, count) from 1; absent for a list that does not page.
    pub page: Option<(u64, u64)>,
}

/// The weight and volume line of a pane header: `weight`/`volume` against an optional maximum.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Deserialize)]
#[serde(default)]
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

/// The engine's current interaction, rendered generically.
#[derive(Clone, Debug, PartialEq)]
pub struct Menu {
    /// The engine's name for the kind of menu, such as `MAIN_MENU`.
    pub context: String,
    /// The engine's id of what the menu shows; it changes with the rows and stays while more pages come.
    pub schema_id: String,
    pub title: String,
    pub message: String,
    /// `good` or `bad` where the native screen colors the message line; empty otherwise.
    pub message_tone: String,
    /// Whether the native category-selection mode is on; absent where the screen has none.
    pub category_mode: Option<bool>,
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
    /// Every pane the engine names, as `(id, label)` in its order.
    pub sections: Vec<(String, String)>,
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

    /// The heading `row` starts a section under, when the menu lists several named panes in one list and
    /// `row` is the first of its pane after `previous`.
    pub fn heading(&self, previous: Option<&MenuChoice>, row: &MenuChoice) -> Option<&str> {
        let group = row.group.as_deref().filter(|_| self.pane.is_none())?;
        let changed = previous.is_none_or(|p| p.group.as_deref() != Some(group));
        let named = self.sections.iter().find(|(id, _)| id == group);
        named
            .filter(|_| changed && self.sections.len() > 1)
            .map(|(_, label)| label.as_str())
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
    /// `sent`, `accepted`, `executing`, `done`, `rejected` or `interrupted`.
    pub stage: String,
    /// The engine's words for the stage, with its reason for a failure; empty until the engine answers.
    pub text: String,
    /// The command reached a terminal stage.
    pub finished: bool,
    /// The words of every stage the command went through, oldest first, so ones that pass within a frame are
    /// kept.
    pub history: Vec<String>,
}

impl CommandStatus {
    pub fn set_stage(&mut self, stage: &str, text: String, finished: bool) {
        if !text.is_empty() {
            self.history.push(text.clone());
        }
        self.stage = stage.into();
        self.text = text;
        self.finished = finished;
    }

    /// What the status line says: the name, with the stage's words once the engine has answered.
    pub fn line(&self) -> String {
        match self.text.as_str() {
            "" => self.label.clone(),
            text => format!("{}: {text}", self.label),
        }
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

/// Why the engine cannot be played right now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FailureKind {
    /// The engine process ended or closed its output.
    Exited,
    /// A request has been outstanding longer than the watchdog allows.
    Stalled,
    /// The engine refused this client's hello: its protocol, which needs another engine build, or its language.
    Incompatible,
}

/// The engine's failure, with its last stderr lines, for an error screen with a Restart button.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Failure {
    pub kind: FailureKind,
    pub message: String,
    pub stderr: Vec<String>,
    /// The engine binary the client started, when the failure is about which engine it is.
    pub engine: Option<std::path::PathBuf>,
}

/// The menu of a 0.1 `interaction`: the engine's open menu and the answers to it. It goes with protocol 0.1;
/// screens of the interaction stack are answered through `GameModel::invoke_affordance`.
pub trait MenuModel {
    /// The open menu; none while there is no 0.1 interaction, and for a cursor the map draws instead.
    fn menu(&self) -> Option<Menu> {
        None
    }
    /// Answers the menu with one of its choices.
    fn choose(&mut self, _choice_id: &str) {}
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
    /// Closes the menu, as its cancel key does, when it can be cancelled.
    fn cancel_menu(&mut self) {}
    /// Fills the menu's text field; `submit` applies it (search) rather than only editing it.
    fn fill(&mut self, _field_id: &str, _value: &str, _submit: bool) {}
    /// Chooses a row with a count, such as a batch craft.
    fn set_count(&mut self, _choice_id: &str, _count: u64) {}
}

/// The overmap squares the client's own map window shows: `cols` by `rows`, centred `pan` squares off the
/// player's square.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OvermapWatch {
    pub cols: i32,
    pub rows: i32,
    pub pan: Pos,
}

/// What the UI needs from the game: `FakeModel` (feature `fake`) in tests, `EngineModel` against a real engine.
pub trait GameModel: MenuModel {
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
    /// Names the state `cell` and `size` are drawn from: equal values mean the same map. `None` when the model
    /// cannot tell, so a view that caches what it draws redraws every frame.
    fn map_version(&self) -> Option<u64> {
        None
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
    /// A game key went down (not a key repeat). When it is a step of the map the engine lets the player hold, the
    /// engine is told to walk until `release_hold`; true then. Any other key is for `press`.
    fn hold(&mut self, _key: &str) -> bool {
        false
    }
    /// The held key came up, or the window lost the keyboard: the engine takes no further step.
    fn release_hold(&mut self) {}
    /// Left click on view cell `at`: the engine plans a route, a repeat walks it.
    fn travel(&mut self, _at: Pos) {}
    /// The planned route in view coordinates, nearest first; empty when none.
    fn route(&self) -> Vec<Pos> {
        vec![]
    }
    /// The engine's localized line for the planned route; none when nothing is planned.
    fn route_text(&self) -> Option<String> {
        None
    }
    /// The engine's `USE_TILES` and `TILES` options, when it publishes them: the map mode a new profile starts in.
    fn display_default(&self) -> Option<(bool, String)> {
        None
    }
    /// Says whether the client draws tiles now, so the engine sends tile ids where it has them.
    fn set_tiles(&mut self, _tiles: bool) {}
    /// Gives the ids of the tints the client's tileset has, so the engine publishes the `tint_keys` among them.
    fn set_tint_ids(&mut self, _ids: &[String]) {}
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
    /// Asks for the native description of the hovered view square; it arrives with later polls.
    fn hover(&mut self, _at: Pos) {}
    /// The native description lines of view square `at`, once the engine has answered for it.
    fn hover_text(&self, at: Pos) -> Option<Vec<String>> {
        let lines = self
            .hover_parts(at)?
            .iter()
            .flat_map(|p| std::iter::once(&p.text).chain(&p.detail))
            .map(|text| markup::plain(text))
            .collect::<Vec<_>>();
        (!lines.is_empty()).then_some(lines)
    }
    /// The facts the engine's mouse view prints of view square `at`, once it has answered for that square.
    fn hover_parts(&self, _at: Pos) -> Option<Vec<crate::protocol::TilePart>> {
        None
    }
    /// What the avatar can do with view square `at`, in the order the engine lists it; asks the engine on
    /// the first call and answers none until it has replied. A model without an engine has no actions.
    fn tile_actions(&mut self, _at: Pos) -> Option<Vec<TileAction>> {
        Some(Vec::new())
    }
    /// Takes the action `action_id` the engine listed for view square `at`; false when nothing was sent.
    fn tile_action(&mut self, _at: Pos, _action_id: &str) -> bool {
        false
    }
    /// The right click on a view square, as the engine's own SEC_SELECT; false when the model has none.
    fn context_click(&mut self, _at: Pos) -> bool {
        false
    }
    /// The shortcut bar the map screen publishes, in the engine's order.
    fn shortcuts(&self) -> Vec<Shortcut> {
        self.screens()
            .iter()
            .find(|s| s.role == ScreenRole::Map)
            .map_or_else(Vec::new, |s| s.shortcuts.clone())
    }
    /// The avatar's item lists the map screen names beside it, in the engine's order; none before it names them.
    fn item_lists(&self) -> Vec<ItemList> {
        let map = self.screens().iter().find(|s| s.role == ScreenRole::Map);
        match map.and_then(|s| s.root.as_ref()) {
            Some(Component::Map(m)) => m.item_lists.clone(),
            _ => vec![],
        }
    }
    /// The engine's cursor (look mode, aiming) moves to view square `at`; a `click` on the square it is
    /// already on confirms it, a hover never does.
    fn aim(&mut self, _at: Pos, _click: bool) -> bool {
        false
    }
    /// The colour names the game data defines, loaded by the client; renderers resolve names with it.
    fn colors(&self) -> &ColorMap {
        static DEFAULT: std::sync::OnceLock<ColorMap> = std::sync::OnceLock::new();
        DEFAULT.get_or_init(ColorMap::default)
    }
    /// The open screens of the interaction stack, outermost first; empty while the boundary has only the `interaction`.
    fn screens(&self) -> &[Screen] {
        &[]
    }
    /// What the client's map window shows, read from the engine while the game goes on; none when it is closed.
    fn watch_overmap(&mut self, _watch: Option<OvermapWatch>) {}
    /// The squares last read for the map window.
    fn watched_overmap(&self) -> Option<&crate::protocol::OvermapWire> {
        None
    }
    /// Runs an affordance of the top screen by id, with the arguments its shape asks for.
    fn invoke_affordance(&mut self, _id: &str, _args: InvokeArgs) -> Result<(), InvokeError> {
        Err(InvokeError::NoScreen)
    }
    /// Fetches the next page of a list that has more rows than it holds.
    fn more_rows(&mut self, _component_id: &str) {}
    /// The rows of a list of the top screen that are known: its first page, then the pages fetched.
    fn rows<'a>(&'a self, list: &'a List) -> Vec<&'a Row> {
        list.rows.iter().collect()
    }
    /// The files of game data that add sprites to the tileset the game draws, in the engine's load order.
    fn mod_tilesets(&self) -> &[ModTileset] {
        &[]
    }
    /// Season of the game calendar (`spring`, `summer`, `autumn`, `winter`), which picks seasonal sprites.
    fn season(&self) -> Option<String> {
        None
    }
    fn size(&self) -> Pos;
    fn cell(&self, at: Pos) -> Cell;
    fn player(&self) -> Pos;
    fn ground(&self) -> &[String];
    fn inventory(&self) -> &[String];
    /// How each row of `ground` is drawn (sprite layers, else glyph and colour); empty for a model without looks.
    fn ground_cells(&self) -> &[Cell] {
        &[]
    }
    /// The same for the rows of `inventory`.
    fn inventory_cells(&self) -> &[Cell] {
        &[]
    }
    /// The engine's id of each `ground` row, empty where it names none; pickup and drop name a row by it, so a
    /// list that changed since the player chose still takes the chosen item.
    fn ground_ids(&self) -> Vec<String>;
    /// The same for the rows of `inventory`.
    fn inventory_ids(&self) -> Vec<String>;
    fn pick_up(&mut self, item_id: &str);
    fn drop_item(&mut self, item_id: &str);
    /// Client-side lines: command results and errors.
    fn messages(&self) -> &[String];
    /// Sounds heard that the game's message log does not hold: grouped, oldest first, faded by age.
    fn sounds(&self) -> Vec<crate::sounds::SoundLine> {
        vec![]
    }
    /// How a list row's look is drawn (sprite layers, else glyph and colour); none for a model without looks.
    fn look_cell(&self, _look: &Look) -> Option<Cell> {
        None
    }
    /// What the avatar is busy with, as the engine names it; none when it is not busy.
    fn activity(&self) -> Option<crate::protocol::Activity> {
        None
    }
    /// The avatar's sidebar as coloured lines; empty before the engine reports one.
    fn sidebar(&self) -> Vec<crate::sidebar::Line> {
        vec![]
    }
    /// The sidebar as the game's panel options arrange it; none before the engine reports them.
    fn sidebar_layout(&self) -> Option<crate::sidebar::Layout> {
        None
    }
    /// The heading of the game's message log panel; none before the engine names it.
    fn log_label(&self) -> Option<String> {
        let layout = self.sidebar_layout()?;
        let log = layout
            .panels
            .into_iter()
            .find(|p| p.body == crate::sidebar::Body::Log);
        log.map(|p| p.label)
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
    /// The folder of the sound pack the engine plays from; none while its sound is off.
    fn soundpack(&self) -> Option<std::path::PathBuf> {
        None
    }
    /// What the engine's sound player was asked to play since the last call.
    fn take_cues(&mut self) -> Vec<crate::audio::Cue> {
        vec![]
    }
}

#[cfg(feature = "fake")]
pub struct FakeModel {
    /// Map size; tests shrink it to emulate loading or a replaced map.
    pub size: Pos,
    player: Pos,
    ground: Vec<String>,
    inventory: Vec<String>,
    messages: Vec<String>,
    effects: Vec<Effect>,
}

#[cfg(feature = "fake")]
impl FakeModel {
    /// An animation the next `take_effects` hands out.
    pub fn queue_effect(&mut self, effect: Effect) {
        self.effects.push(effect);
    }
}

#[cfg(feature = "fake")]
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

#[cfg(feature = "fake")]
impl MenuModel for FakeModel {}

#[cfg(feature = "fake")]
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

    fn item_lists(&self) -> Vec<ItemList> {
        use crate::protocol::{ItemListId, Text};
        let list = |id, label: &str, verb: &str| ItemList {
            id,
            label: Text::Plain(label.into()),
            verb: Text::Plain(verb.into()),
        };
        vec![
            list(ItemListId::Ground, "Ground", "Pick up"),
            list(ItemListId::Inventory, "Inventory", "Drop"),
        ]
    }

    fn log_label(&self) -> Option<String> {
        Some("Message log".into())
    }

    fn size(&self) -> Pos {
        self.size
    }

    fn cell(&self, at: Pos) -> Cell {
        if at == self.player {
            return Cell {
                glyph: '@',
                color: Rgb(255, 255, 255),
                desc: "you".into(),
                ..Cell::default()
            };
        }
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

    fn tile_actions(&mut self, _at: Pos) -> Option<Vec<TileAction>> {
        let action = |id: &str, label: &str| TileAction {
            id: id.into(),
            label: crate::protocol::Text::Plain(label.into()),
            keys: vec![],
        };
        Some(vec![action("look", "Look"), action("travel", "Move here")])
    }

    fn tile_action(&mut self, at: Pos, action_id: &str) -> bool {
        match action_id {
            "look" => self.say(self.cell(at).desc),
            "travel" => self.travel(at),
            _ => return false,
        }
        true
    }

    fn travel(&mut self, target: Pos) {
        while self.player != target {
            let next = Pos {
                x: self.player.x + (target.x - self.player.x).signum(),
                y: self.player.y + (target.y - self.player.y).signum(),
            };
            if self.cell(next).glyph == '#' {
                break;
            }
            self.player = next;
        }
    }

    fn ground(&self) -> &[String] {
        &self.ground
    }

    fn inventory(&self) -> &[String] {
        &self.inventory
    }

    // The fake's items are their own ids.
    fn ground_ids(&self) -> Vec<String> {
        self.ground.clone()
    }

    fn inventory_ids(&self) -> Vec<String> {
        self.inventory.clone()
    }

    fn pick_up(&mut self, item_id: &str) {
        if let Some(index) = self.ground.iter().position(|i| i == item_id) {
            self.inventory.push(self.ground.remove(index));
        }
    }

    fn drop_item(&mut self, item_id: &str) {
        if let Some(index) = self.inventory.iter().position(|i| i == item_id) {
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
