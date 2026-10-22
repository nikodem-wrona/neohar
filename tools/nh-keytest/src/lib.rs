//! `nh-keytest`: the keyboard fidelity matrix.
//!
//! A case says which browser events a user makes, which logical events the keyboard layer must
//! turn them into, and which bytes the encoder must write for the modes of a pane. The expected
//! bytes come from the documents named in `source`, never from the encoder. This crate loads the
//! cases (`tests/keyboard/cases/*.toml`), runs the logical part through the real encoder, and
//! serves the cases and the encoder to the browser test (`main.rs`).

use std::{fs, path::Path};

use nh_proto::{input::LogicalInput, modes::ModeFlags};
use nh_term::input::{
    self, Backspace, Delivery, InputModes, KeyConfig, encode_focus, encode_paste,
};
use serde::{Deserialize, Serialize};

pub mod server;

/// A problem with the cases or with a request.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{path}: {message}")]
    Case { path: String, message: String },
    #[error("{0}")]
    Request(String),
}

/// The platform that a case is for. The browser test passes it to the keyboard layer.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Platform {
    #[default]
    Any,
    Macos,
    Linux,
}

/// Whether the macOS Option key acts as Alt, for the keyboard layer of the UI.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OptionAsAlt {
    #[default]
    None,
    Left,
    Right,
    Both,
}

/// The settings of a case: the one of the encoder and the one of the keyboard layer.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct CaseConfig {
    pub macos_option_as_alt: OptionAsAlt,
    pub backspace: Backspace,
}

/// One browser event, with the fields of `KeyboardEvent` and `CompositionEvent` that matter.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct DomEvent {
    /// `keydown`, `keyup`, `compositionstart`, `compositionupdate` or `compositionend`.
    #[serde(rename = "type")]
    pub kind: String,
    pub key: String,
    pub code: String,
    pub location: u8,
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub meta: bool,
    pub repeat: bool,
    pub is_composing: bool,
    /// The legacy `keyCode`. Browsers report 229 while an input method is active.
    pub key_code: u32,
    pub caps_lock: bool,
    pub num_lock: bool,
    /// `getModifierState("AltGraph")`.
    pub alt_graph: bool,
    /// The `data` of a composition event.
    pub data: String,
}

/// Whether a focus change goes in or out.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Focus {
    In,
    Out,
}

/// Who the case says the event is for.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Who {
    #[default]
    Application,
    Local,
}

/// What the encoder gets: the modes of a pane, the settings and the inputs.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Request {
    /// Mode names: see [`mode_flags`].
    pub modes: Vec<String>,
    /// The kitty keyboard flags (1, 2, 4, 8, 16, or a sum).
    pub kitty: u8,
    pub modify_other_keys: u8,
    pub config: CaseConfig,
    pub logical: Vec<LogicalInput>,
    pub paste: Option<String>,
    pub focus: Option<Focus>,
}

/// The result of a [`Request`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Encoded {
    /// The bytes, as lowercase hexadecimal.
    pub hex: String,
    pub delivery: Who,
}

/// One case of the matrix.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Case {
    pub name: String,
    pub platform: Platform,
    pub modes: Vec<String>,
    pub kitty: u8,
    pub modify_other_keys: u8,
    pub config: CaseConfig,
    /// The browser events, for the Playwright test.
    pub events: Vec<DomEvent>,
    /// The logical events: what the keyboard layer must produce from `events`, or the input of a
    /// case that has no browser events (the mouse).
    pub logical: Vec<LogicalInput>,
    pub paste: Option<String>,
    pub focus: Option<Focus>,
    /// The bytes, in hexadecimal. Spaces are allowed.
    pub expect: String,
    /// Whether the application gets the event or the core keeps it.
    pub delivery: Who,
    /// The document that the expected bytes come from.
    pub source: String,
}

impl Case {
    /// The request that runs this case's own inputs through the encoder.
    pub fn request(&self) -> Request {
        Request {
            modes: self.modes.clone(),
            kitty: self.kitty,
            modify_other_keys: self.modify_other_keys,
            config: self.config.clone(),
            logical: self.logical.clone(),
            paste: self.paste.clone(),
            focus: self.focus,
        }
    }

    /// Whether the case has input that the encoder can run without a browser.
    pub fn has_direct_input(&self) -> bool {
        !self.logical.is_empty() || self.paste.is_some() || self.focus.is_some()
    }

    /// The expected bytes.
    pub fn expected_bytes(&self) -> Result<Vec<u8>, Error> {
        parse_hex(&self.expect).ok_or_else(|| {
            Error::Request(format!("case {:?}: expect is not hexadecimal", self.name))
        })
    }
}

/// The mode flags for the names of a case.
///
/// The names are `alt_screen`, `app_cursor`, `app_keypad`, `bracketed_paste`, `focus_events`,
/// `lnm`, `alternate_scroll`, `mouse_x10`, `mouse_1000`, `mouse_1002`, `mouse_1003`, `mouse_sgr`,
/// `mouse_utf8` and `mouse_urxvt`. The kitty flags and the `modifyOtherKeys` level are numbers.
pub fn mode_flags(names: &[String], kitty: u8, modify_other_keys: u8) -> Result<ModeFlags, Error> {
    let mut flags = ModeFlags::empty();
    for name in names {
        flags |= match name.as_str() {
            "alt_screen" => ModeFlags::ALT_SCREEN,
            "app_cursor" => ModeFlags::APP_CURSOR,
            "app_keypad" => ModeFlags::APP_KEYPAD,
            "bracketed_paste" => ModeFlags::BRACKETED_PASTE,
            "focus_events" => ModeFlags::FOCUS_EVENTS,
            "lnm" => ModeFlags::LNM,
            "alternate_scroll" => ModeFlags::ALTERNATE_SCROLL,
            "mouse_x10" => ModeFlags::MOUSE_X10,
            "mouse_1000" => ModeFlags::MOUSE_1000,
            "mouse_1002" => ModeFlags::MOUSE_1002,
            "mouse_1003" => ModeFlags::MOUSE_1003,
            "mouse_sgr" => ModeFlags::MOUSE_SGR,
            "mouse_utf8" => ModeFlags::MOUSE_UTF8,
            "mouse_urxvt" => ModeFlags::MOUSE_URXVT,
            other => return Err(Error::Request(format!("unknown mode {other:?}"))),
        };
    }
    Ok(flags
        .with_kitty_flags(kitty)
        .with_modify_other_keys(modify_other_keys))
}

/// Runs a request through the encoder.
pub fn run(request: &Request) -> Result<Encoded, Error> {
    let flags = mode_flags(&request.modes, request.kitty, request.modify_other_keys)?;
    let modes = InputModes::new(flags);
    let config = KeyConfig {
        backspace: request.config.backspace,
    };
    let mut out = Vec::new();
    let mut delivery = Who::Application;
    for event in &request.logical {
        if input::encode_input(event, &modes, &config, &mut out) == Delivery::Local {
            delivery = Who::Local;
        }
    }
    if let Some(text) = &request.paste {
        encode_paste(text, &modes, &mut out);
    }
    if let Some(focus) = request.focus {
        encode_focus(focus == Focus::In, &modes, &mut out);
    }
    Ok(Encoded {
        hex: to_hex(&out),
        delivery,
    })
}

/// Lowercase hexadecimal.
pub fn to_hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    bytes.iter().fold(String::new(), |mut hex, b| {
        let _ = write!(hex, "{b:02x}");
        hex
    })
}

/// Bytes from hexadecimal text. Whitespace is ignored.
pub fn parse_hex(text: &str) -> Option<Vec<u8>> {
    let digits: Vec<u8> = text.bytes().filter(|b| !b.is_ascii_whitespace()).collect();
    if !digits.len().is_multiple_of(2) {
        return None;
    }
    digits
        .chunks(2)
        .map(|pair| {
            let high = char::from(pair[0]).to_digit(16)?;
            let low = char::from(pair[1]).to_digit(16)?;
            u8::try_from(high * 16 + low).ok()
        })
        .collect()
}

#[derive(Deserialize)]
struct CaseFile {
    #[serde(default)]
    case: Vec<Case>,
}

/// Loads every `*.toml` file of a directory, in the order of the file names.
pub fn load_cases(dir: &Path) -> Result<Vec<Case>, Error> {
    let fail = |path: &Path, message: String| Error::Case {
        path: path.display().to_string(),
        message,
    };
    let mut files: Vec<_> = fs::read_dir(dir)
        .map_err(|e| fail(dir, e.to_string()))?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|e| e == "toml"))
        .collect();
    files.sort();
    let mut cases = Vec::new();
    let mut names = std::collections::HashSet::new();
    for path in files {
        let text = fs::read_to_string(&path).map_err(|e| fail(&path, e.to_string()))?;
        let file: CaseFile = toml::from_str(&text).map_err(|e| fail(&path, e.to_string()))?;
        for case in file.case {
            check(&case).map_err(|m| fail(&path, format!("case {:?}: {m}", case.name)))?;
            if !names.insert(case.name.clone()) {
                return Err(fail(&path, format!("case {:?} exists twice", case.name)));
            }
            cases.push(case);
        }
    }
    Ok(cases)
}

fn check(case: &Case) -> Result<(), String> {
    if case.name.is_empty() {
        return Err("has no name".into());
    }
    if case.source.trim().is_empty() {
        return Err("has no source for the expected bytes".into());
    }
    if parse_hex(&case.expect).is_none() {
        return Err("expect is not hexadecimal".into());
    }
    if case.events.is_empty() && !case.has_direct_input() {
        return Err("has neither events nor logical input".into());
    }
    mode_flags(&case.modes, case.kitty, case.modify_other_keys).map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_round_trips() {
        assert_eq!(parse_hex("1b 5b 41"), Some(vec![0x1b, 0x5b, 0x41]));
        assert_eq!(parse_hex(""), Some(vec![]));
        assert_eq!(parse_hex("1"), None);
        assert_eq!(parse_hex("zz"), None);
        assert_eq!(to_hex(&[0x1b, 0x5b, 0x41]), "1b5b41");
    }

    #[test]
    fn a_request_runs_through_the_encoder() {
        let request: Request = serde_json::from_str(
            r#"{"modes":["app_cursor"],"logical":[{"type":"key","code":"ArrowUp","key":"ArrowUp"}]}"#,
        )
        .expect("the request decodes");
        let encoded = run(&request).expect("the request runs");
        assert_eq!(encoded.hex, "1b4f41");
        assert_eq!(encoded.delivery, Who::Application);
    }

    #[test]
    fn an_unknown_mode_is_an_error() {
        assert!(mode_flags(&["mouse_9000".into()], 0, 0).is_err());
    }
}
