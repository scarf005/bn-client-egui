use bn_client_egui::keys::KeyMap;

const JSON: &str = r#"[
 {"type":"keybinding","id":"pickup","category":"DEFAULTMODE","name":"Pick up","bindings":[{"input_method":"keyboard","key":"g"}]},
 {"type":"keybinding","id":"look","category":"DEFAULTMODE","name":"Look","bindings":[{"input_method":"keyboard","key":";"},{"input_method":"keyboard","key":"x"},{"input_method":"mouse","key":"LEFTCLICK"}]},
 {"type":"keybinding","id":"jump","category":"DEFAULTMODE","name":"Jump","bindings":[{"input_method":"keyboard","key":"SPACE"}]},
 {"type":"keybinding","id":"pickup","category":"ADVANCED_INVENTORY","name":"Other","bindings":[{"input_method":"keyboard","key":"z"}]},
 {"type":"keybinding","id":"toggle_map_memory","category":"DEFAULTMODE","name":"Toggle"},
 {"//":"comment only"}
]"#;

#[test]
fn only_keyboard_bindings_of_the_requested_context_are_read() {
    let k = KeyMap::from_json(JSON, "DEFAULTMODE").unwrap();
    assert_eq!(k.keys("pickup"), ["g"]);
    assert_eq!(k.keys("look"), [";", "x"]);
    assert!(k.keys("toggle_map_memory").is_empty());
    assert!(k.keys("nonexistent").is_empty());
}

#[test]
fn key_lookup_returns_every_action_bound_to_it() {
    let k = KeyMap::from_json(JSON, "DEFAULTMODE").unwrap();
    assert_eq!(k.actions_for("x"), ["look"]);
    assert_eq!(k.actions_for("SPACE"), ["jump"]);
    assert!(k.actions_for("z").is_empty(), "other contexts do not leak");
}

#[test]
fn movement_actions_fall_back_to_arrows_and_vi_keys() {
    let k = KeyMap::from_json(JSON, "DEFAULTMODE").unwrap();
    assert_eq!(k.actions_for("l"), ["RIGHT"]);
    assert_eq!(k.actions_for("y"), ["LEFTUP"]);
    assert_eq!(k.actions_for("UP"), ["UP"]);
    assert_eq!(k.keys("DOWN"), ["DOWN", "j"]);
}

#[test]
fn bad_json_is_rejected_and_default_keeps_only_movement() {
    assert!(KeyMap::from_json("nope", "DEFAULTMODE").is_none());
    assert_eq!(KeyMap::default().actions_for("h"), ["LEFT"]);
    assert!(KeyMap::default().actions_for("g").is_empty());
}

#[test]
fn load_reads_the_keybindings_under_the_game_root() {
    let dir = std::env::temp_dir().join(format!("bn-keys-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("data/raw/keybindings")).unwrap();
    std::fs::write(dir.join("data/raw/keybindings/keybindings.json"), JSON).unwrap();
    assert_eq!(KeyMap::load(&dir).keys("pickup"), ["g"]);
    std::fs::remove_dir_all(&dir).unwrap();
    assert!(KeyMap::load(&dir).keys("pickup").is_empty());
}
