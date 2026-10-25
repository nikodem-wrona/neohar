//! The canonical input encoder.
//!
//! A client sends logical events: keys, mouse events and text. This module turns each one into the
//! bytes for the PTY, using the modes of the pane at that moment. There is one encoder, for every
//! client. It keeps no state: the same event and the same modes give the same bytes.
//!
//! The encodings follow three documents. The legacy and `modifyOtherKeys` forms follow the xterm
//! control sequences (`ctlseqs`). The enhanced keyboard forms follow the kitty keyboard protocol.
//! The mouse forms follow the xterm mouse protocols. The notes in each module list where neohar
//! chooses between documents, or has to leave something out.

mod focus;
mod key;
mod kitty;
mod legacy;
mod mouse;
mod paste;

use nh_proto::{
    input::{LogicalInput, LogicalKey, LogicalMouse},
    modes::ModeFlags,
};
use serde::{Deserialize, Serialize};

pub use focus::encode_focus;
pub use mouse::{MouseEncoding, MouseMode};
pub use paste::{encode_drop_files, encode_paste, encode_text, path_control};

/// The modes of a pane that decide how input is encoded. The core reads them from the VT engine.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct InputModes {
    flags: ModeFlags,
}

impl From<ModeFlags> for InputModes {
    fn from(flags: ModeFlags) -> Self {
        Self { flags }
    }
}

impl InputModes {
    /// The mode flags of a frame or of the engine.
    pub fn new(flags: ModeFlags) -> Self {
        Self { flags }
    }

    /// DECCKM: the cursor keys send `SS3` sequences.
    pub fn app_cursor(self) -> bool {
        self.flags.contains(ModeFlags::APP_CURSOR)
    }

    /// DECKPAM: the keypad sends `SS3` sequences.
    pub fn app_keypad(self) -> bool {
        self.flags.contains(ModeFlags::APP_KEYPAD)
    }

    /// The kitty keyboard flags as the protocol numbers them (1, 2, 4, 8, 16).
    pub fn kitty_flags(self) -> u8 {
        self.flags.kitty_flags()
    }

    /// The `modifyOtherKeys` level, 0 to 2.
    pub fn modify_other_keys(self) -> u8 {
        self.flags.modify_other_keys()
    }

    /// Whether pasted text goes between the bracketed paste markers (mode 2004).
    pub fn bracketed_paste(self) -> bool {
        self.flags.contains(ModeFlags::BRACKETED_PASTE)
    }

    /// Whether focus changes are reported (mode 1004).
    pub fn focus_events(self) -> bool {
        self.flags.contains(ModeFlags::FOCUS_EVENTS)
    }

    /// LNM: Enter sends a carriage return and a line feed.
    pub fn new_line_mode(self) -> bool {
        self.flags.contains(ModeFlags::LNM)
    }

    /// Alternate scroll (mode 1007) on the alternate screen.
    pub(crate) fn alternate_scroll(self) -> bool {
        self.flags
            .contains(ModeFlags::ALT_SCREEN | ModeFlags::ALTERNATE_SCROLL)
    }

    /// The mouse reporting mode that the application chose, if any. When several are set, the one
    /// that reports the most wins.
    pub fn mouse_mode(self) -> Option<MouseMode> {
        let f = self.flags;
        if f.contains(ModeFlags::MOUSE_1003) {
            Some(MouseMode::Motion)
        } else if f.contains(ModeFlags::MOUSE_1002) {
            Some(MouseMode::Drag)
        } else if f.contains(ModeFlags::MOUSE_1000) {
            Some(MouseMode::Click)
        } else if f.contains(ModeFlags::MOUSE_X10) {
            Some(MouseMode::X10)
        } else {
            None
        }
    }

    /// How mouse reports are written. When several are set, SGR wins over urxvt, and urxvt over
    /// UTF-8.
    pub fn mouse_encoding(self) -> MouseEncoding {
        let f = self.flags;
        if f.contains(ModeFlags::MOUSE_SGR) {
            MouseEncoding::Sgr
        } else if f.contains(ModeFlags::MOUSE_URXVT) {
            MouseEncoding::Urxvt
        } else if f.contains(ModeFlags::MOUSE_UTF8) {
            MouseEncoding::Utf8
        } else {
            MouseEncoding::Legacy
        }
    }
}

/// What the Backspace key sends.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Backspace {
    /// DEL, `0x7f`. Ctrl+Backspace sends BS.
    #[default]
    Del,
    /// BS, `0x08`. Ctrl+Backspace sends DEL.
    Bs,
}

/// The settings that change how keys are encoded.
///
/// The Option key of macOS is not here. Only the client knows its platform and which Option key is
/// held, so the keyboard layer of the UI decides whether Option is Alt.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct KeyConfig {
    pub backspace: Backspace,
}

/// Who an input event is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Delivery {
    /// The application. The bytes are in the output buffer. The buffer can be empty when the
    /// encoding has nothing to say, for example a key release without the kitty event types.
    Application,
    /// The core. A mouse event when the application has not asked for mouse reports: the core
    /// uses it for selection and scrolling. The output buffer is not changed.
    Local,
}

/// Encodes a key event for the pane and appends the bytes to `out`.
pub fn encode_key(key: &LogicalKey, modes: &InputModes, config: &KeyConfig, out: &mut Vec<u8>) {
    key::encode(key, modes, config, out);
}

/// Encodes a mouse event for the pane. Returns who the event is for.
pub fn encode_mouse(mouse: &LogicalMouse, modes: &InputModes, out: &mut Vec<u8>) -> Delivery {
    mouse::encode(mouse, modes, out)
}

/// Encodes any logical input. Text goes through [`encode_text`].
pub fn encode_input(
    input: &LogicalInput,
    modes: &InputModes,
    config: &KeyConfig,
    out: &mut Vec<u8>,
) -> Delivery {
    match input {
        LogicalInput::Key(key) => {
            encode_key(key, modes, config, out);
            Delivery::Application
        }
        LogicalInput::Mouse(mouse) => encode_mouse(mouse, modes, out),
        LogicalInput::Text(text) => {
            encode_text(&text.text, modes, out);
            Delivery::Application
        }
    }
}

/// Writes `n` in decimal.
pub(crate) fn push_decimal(out: &mut Vec<u8>, n: u32) {
    let mut digits = [0u8; 10];
    let mut at = digits.len();
    let mut rest = n;
    loop {
        at -= 1;
        digits[at] = b'0' + (rest % 10) as u8;
        rest /= 10;
        if rest == 0 {
            break;
        }
    }
    out.extend_from_slice(&digits[at..]);
}
