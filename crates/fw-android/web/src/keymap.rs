//! `KeyboardEvent.code` (physical key, layout independent) to Linux evdev keycodes (`linux/input-event-codes.h`),
//! which is what the compositor injects into the Android guest.

const TABLE: &[(&str, u32)] = &[
    ("Escape", 1), ("Digit1", 2), ("Digit2", 3), ("Digit3", 4), ("Digit4", 5), ("Digit5", 6), ("Digit6", 7),
    ("Digit7", 8), ("Digit8", 9), ("Digit9", 10), ("Digit0", 11), ("Minus", 12), ("Equal", 13), ("Backspace", 14),
    ("Tab", 15), ("KeyQ", 16), ("KeyW", 17), ("KeyE", 18), ("KeyR", 19), ("KeyT", 20), ("KeyY", 21), ("KeyU", 22),
    ("KeyI", 23), ("KeyO", 24), ("KeyP", 25), ("BracketLeft", 26), ("BracketRight", 27), ("Enter", 28),
    ("ControlLeft", 29), ("KeyA", 30), ("KeyS", 31), ("KeyD", 32), ("KeyF", 33), ("KeyG", 34), ("KeyH", 35),
    ("KeyJ", 36), ("KeyK", 37), ("KeyL", 38), ("Semicolon", 39), ("Quote", 40), ("Backquote", 41),
    ("ShiftLeft", 42), ("Backslash", 43), ("KeyZ", 44), ("KeyX", 45), ("KeyC", 46), ("KeyV", 47), ("KeyB", 48),
    ("KeyN", 49), ("KeyM", 50), ("Comma", 51), ("Period", 52), ("Slash", 53), ("ShiftRight", 54),
    ("NumpadMultiply", 55), ("AltLeft", 56), ("Space", 57), ("CapsLock", 58), ("F1", 59), ("F2", 60), ("F3", 61),
    ("F4", 62), ("F5", 63), ("F6", 64), ("F7", 65), ("F8", 66), ("F9", 67), ("F10", 68), ("NumLock", 69),
    ("ScrollLock", 70), ("Numpad7", 71), ("Numpad8", 72), ("Numpad9", 73), ("NumpadSubtract", 74), ("Numpad4", 75),
    ("Numpad5", 76), ("Numpad6", 77), ("NumpadAdd", 78), ("Numpad1", 79), ("Numpad2", 80), ("Numpad3", 81),
    ("Numpad0", 82), ("NumpadDecimal", 83), ("IntlBackslash", 86), ("F11", 87), ("F12", 88), ("IntlRo", 89),
    ("NumpadEnter", 96), ("ControlRight", 97), ("NumpadDivide", 98), ("PrintScreen", 99), ("AltRight", 100),
    ("Home", 102), ("ArrowUp", 103), ("PageUp", 104), ("ArrowLeft", 105), ("ArrowRight", 106), ("End", 107),
    ("ArrowDown", 108), ("PageDown", 109), ("Insert", 110), ("Delete", 111), ("AudioVolumeMute", 113),
    ("AudioVolumeDown", 114), ("AudioVolumeUp", 115), ("NumpadEqual", 117), ("Pause", 119), ("NumpadComma", 121),
    ("MetaLeft", 125), ("MetaRight", 126), ("ContextMenu", 127), ("BrowserBack", 158), ("BrowserForward", 159),
    ("MediaTrackNext", 163), ("MediaPlayPause", 164), ("MediaTrackPrevious", 165), ("MediaStop", 166),
    ("F13", 183), ("F14", 184), ("F15", 185), ("F16", 186), ("F17", 187), ("F18", 188), ("F19", 189), ("F20", 190),
    ("F21", 191), ("F22", 192), ("F23", 193), ("F24", 194),
];

/// Linux evdev keycode of a `KeyboardEvent.code` value, `None` for codes without an evdev equivalent.
pub fn evdev(code: &str) -> Option<u32> {
    TABLE.iter().find(|(name, _)| *name == code).map(|(_, value)| *value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn letters_digits_and_arrows_match_input_event_codes() {
        assert_eq!(evdev("KeyA"), Some(30));
        assert_eq!(evdev("Digit0"), Some(11));
        assert_eq!(evdev("Enter"), Some(28));
        assert_eq!(evdev("ArrowDown"), Some(108));
        assert_eq!(evdev("Unidentified"), None);
    }

    #[test]
    fn every_name_and_every_keycode_is_unique() {
        for (index, (name, value)) in TABLE.iter().enumerate() {
            for (other_name, other_value) in &TABLE[index + 1..] {
                assert_ne!(name, other_name);
                assert_ne!(value, other_value, "{name} and {other_name} share evdev code {value}");
            }
        }
    }
}
