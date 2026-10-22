//! The redaction on the output of a logger.
//!
//! A logger such as `tracing-subscriber` writes one event with one call, so the line is whole
//! when it reaches the writer. The writer cuts what it gets into lines and redacts each one. A
//! line that a call leaves open waits for the next call.

use std::io::{self, Write};

use crate::redact;

/// Redacts what is written to the writer below it.
pub struct RedactWriter<W: Write> {
    inner: W,
    /// The part of a line that has no end yet.
    pending: Vec<u8>,
}

impl<W: Write> RedactWriter<W> {
    pub fn new(inner: W) -> Self {
        Self {
            inner,
            pending: Vec::new(),
        }
    }

    fn write_line(&mut self, line: &[u8]) -> io::Result<()> {
        match std::str::from_utf8(line) {
            Ok(text) => self.inner.write_all(redact(text).as_bytes()),
            // Bytes that are no text pass as they are: a secret is text.
            Err(_) => self.inner.write_all(line),
        }
    }
}

impl<W: Write> Write for RedactWriter<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.pending.extend_from_slice(buf);
        while let Some(end) = self.pending.iter().position(|b| *b == b'\n') {
            let line: Vec<u8> = self.pending.drain(..=end).collect();
            self.write_line(&line)?;
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        if !self.pending.is_empty() {
            let rest = std::mem::take(&mut self.pending);
            self.write_line(&rest)?;
        }
        self.inner.flush()
    }
}

impl<W: Write> Drop for RedactWriter<W> {
    fn drop(&mut self) {
        let _ = self.flush();
    }
}
