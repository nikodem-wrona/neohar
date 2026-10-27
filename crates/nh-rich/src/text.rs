//! Text helpers: UTF-16 offsets and size caps.

/// The length of `text` in UTF-16 code units.
pub fn utf16_len(text: &str) -> u32 {
    let units: usize = text.chars().map(char::len_utf16).sum();
    u32::try_from(units).unwrap_or(u32::MAX)
}

/// Turns byte offsets of one string into UTF-16 code unit offsets.
///
/// An ASCII string needs no table, because both offsets are the same.
pub struct Utf16Map {
    /// For each byte offset, the offset in UTF-16 code units. `None` for an ASCII string.
    table: Option<Vec<u32>>,
    len: usize,
}

impl Utf16Map {
    pub fn new(text: &str) -> Self {
        if text.is_ascii() {
            return Self {
                table: None,
                len: text.len(),
            };
        }
        let mut table = Vec::with_capacity(text.len() + 1);
        let mut units = 0u32;
        for ch in text.chars() {
            let width = u32::try_from(ch.len_utf16()).unwrap_or(2);
            // A byte inside a character has the offset of the character. Nobody asks for it.
            for _ in 0..ch.len_utf8() {
                table.push(units);
            }
            units = units.saturating_add(width);
        }
        table.push(units);
        Self {
            table: Some(table),
            len: text.len(),
        }
    }

    /// The offset in UTF-16 code units of a byte offset. An offset past the end gives the end.
    pub fn at(&self, byte: usize) -> u32 {
        let byte = byte.min(self.len);
        match &self.table {
            None => u32::try_from(byte).unwrap_or(u32::MAX),
            Some(table) => table.get(byte).copied().unwrap_or(0),
        }
    }
}

/// A string cut at a size limit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Capped<'a> {
    pub text: &'a str,
    /// Text was cut off.
    pub truncated: bool,
}

/// The cap for the output of a tool: 64 KB.
pub const OUTPUT_CAP: usize = 64 * 1024;

/// Cuts `text` to at most `max` bytes, at a character boundary. When the cut falls inside a line,
/// it moves back to the end of the line before it, if that line end is in the last 256 bytes.
pub fn cap(text: &str, max: usize) -> Capped<'_> {
    if text.len() <= max {
        return Capped {
            text,
            truncated: false,
        };
    }
    let mut end = max;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    // The search goes through bytes: the window can start inside a character, and a line end is
    // one byte that is never part of a longer character.
    let window_start = end.saturating_sub(256);
    if let Some(newline) = text.as_bytes()[window_start..end]
        .iter()
        .rposition(|&byte| byte == b'\n')
    {
        end = window_start + newline + 1;
    }
    Capped {
        text: &text[..end],
        truncated: true,
    }
}

/// The output of a tool, cut at 64 KB.
pub fn cap_output(text: &str) -> Capped<'_> {
    cap(text, OUTPUT_CAP)
}

/// Takes the control sequences of a terminal out of `text`: the OSC sequences (an OSC 8 hyperlink
/// carries a URL that the text does not show), the CSI sequences (colours, cursor moves), the other
/// escape sequences, and the control characters. A tab, a line end and a carriage return stay.
pub fn strip_terminal_controls(text: &str) -> std::borrow::Cow<'_, str> {
    if !text.chars().any(is_control) {
        return std::borrow::Cow::Borrowed(text);
    }
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '\u{1b}' => match chars.next() {
                // OSC: up to BEL or ST (ESC \).
                Some(']') => {
                    while let Some(next) = chars.next() {
                        if next == '\u{7}' {
                            break;
                        }
                        if next == '\u{1b}' {
                            if chars.peek() == Some(&'\\') {
                                chars.next();
                            }
                            break;
                        }
                    }
                }
                // CSI: parameters, then a final byte from `@` to `~`.
                Some('[') => {
                    for next in chars.by_ref() {
                        if ('@'..='~').contains(&next) {
                            break;
                        }
                    }
                }
                // DCS, SOS, PM and APC end with ST.
                Some('P' | 'X' | '^' | '_') => {
                    while let Some(next) = chars.next() {
                        if next == '\u{1b}' {
                            if chars.peek() == Some(&'\\') {
                                chars.next();
                            }
                            break;
                        }
                    }
                }
                // A two-character sequence, such as `ESC c`.
                Some(_) | None => {}
            },
            '\t' | '\n' | '\r' => out.push(ch),
            other if is_control(other) => {}
            other => out.push(other),
        }
    }
    std::borrow::Cow::Owned(out)
}

fn is_control(ch: char) -> bool {
    ch.is_control() && !matches!(ch, '\t' | '\n' | '\r') || ch == '\u{1b}'
}

/// Whether a character changes how the text around it looks, or hides, with no glyph of its own:
/// the format characters of Unicode (the controls of direction such as U+202E, the characters of
/// zero width, the byte order mark, the tags), and the separators of lines and paragraphs. A
/// command or a file name can use them to show a person other text than the text that runs.
pub fn is_invisible(ch: char) -> bool {
    matches!(
        ch,
        '\u{00AD}'
            | '\u{0600}'..='\u{0605}'
            | '\u{061C}'
            | '\u{06DD}'
            | '\u{070F}'
            | '\u{0890}'..='\u{0891}'
            | '\u{08E2}'
            | '\u{115F}'..='\u{1160}'
            | '\u{17B4}'..='\u{17B5}'
            | '\u{180B}'..='\u{180F}'
            | '\u{200B}'..='\u{200F}'
            | '\u{2028}'..='\u{202E}'
            | '\u{2060}'..='\u{206F}'
            | '\u{3164}'
            | '\u{FEFF}'
            | '\u{FFA0}'
            | '\u{FFF9}'..='\u{FFFB}'
            | '\u{110BD}'
            | '\u{110CD}'
            | '\u{13430}'..='\u{1343F}'
            | '\u{1BCA0}'..='\u{1BCA3}'
            | '\u{1D173}'..='\u{1D17A}'
            | '\u{E0001}'
            | '\u{E0020}'..='\u{E007F}'
    )
}

/// Whether a character must not reach a person as it is: an invisible character, or a control
/// character other than a tab and, when `lines` is true, a line feed.
fn needs_escape(ch: char, lines: bool) -> bool {
    is_invisible(ch) || (ch.is_control() && ch != '\t' && !(lines && ch == '\n'))
}

fn push_escape(out: &mut String, ch: char) {
    use std::fmt::Write as _;
    let _ = write!(out, "<U+{:04X}>", u32::from(ch));
}

/// `text` as a person must see it: each invisible character and each control character becomes
/// a visible escape such as `<U+202E>`. A tab stays. Use it for text on one line: a title, a
/// label, the name of a file.
pub fn visible(text: &str) -> std::borrow::Cow<'_, str> {
    escape(text, false)
}

/// [`visible`] for text with many lines: a line feed stays too.
pub fn visible_lines(text: &str) -> std::borrow::Cow<'_, str> {
    escape(text, true)
}

fn escape(text: &str, lines: bool) -> std::borrow::Cow<'_, str> {
    if !text.chars().any(|ch| needs_escape(ch, lines)) {
        return std::borrow::Cow::Borrowed(text);
    }
    let mut out = String::with_capacity(text.len() + 16);
    for ch in text.chars() {
        if needs_escape(ch, lines) {
            push_escape(&mut out, ch);
        } else {
            out.push(ch);
        }
    }
    std::borrow::Cow::Owned(out)
}

/// [`visible_lines`] for text with highlighted ranges: the ranges move with the text, so each
/// still covers the same characters.
pub fn visible_spans(spans: crate::Spans) -> crate::Spans {
    if !spans.text.chars().any(|ch| needs_escape(ch, true)) {
        return spans;
    }
    // For each UTF-16 offset of the old text at a character boundary, the offset in the new one.
    let mut map: Vec<(u32, u32)> = Vec::with_capacity(spans.text.len() + 1);
    let mut text = String::with_capacity(spans.text.len() + 16);
    let (mut old, mut new) = (0u32, 0u32);
    for ch in spans.text.chars() {
        map.push((old, new));
        let before = text.len();
        if needs_escape(ch, true) {
            push_escape(&mut text, ch);
        } else {
            text.push(ch);
        }
        old = old.saturating_add(u32::try_from(ch.len_utf16()).unwrap_or(2));
        new = new.saturating_add(utf16_len(&text[before..]));
    }
    map.push((old, new));
    let at = |offset: u32| match map.binary_search_by_key(&offset, |(o, _)| *o) {
        Ok(index) => map[index].1,
        // Inside a character: the start of that character.
        Err(index) => map[index.saturating_sub(1)].1,
    };
    crate::Spans {
        spans: spans
            .spans
            .into_iter()
            .map(|span| crate::Span {
                start: at(span.start),
                end: at(span.end),
                class: span.class,
            })
            .collect(),
        text,
    }
}

#[cfg(test)]
mod tests;
