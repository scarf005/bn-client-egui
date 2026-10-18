//! A connection in another language than the engine's reads LocalisedString log lines from the engine's own
//! catalogue under the game root.
mod common;
use bn_client_core::transport::ScriptedTransport;
use bn_client_core::{EngineModel, GameModel};
use common::{fixture, subscribe_reply};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

/// The bytes of a `.mo` file: the header and these entries, sorted as the format requires.
fn mo(entries: &[(&str, &str)]) -> Vec<u8> {
    let header = "Content-Type: text/plain; charset=UTF-8\nPlural-Forms: nplurals=1; plural=0;\n";
    let mut all = vec![("", header)];
    all.extend_from_slice(entries);
    all.sort();
    let n = all.len() as u32;
    let (mut originals, mut translations, mut strings) = (vec![], vec![], vec![]);
    for (id, text) in &all {
        originals.push((id.len() as u32, 28 + 16 * n + strings.len() as u32));
        strings.extend_from_slice(id.as_bytes());
        strings.push(0);
        translations.push((text.len() as u32, 28 + 16 * n + strings.len() as u32));
        strings.extend_from_slice(text.as_bytes());
        strings.push(0);
    }
    let mut bytes = vec![];
    for word in [0x950412de_u32, 0, n, 28, 28 + 8 * n, 0, 0] {
        bytes.extend_from_slice(&word.to_le_bytes());
    }
    for (length, offset) in originals.into_iter().chain(translations) {
        bytes.extend_from_slice(&length.to_le_bytes());
        bytes.extend_from_slice(&offset.to_le_bytes());
    }
    bytes.extend_from_slice(&strings);
    bytes
}

/// A game root with the Korean catalogue.
fn root(tag: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("bn-localised-{tag}-{}", std::process::id()));
    let dir = root.join("lang/mo/ko/LC_MESSAGES");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("cataclysm-bn.mo"),
        mo(&[
            ("The %s dies!", "%s이(가) 죽었다!"),
            ("%s x %d", "%s x %d회"),
        ]),
    )
    .unwrap();
    root
}

/// A session of an engine that speaks English, with a connection that asked for `locale`.
fn session(locale: &str) -> (ScriptedTransport, bn_client_core::transport::ScriptHandle) {
    let mut hello = fixture("hello_result");
    hello["result"]["locale"] = json!(locale);
    let (t, h) = ScriptedTransport::new();
    (
        t.on("bn.hello", vec![hello])
            .on("bn.subscribe", subscribe_reply()),
        h,
    )
}

fn logged(sequence: u32, id: &str, count: u32) -> Value {
    let text = json!({"key": "The %s dies!", "args": ["좀비"]});
    let label = if count > 1 {
        json!({"key": "%s x %d", "args": [text, count]})
    } else {
        text.clone()
    };
    json!({"jsonrpc": "2.0", "method": "bn.events", "params": {"epoch": "e7f3", "events": [{
        "sequence": sequence.to_string(), "revision": "32", "type": "message.logged",
        "data": {"id": id, "text": text, "kind": "good", "color": "c_light_green", "count": count, "label": label}}]}})
}

fn shown(model: &EngineModel) -> Vec<String> {
    model.native_log().iter().map(|l| l.shown()).collect()
}

fn connect(
    locale: &str,
    root: Option<&Path>,
) -> (EngineModel, bn_client_core::transport::ScriptHandle) {
    let (t, h) = session(locale);
    let mut model = EngineModel::new(Box::new(t)).with_data_root(root.map(Path::to_path_buf));
    model.poll();
    assert!(model.live());
    (model, h)
}

#[test]
fn a_korean_connection_reads_a_converted_line_and_its_repeat_in_korean() {
    let root = root("ko");
    let (mut model, h) = connect("ko", Some(&root));
    h.push(fixture("events_41_walk"));
    h.push(logged(42, "9", 1));
    h.push(logged(43, "9", 2));
    model.poll();
    assert_eq!(shown(&model), ["좀비이(가) 죽었다! x 2회"]);
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn a_connection_in_a_dialect_reads_its_languages_catalogue_as_the_engine_does() {
    let root = root("ko_KR");
    let (mut model, h) = connect("ko_KR", Some(&root));
    h.push(fixture("events_41_walk"));
    h.push(logged(42, "9", 1));
    model.poll();
    assert_eq!(shown(&model), ["좀비이(가) 죽었다!"]);
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn a_connection_in_the_engines_language_reads_the_format_itself() {
    let root = root("en");
    let (mut model, h) = connect("en", Some(&root));
    h.push(fixture("events_41_walk"));
    h.push(logged(42, "9", 1));
    model.poll();
    assert_eq!(shown(&model), ["The 좀비 dies!"]);
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn a_language_without_a_catalogue_reads_the_format_itself() {
    let (mut model, h) = connect("ko", None);
    h.push(fixture("events_41_walk"));
    h.push(logged(42, "9", 1));
    model.poll();
    assert_eq!(shown(&model), ["The 좀비 dies!"]);
}

#[test]
fn a_snapshot_log_is_worded_the_same_way() {
    let root = root("snapshot");
    let mut subscribe = subscribe_reply();
    subscribe[0]["result"]["messages"] = json!([{
        "id": "3", "text": {"key": "The %s dies!", "args": ["좀비"]}, "kind": "good",
        "color": "c_light_green", "count": 1, "label": {"key": "The %s dies!", "args": ["좀비"]}
    }]);
    let (t, _) = ScriptedTransport::new();
    let mut hello = fixture("hello_result");
    hello["result"]["locale"] = json!("ko");
    let mut model = EngineModel::new(Box::new(
        t.on("bn.hello", vec![hello]).on("bn.subscribe", subscribe),
    ))
    .with_data_root(Some(root.clone()));
    model.poll();
    assert_eq!(shown(&model), ["좀비이(가) 죽었다!"]);
    std::fs::remove_dir_all(&root).unwrap();
}

/// Runs when `BN_ROOT` names a game root with compiled catalogues: every one of them is readable, whatever
/// its plural rule.
#[test]
fn every_compiled_catalogue_of_the_game_root_is_readable() {
    let Some(root) = std::env::var_os("BN_ROOT").map(PathBuf::from) else {
        return;
    };
    let Ok(languages) = std::fs::read_dir(root.join("lang/mo")) else {
        return;
    };
    for language in languages.flatten() {
        let name = language.file_name();
        let name = name.to_string_lossy();
        let path = root
            .join("lang/mo")
            .join(&*name)
            .join("LC_MESSAGES/cataclysm-bn.mo");
        if let Ok(bytes) = std::fs::read(path) {
            bn_client_core::localise::Catalogue::parse(&bytes)
                .unwrap_or_else(|e| panic!("{name}: {e}"));
        }
    }
}
