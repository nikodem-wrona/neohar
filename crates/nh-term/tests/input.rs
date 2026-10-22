//! Properties of the input encoder. The matrix of `tests/keyboard` checks what the bytes
//! are. These tests check what must hold for every input.

use nh_proto::{
    input::{KeyKind, LogicalKey, LogicalMouse, Mods, MouseButton, MouseKind},
    modes::ModeFlags,
};
use nh_term::input::{
    Backspace, Delivery, InputModes, KeyConfig, encode_key, encode_mouse, encode_paste, encode_text,
};
use proptest::prelude::*;

const END: &[u8] = b"\x1b[201~";

fn key_names() -> impl Strategy<Value = String> {
    prop_oneof![
        proptest::sample::select(vec![
            "Enter",
            "Tab",
            "Backspace",
            "Escape",
            "ArrowUp",
            "ArrowDown",
            "ArrowLeft",
            "ArrowRight",
            "Home",
            "End",
            "PageUp",
            "PageDown",
            "Insert",
            "Delete",
            "F1",
            "F4",
            "F5",
            "F12",
            "F20",
            "F21",
            "F35",
            "F36",
            "Shift",
            "Control",
            "Alt",
            "Meta",
            "AltGraph",
            "CapsLock",
            "NumLock",
            "ScrollLock",
            "ContextMenu",
            "PrintScreen",
            "Pause",
            "Dead",
            "Unidentified",
            "Clear",
            " ",
            "a",
            "A",
            "é",
            "@",
            "[",
            "1",
        ])
        .prop_map(String::from),
        ".{0,4}",
    ]
}

fn code_names() -> impl Strategy<Value = String> {
    prop_oneof![
        proptest::sample::select(vec![
            "KeyA",
            "KeyZ",
            "Digit0",
            "Digit9",
            "Minus",
            "Space",
            "Enter",
            "Numpad0",
            "Numpad5",
            "Numpad9",
            "NumpadEnter",
            "NumpadDecimal",
            "NumpadAdd",
            "NumpadComma",
            "ArrowUp",
            "F1",
            "ShiftLeft",
            "",
        ])
        .prop_map(String::from),
        ".{0,8}",
    ]
}

fn mods() -> impl Strategy<Value = Mods> {
    (
        any::<bool>(),
        any::<bool>(),
        any::<bool>(),
        any::<bool>(),
        any::<bool>(),
        any::<bool>(),
    )
        .prop_map(|(shift, ctrl, alt, meta, caps_lock, num_lock)| Mods {
            shift,
            ctrl,
            alt,
            meta,
            caps_lock,
            num_lock,
        })
}

fn kind() -> impl Strategy<Value = KeyKind> {
    prop_oneof![
        Just(KeyKind::Press),
        Just(KeyKind::Repeat),
        Just(KeyKind::Release)
    ]
}

fn flags() -> impl Strategy<Value = ModeFlags> {
    (any::<u32>(), 0u8..32, 0u8..4).prop_map(|(bits, kitty, mok)| {
        ModeFlags::from_bits_retain(bits)
            .with_kitty_flags(kitty)
            .with_modify_other_keys(mok)
    })
}

proptest! {
    #[test]
    fn a_key_never_panics_and_the_output_is_short(
        key in key_names(),
        code in code_names(),
        mods in mods(),
        kind in kind(),
        text in proptest::option::of(".{0,4}"),
        location in 0u8..4,
        flags in flags(),
        bs in prop_oneof![Just(Backspace::Del), Just(Backspace::Bs)],
    ) {
        let event = LogicalKey { code, key: key.clone(), mods, kind, text, location };
        let mut out = Vec::new();
        encode_key(&event, &InputModes::new(flags), &KeyConfig { backspace: bs }, &mut out);
        // The longest sequence has four numbers and the text of the key.
        prop_assert!(out.len() <= 48 + key.len(), "{} bytes for {key:?}", out.len());
    }

    #[test]
    fn a_plain_ascii_key_in_legacy_mode_is_itself(c in proptest::char::range(' ', '~')) {
        let event = LogicalKey {
            code: "KeyX".into(),
            key: c.to_string(),
            mods: Mods::default(),
            kind: KeyKind::Press,
            text: None,
            location: 0,
        };
        let mut out = Vec::new();
        encode_key(&event, &InputModes::default(), &KeyConfig::default(), &mut out);
        prop_assert_eq!(out, vec![c as u8]);
    }

    #[test]
    fn a_mouse_event_never_panics(
        kind in prop_oneof![
            Just(MouseKind::Down), Just(MouseKind::Up), Just(MouseKind::Move), Just(MouseKind::Wheel)
        ],
        button in prop_oneof![
            Just(MouseButton::None), Just(MouseButton::Left), Just(MouseButton::Middle),
            Just(MouseButton::Right)
        ],
        col in any::<u16>(),
        row in any::<u16>(),
        mods in mods(),
        wheel_x in any::<i16>(),
        wheel_y in any::<i16>(),
        flags in flags(),
    ) {
        let event = LogicalMouse { kind, button, col, row, mods, wheel_x, wheel_y };
        let mut out = Vec::new();
        let delivery = encode_mouse(&event, &InputModes::new(flags), &mut out);
        if delivery == Delivery::Local {
            prop_assert!(out.is_empty());
        }
        // 16 notches on two axes, about 24 bytes each at the most.
        prop_assert!(out.len() <= 32 * 24);
    }

    #[test]
    fn a_bracketed_paste_cannot_end_itself(text in "(\x1b|\\[|2|0|1|~|a|\n|\r){0,40}") {
        let mut out = Vec::new();
        encode_paste(&text, &InputModes::new(ModeFlags::BRACKETED_PASTE), &mut out);
        prop_assert!(out.starts_with(b"\x1b[200~"));
        prop_assert!(out.ends_with(END));
        let inside = &out[6..out.len() - END.len()];
        prop_assert!(!inside.windows(END.len()).any(|w| w == END), "{inside:?}");
        prop_assert!(!inside.contains(&b'\n'));
    }

    #[test]
    fn text_without_a_line_end_is_not_changed(text in "[^\r\n]{0,20}", flags in flags()) {
        let mut out = Vec::new();
        encode_text(&text, &InputModes::new(flags), &mut out);
        prop_assert_eq!(out, text.into_bytes());
    }
}
