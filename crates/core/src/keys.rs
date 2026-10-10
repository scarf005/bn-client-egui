//! Names of the keys the engine does not send as typed text. Which key runs which action is the
//! engine's to say: it sends `keys` with every action, including the player's own rebinds.

/// The engine's name for a key pressed with modifiers: `CTRL+S` for a control letter, `BACKTAB` for
/// Shift+Tab, else `base` unchanged (shifted letters and symbols already arrive as typed text).
pub fn chord(base: &str, ctrl: bool, shift: bool) -> String {
    match base {
        "TAB" if shift => "BACKTAB".into(),
        b if ctrl && b.len() == 1 && b.as_bytes()[0].is_ascii_alphabetic() => {
            format!("CTRL+{}", b.to_ascii_uppercase())
        }
        b => b.into(),
    }
}

/// The engine's name for function key `n` (1 to 12).
pub fn function(n: u8) -> String {
    format!("F{n}")
}

/// The engine's name for the numpad key a typed digit may have come from.
pub fn numpad(digit: &str) -> Option<String> {
    (digit.len() == 1 && digit.as_bytes()[0].is_ascii_digit()).then(|| format!("NUMPAD_{digit}"))
}
