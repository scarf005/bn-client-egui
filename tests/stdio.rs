#![cfg(unix)]
use bn_client_egui::protocol::Incoming;
use bn_client_egui::transport::{StdioTransport, Transport, request};
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

#[test]
fn stdio_transport_passes_private_dirs_and_speaks_line_delimited_json() {
    let dir = std::env::temp_dir().join(format!("bn-stdio-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let engine = dir.join("fake-engine");
    // Reads one request, answers it with the args it was started with, then exits.
    std::fs::write(
        &engine,
        "#!/bin/sh\nread line\nprintf '{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{\"args\":\"%s\"}}\\n' \"$*\"\nprintf 'not json\\n'\n",
    )
    .unwrap();
    std::fs::set_permissions(&engine, std::fs::Permissions::from_mode(0o755)).unwrap();

    let mut t = StdioTransport::spawn(&engine).unwrap();
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
        args.contains("bn-client-egui-") && args.contains("--configdir"),
        "{args}"
    );
    assert!(matches!(recv(&mut t), Incoming::Malformed(_)));
    assert_eq!(recv(&mut t), Incoming::Closed);
    drop(t);
    std::fs::remove_dir_all(dir).unwrap();
}
