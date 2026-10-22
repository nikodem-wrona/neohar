//! The xterm key encodings, with and without `modifyOtherKeys` (`ctlseqs`: "PC-Style Function
//! Keys", "Alt and Meta Keys", "Control Characters", `modifyOtherKeys`).
//!
//! Notes on the choices:
//!
//! - A modified cursor, editing or function key is `CSI 1 ; m X` or `CSI n ; m ~`, in normal and in
//!   application cursor mode. Only a key without modifiers changes with DECCKM.
//! - Alt sends `ESC` before the bytes of the key (the `metaSendsEscape` behaviour).
//! - Ctrl with a letter sends the C0 code. Ctrl with `@ [ \ ] ^ _` or Space sends the C0 code too.
//!   Ctrl with another character sends the character, as xterm does at `modifyOtherKeys` 0.
//! - Where Ctrl needs an ASCII letter and the key is not ASCII (a Cyrillic layout, for example),
//!   the letter of the same physical key on the US layout is used, so that Ctrl+C is still ETX.
//! - The Meta key (Command, Windows, Super) has no legacy encoding. A character key with it sends
//!   nothing, unless `modifyOtherKeys` is on.
//! - `modifyOtherKeys` level 2 sends `CSI 27 ; m ; code ~` for every key with Ctrl, Alt or Meta, and
//!   for Enter, Tab, Backspace and Escape with any modifier. Level 1 does so only where the legacy
//!   form loses a modifier: Ctrl with a character that has no C0 code, and Meta.

use nh_proto::input::LogicalKey;

use super::{
    InputModes, KeyConfig,
    key::{Key, Kp, Nav, esc_prefix, mod_bits, us_base},
    push_decimal,
};

/// The `CSI n ~` numbers of F5 to F20.
const FUNCTION_TILDE: [u32; 16] = [
    15, 17, 18, 19, 20, 21, 23, 24, 25, 26, 28, 29, 31, 32, 33, 34,
];

pub(super) fn encode(
    event: &LogicalKey,
    key: Key<'_>,
    modes: &InputModes,
    config: &KeyConfig,
    out: &mut Vec<u8>,
) {
    let bits = mod_bits(&event.mods, false);
    let mok = modes.modify_other_keys();
    match key {
        Key::Text(text) => encode_text_key(event, text, modes, out),
        Key::Enter => enter(event, bits, mok, modes, out),
        Key::Tab => tab(event, bits, mok, out),
        Key::Backspace => backspace(event, bits, mok, config, out),
        Key::Escape => {
            if mok == 2 && bits != 0 {
                other_keys(out, bits, 27);
            } else {
                if event.mods.alt {
                    esc_prefix(out);
                }
                out.push(0x1b);
            }
        }
        Key::Nav(nav) | Key::KeypadNav(nav) => navigation(nav, bits, modes, out),
        Key::F(n) => function(n, bits, out),
        Key::Keypad(kp) => keypad(event, kp, bits, modes, out),
        Key::Standalone(_) | Key::Menu | Key::PrintScreen | Key::Pause | Key::Ignored => {}
    }
}

/// The C0 code that Ctrl and `c` make.
fn control_code(c: char) -> Option<u8> {
    match c {
        'a'..='z' | 'A'..='Z' => Some(c as u8 & 0x1f),
        ' ' | '@' => Some(0x00),
        '[' => Some(0x1b),
        '\\' => Some(0x1c),
        ']' => Some(0x1d),
        '^' => Some(0x1e),
        '_' => Some(0x1f),
        _ => None,
    }
}

fn encode_text_key(event: &LogicalKey, text: &str, modes: &InputModes, out: &mut Vec<u8>) {
    let mods = &event.mods;
    let mut chars = text.chars();
    let (Some(c), None) = (chars.next(), chars.next()) else {
        // Several characters in one key: there is nothing to modify.
        if mods.alt {
            esc_prefix(out);
        }
        out.extend_from_slice(text.as_bytes());
        return;
    };
    let ctrl_char = if mods.ctrl && !c.is_ascii() {
        us_base(&event.code).map_or(
            c,
            |(plain, shifted)| {
                if mods.shift { shifted } else { plain }
            },
        )
    } else {
        c
    };
    let control = if mods.ctrl {
        control_code(ctrl_char)
    } else {
        None
    };
    let mok = modes.modify_other_keys();
    let wanted = match mok {
        2 => mods.ctrl || mods.alt || mods.meta,
        1 => mods.meta || (mods.ctrl && control.is_none()),
        _ => false,
    };
    if wanted {
        let code = if mods.shift {
            let mut upper = c.to_uppercase();
            match (upper.next(), upper.next()) {
                (Some(u), None) => u,
                _ => c,
            }
        } else {
            c
        };
        other_keys(out, mod_bits(mods, false), u32::from(code));
        return;
    }
    if mods.meta {
        return;
    }
    if mods.alt {
        esc_prefix(out);
    }
    match control {
        Some(byte) => out.push(byte),
        None => {
            let mut buffer = [0u8; 4];
            out.extend_from_slice(c.encode_utf8(&mut buffer).as_bytes());
        }
    }
}

/// `CSI 27 ; m ; code ~`.
fn other_keys(out: &mut Vec<u8>, bits: u32, code: u32) {
    out.extend_from_slice(b"\x1b[27;");
    push_decimal(out, 1 + bits);
    out.push(b';');
    push_decimal(out, code);
    out.push(b'~');
}

fn enter(event: &LogicalKey, bits: u32, mok: u8, modes: &InputModes, out: &mut Vec<u8>) {
    if mok == 2 && bits != 0 {
        other_keys(out, bits, 13);
        return;
    }
    if event.mods.alt {
        esc_prefix(out);
    }
    out.push(b'\r');
    if modes.new_line_mode() {
        out.push(b'\n');
    }
}

fn tab(event: &LogicalKey, bits: u32, mok: u8, out: &mut Vec<u8>) {
    if mok == 2 && bits != 0 {
        other_keys(out, bits, 9);
        return;
    }
    if event.mods.alt {
        esc_prefix(out);
    }
    if event.mods.shift {
        out.extend_from_slice(b"\x1b[Z");
    } else {
        out.push(b'\t');
    }
}

fn backspace(event: &LogicalKey, bits: u32, mok: u8, config: &KeyConfig, out: &mut Vec<u8>) {
    let (plain, swapped) = match config.backspace {
        super::Backspace::Del => (0x7f, 0x08),
        super::Backspace::Bs => (0x08, 0x7f),
    };
    if mok == 2 && bits != 0 {
        other_keys(out, bits, u32::from(plain));
        return;
    }
    if event.mods.alt {
        esc_prefix(out);
    }
    out.push(if event.mods.ctrl { swapped } else { plain });
}

fn navigation(nav: Nav, bits: u32, modes: &InputModes, out: &mut Vec<u8>) {
    if let Some(letter) = nav.letter() {
        if bits != 0 {
            out.extend_from_slice(b"\x1b[1;");
            push_decimal(out, 1 + bits);
        } else if modes.app_cursor() {
            out.extend_from_slice(b"\x1bO");
        } else {
            out.extend_from_slice(b"\x1b[");
        }
        out.push(letter);
    } else if let Some(number) = nav.tilde() {
        tilde(out, number, bits);
    }
}

/// `CSI n ~` or `CSI n ; m ~`.
fn tilde(out: &mut Vec<u8>, number: u32, bits: u32) {
    out.extend_from_slice(b"\x1b[");
    push_decimal(out, number);
    if bits != 0 {
        out.push(b';');
        push_decimal(out, 1 + bits);
    }
    out.push(b'~');
}

fn function(n: u8, bits: u32, out: &mut Vec<u8>) {
    match n {
        1..=4 => {
            let letter = b'P' + (n - 1);
            if bits == 0 {
                out.extend_from_slice(b"\x1bO");
            } else {
                out.extend_from_slice(b"\x1b[1;");
                push_decimal(out, 1 + bits);
            }
            out.push(letter);
        }
        5..=20 => {
            if let Some(number) = FUNCTION_TILDE.get(usize::from(n - 5)) {
                tilde(out, *number, bits);
            }
        }
        // xterm has no sequence for F21 and above.
        _ => {}
    }
}

fn keypad(event: &LogicalKey, kp: Kp, bits: u32, modes: &InputModes, out: &mut Vec<u8>) {
    if modes.app_keypad() {
        out.extend_from_slice(b"\x1bO");
        if bits != 0 {
            push_decimal(out, 1 + bits);
        }
        out.push(kp.app_final());
        return;
    }
    match kp.text() {
        None => enter(event, bits, modes.modify_other_keys(), modes, out),
        Some(c) => {
            let mut buffer = [0u8; 4];
            encode_text_key(event, c.encode_utf8(&mut buffer), modes, out);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::{Backspace, encode_key};
    use super::*;
    use nh_proto::{
        input::{KeyKind, Mods},
        modes::ModeFlags,
    };

    fn key(key: &str, code: &str, mods: Mods) -> LogicalKey {
        LogicalKey {
            code: code.into(),
            key: key.into(),
            mods,
            kind: KeyKind::Press,
            text: None,
            location: 0,
        }
    }

    fn bytes(event: &LogicalKey, flags: ModeFlags) -> Vec<u8> {
        let mut out = Vec::new();
        encode_key(
            event,
            &InputModes::new(flags),
            &KeyConfig::default(),
            &mut out,
        );
        out
    }

    const NONE: Mods = Mods {
        shift: false,
        ctrl: false,
        alt: false,
        meta: false,
        caps_lock: false,
        num_lock: false,
    };
    const CTRL: Mods = Mods { ctrl: true, ..NONE };
    const ALT: Mods = Mods { alt: true, ..NONE };

    #[test]
    fn plain_and_control_letters() {
        assert_eq!(bytes(&key("a", "KeyA", NONE), ModeFlags::empty()), b"a");
        assert_eq!(bytes(&key("c", "KeyC", CTRL), ModeFlags::empty()), [0x03]);
        assert_eq!(
            bytes(&key("é", "Semicolon", NONE), ModeFlags::empty()),
            "é".as_bytes()
        );
        assert_eq!(bytes(&key("c", "KeyC", ALT), ModeFlags::empty()), b"\x1bc");
    }

    #[test]
    fn control_on_a_layout_without_latin_letters_uses_the_us_key() {
        // Cyrillic "с" is on the physical key KeyC.
        assert_eq!(bytes(&key("с", "KeyC", CTRL), ModeFlags::empty()), [0x03]);
    }

    #[test]
    fn cursor_keys_follow_application_mode_only_without_modifiers() {
        let up = key("ArrowUp", "ArrowUp", NONE);
        assert_eq!(bytes(&up, ModeFlags::empty()), b"\x1b[A");
        assert_eq!(bytes(&up, ModeFlags::APP_CURSOR), b"\x1bOA");
        let ctrl_up = key("ArrowUp", "ArrowUp", CTRL);
        assert_eq!(bytes(&ctrl_up, ModeFlags::empty()), b"\x1b[1;5A");
        assert_eq!(bytes(&ctrl_up, ModeFlags::APP_CURSOR), b"\x1b[1;5A");
    }

    #[test]
    fn function_keys() {
        assert_eq!(bytes(&key("F1", "F1", NONE), ModeFlags::empty()), b"\x1bOP");
        assert_eq!(
            bytes(&key("F3", "F3", CTRL), ModeFlags::empty()),
            b"\x1b[1;5R"
        );
        assert_eq!(
            bytes(&key("F5", "F5", NONE), ModeFlags::empty()),
            b"\x1b[15~"
        );
        assert_eq!(
            bytes(&key("F12", "F12", ALT), ModeFlags::empty()),
            b"\x1b[24;3~"
        );
        assert_eq!(
            bytes(&key("F20", "F20", NONE), ModeFlags::empty()),
            b"\x1b[34~"
        );
        assert!(bytes(&key("F21", "F21", NONE), ModeFlags::empty()).is_empty());
    }

    #[test]
    fn backspace_follows_the_setting() {
        let event = key("Backspace", "Backspace", NONE);
        let mut out = Vec::new();
        let bs = KeyConfig {
            backspace: Backspace::Bs,
        };
        encode_key(&event, &InputModes::default(), &bs, &mut out);
        assert_eq!(out, [0x08]);
        assert_eq!(bytes(&event, ModeFlags::empty()), [0x7f]);
        assert_eq!(
            bytes(&key("Backspace", "Backspace", CTRL), ModeFlags::empty()),
            [0x08]
        );
    }

    #[test]
    fn enter_follows_new_line_mode() {
        let event = key("Enter", "Enter", NONE);
        assert_eq!(bytes(&event, ModeFlags::empty()), b"\r");
        assert_eq!(bytes(&event, ModeFlags::LNM), b"\r\n");
    }

    #[test]
    fn modify_other_keys_levels() {
        let level = |n: u8| ModeFlags::empty().with_modify_other_keys(n);
        let ctrl_i = key("i", "KeyI", CTRL);
        assert_eq!(bytes(&ctrl_i, level(0)), [0x09]);
        assert_eq!(bytes(&ctrl_i, level(1)), [0x09]);
        assert_eq!(bytes(&ctrl_i, level(2)), b"\x1b[27;5;105~");
        let ctrl_one = key("1", "Digit1", CTRL);
        assert_eq!(bytes(&ctrl_one, level(0)), b"1");
        assert_eq!(bytes(&ctrl_one, level(1)), b"\x1b[27;5;49~");
        let shift_enter = key(
            "Enter",
            "Enter",
            Mods {
                shift: true,
                ..NONE
            },
        );
        assert_eq!(bytes(&shift_enter, level(1)), b"\r");
        assert_eq!(bytes(&shift_enter, level(2)), b"\x1b[27;2;13~");
    }

    #[test]
    fn a_release_sends_nothing_in_legacy_mode() {
        let mut event = key("a", "KeyA", NONE);
        event.kind = KeyKind::Release;
        assert!(bytes(&event, ModeFlags::empty()).is_empty());
    }
}
