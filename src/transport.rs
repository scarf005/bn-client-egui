//! Newline-delimited JSON-RPC 2.0 transports. No async runtime: the stdio transport reads on a
//! background thread and the UI thread polls a channel.

use crate::protocol::{self, Incoming};
use serde_json::{Value, json};
use std::collections::{HashMap, VecDeque};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};

pub trait Transport {
    /// Sends one JSON-RPC message (request or notification).
    fn send(&mut self, msg: Value);
    /// The next parsed incoming message, without blocking.
    fn recv(&mut self) -> Option<Incoming>;
}

pub fn request(id: u64, method: &str, params: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params })
}

/// Engine child process speaking JSON-RPC on stdin/stdout, with private user/config dirs.
pub struct StdioTransport {
    child: Child,
    out: Sender<String>,
    incoming: Receiver<Incoming>,
    dir: PathBuf,
}

impl StdioTransport {
    pub fn spawn(engine: &Path) -> std::io::Result<Self> {
        let dir = std::env::temp_dir().join(format!("bn-client-egui-{}", std::process::id()));
        let (user, config) = (dir.join("user"), dir.join("config"));
        std::fs::create_dir_all(&user)?;
        std::fs::create_dir_all(&config)?;
        let mut child = Command::new(engine)
            .args(["--client", "mcp", "--userdir"])
            .arg(&user)
            .arg("--configdir")
            .arg(&config)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;
        let stdin = child.stdin.take().expect("piped stdin");
        let stdout = child.stdout.take().expect("piped stdout");
        let (tx, incoming) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else { break };
                if !line.trim().is_empty() && tx.send(protocol::parse_line(&line)).is_err() {
                    return;
                }
            }
            let _ = tx.send(Incoming::Closed);
        });
        let (out, lines) = mpsc::channel::<String>();
        std::thread::spawn(move || {
            let mut stdin = stdin;
            for line in lines {
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
            dir,
        })
    }
}

impl Transport for StdioTransport {
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
        let _ = std::fs::remove_dir_all(&self.dir);
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
