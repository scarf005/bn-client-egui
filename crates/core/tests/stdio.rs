#![cfg(unix)]
use bn_client_core::launch::LaunchArgs;
use bn_client_core::protocol::Incoming;
use bn_client_core::transport::{Profile, StdioTransport, Transport, request};
use clap::Parser;
use serde_json::json;
use std::os::unix::fs::PermissionsExt;
use std::time::{Duration, Instant};

fn recv(t: &mut StdioTransport) -> Incoming {
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        if let Some(m) = t.recv() {
            return m;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    panic!("no message in 10 s");
}

/// Writes an executable script into `dir` and spawns it with `spawn`. Test threads run concurrently, and a
/// child forked by another thread's `spawn` inherits our still-open write descriptor until it execs, which makes
/// executing the script fail with ETXTBSY; the lock keeps writing and spawning from interleaving.
fn spawn_script(
    dir: &std::path::Path,
    script: &str,
    spawn: impl FnOnce(&std::path::Path) -> std::io::Result<StdioTransport>,
) -> StdioTransport {
    static SPAWN: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _guard = SPAWN.lock().unwrap_or_else(|e| e.into_inner());
    std::fs::create_dir_all(dir).unwrap();
    let engine = dir.join("fake-engine");
    std::fs::write(&engine, script).unwrap();
    std::fs::set_permissions(&engine, std::fs::Permissions::from_mode(0o755)).unwrap();
    spawn(&engine).unwrap()
}

#[test]
fn stdio_transport_passes_private_dirs_and_speaks_line_delimited_json() {
    let dir = std::env::temp_dir().join(format!("bn-stdio-test-{}", std::process::id()));
    // Reads one request, answers it with the args it was started with, then exits.
    let script = "#!/bin/sh\nread line\nprintf '{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{\"args\":\"%s\"}}\\n' \"$*\"\nprintf 'not json\\n'\n";
    let mut t = spawn_script(&dir, script, StdioTransport::spawn_fresh);
    t.send(request(1, "bn.hello", json!({})));
    let Incoming::Response {
        id: 1,
        result: Ok(v),
    } = recv(&mut t)
    else {
        panic!("expected the hello response")
    };
    let args = v["args"].as_str().unwrap();
    assert!(args.starts_with("--client mcp --userdir "), "{args}");
    assert!(
        args.contains("bn-client-") && args.contains("--configdir"),
        "{args}"
    );
    assert!(matches!(recv(&mut t), Incoming::Malformed(_)));
    assert_eq!(recv(&mut t), Incoming::Closed);
    drop(t);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn waker_runs_for_every_incoming_message_so_the_ui_need_not_poll() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    let dir = std::env::temp_dir().join(format!("bn-stdio-wake-{}", std::process::id()));
    let script = "#!/bin/sh\nread line\nprintf '{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{}}\\n'\n";
    let mut t = spawn_script(&dir, script, StdioTransport::spawn_fresh);
    let woken = Arc::new(AtomicUsize::new(0));
    let w = woken.clone();
    t.set_waker(Arc::new(move || {
        w.fetch_add(1, Ordering::SeqCst);
    }));
    t.send(request(1, "bn.hello", json!({})));
    assert!(matches!(recv(&mut t), Incoming::Response { .. }));
    assert_eq!(recv(&mut t), Incoming::Closed);
    assert_eq!(
        woken.load(Ordering::SeqCst),
        2,
        "response and close each wake the UI"
    );
    drop(t);
    std::fs::remove_dir_all(dir).unwrap();
}

/// Starts a fake engine that reports its command-line arguments.
fn args_of(profile: Option<&Profile>) -> String {
    static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let n = N.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!("bn-stdio-args-{}-{n}", std::process::id()));
    let script = "#!/bin/sh\nread line\nprintf '{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{\"args\":\"%s\"}}\\n' \"$*\"\n";
    let mut t = spawn_script(&dir, script, |engine| match profile {
        Some(p) => StdioTransport::spawn_with(engine, p),
        None => StdioTransport::spawn(engine),
    });
    t.send(request(1, "bn.hello", json!({})));
    let Incoming::Response { result: Ok(v), .. } = recv(&mut t) else {
        panic!("expected the hello response")
    };
    drop(t);
    std::fs::remove_dir_all(dir).unwrap();
    v["args"].as_str().unwrap().to_string()
}

#[test]
fn default_profile_leaves_user_and_config_dirs_to_the_engine() {
    assert_eq!(args_of(None), "--client mcp");
}

#[test]
fn explicit_dirs_are_passed_through_unchanged() {
    let profile = Profile {
        userdir: Some("/data/user".into()),
        configdir: Some("/data/config".into()),
        ..Profile::default()
    };
    assert_eq!(
        args_of(Some(&profile)),
        "--client mcp --userdir /data/user --configdir /data/config"
    );
}

#[derive(Parser)]
struct Cli {
    #[command(flatten)]
    launch: LaunchArgs,
}

#[test]
fn profile_args_pick_fresh_and_dirs_and_keep_the_rest() {
    let args = [
        "bn",
        "--engine",
        "e",
        "--userdir",
        "/u",
        "--fresh-profile",
        "--configdir",
        "/c",
    ];
    let cli = Cli::try_parse_from(args).unwrap().launch;
    let p = cli.profile;
    assert!(p.fresh);
    assert_eq!(p.userdir.as_deref(), Some(std::path::Path::new("/u")));
    assert_eq!(p.configdir.as_deref(), Some(std::path::Path::new("/c")));
    assert_eq!(cli.engine.as_deref(), Some(std::path::Path::new("e")));
    assert!(Cli::try_parse_from(["bn", "--userdir"]).is_err());
}
