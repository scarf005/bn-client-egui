//! What the client asks: the commands it submits and the reads it requests, with their results.

use super::*;

/// What a command is conditional on. The form follows the operation: an `invoke` carries the expectation its
/// affordance's scope names, every other operation the 0.1 form.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Expect {
    /// Holds while that screen is the top of the stack (`expect: screen`).
    Screen(ScreenExpect),
    /// Holds while the boundary and revision stand (`expect: boundary`).
    Boundary(BoundaryExpect),
    Interaction(InteractionExpect),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScreenExpect {
    pub screen_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BoundaryExpect {
    pub boundary_id: String,
    pub revision: String,
}

/// The 0.1 expectation of the operations that are not an `invoke`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InteractionExpect {
    pub revision: String,
    pub boundary_id: String,
    pub schema_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Operation {
    Choose {
        choice_id: String,
    },
    Fill {
        field_id: String,
        value: String,
        submit: bool,
    },
    SetCount {
        choice_id: String,
        count: u64,
    },
    SetTarget {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        candidate_id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pos: Option<WorldPos>,
    },
    Cancel,
    Action {
        action_id: String,
    },
    /// The right click (SEC_SELECT) on a map square: examine, close a door, pick up or fire.
    Context {
        pos: WorldPos,
    },
    /// The left click of Tiles and curses on a map square: plans a route, a repeat walks it.
    Travel {
        pos: WorldPos,
    },
    /// Picks up `count` of the `avatar.ground` row `item_id`, in the row's native units.
    Pickup {
        item_id: String,
        count: u64,
    },
    /// Drops `count` of the `avatar.inventory` row `item_id`, in the row's native units.
    Drop {
        item_id: String,
        count: u64,
    },
    /// Takes one of the actions a described map square lists, as its native action does.
    TileAction {
        #[serde(deserialize_with = "opaque_id")]
        action_id: String,
        pos: WorldPos,
    },
    /// Sets the mode `mode` of `avatar.modes` to its value `value`; a mode that holds it is left alone.
    SetMode {
        #[serde(deserialize_with = "opaque_id")]
        mode: String,
        #[serde(deserialize_with = "opaque_id")]
        value: String,
    },
    /// Runs an affordance of the top screen, by id.
    Invoke {
        #[serde(deserialize_with = "opaque_id")]
        affordance_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        args: Option<InvokeArgs>,
    },
}

/// The members an affordance's `args` shape names, and no others.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InvokeArgs {
    #[serde(
        default,
        deserialize_with = "optional_id",
        skip_serializing_if = "Option::is_none"
    )]
    pub row: Option<String>,
    #[serde(
        default,
        deserialize_with = "some_rows",
        skip_serializing_if = "Option::is_none"
    )]
    pub rows: Option<Vec<RowPick>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub count: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pos: Option<WorldPos>,
    #[serde(
        default,
        deserialize_with = "optional_id",
        skip_serializing_if = "Option::is_none"
    )]
    pub pane: Option<String>,
}

/// One row of a `rows` argument, with how much of it where the shape counts.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RowPick {
    #[serde(deserialize_with = "opaque_id")]
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub count: Option<u64>,
}

/// An id the engine mints: 1 to 256 characters, opaque to the client.
fn opaque_id<'de, D: serde::Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    let id = String::deserialize(d)?;
    match (1..=256).contains(&id.chars().count()) {
        true => Ok(id),
        false => Err(serde::de::Error::custom(
            "expected an id of 1 to 256 characters",
        )),
    }
}

fn optional_id<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<String>, D::Error> {
    opaque_id(d).map(Some)
}

fn some_rows<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<Vec<RowPick>>, D::Error> {
    match Vec::<RowPick>::deserialize(d)? {
        rows if rows.is_empty() => Err(serde::de::Error::custom("expected at least one row")),
        rows => Ok(Some(rows)),
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "SubmitWire")]
pub struct Submit {
    pub epoch: String,
    pub expect: Expect,
    pub operation: Operation,
}

#[derive(Deserialize)]
struct SubmitWire {
    epoch: String,
    expect: Expect,
    operation: Operation,
}

/// An `invoke` carries a 0.2 expectation and every other operation the 0.1 one.
impl TryFrom<SubmitWire> for Submit {
    type Error = String;

    fn try_from(wire: SubmitWire) -> Result<Self, String> {
        let invoke = matches!(wire.operation, Operation::Invoke { .. });
        let legacy = matches!(wire.expect, Expect::Interaction(_));
        match invoke != legacy {
            true => Ok(Self {
                epoch: wire.epoch,
                expect: wire.expect,
                operation: wire.operation,
            }),
            false => Err("the operation does not take this form of expectation".into()),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChoicesParams {
    pub epoch: String,
    pub boundary_id: String,
    /// 0.2: the list whose rows are asked for; absent asks for the 0.1 interaction's choices.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub component_id: Option<String>,
    pub offset: u64,
    #[serde(deserialize_with = "page_limit")]
    pub limit: u64,
}

/// A page holds 1 to 200 rows.
fn page_limit<'de, D: serde::Deserializer<'de>>(d: D) -> Result<u64, D::Error> {
    match u64::deserialize(d)? {
        n if (1..=PAGE_ROWS).contains(&n) => Ok(n),
        _ => Err(serde::de::Error::custom("expected 1 to 200 rows")),
    }
}

/// The most rows the engine sends for a page, and a page of a list holds.
pub const PAGE_ROWS: u64 = 200;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ChoicesResult {
    pub boundary_id: String,
    pub total: u64,
    pub choices: Vec<Choice>,
}

/// The rows of a list past its first page.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RowsResult {
    pub boundary_id: String,
    pub component_id: String,
    pub total: u64,
    pub rows: Vec<Row>,
}

/// The answer of `bn.interaction.choices`: rows of a named list in 0.2, the interaction's choices in 0.1.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ChoicesReply {
    Rows(RowsResult),
    Choices(ChoicesResult),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorldCellsParams {
    pub epoch: String,
    pub min: WorldPos,
    pub max: WorldPos,
}

/// A native screen the client shows in a window of its own while the game goes on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PanelKind {
    Character,
    Inventory,
}

/// `bn.panel.view`: the screen of `panel` with its cursor on `row`, else where the native screen opens it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PanelParams {
    pub epoch: String,
    pub panel: PanelKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub row: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WorldCellsResult {
    pub at: At,
    pub cells: Vec<Cell>,
    pub forgotten: Vec<WorldPos>,
}

/// Params of `bn.world.describe`: what the native mouse view prints for a hovered square.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DescribeParams {
    pub epoch: String,
    pub boundary_id: String,
    pub pos: WorldPos,
}

/// One fact the engine's look view prints of a square.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TilePart {
    pub kind: String,
    /// Markup.
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    /// What the fact is about, such as a terrain id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// Numbers the fact reports, by name.
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub values: std::collections::BTreeMap<String, i64>,
}

/// Something the avatar can do with a described square now.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TileAction {
    /// The native action's id, or `travel`, `attack` or `talk`; opaque to the client.
    pub id: String,
    pub label: Text,
    /// The player's bindings of the native action.
    pub keys: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DescribeResult {
    pub parts: Vec<TilePart>,
    pub actions: Vec<TileAction>,
}

/// Params of `bn.command.result`; its result is a `CommandNotice`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResultParams {
    pub epoch: String,
    pub command_id: String,
}
