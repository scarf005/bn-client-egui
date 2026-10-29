//! Serde types for the unified engine protocol 0.2 (newline-delimited JSON-RPC 2.0).
//! Shapes follow protocol-unification.md rev 2 (methods §1, clock §2, values §3).

mod component;
mod grid;
mod menu;
mod overmap;
mod request;
mod screen;
mod session;
mod sidebar;
mod stream;
mod world;

pub use component::*;
pub use grid::*;
pub use menu::*;
pub use overmap::*;
pub use request::*;
pub use screen::*;
pub use session::*;
pub use sidebar::*;
pub use stream::*;
pub use world::*;

use serde::{Deserialize, Serialize};

pub const VERSION: &str = "0.2";
/// The raw-byte SHA-256 prefix of the vendored `schema/0.2.schema.json`, computed by `build.rs`: what a 0.2 hello
/// tells the engine this client was built against.
pub const SCHEMA_HASH: &str = env!("BN_SCHEMA_HASH");

fn is_default<T: Default + PartialEq>(v: &T) -> bool {
    *v == T::default()
}

/// Absolute map square; `dim` is `""` for the primary dimension.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct WorldPos {
    pub dim: String,
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

/// Stream position: includes every event up to `sequence`. Counters are decimal strings.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct At {
    pub epoch: String,
    pub sequence: String,
    pub revision: String,
}

/// Player-facing text: a string already localized for the connection, or a message key with its arguments
/// for the client to render from the engine's catalogues.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Text {
    Plain(String),
    Localised(LocalisedString),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LocalisedString {
    /// The msgid. The empty key joins `args`.
    pub key: String,
    /// The gettext context.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ctx: Option<String>,
    /// The count that selects the plural form.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plural: Option<serde_json::Number>,
    /// The msgid of the plural form, shown for a count other than one when no catalogue translates the key.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plural_key: Option<String>,
    /// The values of the placeholders, in order.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub args: Option<Vec<TextArg>>,
    /// What a `<npcname>` in the worded line stands for.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub npcname: Option<Box<Text>>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum TextArg {
    Number(serde_json::Number),
    Text(Text),
}

impl Text {
    /// What is shown: a string as it is, a message rendered from the engine's catalogue.
    pub fn shown(&self) -> String {
        match self {
            Text::Plain(text) => text.clone(),
            Text::Localised(message) => crate::localise::render(message),
        }
    }
}
