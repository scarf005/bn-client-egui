//! Newline-delimited JSON-RPC 2.0 transports. No async runtime: the stdio transport reads on a
//! background thread and the UI thread polls a channel.

use crate::protocol::{self, Incoming};
use serde_json::{Value, json};
use std::collections::{HashMap, VecDeque};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::Instant;

/// Called from the reader thread whenever a message arrives, e.g. to request a UI repaint.
pub type Waker = Arc<dyn Fn() + Send + Sync>;

pub trait Transport {
    /// Registers a callback run for every incoming message.
    fn set_waker(&mut self, _waker: Waker) {}
    /// Sends one JSON-RPC message (request or notification).
    fn send(&mut self, msg: Value);
    /// The next parsed incoming message, without blocking.
    fn recv(&mut self) -> Option<Incoming>;
}

pub fn request(id: u64, method: &str, params: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params })
}

/// Directory holding the engine's game data: `env_root` (from `BN_ROOT`) if set, else the nearest ancestor of
/// the engine binary that contains `data/json/`. A CMake build tree also has a `data/` directory, so plain
/// `data/` is not enough. The engine resolves `data/` against this base path.
pub fn game_root(engine: &Path, env_root: Option<PathBuf>) -> Option<PathBuf> {
    env_root.or_else(|| {
        std::fs::canonicalize(engine)
            .ok()?
            .ancestors()
            .skip(1)
            .find(|dir| dir.join("data/json").is_dir())
            .map(Path::to_path_buf)
    })
}

/// Appends every JSON-RPC line with a millisecond offset (`->` sent, `<-` received) to the file named by
/// `BN_CLIENT_TRACE`, if set.
#[derive(Clone)]
struct Trace(Option<Arc<Mutex<(std::fs::File, Instant)>>>);

impl Trace {
    fn from_env() -> Self {
        let file = std::env::var_os("BN_CLIENT_TRACE")
            .and_then(|p| std::fs::File::create(p).ok())
            .map(|f| Arc::new(Mutex::new((f, Instant::now()))));
        Self(file)
    }

    fn line(&self, arrow: &str, line: &str) {
        if let Some(t) = &self.0 {
            let (file, start) = &mut *t.lock().unwrap();
            let _ = writeln!(file, "{:>8} {arrow} {line}", start.elapsed().as_millis());
        }
    }
}

/// Which user and config directories the engine runs with.
#[derive(clap::Args, Clone, Debug, Default, PartialEq, Eq)]
pub struct Profile {
    /// Run in a private throwaway profile deleted on exit, instead of the engine's own default profile
    #[arg(id = "fresh", long = "fresh-profile", env = "BN_FRESH_PROFILE")]
    pub fresh: bool,
    /// Engine `--userdir` (saves); ignored with --fresh-profile
    #[arg(long, value_name = "PATH")]
    pub userdir: Option<PathBuf>,
    /// Engine `--configdir` (options, keybindings); ignored with --fresh-profile
    #[arg(long, value_name = "PATH")]
    pub configdir: Option<PathBuf>,
}

impl Profile {
    pub fn fresh() -> Self {
        Self {
            fresh: true,
            ..Self::default()
        }
    }
}

/// Engine child process speaking JSON-RPC on stdin/stdout.
pub struct StdioTransport {
    child: Child,
    out: Sender<String>,
    incoming: Receiver<Incoming>,
    waker: Arc<Mutex<Option<Waker>>>,
    /// The throwaway profile directory, removed on drop.
    dir: Option<PathBuf>,
}

impl StdioTransport {
    /// Runs the engine on its own default user and config directories (your real saves and options).
    pub fn spawn(engine: &Path) -> std::io::Result<Self> {
        Self::spawn_with(engine, &Profile::default())
    }

    /// Runs the engine in a private throwaway profile, for tests.
    pub fn spawn_fresh(engine: &Path) -> std::io::Result<Self> {
        Self::spawn_with(engine, &Profile::fresh())
    }

    pub fn spawn_with(engine: &Path, profile: &Profile) -> std::io::Result<Self> {
        let (dir, userdir, configdir) = if profile.fresh {
            static SPAWNED: AtomicUsize = AtomicUsize::new(0);
            let n = SPAWNED.fetch_add(1, Ordering::Relaxed);
            let dir = std::env::temp_dir().join(format!("bn-client-{}-{n}", std::process::id()));
            let (user, config) = (dir.join("user"), dir.join("config"));
            std::fs::create_dir_all(&user)?;
            std::fs::create_dir_all(&config)?;
            (Some(dir), Some(user), Some(config))
        } else {
            (None, profile.userdir.clone(), profile.configdir.clone())
        };
        let mut command = Command::new(engine);
        if let Some(root) = game_root(engine, std::env::var_os("BN_ROOT").map(PathBuf::from)) {
            command
                .arg("--basepath")
                .arg(format!("{}/", root.display()));
        }
        command.args(["--client", "mcp"]);
        // The engine appends file names to these without a separator, so each must end in one.
        let dir_arg = |dir: &Path| {
            let mut arg = dir.as_os_str().to_os_string();
            if !arg.to_string_lossy().ends_with(std::path::MAIN_SEPARATOR) {
                arg.push(std::path::MAIN_SEPARATOR_STR);
            }
            arg
        };
        if let Some(user) = &userdir {
            command.arg("--userdir").arg(dir_arg(user));
        }
        if let Some(config) = &configdir {
            command.arg("--configdir").arg(dir_arg(config));
        }
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;
        let stdin = child.stdin.take().expect("piped stdin");
        let stdout = child.stdout.take().expect("piped stdout");
        let trace = Trace::from_env();
        let (tx, incoming) = mpsc::channel();
        let waker: Arc<Mutex<Option<Waker>>> = Arc::default();
        let wake = waker.clone();
        let notify = move |msg| {
            let sent = tx.send(msg).is_ok();
            if let Some(w) = wake.lock().unwrap().as_ref() {
                w();
            }
            sent
        };
        let read_trace = trace.clone();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else { break };
                read_trace.line("<-", &line);
                if !line.trim().is_empty() && !notify(protocol::parse_line(&line)) {
                    return;
                }
            }
            notify(Incoming::Closed);
        });
        let (out, lines) = mpsc::channel::<String>();
        std::thread::spawn(move || {
            let mut stdin = stdin;
            for line in lines {
                trace.line("->", &line);
                if writeln!(stdin, "{line}")
                    .and_then(|()| stdin.flush())
                    .is_err()
                {
                    return;
                }
            }
        });
        Ok(Self {
            child,
            out,
            incoming,
            waker,
            dir,
        })
    }
}

impl Transport for StdioTransport {
    fn set_waker(&mut self, waker: Waker) {
        *self.waker.lock().unwrap() = Some(waker);
    }

    fn send(&mut self, msg: Value) {
        let _ = self.out.send(msg.to_string());
    }

    fn recv(&mut self) -> Option<Incoming> {
        self.incoming.try_recv().ok()
    }
}

impl Drop for StdioTransport {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        if let Some(dir) = &self.dir {
            let _ = std::fs::remove_dir_all(dir);
        }
    }
}

/// Test handle shared with a `ScriptedTransport` that was moved into a model.
#[derive(Clone, Default)]
pub struct ScriptHandle {
    sent: Arc<Mutex<Vec<Value>>>,
    inbox: Arc<Mutex<VecDeque<Value>>>,
}

impl ScriptHandle {
    /// Every message the client has sent, in order.
    pub fn sent(&self) -> Vec<Value> {
        self.sent.lock().unwrap().clone()
    }

    pub fn sent_methods(&self) -> Vec<String> {
        let sent = self.sent();
        sent.iter()
            .filter_map(|m| m["method"].as_str())
            .map(String::from)
            .collect()
    }

    /// Queues an unsolicited engine message.
    pub fn push(&self, msg: Value) {
        self.inbox.lock().unwrap().push_back(msg);
    }
}

/// Replays canned engine messages: each request for a method pops the next scripted reply group
/// for that method; a response in the group gets the request's id.
#[derive(Default)]
pub struct ScriptedTransport {
    replies: HashMap<String, VecDeque<Vec<Value>>>,
    handle: ScriptHandle,
}

impl ScriptedTransport {
    pub fn new() -> (Self, ScriptHandle) {
        let t = Self::default();
        let h = t.handle.clone();
        (t, h)
    }

    pub fn on(mut self, method: &str, reply: Vec<Value>) -> Self {
        self.replies
            .entry(method.to_string())
            .or_default()
            .push_back(reply);
        self
    }
}

impl Transport for ScriptedTransport {
    fn send(&mut self, msg: Value) {
        self.handle.sent.lock().unwrap().push(msg.clone());
        let group = msg["method"]
            .as_str()
            .and_then(|m| self.replies.get_mut(m)?.pop_front());
        let mut inbox = self.handle.inbox.lock().unwrap();
        for mut reply in group.unwrap_or_default() {
            if reply.get("method").is_none() {
                reply["id"] = msg["id"].clone();
            }
            inbox.push_back(reply);
        }
    }

    fn recv(&mut self) -> Option<Incoming> {
        let msg = self.handle.inbox.lock().unwrap().pop_front()?;
        Some(protocol::parse_message(&msg).unwrap_or_else(|e| Incoming::Malformed(e.to_string())))
    }
}

#[cfg(test)]
mod root_tests {
    use super::game_root;
    use std::path::PathBuf;

    #[test]
    fn game_root_is_the_nearest_ancestor_with_data() {
        let base = std::env::temp_dir().join(format!("bn-root-test-{}", std::process::id()));
        let bin = base.join("out/build/src");
        std::fs::create_dir_all(base.join("data/json")).unwrap();
        // A build tree's own data/ (no json/) must not stop the search.
        std::fs::create_dir_all(base.join("out/build/data")).unwrap();
        std::fs::create_dir_all(&bin).unwrap();
        let engine = bin.join("engine");
        std::fs::write(&engine, "").unwrap();
        let found = game_root(&engine, None).unwrap();
        assert_eq!(found, std::fs::canonicalize(&base).unwrap());
        assert_eq!(
            game_root(&engine, Some(PathBuf::from("/x"))),
            Some(PathBuf::from("/x"))
        );
        assert_eq!(game_root(&base.join("missing"), None), None);
        std::fs::remove_dir_all(&base).unwrap();
    }
}
