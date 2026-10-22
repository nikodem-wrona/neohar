//! The keys that a scenario can press, as the bytes that a terminal sends.

/// The bytes of a key, or `None` for a name that is not known. A name is lower case, with `-`
/// between a modifier and a key: `ctrl-c`, `shift-tab`.
pub fn key_bytes(name: &str) -> Option<Vec<u8>> {
    let bytes: &[u8] = match name {
        "enter" => b"\r",
        "esc" | "escape" => b"\x1b",
        "tab" => b"\t",
        "shift-tab" => b"\x1b[Z",
        "backspace" => b"\x7f",
        "space" => b" ",
        "up" => b"\x1b[A",
        "down" => b"\x1b[B",
        "right" => b"\x1b[C",
        "left" => b"\x1b[D",
        "home" => b"\x1b[H",
        "end" => b"\x1b[F",
        "pageup" => b"\x1b[5~",
        "pagedown" => b"\x1b[6~",
        "delete" => b"\x1b[3~",
        "f1" => b"\x1bOP",
        "f2" => b"\x1bOQ",
        "f3" => b"\x1bOR",
        "f4" => b"\x1bOS",
        "f5" => b"\x1b[15~",
        "f6" => b"\x1b[17~",
        "f7" => b"\x1b[18~",
        "f8" => b"\x1b[19~",
        "f9" => b"\x1b[20~",
        "f10" => b"\x1b[21~",
        "f11" => b"\x1b[23~",
        "f12" => b"\x1b[24~",
        _ => {
            // `ctrl-a` to `ctrl-z` are the bytes 1 to 26.
            let letter = name.strip_prefix("ctrl-")?;
            let mut chars = letter.chars();
            let (Some(c), None) = (chars.next(), chars.next()) else {
                return None;
            };
            return c.is_ascii_lowercase().then(|| vec![c as u8 - b'a' + 1]);
        }
    };
    Some(bytes.to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_common_keys_have_their_bytes() {
        assert_eq!(key_bytes("enter"), Some(b"\r".to_vec()));
        assert_eq!(key_bytes("esc"), Some(vec![0x1b]));
        assert_eq!(key_bytes("down"), Some(b"\x1b[B".to_vec()));
        assert_eq!(key_bytes("shift-tab"), Some(b"\x1b[Z".to_vec()));
        assert_eq!(key_bytes("f2"), Some(b"\x1bOQ".to_vec()));
        assert_eq!(key_bytes("f5"), Some(b"\x1b[15~".to_vec()));
        assert_eq!(key_bytes("ctrl-c"), Some(vec![3]));
        assert_eq!(key_bytes("ctrl-d"), Some(vec![4]));
        assert_eq!(key_bytes("ctrl-z"), Some(vec![26]));
    }

    #[test]
    fn a_name_that_is_not_a_key_is_none() {
        for name in [
            "", "Enter", "ctrl-", "ctrl-ab", "ctrl-1", "alt-x", "f13", "f0",
        ] {
            assert_eq!(key_bytes(name), None, "{name:?}");
        }
    }
}
