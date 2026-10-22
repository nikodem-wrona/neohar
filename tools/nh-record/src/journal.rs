//! The journal file that `nh-record` saves.
//!
//! A journal file is the holder's own framing: a series of framed
//! [`HolderEvent`]s, each a `u32` little-endian length and a `postcard` body. The file holds the
//! `Output` events of the PTY stream and the `Marker` events, in the order and with the offsets
//! that the holder sent them. The first event is a synthetic `Marker::Resize` at offset 0 with the
//! size at the start, because the holder writes no marker for its first size.

use std::{
    fmt::Write as _,
    io::{Read, Write},
};

use nh_proto::{
    framing::{self, FramingError},
    holder::{HolderEvent, Marker, StreamId},
};

/// An error from reading or writing a journal file.
#[derive(Debug, thiserror::Error)]
pub enum JournalError {
    #[error("journal: {0}")]
    Framing(#[from] FramingError),
}

/// Reads every event of a journal file. A file that ends between two events is complete. A file
/// that ends inside an event is an error.
pub fn read_journal(r: &mut impl Read) -> Result<Vec<HolderEvent>, JournalError> {
    let mut events = Vec::new();
    loop {
        match framing::read_msg::<_, HolderEvent>(r) {
            Ok(event) => events.push(event),
            Err(FramingError::Eof) => return Ok(events),
            Err(e) => return Err(e.into()),
        }
    }
}

/// Writes events as a journal file, one framed message each.
pub fn write_journal(w: &mut impl Write, events: &[HolderEvent]) -> Result<(), JournalError> {
    for event in events {
        framing::write_msg(w, event)?;
    }
    Ok(())
}

/// The PTY output bytes of a journal, in order, without the markers.
pub fn output_bytes(events: &[HolderEvent]) -> Vec<u8> {
    let mut out = Vec::new();
    for event in events {
        if let HolderEvent::Output {
            stream: StreamId::Pty,
            bytes,
            ..
        } = event
        {
            out.extend_from_slice(bytes);
        }
    }
    out
}

/// The `Resize` markers of a journal, as `(offset, cols, rows)`.
pub fn resizes(events: &[HolderEvent]) -> Vec<(u64, u16, u16)> {
    events
        .iter()
        .filter_map(|event| match event {
            HolderEvent::Marker {
                offset,
                marker: Marker::Resize { cols, rows },
            } => Some((*offset, *cols, *rows)),
            _ => None,
        })
        .collect()
}

/// Shows bytes with control characters as escapes: `\e`, `\r`, `\n`, `\xNN`. UTF-8 stays as text.
pub fn escape_bytes(bytes: &[u8]) -> String {
    let mut out = String::new();
    for chunk in bytes.utf8_chunks() {
        for c in chunk.valid().chars() {
            match c {
                '\x1b' => out.push_str("\\e"),
                '\r' => out.push_str("\\r"),
                '\n' => out.push_str("\\n"),
                '\t' => out.push_str("\\t"),
                '\\' => out.push_str("\\\\"),
                c if c.is_control() => {
                    let _ = write!(out, "\\x{:02x}", u32::from(c));
                }
                c => out.push(c),
            }
        }
        for byte in chunk.invalid() {
            let _ = write!(out, "\\x{byte:02x}");
        }
    }
    out
}

/// One line per event, for a person who looks at a journal: `nh-record --dump`.
pub fn dump(events: &[HolderEvent]) -> String {
    let mut out = String::new();
    for event in events {
        let _ = match event {
            HolderEvent::Output {
                offset,
                stream,
                bytes,
            } => writeln!(
                out,
                "output {offset} {stream:?} {} bytes: {}",
                bytes.len(),
                escape_bytes(bytes)
            ),
            HolderEvent::Marker { offset, marker } => {
                writeln!(out, "marker {offset} {marker:?}")
            }
            other => writeln!(out, "event {other:?}"),
        };
    }
    out
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;

    fn sample() -> Vec<HolderEvent> {
        vec![
            HolderEvent::Marker {
                offset: 0,
                marker: Marker::Resize {
                    cols: 100,
                    rows: 30,
                },
            },
            HolderEvent::Output {
                offset: 0,
                stream: StreamId::Pty,
                bytes: b"hello\r\n".to_vec(),
            },
            HolderEvent::Marker {
                offset: 7,
                marker: Marker::CutPoint,
            },
        ]
    }

    #[test]
    fn a_journal_round_trips() {
        let mut file = Vec::new();
        write_journal(&mut file, &sample()).expect("written");
        assert_eq!(
            read_journal(&mut Cursor::new(file)).expect("read"),
            sample()
        );
    }

    #[test]
    fn an_empty_file_is_an_empty_journal() {
        assert!(
            read_journal(&mut Cursor::new(Vec::new()))
                .expect("read")
                .is_empty()
        );
    }

    #[test]
    fn a_file_that_ends_inside_an_event_is_an_error() {
        let mut file = Vec::new();
        write_journal(&mut file, &sample()).expect("written");
        file.pop();
        assert!(read_journal(&mut Cursor::new(file)).is_err());
    }

    #[test]
    fn output_and_resizes_are_picked_out() {
        assert_eq!(output_bytes(&sample()), b"hello\r\n");
        assert_eq!(resizes(&sample()), vec![(0, 100, 30)]);
    }

    #[test]
    fn escapes_show_control_bytes() {
        assert_eq!(escape_bytes(b"\x1b[1mA\r\n\x00"), "\\e[1mA\\r\\n\\x00");
        assert_eq!(escape_bytes("é".as_bytes()), "é");
        assert_eq!(escape_bytes(&[0xff]), "\\xff");
    }
}
