//! Logical input events.
//!
//! A client turns a browser event into one of these and sends it with `pane.input`. The core
//! encodes it into bytes for the modes of the pane (`nh-term::input`). The types are JSON on
//! the API, so they use readable names: `snake_case` enums, and a list of names for modifiers.
//! They hold no state and no policy. They say what happened, not what to write to the PTY.

use serde::{Deserialize, Serialize};

#[cfg(feature = "api")]
use schemars::JsonSchema;

/// One modifier, in the order of the list form of [`Mods`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "api", derive(JsonSchema))]
#[serde(rename_all = "snake_case")]
enum ModName {
    Shift,
    Ctrl,
    Alt,
    Meta,
    CapsLock,
    NumLock,
}

/// The modifiers that were active when an event happened.
///
/// On the wire this is a list of names (`["ctrl", "shift"]`), in the order of the fields, so a
/// reader sees the chord. `meta` is the Command key on macOS and the Windows key elsewhere; the
/// kitty keyboard protocol calls it `super`. `caps_lock` and `num_lock` are the lock states, not
/// key presses.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "api", derive(JsonSchema))]
#[serde(from = "Vec<ModName>", into = "Vec<ModName>")]
#[cfg_attr(feature = "api", schemars(with = "Vec<ModName>"))]
pub struct Mods {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub meta: bool,
    pub caps_lock: bool,
    pub num_lock: bool,
}

impl Mods {
    /// Whether no modifier of any kind is active, locks included.
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    /// Whether `ctrl`, `alt` or `meta` is active. `shift` and the locks do not count.
    pub fn has_command(&self) -> bool {
        self.ctrl || self.alt || self.meta
    }
}

impl From<Vec<ModName>> for Mods {
    fn from(names: Vec<ModName>) -> Self {
        let mut mods = Self::default();
        for name in names {
            match name {
                ModName::Shift => mods.shift = true,
                ModName::Ctrl => mods.ctrl = true,
                ModName::Alt => mods.alt = true,
                ModName::Meta => mods.meta = true,
                ModName::CapsLock => mods.caps_lock = true,
                ModName::NumLock => mods.num_lock = true,
            }
        }
        mods
    }
}

impl From<Mods> for Vec<ModName> {
    fn from(mods: Mods) -> Self {
        [
            (mods.shift, ModName::Shift),
            (mods.ctrl, ModName::Ctrl),
            (mods.alt, ModName::Alt),
            (mods.meta, ModName::Meta),
            (mods.caps_lock, ModName::CapsLock),
            (mods.num_lock, ModName::NumLock),
        ]
        .into_iter()
        .filter_map(|(on, name)| on.then_some(name))
        .collect()
    }
}

/// What the key did. A client sends `release` only when the pane's frame has the kitty "report
/// event types" mode bit.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "api", derive(JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum KeyKind {
    #[default]
    Press,
    Repeat,
    Release,
}

/// A key event. The fields are those of the DOM `KeyboardEvent`, after the client has dealt with
/// composition, dead keys, `AltGraph` and the Option key of macOS.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "api", derive(JsonSchema))]
pub struct LogicalKey {
    /// The physical key (`KeyboardEvent.code`), for example `KeyC` or `Numpad1`.
    pub code: String,
    /// The key in the layout of the user (`KeyboardEvent.key`), for example `c`, `C`, `Enter`.
    pub key: String,
    #[serde(default)]
    pub mods: Mods,
    #[serde(default)]
    pub kind: KeyKind,
    /// The text that the key produces, when the client knows it. The kitty protocol sends it when
    /// the application asks for the associated text.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// `KeyboardEvent.location`: 0 standard, 1 left, 2 right, 3 numeric keypad.
    #[serde(default)]
    pub location: u8,
}

/// What a mouse event is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "api", derive(JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum MouseKind {
    Down,
    Up,
    /// The pointer moved. `button` is the button that is held, if any.
    Move,
    Wheel,
}

/// A mouse button.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "api", derive(JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum MouseButton {
    #[default]
    None,
    Left,
    Middle,
    Right,
}

/// A mouse event at a cell of the grid.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "api", derive(JsonSchema))]
pub struct LogicalMouse {
    pub kind: MouseKind,
    #[serde(default)]
    pub button: MouseButton,
    /// The column of the cell, counted from 0 at the left.
    pub col: u16,
    /// The row of the cell, counted from 0 at the top.
    pub row: u16,
    #[serde(default)]
    pub mods: Mods,
    /// Horizontal wheel notches. Positive is to the right, as in `WheelEvent.deltaX`.
    #[serde(default)]
    pub wheel_x: i16,
    /// Vertical wheel notches. Positive is down, as in `WheelEvent.deltaY`.
    #[serde(default)]
    pub wheel_y: i16,
}

/// Text that is not a key press: the result of an input method or a dead key.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "api", derive(JsonSchema))]
pub struct LogicalText {
    pub text: String,
}

/// The `event` of `pane.input`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "api", derive(JsonSchema))]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum LogicalInput {
    Key(LogicalKey),
    Mouse(LogicalMouse),
    Text(LogicalText),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modifiers_are_a_list_of_names_in_field_order() {
        let mods = Mods {
            ctrl: true,
            shift: true,
            num_lock: true,
            ..Mods::default()
        };
        let json = serde_json::to_string(&mods).expect("the modifiers serialize");
        assert_eq!(json, r#"["shift","ctrl","num_lock"]"#);
        let back: Mods = serde_json::from_str(r#"["num_lock","ctrl","shift","ctrl"]"#)
            .expect("the list decodes in any order and with repeats");
        assert_eq!(back, mods);
        let none = serde_json::to_string(&Mods::default()).expect("no modifiers serialize");
        assert_eq!(none, "[]");
    }

    #[test]
    fn an_unknown_modifier_is_an_error() {
        assert!(serde_json::from_str::<Mods>(r#"["hyper"]"#).is_err());
    }

    #[test]
    fn a_key_decodes_with_its_defaults() {
        let input: LogicalInput =
            serde_json::from_str(r#"{"type":"key","code":"KeyC","key":"c","mods":["ctrl"]}"#)
                .expect("a key with only the required fields decodes");
        assert_eq!(
            input,
            LogicalInput::Key(LogicalKey {
                code: "KeyC".into(),
                key: "c".into(),
                mods: Mods {
                    ctrl: true,
                    ..Mods::default()
                },
                kind: KeyKind::Press,
                text: None,
                location: 0,
            })
        );
    }

    #[test]
    fn every_input_survives_json() {
        let inputs = [
            LogicalInput::Key(LogicalKey {
                code: "KeyA".into(),
                key: "A".into(),
                mods: Mods {
                    shift: true,
                    caps_lock: true,
                    ..Mods::default()
                },
                kind: KeyKind::Release,
                text: Some("A".into()),
                location: 2,
            }),
            LogicalInput::Mouse(LogicalMouse {
                kind: MouseKind::Wheel,
                button: MouseButton::None,
                col: 3,
                row: 4,
                mods: Mods::default(),
                wheel_x: -1,
                wheel_y: 2,
            }),
            LogicalInput::Text(LogicalText {
                text: "日本語".into(),
            }),
        ];
        for input in inputs {
            let json = serde_json::to_string(&input).expect("the input serializes");
            let back: LogicalInput = serde_json::from_str(&json).expect("the input decodes");
            assert_eq!(back, input, "{json}");
        }
    }

    #[test]
    fn the_type_tag_names_the_variant() {
        let json = serde_json::to_string(&LogicalInput::Text(LogicalText { text: "x".into() }))
            .expect("the input serializes");
        assert_eq!(json, r#"{"type":"text","text":"x"}"#);
    }
}
