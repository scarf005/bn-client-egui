//! A test that starts an engine must never run it on the developer's own profile: that would read
//! and write their saves, options and debug log, and a second engine on it can end a first one.

use std::path::Path;

fn sources(dir: &Path, found: &mut Vec<std::path::PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap().flatten() {
        let path = entry.path();
        if path.is_dir() {
            sources(&path, found);
        } else if path.extension().is_some_and(|e| e == "rs") {
            found.push(path);
        }
    }
}

#[test]
fn no_test_spawns_an_engine_on_the_default_profile() {
    let crates = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut files = vec![];
    for member in std::fs::read_dir(&crates).unwrap().flatten() {
        let tests = member.path().join("tests");
        if tests.is_dir() {
            sources(&tests, &mut files);
        }
    }
    let offenders = files
        .iter()
        .filter(|f| !f.ends_with("no_real_profile.rs"))
        .filter(|f| {
            std::fs::read_to_string(f)
                .unwrap()
                .contains("StdioTransport::spawn(")
        })
        .collect::<Vec<_>>();
    assert!(
        offenders.is_empty(),
        "use StdioTransport::spawn_fresh (or spawn_with an explicit throwaway profile): {offenders:?}"
    );
}
