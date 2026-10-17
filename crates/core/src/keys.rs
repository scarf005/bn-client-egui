//! Names of the keys the engine does not send as typed text. Which key runs which action is the
//! engine's to say: it sends `keys` with every action, including the player's own rebinds.

/// A pressed key, whatever device it came from. Each client maps only its own device keys to this.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    Char(char),
    Up,
    Down,
    Left,
    Right,
    Home,
    End,
    PageUp,
    PageDown,
    Enter,
    Tab,
    Backspace,
    Delete,
    Escape,
    /// Function key `n`.
    F(u8),
}

/// The modifiers held with a key. Shifted letters and symbols already arrive as typed text.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Mods {
    pub ctrl: bool,
    pub shift: bool,
}

/// The engine's name for a key, as its keybinding files spell it, or `None` for a key the engine has no
/// name for. Like the native game, Ctrl takes only letters (`CTRL+S`; Ctrl+I and Ctrl+J are the control
/// codes of `TAB` and `RETURN`) and Shift only Tab (`BACKTAB`).
pub fn name(key: Key, mods: Mods) -> Option<String> {
    let named = match key {
        Key::Char(' ') => "SPACE",
        Key::Char(c) if mods.ctrl && c.is_ascii_alphabetic() => match c.to_ascii_uppercase() {
            'I' => "TAB",
            'J' => "RETURN",
            upper => return Some(format!("CTRL+{upper}")),
        },
        Key::Char(c) => return Some(c.to_string()),
        Key::Up => "UP",
        Key::Down => "DOWN",
        Key::Left => "LEFT",
        Key::Right => "RIGHT",
        Key::Home => "HOME",
        Key::End => "END",
        Key::PageUp => "PPAGE",
        Key::PageDown => "NPAGE",
        Key::Enter => "RETURN",
        Key::Tab if mods.shift => "BACKTAB",
        Key::Tab => "TAB",
        Key::Backspace => "BACKSPACE",
        Key::Delete => "DELETE",
        Key::Escape => "ESC",
        Key::F(n @ 1..=15) => return Some(format!("F{n}")),
        Key::F(_) => return None,
    };
    Some(named.into())
}

/// The engine's name for the numpad key a typed digit may have come from.
pub fn numpad(digit: &str) -> Option<String> {
    (digit.len() == 1 && digit.as_bytes()[0].is_ascii_digit()).then(|| format!("NUMPAD_{digit}"))
}
