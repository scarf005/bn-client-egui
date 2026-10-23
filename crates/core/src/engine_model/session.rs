//! Opening a session with the engine and taking in what it sends.

use super::{DEFAULT_MAX_HZ, EngineModel, Paged, Pending, Respawn, STALL, VIEW};
use crate::clock::Clock;
use crate::colors::ColorMap;
use crate::interaction::Pages;
use crate::localise::{Catalogue, Value as Arg, tr, tr_format};
use crate::model::{Failure, FailureKind, Loading};
use crate::protocol::{self, Incoming, LoadingNotice};
use crate::replica::{Outcome, Replica};
use crate::transport::{Transport, request};
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

/// How a session starts.
#[derive(Clone, Debug)]
pub struct ConnectOptions {
    /// The client draws tiles, so the engine sends tile ids too.
    pub tiles: bool,
    /// The most frames per second the client can show while an activity runs.
    pub max_hz: u16,
    /// The BN language id the client wants the engine's text in; the engine's own when absent.
    pub locale: Option<String>,
}

impl Default for ConnectOptions {
    fn default() -> Self {
        Self {
            tiles: false,
            max_hz: DEFAULT_MAX_HZ,
            locale: None,
        }
    }
}

impl EngineModel {
    pub fn new(transport: Box<dyn Transport>) -> Self {
        Self::connect(transport, false)
    }

    /// Starts the session; `tiles` says the client draws tiles, so the engine sends tile ids too.
    pub fn connect(transport: Box<dyn Transport>, tiles: bool) -> Self {
        Self::connect_with(transport, tiles, DEFAULT_MAX_HZ)
    }

    /// Like `connect`, with the most frames per second the client can show while an activity runs.
    pub fn connect_with(transport: Box<dyn Transport>, tiles: bool, max_hz: u16) -> Self {
        Self::open(
            transport,
            ConnectOptions {
                tiles,
                max_hz,
                locale: None,
            },
        )
    }

    /// Starts the session as `options` say.
    pub fn open(transport: Box<dyn Transport>, options: ConnectOptions) -> Self {
        let ConnectOptions {
            tiles,
            max_hz,
            locale,
        } = options;
        let mut m = Self {
            max_hz,
            chrome: locale.as_deref().and_then(Catalogue::client),
            locale,
            data_root: None,
            catalogue: None,
            soundpack: None,
            tiles,
            tiles_told: tiles,
            tint_ids: vec![],
            tints_told: vec![],
            display: None,
            tiles_retry: Duration::ZERO,
            transport,
            respawn: None,
            engine: None,
            waker: None,
            failure: None,
            heard: Duration::ZERO,
            stall: STALL,
            replica: Replica::default(),
            next_id: 0,
            pending: HashMap::new(),
            subscribing: false,
            connected: true,
            busy: false,
            screen_queue: 1,
            unanswered: 0,
            holding: None,
            running: HashSet::new(),
            command_id: None,
            sent: None,
            resend: None,
            origin: None,
            wanted: VIEW,
            declared: VIEW,
            resized: None,
            clock: Clock::default(),
            messages: vec![],
            told: vec![],
            sounds: vec![],
            native: vec![],
            ground: vec![],
            inventory: vec![],
            ground_cells: vec![],
            inventory_cells: vec![],
            ground_items: vec![],
            paged: Paged::default(),
            pages: Pages::default(),
            row_requests: HashMap::new(),
            explosion_tiles: HashMap::new(),
            volley: None,
            inventory_items: vec![],
            trace: vec![],
            received: 0,
            colors: ColorMap::default(),
            loading: None,
            shown: vec![],
            status: None,
            at_main_menu: false,
            hovered: None,
            describing: None,
            describe_wanted: None,
            describe_refused: None,
            overmap: Default::default(),
            panels: HashMap::new(),
        };
        m.hello();
        m
    }

    /// Opens the session with the delivery asked for now.
    fn hello(&mut self) {
        let params = protocol::HelloParams::new("bn-client")
            .with_viewport(protocol::Viewport {
                cols: VIEW.x as u32,
                rows: VIEW.y as u32,
            })
            .with_tiles(self.tiles);
        self.call("bn.hello", json!(self.with_session(params)), Pending::Hello);
    }

    /// A hello with the delivery this client needs.
    pub(super) fn with_session(&self, params: protocol::HelloParams) -> protocol::HelloParams {
        params
            .with_locale(self.locale.clone())
            .with_delivery(crate::transport::CREDIT_WINDOW, Some(self.max_hz))
    }

    /// Times the resize debounce with `clock`, so tests can advance it instead of sleeping.
    pub fn with_clock(mut self, clock: Clock) -> Self {
        self.clock = clock;
        self
    }

    /// How to start the engine again, for `restart`.
    pub fn with_respawn(mut self, respawn: Respawn) -> Self {
        self.respawn = Some(respawn);
        self
    }

    /// The engine binary the client started, for a failure that is about which engine it is.
    pub fn with_engine(mut self, engine: std::path::PathBuf) -> Self {
        self.engine = Some(engine);
        self
    }

    /// Silence that counts as a stall while a request is outstanding.
    pub fn with_stall(mut self, stall: Duration) -> Self {
        self.set_stall(stall);
        self
    }

    pub fn set_stall(&mut self, stall: Duration) {
        self.stall = stall;
    }

    /// Words the engine's LocalisedStrings from the catalogues under this game root.
    pub fn with_data_root(mut self, root: Option<std::path::PathBuf>) -> Self {
        self.data_root = root;
        self
    }

    /// Uses the game's own palette for look colours.
    pub fn with_colors(mut self, colors: ColorMap) -> Self {
        self.colors = colors;
        self
    }

    /// How many engine messages have arrived so far; live tests fail after 120 s without one, however
    /// busy the engine is: loading steps and activities send heartbeats.
    pub fn received(&self) -> u64 {
        self.received
    }

    /// Timestamped protocol milestones (`submit`, `received`, stage names, `events`) since the last call.
    pub fn take_trace(&mut self) -> Vec<(Instant, &'static str)> {
        std::mem::take(&mut self.trace)
    }

    pub(super) fn mark(&mut self, what: &'static str) {
        if self.trace.len() < 4096 {
            self.trace.push((Instant::now(), what));
        }
    }

    /// Sends a request and returns its id.
    pub(super) fn call(&mut self, method: &str, params: Value, kind: Pending) -> u64 {
        self.next_id += 1;
        self.pending.insert(self.next_id, kind);
        self.transport.send(request(self.next_id, method, params));
        self.next_id
    }

    pub(super) fn subscribe(&mut self) {
        if !self.subscribing {
            self.subscribing = true;
            self.call("bn.subscribe", json!({}), Pending::Subscribe);
        }
    }

    fn resubscribe_on(&mut self, outcome: Outcome) {
        if matches!(outcome, Outcome::Resubscribe(_)) {
            self.subscribe();
        }
    }

    /// Tells the transport what a hello granted; a new hello replaces the old grant, so an answer without
    /// one ends credits.
    pub(super) fn set_credits(&mut self, hello: &Value) {
        let window = serde_json::from_value::<protocol::Hello>(hello.clone())
            .ok()
            .and_then(|h| h.delivery)
            .map(|d| d.credits);
        self.transport.set_credits(window);
    }

    pub(super) fn handle(&mut self, msg: Incoming) {
        profiling::scope!("model.handle");
        match msg {
            Incoming::Response { id, result } => self.response(id, result),
            Incoming::SnapshotPart(p) => {
                let outcome = self.replica.part(p);
                self.resubscribe_on(outcome);
            }
            Incoming::Events(e) if !self.subscribing => {
                self.mark("events");
                let outcome = self.replica.events(e);
                self.resubscribe_on(outcome);
            }
            Incoming::Events(_) => {}
            Incoming::Command(n) => self.command_notice(n),
            Incoming::Resync(r) => {
                let outcome = self.replica.resync(&r);
                self.resubscribe_on(outcome);
            }
            Incoming::Loading(n) => {
                self.loading = match n {
                    LoadingNotice::Progress(p) => Some(Loading {
                        title: p.title,
                        entries: p.entries,
                        index: p.index,
                        image: p.image.map(|i| i.path),
                    }),
                    LoadingNotice::Done(_) => None,
                };
            }
            Incoming::Progress | Incoming::Unknown(_) => {}
            Incoming::Malformed(e) => self.tell(tr_format(
                "Bad engine message: %s",
                &[Arg::Text(e.to_string())],
            )),
            Incoming::Closed => {
                self.connected = false;
                self.unanswered = 0;
                self.running.clear();
                self.busy = false;
                self.tell(tr("Engine connection closed."));
                let message = match self.transport.exit_reason() {
                    Some(reason) => tr_format("The engine stopped (%s).", &[Arg::Text(reason)]),
                    None => tr("The engine stopped."),
                };
                self.failure = Some(Failure {
                    kind: FailureKind::Exited,
                    message,
                    stderr: self.transport.stderr_tail(),
                    engine: None,
                });
            }
        }
    }
}
