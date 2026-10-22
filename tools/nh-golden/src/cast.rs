//! The asciicast v2 writer (`session.cast`).
//!
//! The file is one JSON header line and then one `[seconds, code, data]` line per event: `o` for
//! output, `i` for input that a step sent, `m` for a marker (a `snapshot` step) and `r` for a
//! resize. The seconds are taken when the output arrives at the recorder. The header has no
//! wall-clock time, so that two recordings differ only in their timing.

use std::{
    io::{self, Write},
    time::Duration,
};

use serde_json::json;

/// Writes events to a sink.
pub struct CastWriter<W: Write> {
    out: W,
    /// The start of a UTF-8 sequence that the last chunk ended in the middle of.
    pending: Vec<u8>,
}

/// The seconds of an event, rounded to microseconds.
fn seconds(at: Duration) -> f64 {
    at.as_micros() as f64 / 1_000_000.0
}

impl<W: Write> CastWriter<W> {
    /// Writes the header.
    pub fn new(mut out: W, cols: u16, rows: u16, title: &str, term: &str) -> io::Result<Self> {
        let header = json!({
            "version": 2,
            "width": cols,
            "height": rows,
            "title": title,
            "env": { "TERM": term },
        });
        writeln!(out, "{header}")?;
        Ok(Self {
            out,
            pending: Vec::new(),
        })
    }

    fn event(&mut self, at: Duration, code: &str, data: &str) -> io::Result<()> {
        writeln!(self.out, "{}", json!([seconds(at), code, data]))
    }

    /// Output of the program. A character that a chunk cuts in two is written with the next chunk.
    /// Bytes that are not UTF-8 become U+FFFD.
    pub fn output(&mut self, at: Duration, bytes: &[u8]) -> io::Result<()> {
        let mut data = std::mem::take(&mut self.pending);
        data.extend_from_slice(bytes);
        let mut text = String::new();
        let mut rest = data.as_slice();
        loop {
            match std::str::from_utf8(rest) {
                Ok(valid) => {
                    text.push_str(valid);
                    break;
                }
                Err(e) => {
                    let (valid, tail) = rest.split_at(e.valid_up_to());
                    text.push_str(&String::from_utf8_lossy(valid));
                    match e.error_len() {
                        // The chunk ends in the middle of a character. Keep it for the next one.
                        None => {
                            self.pending = tail.to_vec();
                            break;
                        }
                        Some(len) => {
                            text.push('\u{fffd}');
                            rest = &tail[len..];
                        }
                    }
                }
            }
        }
        if text.is_empty() {
            return Ok(());
        }
        self.event(at, "o", &text)
    }

    /// Input that a step sent to the program.
    pub fn input(&mut self, at: Duration, bytes: &[u8]) -> io::Result<()> {
        self.event(at, "i", &String::from_utf8_lossy(bytes))
    }

    pub fn marker(&mut self, at: Duration, name: &str) -> io::Result<()> {
        self.event(at, "m", name)
    }

    pub fn resize(&mut self, at: Duration, cols: u16, rows: u16) -> io::Result<()> {
        self.event(at, "r", &format!("{cols}x{rows}"))
    }

    /// The sink, to read what was written so far.
    pub fn get_ref(&self) -> &W {
        &self.out
    }

    /// Flushes the sink and gives it back.
    pub fn finish(mut self) -> io::Result<W> {
        // A character that stays cut at the end of the recording is written as it is.
        if !self.pending.is_empty() {
            let tail = String::from_utf8_lossy(&std::mem::take(&mut self.pending)).into_owned();
            self.event(Duration::ZERO, "o", &tail)?;
        }
        self.out.flush()?;
        Ok(self.out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(bytes: Vec<u8>) -> Vec<serde_json::Value> {
        String::from_utf8(bytes)
            .expect("UTF-8")
            .lines()
            .map(|l| serde_json::from_str(l).expect("JSON"))
            .collect()
    }

    fn finished(w: CastWriter<Vec<u8>>) -> Vec<serde_json::Value> {
        lines(w.finish().expect("finish"))
    }

    #[test]
    fn the_header_has_the_size_and_no_clock_time() {
        let w = CastWriter::new(Vec::new(), 120, 40, "t", "xterm-256color").expect("header");
        let v = finished(w);
        assert_eq!(v[0]["version"], 2);
        assert_eq!(
            (v[0]["width"].as_u64(), v[0]["height"].as_u64()),
            (Some(120), Some(40))
        );
        assert_eq!(v[0]["env"]["TERM"], "xterm-256color");
        assert!(v[0].get("timestamp").is_none());
    }

    #[test]
    fn events_carry_the_time_the_code_and_the_data() {
        let mut w = CastWriter::new(Vec::new(), 80, 24, "t", "xterm").expect("header");
        w.output(Duration::from_millis(1500), b"hi\x1b[0m")
            .expect("output");
        w.input(Duration::from_micros(2_000_001), b"\r")
            .expect("input");
        w.marker(Duration::from_secs(3), "answered")
            .expect("marker");
        w.resize(Duration::from_secs(4), 100, 30).expect("resize");
        let v = finished(w);
        assert_eq!(v[1], json!([1.5, "o", "hi\u{1b}[0m"]));
        assert_eq!(v[2], json!([2.000001, "i", "\r"]));
        assert_eq!(v[3], json!([3.0, "m", "answered"]));
        assert_eq!(v[4], json!([4.0, "r", "100x30"]));
    }

    #[test]
    fn a_character_cut_between_two_chunks_is_not_damaged() {
        let bytes = "a❯b".as_bytes();
        let (first, second) = bytes.split_at(2); // inside the three bytes of ❯
        let mut w = CastWriter::new(Vec::new(), 80, 24, "t", "xterm").expect("header");
        w.output(Duration::from_secs(1), first).expect("first");
        w.output(Duration::from_secs(2), second).expect("second");
        let v = finished(w);
        assert_eq!(v[1], json!([1.0, "o", "a"]));
        assert_eq!(v[2], json!([2.0, "o", "❯b"]));
    }

    #[test]
    fn bytes_that_are_not_utf8_become_the_replacement_character() {
        let mut w = CastWriter::new(Vec::new(), 80, 24, "t", "xterm").expect("header");
        w.output(Duration::from_secs(1), b"a\xffb").expect("output");
        let v = finished(w);
        assert_eq!(v[1], json!([1.0, "o", "a\u{fffd}b"]));
    }
}
