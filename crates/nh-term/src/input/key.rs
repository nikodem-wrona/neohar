//! What a key event is, and the tables that the key encoders share.

use nh_proto::input::{KeyKind, LogicalKey, Mods};

use super::{InputModes, KeyConfig, kitty, legacy};

/// A navigation key. The same names serve the keypad when NumLock is off.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Nav {
    Up,
    Down,
    Right,
    Left,
    Home,
    End,
    Begin,
    Insert,
    Delete,
    PageUp,
    PageDown,
}

impl Nav {
    /// The last byte of the `CSI` and `SS3` form (`CSI A`), for the keys that have one.
    pub(super) fn letter(self) -> Option<u8> {
        match self {
            Nav::Up => Some(b'A'),
            Nav::Down => Some(b'B'),
            Nav::Right => Some(b'C'),
            Nav::Left => Some(b'D'),
            Nav::End => Some(b'F'),
            Nav::Begin => Some(b'E'),
            Nav::Home => Some(b'H'),
            Nav::Insert | Nav::Delete | Nav::PageUp | Nav::PageDown => None,
        }
    }

    /// The number of the `CSI n ~` form, for the keys that have one.
    pub(super) fn tilde(self) -> Option<u32> {
        match self {
            Nav::Insert => Some(2),
            Nav::Delete => Some(3),
            Nav::PageUp => Some(5),
            Nav::PageDown => Some(6),
            _ => None,
        }
    }

    /// The code of the keypad key in the kitty protocol, for the keypad keys that have one.
    pub(super) fn keypad_code(self) -> Option<u32> {
        match self {
            Nav::Left => Some(57417),
            Nav::Right => Some(57418),
            Nav::Up => Some(57419),
            Nav::Down => Some(57420),
            Nav::PageUp => Some(57421),
            Nav::PageDown => Some(57422),
            Nav::Home => Some(57423),
            Nav::End => Some(57424),
            Nav::Insert => Some(57425),
            Nav::Delete => Some(57426),
            Nav::Begin => None,
        }
    }
}

/// A key of the numeric keypad that produces a character or Enter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Kp {
    Digit(u8),
    Decimal,
    Divide,
    Multiply,
    Subtract,
    Add,
    Enter,
    Equal,
    Separator,
}

impl Kp {
    /// The character that the key produces. `None` for Enter.
    pub(super) fn text(self) -> Option<char> {
        Some(match self {
            Kp::Digit(n) => char::from(b'0' + n),
            Kp::Decimal => '.',
            Kp::Divide => '/',
            Kp::Multiply => '*',
            Kp::Subtract => '-',
            Kp::Add => '+',
            Kp::Equal => '=',
            Kp::Separator => ',',
            Kp::Enter => return None,
        })
    }

    /// The last byte of the `SS3` form in application keypad mode (`ctlseqs`, "VT100 Keypad").
    pub(super) fn app_final(self) -> u8 {
        match self {
            Kp::Digit(n) => b'p' + n,
            Kp::Decimal => b'n',
            Kp::Divide => b'o',
            Kp::Multiply => b'j',
            Kp::Subtract => b'm',
            Kp::Add => b'k',
            Kp::Enter => b'M',
            Kp::Equal => b'X',
            Kp::Separator => b'l',
        }
    }

    /// The code of the key in the kitty protocol.
    pub(super) fn kitty_code(self) -> u32 {
        match self {
            Kp::Digit(n) => 57399 + u32::from(n),
            Kp::Decimal => 57409,
            Kp::Divide => 57410,
            Kp::Multiply => 57411,
            Kp::Subtract => 57412,
            Kp::Add => 57413,
            Kp::Enter => 57414,
            Kp::Equal => 57415,
            Kp::Separator => 57416,
        }
    }
}

/// A key that only changes the modifiers or a lock. The kitty protocol reports them with flag 8.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Standalone {
    LeftShift,
    LeftControl,
    LeftAlt,
    LeftSuper,
    RightShift,
    RightControl,
    RightAlt,
    RightSuper,
    IsoLevel3Shift,
    CapsLock,
    ScrollLock,
    NumLock,
}

impl Standalone {
    pub(super) fn kitty_code(self) -> u32 {
        match self {
            Standalone::CapsLock => 57358,
            Standalone::ScrollLock => 57359,
            Standalone::NumLock => 57360,
            Standalone::LeftShift => 57441,
            Standalone::LeftControl => 57442,
            Standalone::LeftAlt => 57443,
            Standalone::LeftSuper => 57444,
            Standalone::RightShift => 57447,
            Standalone::RightControl => 57448,
            Standalone::RightAlt => 57449,
            Standalone::RightSuper => 57450,
            Standalone::IsoLevel3Shift => 57453,
        }
    }
}

/// The kind of a key, found from `key`, `code` and `location`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Key<'a> {
    /// A key that produces text. The string is `key`.
    Text(&'a str),
    Enter,
    Tab,
    Backspace,
    Escape,
    Nav(Nav),
    /// A function key, 1 to 35.
    F(u8),
    /// A keypad key that produces a character, or Enter.
    Keypad(Kp),
    /// A keypad key with NumLock off. It acts as a navigation key.
    KeypadNav(Nav),
    Standalone(Standalone),
    Menu,
    PrintScreen,
    Pause,
    /// A key that no terminal encoding has: media keys, `Dead`, `Unidentified`, and so on.
    Ignored,
}

impl<'a> Key<'a> {
    pub(super) fn of(event: &'a LogicalKey) -> Key<'a> {
        if let Some(kp) = keypad(event) {
            return kp;
        }
        let key = event.key.as_str();
        match key {
            "Enter" => Key::Enter,
            "Tab" => Key::Tab,
            "Backspace" => Key::Backspace,
            "Escape" => Key::Escape,
            "ArrowUp" => Key::Nav(Nav::Up),
            "ArrowDown" => Key::Nav(Nav::Down),
            "ArrowRight" => Key::Nav(Nav::Right),
            "ArrowLeft" => Key::Nav(Nav::Left),
            "Home" => Key::Nav(Nav::Home),
            "End" => Key::Nav(Nav::End),
            "Insert" => Key::Nav(Nav::Insert),
            "Delete" => Key::Nav(Nav::Delete),
            "PageUp" => Key::Nav(Nav::PageUp),
            "PageDown" => Key::Nav(Nav::PageDown),
            "ContextMenu" => Key::Menu,
            "PrintScreen" => Key::PrintScreen,
            "Pause" => Key::Pause,
            "CapsLock" => Key::Standalone(Standalone::CapsLock),
            "ScrollLock" => Key::Standalone(Standalone::ScrollLock),
            "NumLock" => Key::Standalone(Standalone::NumLock),
            "Shift" => Key::Standalone(side(
                event.location,
                Standalone::LeftShift,
                Standalone::RightShift,
            )),
            "Control" => Key::Standalone(side(
                event.location,
                Standalone::LeftControl,
                Standalone::RightControl,
            )),
            "Alt" => Key::Standalone(side(
                event.location,
                Standalone::LeftAlt,
                Standalone::RightAlt,
            )),
            "Meta" | "OS" => Key::Standalone(side(
                event.location,
                Standalone::LeftSuper,
                Standalone::RightSuper,
            )),
            "AltGraph" => Key::Standalone(Standalone::IsoLevel3Shift),
            _ => function_key(key).map_or_else(|| text_or_ignored(key), Key::F),
        }
    }
}

fn side(location: u8, left: Standalone, right: Standalone) -> Standalone {
    if location == 2 { right } else { left }
}

/// `F1` to `F35`.
fn function_key(key: &str) -> Option<u8> {
    let number = key.strip_prefix('F')?;
    if number.is_empty() || number.starts_with('0') || number.len() > 2 {
        return None;
    }
    let n: u8 = number.parse().ok()?;
    (1..=35).contains(&n).then_some(n)
}

/// A key string that is a character or a few, as opposed to a name such as `MediaPlay`.
fn text_or_ignored(key: &str) -> Key<'_> {
    let mut chars = key.chars();
    let Some(first) = chars.next() else {
        return Key::Ignored;
    };
    let single = chars.next().is_none();
    if single {
        // C0 and C1 controls are not text. Some browsers report them for Ctrl chords.
        return if first.is_control() {
            Key::Ignored
        } else {
            Key::Text(key)
        };
    }
    // A name is ASCII letters and digits ("Unidentified", "MediaPlay", "BrowserBack").
    let name = first.is_ascii_alphabetic() && key.bytes().all(|b| b.is_ascii_alphanumeric());
    if name { Key::Ignored } else { Key::Text(key) }
}

/// The keypad key for `code`, when the event is one.
fn keypad(event: &LogicalKey) -> Option<Key<'_>> {
    let rest = event.code.strip_prefix("Numpad")?;
    let key = event.key.as_str();
    let printable = key.chars().count() == 1;
    let digit = |n: u8| {
        if printable && key.as_bytes().first().is_some_and(u8::is_ascii_digit) {
            Key::Keypad(Kp::Digit(n))
        } else {
            Key::KeypadNav(match n {
                0 => Nav::Insert,
                1 => Nav::End,
                2 => Nav::Down,
                3 => Nav::PageDown,
                4 => Nav::Left,
                5 => Nav::Begin,
                6 => Nav::Right,
                7 => Nav::Home,
                8 => Nav::Up,
                _ => Nav::PageUp,
            })
        }
    };
    Some(match rest {
        "0" => digit(0),
        "1" => digit(1),
        "2" => digit(2),
        "3" => digit(3),
        "4" => digit(4),
        "5" => digit(5),
        "6" => digit(6),
        "7" => digit(7),
        "8" => digit(8),
        "9" => digit(9),
        "Decimal" if printable => Key::Keypad(Kp::Decimal),
        "Decimal" => Key::KeypadNav(Nav::Delete),
        "Divide" => Key::Keypad(Kp::Divide),
        "Multiply" => Key::Keypad(Kp::Multiply),
        "Subtract" => Key::Keypad(Kp::Subtract),
        "Add" => Key::Keypad(Kp::Add),
        "Enter" => Key::Keypad(Kp::Enter),
        "Equal" => Key::Keypad(Kp::Equal),
        "Comma" => Key::Keypad(Kp::Separator),
        _ => return None,
    })
}

/// The characters of a key on the US layout (PC-101): without Shift, then with Shift. The kitty
/// protocol calls this the base layout key. neohar uses it where the DOM does not say what a key
/// would produce on another layout.
pub(super) fn us_base(code: &str) -> Option<(char, char)> {
    if let Some(letter) = code.strip_prefix("Key") {
        let c = letter.chars().next().filter(char::is_ascii_uppercase)?;
        return Some((c.to_ascii_lowercase(), c));
    }
    if let Some(digit) = code.strip_prefix("Digit") {
        let n = digit.bytes().next().filter(u8::is_ascii_digit)?;
        return Some((
            char::from(n),
            b")!@#$%^&*("
                .get(usize::from(n - b'0'))
                .map(|b| char::from(*b))?,
        ));
    }
    Some(match code {
        "Backquote" => ('`', '~'),
        "Minus" => ('-', '_'),
        "Equal" => ('=', '+'),
        "BracketLeft" => ('[', '{'),
        "BracketRight" => (']', '}'),
        "Backslash" => ('\\', '|'),
        "Semicolon" => (';', ':'),
        "Quote" => ('\'', '"'),
        "Comma" => (',', '<'),
        "Period" => ('.', '>'),
        "Slash" => ('/', '?'),
        "Space" => (' ', ' '),
        _ => return None,
    })
}

/// The modifier bits of the xterm and kitty parameter: Shift 1, Alt 2, Ctrl 4, Meta (the Super key)
/// 8. `locks` adds Caps Lock 64 and Num Lock 128, which only the kitty protocol has.
pub(super) fn mod_bits(mods: &Mods, locks: bool) -> u32 {
    u32::from(mods.shift)
        | u32::from(mods.alt) << 1
        | u32::from(mods.ctrl) << 2
        | u32::from(mods.meta) << 3
        | u32::from(locks && mods.caps_lock) << 6
        | u32::from(locks && mods.num_lock) << 7
}

/// Appends `ESC` before the bytes of an Alt chord.
pub(super) fn esc_prefix(out: &mut Vec<u8>) {
    out.push(0x1b);
}

/// Encodes a key event for the modes of the pane.
pub(super) fn encode(
    event: &LogicalKey,
    modes: &InputModes,
    config: &KeyConfig,
    out: &mut Vec<u8>,
) {
    let key = Key::of(event);
    if modes.kitty_flags() != 0 {
        kitty::encode(event, key, modes, config, out);
    } else if event.kind != KeyKind::Release {
        legacy::encode(event, key, modes, config, out);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(key: &str, code: &str) -> LogicalKey {
        LogicalKey {
            code: code.into(),
            key: key.into(),
            mods: Mods::default(),
            kind: KeyKind::Press,
            text: None,
            location: 0,
        }
    }

    #[test]
    fn keys_are_told_apart_by_key_and_by_code() {
        assert_eq!(Key::of(&event("a", "KeyA")), Key::Text("a"));
        assert_eq!(Key::of(&event(" ", "Space")), Key::Text(" "));
        assert_eq!(Key::of(&event("ArrowUp", "ArrowUp")), Key::Nav(Nav::Up));
        assert_eq!(Key::of(&event("F12", "F12")), Key::F(12));
        assert_eq!(Key::of(&event("F35", "F35")), Key::F(35));
        assert_eq!(Key::of(&event("F36", "F36")), Key::Ignored);
        assert_eq!(Key::of(&event("F0", "F0")), Key::Ignored);
        assert_eq!(Key::of(&event("F", "KeyF")), Key::Text("F"));
        assert_eq!(Key::of(&event("Dead", "Quote")), Key::Ignored);
        assert_eq!(Key::of(&event("Unidentified", "")), Key::Ignored);
        assert_eq!(Key::of(&event("MediaPlay", "MediaPlay")), Key::Ignored);
        assert_eq!(Key::of(&event("\u{3}", "KeyC")), Key::Ignored);
        assert_eq!(Key::of(&event("ß", "KeyS")), Key::Text("ß"));
        assert_eq!(Key::of(&event("", "KeyA")), Key::Ignored);
    }

    #[test]
    fn the_keypad_follows_num_lock() {
        assert_eq!(Key::of(&event("1", "Numpad1")), Key::Keypad(Kp::Digit(1)));
        assert_eq!(Key::of(&event("End", "Numpad1")), Key::KeypadNav(Nav::End));
        assert_eq!(
            Key::of(&event("Clear", "Numpad5")),
            Key::KeypadNav(Nav::Begin)
        );
        assert_eq!(
            Key::of(&event(",", "NumpadDecimal")),
            Key::Keypad(Kp::Decimal)
        );
        assert_eq!(
            Key::of(&event("Delete", "NumpadDecimal")),
            Key::KeypadNav(Nav::Delete)
        );
        assert_eq!(
            Key::of(&event("Enter", "NumpadEnter")),
            Key::Keypad(Kp::Enter)
        );
    }

    #[test]
    fn the_us_layout_table_covers_the_printable_keys() {
        assert_eq!(us_base("KeyQ"), Some(('q', 'Q')));
        assert_eq!(us_base("Digit1"), Some(('1', '!')));
        assert_eq!(us_base("Digit0"), Some(('0', ')')));
        assert_eq!(us_base("Slash"), Some(('/', '?')));
        assert_eq!(us_base("Space"), Some((' ', ' ')));
        assert_eq!(us_base("Enter"), None);
        assert_eq!(us_base("KeyÄ"), None);
    }

    #[test]
    fn modifier_bits_follow_the_xterm_order() {
        let mods = Mods {
            shift: true,
            alt: true,
            ctrl: true,
            meta: true,
            caps_lock: true,
            num_lock: true,
        };
        assert_eq!(mod_bits(&mods, false), 0b1111);
        assert_eq!(mod_bits(&mods, true), 0b1100_1111);
    }
}
