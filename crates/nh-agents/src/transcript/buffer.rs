//! Reads lines from the bytes of a file that grows.
//!
//! The harness appends a line at a time, and a read can end in the middle of one. The buffer keeps
//! the bytes of the unfinished line and gives them back with the next read. It tells the offset of
//! each line, so the caller can save the offset of the first byte that it has not used.

/// A whole line and the offset of its first byte in the file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Line {
    pub offset: u64,
    pub text: String,
}

/// The unfinished line of a file.
#[derive(Debug, Default)]
pub struct LineBuffer {
    /// The offset of the first byte of `carry`.
    start: u64,
    carry: Vec<u8>,
}

impl LineBuffer {
    /// A buffer that goes on at `offset`, a place after a whole line.
    pub fn at(offset: u64) -> Self {
        Self {
            start: offset,
            carry: Vec::new(),
        }
    }

    /// The offset of the first byte that no line has used: the place to save and to read from.
    pub fn offset(&self) -> u64 {
        self.start
    }

    /// Takes bytes that follow the ones before. It gives the lines that these bytes finish. A line
    /// with bytes that are not UTF-8 is read with replacement characters.
    pub fn push(&mut self, bytes: &[u8]) -> Vec<Line> {
        self.carry.extend_from_slice(bytes);
        let mut lines = Vec::new();
        let mut from = 0;
        while let Some(at) = self.carry[from..].iter().position(|b| *b == b'\n') {
            let end = from + at;
            let raw = &self.carry[from..end];
            let raw = raw.strip_suffix(b"\r").unwrap_or(raw);
            if !raw.is_empty() {
                lines.push(Line {
                    offset: self.start + from as u64,
                    text: String::from_utf8_lossy(raw).into_owned(),
                });
            }
            from = end + 1;
        }
        self.carry.drain(..from);
        self.start += from as u64;
        lines
    }

    /// The file got shorter or was replaced: read it again from the start.
    pub fn reset(&mut self) {
        self.start = 0;
        self.carry.clear();
    }
}
