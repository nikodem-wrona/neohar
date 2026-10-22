//! The names of keys in a manifest (`[ui] interrupt_keys`, `quit_keys`) as the keys
//! that a person would press. The pane encodes them for the modes it is in.

use nh_proto::input::{KeyKind, LogicalInput, LogicalKey, Mods};

fn key(code: &str, name: &str, mods: Mods, text: Option<&str>) -> LogicalInput {
    LogicalInput::Key(LogicalKey {
        code: code.to_owned(),
        key: name.to_owned(),
        mods,
        kind: KeyKind::default(),
        text: text.map(str::to_owned),
        location: 0,
    })
}

/// A key from its name: `esc`, `enter`, `tab`, `space`, `backspace`, an arrow, one letter or
/// digit, and the same with `ctrl+` or `alt+` before it. `None` for any other text.
pub(crate) fn logical_key(name: &str) -> Option<LogicalInput> {
    let lower = name.trim().to_ascii_lowercase();
    let mut parts: Vec<&str> = lower.split('+').collect();
    let base = parts.pop().filter(|base| !base.is_empty())?;
    let mut mods = Mods::default();
    for part in parts {
        match part {
            "ctrl" | "control" => mods.ctrl = true,
            "alt" | "option" => mods.alt = true,
            "shift" => mods.shift = true,
            _ => return None,
        }
    }
    let plain = Mods::default();
    let special = match base {
        "esc" | "escape" => Some(("Escape", "Escape")),
        "enter" | "return" => Some(("Enter", "Enter")),
        "tab" => Some(("Tab", "Tab")),
        "backspace" => Some(("Backspace", "Backspace")),
        "up" => Some(("ArrowUp", "ArrowUp")),
        "down" => Some(("ArrowDown", "ArrowDown")),
        "left" => Some(("ArrowLeft", "ArrowLeft")),
        "right" => Some(("ArrowRight", "ArrowRight")),
        _ => None,
    };
    if let Some((code, name)) = special {
        return Some(key(code, name, mods, None));
    }
    if base == "space" {
        let text = (mods == plain).then_some(" ");
        return Some(key("Space", " ", mods, text));
    }
    let mut chars = base.chars();
    let (Some(c), None) = (chars.next(), chars.next()) else {
        return None;
    };
    if !c.is_ascii_alphanumeric() {
        return None;
    }
    let code = if c.is_ascii_digit() {
        format!("Digit{c}")
    } else {
        format!("Key{}", c.to_ascii_uppercase())
    };
    // A key with ctrl or alt makes no text of its own.
    let text = (!mods.ctrl && !mods.alt).then(|| c.to_string());
    Some(key(&code, &c.to_string(), mods, text.as_deref()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parts(input: &LogicalInput) -> (String, String, Mods) {
        let LogicalInput::Key(k) = input else {
            panic!("a key");
        };
        (k.code.clone(), k.key.clone(), k.mods)
    }

    #[test]
    fn the_names_of_the_manifests_are_keys() {
        assert_eq!(parts(&logical_key("esc").unwrap()).0, "Escape");
        assert_eq!(parts(&logical_key("Enter").unwrap()).0, "Enter");
        let (code, name, mods) = parts(&logical_key("ctrl+c").unwrap());
        assert_eq!((code.as_str(), name.as_str()), ("KeyC", "c"));
        assert!(mods.ctrl && !mods.alt);
        assert_eq!(parts(&logical_key("alt+7").unwrap()).0, "Digit7");
        assert_eq!(parts(&logical_key("up").unwrap()).0, "ArrowUp");
    }

    #[test]
    fn a_key_with_ctrl_has_no_text() {
        let LogicalInput::Key(k) = logical_key("ctrl+d").unwrap() else {
            panic!("a key");
        };
        assert_eq!(k.text, None);
        let LogicalInput::Key(k) = logical_key("x").unwrap() else {
            panic!("a key");
        };
        assert_eq!(k.text.as_deref(), Some("x"));
    }

    #[test]
    fn other_text_is_no_key() {
        for name in ["", "ctrl+", "hyper+c", "f13", "ctrl+esc+c", "é", "ab"] {
            assert!(logical_key(name).is_none(), "{name:?}");
        }
    }
}
