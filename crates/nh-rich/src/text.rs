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
    let window_start = end.saturating_sub(256);
    if let Some(newline) = text[window_start..end].rfind('\n') {
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

#[cfg(test)]
mod tests;
