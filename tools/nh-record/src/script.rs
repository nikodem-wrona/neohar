//! The step script of `nh-record`.
//!
//! YAML needs a parser crate, and the workspace has none, so the script is a plain text file with
//! one step per line. This is the whole format:
//!
//! ```text
//! # A comment. A blank line is ignored too.
//! send "text with \r \n \t \e \xNN escapes"   # bytes for the child's stdin
//! expect "text"                               # wait until this text appears in the output
//! wait 300                                    # keep reading events for 300 ms
//! resize 80 24                                # change the PTY size; the journal gets a marker
//! expect_exit                                 # wait until the child exits
//! ```
//!
//! - A string is in double quotes and is not empty. Inside it, `\r`, `\n`, `\t`, `\e` (ESC), `\a`
//!   (BEL), `\b` (BS), `\\`, `\"` and `\xNN` (two hex digits, one byte) are escapes. Every other
//!   backslash sequence is an error. Other characters, UTF-8 text included, stand for themselves,
//!   and a `#` in a string is part of the string. A `# comment` can follow the closing quote.
//! - `send` writes the bytes to the child as one `Input` request. Each `send` has a new input id.
//! - `expect` looks for the bytes in the raw PTY output, escape sequences included. The search
//!   starts after the end of the text that the previous `expect` found, so a series of `expect`
//!   steps goes forward through the output. It fails when the text does not appear within the
//!   step timeout (15 s unless `--timeout` says otherwise), or when the child exits first.
//! - `wait` is for programs that redraw on a timer. Use `expect` where a program says what it did.
//! - `expect_exit` waits for the `ChildExited` event. The holder sends it after all output.
//!
//! After the last step, the recorder reads until the output has been quiet for 300 ms (at most
//! 3 s), so that the last repaint is in the journal. A child that is still running is then killed.

use std::time::Duration;

/// One step of a script.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Step {
    /// Write these bytes to the child.
    Send(Vec<u8>),
    /// Wait until the output, after the previous match, contains these bytes.
    Expect(Vec<u8>),
    /// Keep reading events for this long.
    Wait(Duration),
    /// Change the PTY size.
    Resize { cols: u16, rows: u16 },
    /// Wait until the child has exited.
    ExpectExit,
}

/// A script line that cannot be parsed.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("script line {line}: {message}")]
pub struct ScriptError {
    /// The line number, counted from 1.
    pub line: usize,
    pub message: String,
}

/// Parses a whole script. The error names the first bad line.
pub fn parse_script(text: &str) -> Result<Vec<Step>, ScriptError> {
    let mut steps = Vec::new();
    for (index, raw) in text.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let at = |message: String| ScriptError {
            line: index + 1,
            message,
        };
        let (keyword, rest) = match line.split_once(char::is_whitespace) {
            Some((keyword, rest)) => (keyword, rest.trim()),
            None => (line, ""),
        };
        let step = match keyword {
            "send" | "expect" => {
                let bytes = quoted(rest).map_err(at)?;
                if bytes.is_empty() {
                    return Err(at(format!("{keyword} needs a text that is not empty")));
                }
                if keyword == "send" {
                    Step::Send(bytes)
                } else {
                    Step::Expect(bytes)
                }
            }
            "wait" => {
                let ms = rest.parse::<u64>().map_err(|_| {
                    at(format!("wait needs a number of milliseconds, not {rest:?}"))
                })?;
                Step::Wait(Duration::from_millis(ms))
            }
            "resize" => {
                let mut numbers = rest.split_whitespace();
                let (Some(cols), Some(rows), None) =
                    (numbers.next(), numbers.next(), numbers.next())
                else {
                    return Err(at("resize needs two numbers: columns and rows".into()));
                };
                let size = |text: &str| {
                    text.parse::<u16>()
                        .ok()
                        .filter(|n| *n > 0)
                        .ok_or_else(|| at(format!("{text:?} is not a size from 1 to 65535")))
                };
                Step::Resize {
                    cols: size(cols)?,
                    rows: size(rows)?,
                }
            }
            "expect_exit" => {
                if !rest.is_empty() {
                    return Err(at("expect_exit takes no argument".into()));
                }
                Step::ExpectExit
            }
            other => return Err(at(format!("unknown step {other:?}"))),
        };
        steps.push(step);
    }
    Ok(steps)
}

/// Reads `"..."` at the start of `text` and decodes it. Only a comment may follow the quote.
fn quoted(text: &str) -> Result<Vec<u8>, String> {
    let body = text
        .strip_prefix('"')
        .ok_or("the argument must be in double quotes")?;
    let mut end = None;
    let mut escaped = false;
    for (i, c) in body.char_indices() {
        match (escaped, c) {
            (true, _) => escaped = false,
            (false, '\\') => escaped = true,
            (false, '"') => {
                end = Some(i);
                break;
            }
            _ => {}
        }
    }
    let end = end.ok_or("the closing double quote is missing")?;
    let after = body[end + 1..].trim();
    if !after.is_empty() && !after.starts_with('#') {
        return Err(format!("unexpected text after the string: {after:?}"));
    }
    decode_escapes(&body[..end])
}

/// Decodes the escapes of a script string (the text between the quotes).
pub fn decode_escapes(text: &str) -> Result<Vec<u8>, String> {
    let mut out = Vec::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            let mut buf = [0u8; 4];
            out.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
            continue;
        }
        let escape = chars.next().ok_or("a backslash ends the string")?;
        out.push(match escape {
            'r' => b'\r',
            'n' => b'\n',
            't' => b'\t',
            'e' => 0x1b,
            'a' => 0x07,
            'b' => 0x08,
            '\\' => b'\\',
            '"' => b'"',
            'x' => {
                let digits: String = chars.by_ref().take(2).collect();
                if digits.len() != 2 || !digits.bytes().all(|b| b.is_ascii_hexdigit()) {
                    return Err(format!("\\x needs two hex digits, not {digits:?}"));
                }
                u8::from_str_radix(&digits, 16).map_err(|e| e.to_string())?
            }
            other => return Err(format!("unknown escape \\{other}")),
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_decode_to_bytes() {
        assert_eq!(
            decode_escapes(r"a\r\n\t\e\a\b\\\x41\x7f\x00").expect("valid"),
            b"a\r\n\t\x1b\x07\x08\\A\x7f\x00"
        );
        assert_eq!(
            decode_escapes(r#"say \"hi\""#).expect("valid"),
            b"say \"hi\""
        );
    }

    #[test]
    fn text_that_is_not_ascii_stays_utf8() {
        assert_eq!(
            decode_escapes("日本 ✓").expect("valid"),
            "日本 ✓".as_bytes()
        );
    }

    #[test]
    fn bad_escapes_are_errors() {
        assert!(decode_escapes(r"\q").is_err());
        assert!(decode_escapes(r"\x4").is_err());
        assert!(decode_escapes(r"\xZZ").is_err());
        assert!(decode_escapes(r"\x").is_err());
        assert!(decode_escapes("abc\\").is_err());
    }

    #[test]
    fn a_script_parses_into_steps() {
        let script = "\
# vim session
send \"ihello\\e\"   # insert, then leave insert mode
expect \"-- INSERT --\"

wait 250
resize 80 24
send \"\\x03\"
expect_exit
";
        assert_eq!(
            parse_script(script).expect("valid"),
            vec![
                Step::Send(b"ihello\x1b".to_vec()),
                Step::Expect(b"-- INSERT --".to_vec()),
                Step::Wait(Duration::from_millis(250)),
                Step::Resize { cols: 80, rows: 24 },
                Step::Send(vec![3]),
                Step::ExpectExit,
            ]
        );
    }

    #[test]
    fn a_quote_inside_a_string_does_not_end_it() {
        assert_eq!(
            parse_script(r#"send "echo \"a # b\"\r" # note"#).expect("valid"),
            vec![Step::Send(b"echo \"a # b\"\r".to_vec())]
        );
    }

    #[test]
    fn the_error_names_the_line() {
        let cases = [
            ("send hello", 1),
            ("\n\nsend \"open", 3),
            ("# c\nexpect \"\"", 2),
            ("send \"\"", 1),
            ("wait soon", 1),
            ("wait", 1),
            ("resize 80", 1),
            ("resize 80 24 3", 1),
            ("resize 0 24", 1),
            ("resize 70000 24", 1),
            ("expect_exit now", 1),
            ("jump", 1),
            ("send \"a\" b", 1),
            ("send \"\\q\"", 1),
        ];
        for (script, line) in cases {
            let error = parse_script(script).expect_err(script);
            assert_eq!(error.line, line, "{script:?}: {error}");
        }
    }

    #[test]
    fn an_empty_script_has_no_steps() {
        assert_eq!(parse_script("").expect("valid"), vec![]);
        assert_eq!(parse_script("# only a comment\n\n").expect("valid"), vec![]);
    }
}
