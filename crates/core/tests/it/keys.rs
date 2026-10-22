use bn_client_core::keys::{Key, Mods, name, numpad};

const CTRL: Mods = Mods {
    ctrl: true,
    shift: false,
};
const SHIFT: Mods = Mods {
    ctrl: false,
    shift: true,
};

#[test]
fn modified_keys_get_the_engines_names() {
    assert_eq!(name(Key::Char('s'), CTRL).as_deref(), Some("CTRL+S"));
    assert_eq!(name(Key::Tab, SHIFT).as_deref(), Some("BACKTAB"));
    assert_eq!(name(Key::Tab, Mods::default()).as_deref(), Some("TAB"));
    assert_eq!(
        name(Key::Up, CTRL).as_deref(),
        Some("UP"),
        "only letters take CTRL+"
    );
    assert_eq!(name(Key::F(4), Mods::default()).as_deref(), Some("F4"));
    assert_eq!(numpad("8").as_deref(), Some("NUMPAD_8"));
    assert_eq!(numpad("x"), None);
}

#[test]
fn control_letters_with_their_own_names_take_them_as_the_native_game_does() {
    assert_eq!(name(Key::Char('i'), CTRL).as_deref(), Some("TAB"));
    assert_eq!(name(Key::Char('J'), CTRL).as_deref(), Some("RETURN"));
    assert_eq!(
        name(Key::Char('1'), CTRL).as_deref(),
        Some("1"),
        "the engine has no CTRL+1"
    );
    assert_eq!(
        name(Key::Char(' '), Mods::default()).as_deref(),
        Some("SPACE")
    );
}

#[test]
fn function_keys_past_the_engines_last_get_no_name() {
    assert_eq!(name(Key::F(15), Mods::default()).as_deref(), Some("F15"));
    assert_eq!(name(Key::F(16), Mods::default()), None);
    assert_eq!(name(Key::F(0), Mods::default()), None);
}
