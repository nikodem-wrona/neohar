//! The kitty keyboard protocol, flags 1, 2, 4, 8 and 16 (<https://sw.kovidgoyal.net/kitty/keyboard-protocol/>).
//!
//! The form is `CSI code[:shifted[:base]] ; modifiers[:event] ; text u`. Keys that have a legacy
//! `CSI` or `SS3` form keep it (`CSI 1 ; m A`, `CSI n ; m ~`) and add the event type after the
//! modifiers. Notes on the choices:
//!
//! - Flag 1: a key that produces text is text, with or without Shift. With Ctrl, Alt or Meta it is
//!   `CSI u`. Escape is always `CSI u`. Enter, Tab and Backspace send their legacy bytes until a
//!   modifier is held, Shift included.
//! - Flag 2: repeat and release events are reported for keys that have an escape code. A key that
//!   sends plain text, and Enter, Tab and Backspace without a modifier, have no release event
//!   unless flag 8 is set.
//! - Flag 4: the shifted key and the base layout key are added to events that are `CSI u`. The base
//!   layout key is the character of the same physical key on the US layout. The DOM has no
//!   "unshifted key of the current layout", so the code of a shifted symbol also comes from the US
//!   layout.
//! - Flag 8: every key is `CSI u`, text keys, Enter, Tab and Backspace included, and so are the
//!   modifier and lock keys. Only with this flag the Caps Lock and Num Lock bits are in the
//!   modifier field. Without it, a plain arrow key on a keyboard with Num Lock on would not be
//!   plain.
//! - Flag 16: the text of a key without Ctrl, Alt or Meta is the third field. The client sends the
//!   text. A control character is never sent as text.
//! - The DOM has no Hyper key and no "Meta" separate from Super, so those bits are never set.

use nh_proto::input::{KeyKind, LogicalKey};

use super::{
    InputModes, KeyConfig,
    key::{Key, Kp, Nav, mod_bits, us_base},
    legacy, push_decimal,
};

/// The `CSI n ~` numbers of F5 to F12.
const FUNCTION_TILDE: [u32; 8] = [15, 17, 18, 19, 20, 21, 23, 24];

pub(super) fn encode(
    event: &LogicalKey,
    key: Key<'_>,
    modes: &InputModes,
    config: &KeyConfig,
    out: &mut Vec<u8>,
) {
    let flags = modes.kitty_flags();
    let all_keys = flags & 8 != 0;
    let event_types = flags & 2 != 0;
    let release = event.kind == KeyKind::Release;
    if release && !event_types {
        return;
    }
    let ctx = Ctx {
        event,
        modes,
        config,
        all_keys,
        alternates: flags & 4 != 0,
        want_text: flags & 16 != 0,
        bits: mod_bits(&event.mods, all_keys),
        kind: match event.kind {
            KeyKind::Press => 1,
            KeyKind::Repeat if event_types => 2,
            KeyKind::Repeat => 1,
            KeyKind::Release => 3,
        },
        // Ctrl, Alt and Meta make a text key an escape code.
        plain: !event.mods.has_command(),
    };
    match key {
        Key::Text(text) => ctx.text(text, out),
        Key::Enter => ctx.legacy_unless_modified(13, key, out),
        Key::Tab => ctx.legacy_unless_modified(9, key, out),
        Key::Backspace => ctx.legacy_unless_modified(127, key, out),
        Key::Escape => ctx.csi_u(27, None, None, out),
        Key::Nav(nav) => ctx.navigation(nav, out),
        Key::KeypadNav(Nav::Begin) => ctx.navigation(Nav::Begin, out),
        Key::KeypadNav(nav) => {
            if let Some(code) = nav.keypad_code() {
                ctx.csi_u(code, None, None, out);
            }
        }
        Key::F(n) => ctx.function(n, out),
        Key::Keypad(kp) => ctx.keypad(kp, out),
        Key::Standalone(standalone) => {
            if all_keys {
                ctx.csi_u(standalone.kitty_code(), None, None, out);
            }
        }
        Key::Menu => ctx.csi_u(57363, None, None, out),
        Key::PrintScreen => ctx.csi_u(57361, None, None, out),
        Key::Pause => ctx.csi_u(57362, None, None, out),
        Key::Ignored => {}
    }
}

/// What the encoder knows about the event, found once.
struct Ctx<'a> {
    event: &'a LogicalKey,
    modes: &'a InputModes,
    config: &'a KeyConfig,
    all_keys: bool,
    alternates: bool,
    want_text: bool,
    /// The modifier bits, with the lock bits only under flag 8.
    bits: u32,
    /// The event type: 1 press, 2 repeat, 3 release.
    kind: u8,
    /// The event has no Ctrl, Alt or Meta.
    plain: bool,
}

/// The alternate keys of a `CSI u` sequence.
struct Alternates {
    shifted: Option<u32>,
    base: Option<u32>,
}

impl Ctx<'_> {
    /// A key that produces text.
    fn text(&self, text: &str, out: &mut Vec<u8>) {
        if !self.all_keys && self.plain {
            if self.kind != 3 {
                out.extend_from_slice(text.as_bytes());
            }
            return;
        }
        let mut chars = text.chars();
        let single = match (chars.next(), chars.next()) {
            (Some(c), None) => Some(c),
            _ => None,
        };
        let (code, alternates) = match single {
            Some(c) => self.key_codes(c),
            None => (
                0,
                Alternates {
                    shifted: None,
                    base: None,
                },
            ),
        };
        let associated = (self.want_text && self.all_keys && self.plain && self.kind != 3)
            .then(|| self.event.text.as_deref().unwrap_or(text))
            .filter(|t| !t.is_empty() && !t.chars().any(char::is_control));
        self.csi_u(code, self.alternates.then_some(alternates), associated, out);
    }

    /// The code of the key and its alternate keys.
    fn key_codes(&self, c: char) -> (u32, Alternates) {
        let us = us_base(&self.event.code);
        let unshifted = if self.event.mods.shift && !c.is_alphabetic() {
            us.map_or(c, |(plain, _)| plain)
        } else {
            lowercase(c)
        };
        let shifted = (self.event.mods.shift && c != unshifted).then_some(u32::from(c));
        let base = us
            .map(|(plain, _)| plain)
            .filter(|plain| *plain != unshifted)
            .map(u32::from);
        (u32::from(unshifted), Alternates { shifted, base })
    }

    /// Enter, Tab and Backspace keep their bytes until a modifier or flag 8 makes them `CSI u`.
    fn legacy_unless_modified(&self, code: u32, key: Key<'_>, out: &mut Vec<u8>) {
        if !self.all_keys && mod_bits(&self.event.mods, false) == 0 {
            if self.kind != 3 {
                legacy::encode(self.event, key, self.modes, self.config, out);
            }
            return;
        }
        self.csi_u(code, None, None, out);
    }

    /// `CSI code[:alternates] ; modifiers[:event] ; text u`.
    fn csi_u(
        &self,
        code: u32,
        alternates: Option<Alternates>,
        text: Option<&str>,
        out: &mut Vec<u8>,
    ) {
        out.extend_from_slice(b"\x1b[");
        push_decimal(out, code);
        if let Some(Alternates { shifted, base }) = alternates {
            if shifted.is_some() || base.is_some() {
                out.push(b':');
                if let Some(shifted) = shifted {
                    push_decimal(out, shifted);
                }
            }
            if let Some(base) = base {
                out.push(b':');
                push_decimal(out, base);
            }
        }
        self.fields(text, out);
        out.push(b'u');
    }

    /// The modifier field, as `;m` or `;m:event`, and the text field. Nothing when both are empty.
    fn fields(&self, text: Option<&str>, out: &mut Vec<u8>) {
        let modifiers = self.bits != 0 || self.kind != 1;
        if modifiers || text.is_some() {
            out.push(b';');
        }
        if modifiers {
            push_decimal(out, 1 + self.bits);
            if self.kind != 1 {
                out.push(b':');
                push_decimal(out, u32::from(self.kind));
            }
        }
        if let Some(text) = text {
            out.push(b';');
            for (i, c) in text.chars().enumerate() {
                if i > 0 {
                    out.push(b':');
                }
                push_decimal(out, u32::from(c));
            }
        }
    }

    /// Whether the key can use its short form: no modifiers and a press.
    fn short(&self) -> bool {
        self.bits == 0 && self.kind == 1
    }

    /// `CSI 1 ; m X`, with the number written because the modifiers follow.
    fn letter_form(&self, number: u32, letter: u8, out: &mut Vec<u8>) {
        out.extend_from_slice(b"\x1b[");
        push_decimal(out, number);
        self.fields(None, out);
        out.push(letter);
    }

    /// `CSI n ; m ~`.
    fn tilde_form(&self, number: u32, out: &mut Vec<u8>) {
        out.extend_from_slice(b"\x1b[");
        push_decimal(out, number);
        self.fields(None, out);
        out.push(b'~');
    }

    fn navigation(&self, nav: Nav, out: &mut Vec<u8>) {
        if let Some(letter) = nav.letter() {
            if self.short() {
                out.extend_from_slice(if self.modes.app_cursor() {
                    b"\x1bO"
                } else {
                    b"\x1b["
                });
                out.push(letter);
            } else {
                self.letter_form(1, letter, out);
            }
        } else if let Some(number) = nav.tilde() {
            self.tilde_form(number, out);
        }
    }

    fn function(&self, n: u8, out: &mut Vec<u8>) {
        match n {
            1..=4 if self.short() => {
                out.extend_from_slice(b"\x1bO");
                out.push(b'P' + (n - 1));
            }
            // F3 has no `CSI 1 ; m R` form because that is a cursor position report.
            3 => self.tilde_form(13, out),
            1..=4 => self.letter_form(1, b'P' + (n - 1), out),
            5..=12 => {
                if let Some(number) = FUNCTION_TILDE.get(usize::from(n - 5)) {
                    self.tilde_form(*number, out);
                }
            }
            _ => self.csi_u(57376 + u32::from(n) - 13, None, None, out),
        }
    }

    fn keypad(&self, kp: Kp, out: &mut Vec<u8>) {
        let Some(c) = kp.text() else {
            // Keypad Enter is a key of its own, even under flag 1.
            self.csi_u(kp.kitty_code(), None, None, out);
            return;
        };
        if !self.all_keys && self.plain {
            if self.kind != 3 {
                let mut buffer = [0u8; 4];
                out.extend_from_slice(c.encode_utf8(&mut buffer).as_bytes());
            }
            return;
        }
        let mut buffer = [0u8; 4];
        let text = &*c.encode_utf8(&mut buffer);
        let associated =
            (self.want_text && self.all_keys && self.plain && self.kind != 3).then_some(text);
        self.csi_u(kp.kitty_code(), None, associated, out);
    }
}

/// The lowercase form of `c`, when it is one character.
fn lowercase(c: char) -> char {
    let mut lower = c.to_lowercase();
    match (lower.next(), lower.next()) {
        (Some(l), None) => l,
        _ => c,
    }
}

#[cfg(test)]
mod tests {
    use super::super::encode_key;
    use super::*;
    use nh_proto::{input::Mods, modes::ModeFlags};

    fn key(key: &str, code: &str, mods: Mods, kind: KeyKind) -> LogicalKey {
        LogicalKey {
            code: code.into(),
            key: key.into(),
            mods,
            kind,
            text: None,
            location: 0,
        }
    }

    fn bytes(event: &LogicalKey, kitty: u8) -> Vec<u8> {
        let mut out = Vec::new();
        encode_key(
            event,
            &InputModes::new(ModeFlags::empty().with_kitty_flags(kitty)),
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
    const SHIFT: Mods = Mods {
        shift: true,
        ..NONE
    };

    #[test]
    fn flag_1_keeps_text_and_escapes_the_rest() {
        let press = KeyKind::Press;
        assert_eq!(bytes(&key("a", "KeyA", NONE, press), 1), b"a");
        assert_eq!(bytes(&key("A", "KeyA", SHIFT, press), 1), b"A");
        assert_eq!(bytes(&key("c", "KeyC", CTRL, press), 1), b"\x1b[99;5u");
        assert_eq!(bytes(&key("Escape", "Escape", NONE, press), 1), b"\x1b[27u");
        assert_eq!(bytes(&key("Enter", "Enter", NONE, press), 1), b"\r");
        assert_eq!(
            bytes(&key("Enter", "Enter", SHIFT, press), 1),
            b"\x1b[13;2u"
        );
        assert_eq!(bytes(&key("ArrowUp", "ArrowUp", NONE, press), 1), b"\x1b[A");
        assert_eq!(
            bytes(&key("ArrowUp", "ArrowUp", CTRL, press), 1),
            b"\x1b[1;5A"
        );
    }

    #[test]
    fn flag_2_adds_event_types_to_escape_codes_only() {
        let release = KeyKind::Release;
        assert_eq!(bytes(&key("a", "KeyA", NONE, release), 3), b"");
        assert_eq!(bytes(&key("c", "KeyC", CTRL, release), 3), b"\x1b[99;5:3u");
        assert_eq!(bytes(&key("Enter", "Enter", NONE, release), 3), b"");
        assert_eq!(
            bytes(&key("ArrowUp", "ArrowUp", NONE, release), 3),
            b"\x1b[1;1:3A"
        );
        assert_eq!(
            bytes(&key("c", "KeyC", CTRL, KeyKind::Repeat), 3),
            b"\x1b[99;5:2u"
        );
    }

    #[test]
    fn flag_8_escapes_every_key() {
        let press = KeyKind::Press;
        assert_eq!(bytes(&key("a", "KeyA", NONE, press), 8), b"\x1b[97u");
        assert_eq!(bytes(&key("A", "KeyA", SHIFT, press), 8), b"\x1b[97;2u");
        assert_eq!(bytes(&key("Enter", "Enter", NONE, press), 8), b"\x1b[13u");
        assert_eq!(
            bytes(&key("Shift", "ShiftLeft", SHIFT, press), 8),
            b"\x1b[57441;2u"
        );
        let locked = Mods {
            caps_lock: true,
            ..NONE
        };
        assert_eq!(bytes(&key("A", "KeyA", locked, press), 8), b"\x1b[97;65u");
    }

    #[test]
    fn flag_16_adds_the_text() {
        let press = KeyKind::Press;
        assert_eq!(
            bytes(&key("A", "KeyA", SHIFT, press), 8 | 16),
            b"\x1b[97;2;65u"
        );
        assert_eq!(
            bytes(&key("a", "KeyA", NONE, press), 8 | 16),
            b"\x1b[97;;97u"
        );
        assert_eq!(bytes(&key("c", "KeyC", CTRL, press), 8 | 16), b"\x1b[99;5u");
    }

    #[test]
    fn flag_4_adds_the_shifted_and_the_base_layout_key() {
        let press = KeyKind::Press;
        let ctrl_shift = Mods {
            ctrl: true,
            shift: true,
            ..NONE
        };
        assert_eq!(
            bytes(&key("A", "KeyA", ctrl_shift, press), 4),
            b"\x1b[97:65;6u"
        );
        // Cyrillic "с" on the key C.
        assert_eq!(
            bytes(&key("с", "KeyC", CTRL, press), 4),
            "\x1b[1089::99;5u".as_bytes()
        );
    }

    #[test]
    fn f3_uses_the_tilde_form_once_it_has_modifiers() {
        let press = KeyKind::Press;
        assert_eq!(bytes(&key("F3", "F3", NONE, press), 1), b"\x1bOR");
        assert_eq!(bytes(&key("F3", "F3", CTRL, press), 1), b"\x1b[13;5~");
        assert_eq!(bytes(&key("F13", "F13", NONE, press), 1), b"\x1b[57376u");
    }
}
