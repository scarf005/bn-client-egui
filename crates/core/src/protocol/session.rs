//! The hello that opens a connection, and the delivery of frames it agrees on.

use super::*;
use serde_json::Value;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Hello {
    pub version: String,
    pub epoch: String,
    pub engine: Value,
    pub limits: Value,
    /// Present when the client opted in to credit delivery.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delivery: Option<DeliveryGrant>,
    /// The schema hash the engine was built with; a 0.2 engine answers with the one the hello named.
    #[serde(
        default,
        deserialize_with = "schema_hash",
        skip_serializing_if = "Option::is_none"
    )]
    pub schema: Option<String>,
    /// The locale this connection receives, and the engine's own.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub locale: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub engine_locale: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<Role>,
}

impl Hello {
    /// How many screen-scoped commands the engine holds behind the running one; 1 when it does not say.
    pub fn screen_queue(&self) -> usize {
        self.limits["screen_queue"]
            .as_u64()
            .map_or(1, |n| n.max(1) as usize)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Controller,
    Spectator,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeliveryGrant {
    pub credits: u16,
    pub held_frames: u64,
    pub held_bytes: u64,
    /// The frame rate cap the hello asked for, echoed; an engine without the cap leaves it out.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_hz: Option<u16>,
}

/// A count of pushed frames: a window of 1 to 256.
fn frame_window<'de, D: serde::Deserializer<'de>>(d: D) -> Result<u16, D::Error> {
    let n = u16::deserialize(d)?;
    match (1..=256).contains(&n) {
        true => Ok(n),
        false => Err(serde::de::Error::custom("expected 1 to 256 frames")),
    }
}

/// A frames-per-second cap: 1 to 240.
fn frame_rate<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<u16>, D::Error> {
    match Option::<u16>::deserialize(d)? {
        Some(n) if !(1..=240).contains(&n) => Err(serde::de::Error::custom("expected 1 to 240 Hz")),
        hz => Ok(hz),
    }
}

/// Opts in to credit delivery.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeliveryRequest {
    #[serde(deserialize_with = "frame_window")]
    pub credits: u16,
    /// The most frames per second the engine publishes while an activity runs.
    #[serde(
        default,
        deserialize_with = "frame_rate",
        skip_serializing_if = "Option::is_none"
    )]
    pub max_hz: Option<u16>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreditParams {
    #[serde(deserialize_with = "frame_window")]
    pub frames: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreditResult {
    pub credits: u16,
}

/// `sha256:` and 16 hex digits.
fn schema_hash<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<String>, D::Error> {
    let hash = String::deserialize(d)?;
    let digits = hash.strip_prefix("sha256:").unwrap_or_default();
    let hex = digits
        .bytes()
        .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'));
    match digits.len() == 16 && hex {
        true => Ok(Some(hash)),
        false => Err(serde::de::Error::custom(
            "expected sha256: and 16 hex digits",
        )),
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClientInfo {
    pub name: String,
    pub version: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HelloParams {
    pub versions: Vec<String>,
    pub client: ClientInfo,
    /// Map cells the client can show; the engine sizes its terrain window to them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub viewport: Option<Viewport>,
    /// The client draws tiles, so the engine adds tile ids where it has them (the overmap).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub tiles: bool,
    /// The ids of the tints the client's tileset has; the engine publishes `tint_keys` among them.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tint_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delivery: Option<DeliveryRequest>,
    /// 0.2: the hash of the schema the client was built with.
    #[serde(
        default,
        deserialize_with = "schema_hash",
        skip_serializing_if = "Option::is_none"
    )]
    pub schema: Option<String>,
    /// 0.2: the BN language id the client wants its text in; the engine's own when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub locale: Option<String>,
    /// 0.2: the members above that say what the client can do.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capabilities: Option<Capabilities>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Capabilities {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tiles: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tint_ids: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub viewport: Option<Viewport>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delivery: Option<DeliveryRequest>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Viewport {
    pub cols: u32,
    pub rows: u32,
}

impl HelloParams {
    pub fn new(client: &str) -> Self {
        Self {
            versions: vec![VERSION.into()],
            client: ClientInfo {
                name: client.into(),
                version: env!("CARGO_PKG_VERSION").into(),
            },
            viewport: None,
            tiles: false,
            tint_ids: vec![],
            delivery: None,
            schema: Some(SCHEMA_HASH.into()),
            locale: None,
            capabilities: None,
        }
    }

    pub fn with_tint_ids(self, tint_ids: Vec<String>) -> Self {
        Self { tint_ids, ..self }
    }

    pub fn with_tiles(self, tiles: bool) -> Self {
        Self { tiles, ..self }
    }

    /// Asks for the engine's text in a BN language id (`lang/languages.json`).
    pub fn with_locale(self, locale: Option<String>) -> Self {
        Self { locale, ..self }
    }

    /// Asks for credits and a frame rate cap, kept within the 1 to 240 the engine accepts.
    pub fn with_delivery(self, credits: u16, max_hz: Option<u16>) -> Self {
        Self {
            delivery: Some(DeliveryRequest {
                credits,
                max_hz: max_hz.map(|hz| hz.clamp(1, 240)),
            }),
            ..self
        }
    }

    pub fn with_viewport(self, viewport: Viewport) -> Self {
        Self {
            viewport: Some(viewport),
            ..self
        }
    }
}
