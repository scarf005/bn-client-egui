//! The components a screen is drawn from, by kind; the grid has a module of its own.

use super::*;
use serde_json::Value;

/// The members every component has.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Head {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<Text>,
    /// The ids of the screen's affordances that act on this component.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub affordances: Vec<String>,
    /// What this part of a workspace screen's root split is for; none counts as the inspector.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slot: Option<Slot>,
    /// The part starts folded to its heading on a `workspace` screen.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub collapsed: bool,
}

/// What a part of a `workspace` screen's root split is for, which the client lays out as it likes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Slot {
    /// The tools and their settings.
    Tools,
    /// The document being edited.
    Canvas,
    /// What the tools put down.
    Palette,
    /// The parts of the document.
    Outline,
    /// The settings of what is picked.
    Inspector,
    /// Messages and history.
    Log,
    /// A line of facts about the cursor and the document.
    Status,
}

/// A generic component, by `kind`: a client draws each without knowing the screen.
// A screen holds a few components, read once per screen: the size of the largest kind does not matter.
#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Component {
    List(List),
    Tabs(Tabs),
    Split(Split),
    Panel(Panel),
    Form(Form),
    Text(TextBlock),
    Grid(Grid),
    Map(MapComponent),
    Canvas(CanvasComponent),
}

impl Component {
    pub fn head(&self) -> &Head {
        match self {
            Component::List(c) => &c.head,
            Component::Tabs(c) => &c.head,
            Component::Split(c) => &c.head,
            Component::Panel(c) => &c.head,
            Component::Form(c) => &c.head,
            Component::Text(c) => &c.head,
            Component::Grid(c) => &c.head,
            Component::Map(c) => &c.head,
            Component::Canvas(c) => &c.head,
        }
    }

    pub fn id(&self) -> &str {
        &self.head().id
    }

    /// This component and every one inside it, each before the ones inside it.
    pub fn nested(&self) -> Vec<&Component> {
        std::iter::once(self)
            .chain(self.children().into_iter().flat_map(Component::nested))
            .collect()
    }

    /// The components directly inside this one.
    pub fn children(&self) -> Vec<&Component> {
        match self {
            Component::Tabs(c) => c.child.iter().map(|child| &**child).collect(),
            Component::Split(c) => c.children.iter().collect(),
            Component::Panel(c) => c.child.iter().map(|child| &**child).collect(),
            _ => vec![],
        }
    }

    /// The streamed world's map in this component or inside it, when the screen puts a cursor on it.
    pub fn cursor_map(&self) -> Option<&MapComponent> {
        match self {
            Component::Map(m) if m.mode == MapMode::World && m.target.is_some() => Some(m),
            _ => self.children().into_iter().find_map(Component::cursor_map),
        }
    }

    /// This component or the one inside it with `id`.
    pub fn find(&self, id: &str) -> Option<&Component> {
        match self.id() == id {
            true => Some(self),
            false => self.children().into_iter().find_map(|c| c.find(id)),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Selection {
    None,
    Single,
    Multi,
    Count,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct List {
    #[serde(flatten)]
    pub head: Head,
    pub columns: Vec<ListColumn>,
    /// The first page, at most 200 rows from `offset`; `bn.interaction.choices` pages the rest.
    pub rows: Vec<Row>,
    pub total: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub offset: Option<u64>,
    pub selection: Selection,
    /// The affordance that runs on a row's activation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub activate: Option<String>,
    /// The list takes a filter text: its filter affordance is among `affordances`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filterable: Option<bool>,
    /// The filter text the list holds; none when it holds none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filter: Option<String>,
    /// What the list says while it has no rows.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub empty: Option<Text>,
    /// The most rows the list shows at once, scrolling the rest; none: no limit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rows_shown: Option<u32>,
}

impl List {
    /// The affordance of `screen` that sets this list's filter text in one invoke, with no prompt: the list's
    /// own, taking only a `text`.
    pub fn search<'a>(&self, screen: &'a Screen) -> Option<&'a Affordance> {
        let sets_text = |shape: &ArgShape| {
            shape.text.is_some()
                && shape.row.is_none()
                && shape.rows.is_none()
                && shape.pane.is_none()
        };
        self.filterable
            .filter(|&filterable| filterable)
            .and_then(|_| {
                self.head
                    .affordances
                    .iter()
                    .filter_map(|id| screen.affordances.iter().find(|a| &a.id == id))
                    .find(|a| a.args.as_ref().is_some_and(sets_text))
            })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListColumn {
    pub key: String,
    pub label: Text,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Row {
    pub id: String,
    pub label: Text,
    pub description: Text,
    pub denial: Text,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hotkey: Option<String>,
    pub enabled: bool,
    pub selectable: bool,
    pub selected: bool,
    pub columns: Vec<RowColumn>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub favorite: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key_color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub markup: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nest_color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mark: Option<Mark>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selected_count: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub minimum_count: Option<u64>,
    /// How much of the row there is, in native units.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<u64>,
    /// A heading over the rows that follow: it is not a thing to choose.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub header: Option<bool>,
    /// The row switches something on or off, and this is which it is now.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub toggled: Option<bool>,
    /// The affordances this row allows; absent: all of the list's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub affordances: Option<Vec<RowAffordance>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub look: Option<Look>,
    /// The screen affordance that does what the row does, such as a popup's `YES`; not a button again.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub affordance: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RowColumn {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    pub label: Text,
    pub value: Text,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RowAffordance {
    pub id: String,
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub denial: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Tabs {
    #[serde(flatten)]
    pub head: Head,
    pub tabs: Vec<Tab>,
    /// The affordance that selects a tab.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub select: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub child: Option<Box<Component>>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tab {
    pub id: String,
    pub label: Text,
    pub selected: bool,
    pub enabled: bool,
    /// Why the tab is off, in the engine's words.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub denial: Option<Text>,
    /// The key that chooses the tab.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hotkey: Option<String>,
    /// Something in the tab is new to the player.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub unread: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Relation {
    Peers,
    Detail,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Split {
    #[serde(flatten)]
    pub head: Head,
    pub children: Vec<Component>,
    pub relation: Relation,
    /// The child that has the focus, by id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Tone {
    Good,
    Bad,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Panel {
    #[serde(flatten)]
    pub head: Head,
    pub title: Text,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subtitle: Option<Text>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stats: Option<Vec<PanelStat>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filter: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub child: Option<Box<Component>>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PanelStat {
    pub key: String,
    pub label: Text,
    pub value: Text,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<Text>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unit: Option<Text>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tone: Option<Tone>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Form {
    #[serde(flatten)]
    pub head: Head,
    pub fields: Vec<FormField>,
    /// The affordance that confirms the form.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub submit: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FieldKind {
    Text,
    Integer,
    Float,
    Key,
    Bool,
    Select,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FormField {
    pub id: String,
    pub label: Text,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<Text>,
    /// The current value, in the form `fill` takes.
    pub value: String,
    #[serde(rename = "type")]
    pub kind: FieldKind,
    /// -1: unbounded.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_length: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub printable: Option<bool>,
    /// The text may run over several lines.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub multiline: Option<bool>,
    /// Where the cursor stands in the text, in characters from its start.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub options: Option<Vec<FieldOption>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min: Option<serde_json::Number>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<serde_json::Number>,
    /// The form's submit refuses while the field is empty.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub required: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FieldOption {
    pub id: String,
    pub label: Text,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextRole {
    TerminalOutput,
    Banner,
    /// The question the parts after it in its split answer.
    Prompt,
}

/// Text with the native colour tags in `markup`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextBlock {
    #[serde(flatten)]
    pub head: Head,
    pub markup: Text,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tone: Option<Tone>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<TextRole>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MapMode {
    World,
    Overmap,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MapComponent {
    #[serde(flatten)]
    pub head: Head,
    pub mode: MapMode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub overmap: Option<OvermapWire>,
    /// The avatar's item lists the world map shows beside it, in order, also while the map is covered.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub item_lists: Vec<ItemList>,
}

/// A place the cursor of a target screen can jump to: a creature in sight, or a square.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct TargetCandidate {
    pub id: String,
    pub label: Text,
    pub position: WorldPos,
    #[serde(default)]
    pub creature: bool,
}

/// Squares a screen highlights on the map besides its cursor: every square, or the rectangle with both corners.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "ScreenMarkWire")]
pub struct ScreenMark {
    pub role: MarkRole,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub squares: Vec<WorldPos>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rect: Option<Coverage>,
}

#[derive(Deserialize)]
struct ScreenMarkWire {
    role: MarkRole,
    squares: Option<Vec<WorldPos>>,
    rect: Option<Coverage>,
}

/// A mark is its squares or its rectangle, never both.
impl TryFrom<ScreenMarkWire> for ScreenMark {
    type Error = String;

    fn try_from(wire: ScreenMarkWire) -> Result<Self, String> {
        match wire.squares.is_some() != wire.rect.is_some() {
            true => Ok(Self {
                role: wire.role,
                squares: wire.squares.unwrap_or_default(),
                rect: wire.rect,
            }),
            false => Err("a mark has either squares or a rect".into()),
        }
    }
}

/// What a screen's marked squares are.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MarkRole {
    /// The squares the map editor's edits apply to.
    Selection,
    /// The edge of the overmap terrain square mapgen targets.
    OmtBorder,
    /// The rectangle a mapgen preview is generated into, marked at its corners.
    Stamp,
}

/// A list of the avatar's items beside the map, under the engine's heading.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ItemList {
    pub id: ItemListId,
    pub label: Text,
    /// What moving a row out of the list does.
    pub verb: Text,
}

/// The avatar member that holds an item list's rows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ItemListId {
    #[serde(rename = "avatar.ground")]
    Ground,
    #[serde(rename = "avatar.inventory")]
    Inventory,
}

impl MapComponent {
    /// The square the screen's cursor is on.
    pub fn cursor(&self) -> Option<WorldPos> {
        serde_json::from_value(self.target.as_ref()?["current"].clone()).ok()
    }

    /// The square the cursor's line of fire leaves: the avatar's.
    pub fn source(&self) -> Option<WorldPos> {
        serde_json::from_value(self.target.as_ref()?["source"].clone()).ok()
    }

    /// The squares the screen highlights besides its cursor; a mark of a role this client does not know is left
    /// out.
    pub fn marks(&self) -> Vec<ScreenMark> {
        let marks = self.target.as_ref().map(|t| &t["marks"]);
        let marks = marks.and_then(Value::as_array).into_iter().flatten();
        marks
            .filter_map(|m| serde_json::from_value(m.clone()).ok())
            .collect()
    }

    /// The squares and creatures the cursor can jump to, in the engine's order; the first is where it is.
    pub fn candidates(&self) -> Vec<TargetCandidate> {
        let list = self.target.as_ref().map(|t| &t["candidates"]);
        let list = list.and_then(Value::as_array).into_iter().flatten();
        list.filter_map(|c| serde_json::from_value(c.clone()).ok())
            .collect()
    }

    /// The squares the line of fire crosses from the avatar to the cursor, as the engine draws it.
    pub fn trajectory(&self) -> Vec<WorldPos> {
        let line = self.target.as_ref().map(|t| &t["trajectory"]);
        let line = line.and_then(Value::as_array).into_iter().flatten();
        line.filter_map(|p| serde_json::from_value(p.clone()).ok())
            .collect()
    }

    /// What the engine says of the cursor's square: the screen's status in words, then each fact it prints of
    /// the square with what more it says of it, as markup lines.
    pub fn cursor_lines(&self) -> Vec<String> {
        let Some(target) = &self.target else {
            return vec![];
        };
        let status = target["status_text"].as_str().map(String::from);
        let parts = target["parts"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|p| serde_json::from_value::<TilePart>(p.clone()).ok())
            .flat_map(|p| std::iter::once(p.text).chain(p.detail));
        status.into_iter().chain(parts).collect()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CanvasComponent {
    #[serde(flatten)]
    pub head: Head,
    pub cols: usize,
    pub rows: Vec<Vec<(String, u8, u8)>>,
}
