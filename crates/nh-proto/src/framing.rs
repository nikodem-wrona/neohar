//! Message framing for the holder socket.
//!
//! A message is a `u32` little-endian length followed by that many bytes of `postcard`. The length
//! is at most [`MAX_MSG_BYTES`]. This module uses `std::io` only. The holder and the tests use it
//! directly. The async client in `nh-core` uses the same layout.

use std::io::{self, Read, Write};

use serde::{Serialize, de::DeserializeOwned};

/// The largest message body in bytes: 4 MiB.
pub const MAX_MSG_BYTES: usize = 4 * 1024 * 1024;

/// An error from reading or writing a framed message.
#[derive(Debug, thiserror::Error)]
pub enum FramingError {
    /// The stream ended before the first byte of a message. This is a normal close.
    #[error("the stream ended between messages")]
    Eof,
    /// The stream ended inside a message.
    #[error("the stream ended inside a message")]
    Truncated,
    /// The message is longer than [`MAX_MSG_BYTES`].
    #[error("message of {len} bytes is over the limit of {MAX_MSG_BYTES} bytes")]
    TooLarge { len: u64 },
    /// The body has bytes after the value.
    #[error("{extra} unexpected bytes after the message value")]
    TrailingBytes { extra: usize },
    /// The body is not a valid value of the expected type.
    #[error("invalid message: {0}")]
    Codec(#[from] postcard::Error),
    #[error("i/o error: {0}")]
    Io(#[from] io::Error),
}

/// Writes one framed message. The header and the body go out in a single `write_all`.
pub fn write_msg<W: Write, T: Serialize + ?Sized>(w: &mut W, msg: &T) -> Result<(), FramingError> {
    let mut buf = postcard::to_extend(msg, vec![0u8; 4])?;
    let len = buf.len() - 4;
    if len > MAX_MSG_BYTES {
        return Err(FramingError::TooLarge { len: len as u64 });
    }
    // `len` is at most 4 MiB, so it fits in a `u32`.
    buf[..4].copy_from_slice(&(len as u32).to_le_bytes());
    w.write_all(&buf)?;
    Ok(())
}

/// Reads one framed message.
///
/// Returns [`FramingError::Eof`] when the stream ends before the first byte. The length is checked
/// before any allocation, so a peer cannot make this function allocate more than 4 MiB.
pub fn read_msg<R: Read, T: DeserializeOwned>(r: &mut R) -> Result<T, FramingError> {
    let mut header = [0u8; 4];
    let mut got = 0;
    while got < header.len() {
        match r.read(&mut header[got..]) {
            Ok(0) if got == 0 => return Err(FramingError::Eof),
            Ok(0) => return Err(FramingError::Truncated),
            Ok(n) => got += n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e.into()),
        }
    }
    let len = u32::from_le_bytes(header) as usize;
    if len > MAX_MSG_BYTES {
        return Err(FramingError::TooLarge { len: len as u64 });
    }
    let mut body = vec![0u8; len];
    r.read_exact(&mut body).map_err(|e| match e.kind() {
        io::ErrorKind::UnexpectedEof => FramingError::Truncated,
        _ => e.into(),
    })?;
    let (value, rest) = postcard::take_from_bytes(&body)?;
    if !rest.is_empty() {
        return Err(FramingError::TrailingBytes { extra: rest.len() });
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;
    use crate::holder::{HolderEvent, HolderRequest};

    #[test]
    fn messages_round_trip_through_a_stream() {
        let mut wire = Vec::new();
        write_msg(&mut wire, &HolderRequest::Ping { nonce: 9 }).expect("the message is written");
        write_msg(&mut wire, &HolderRequest::Status).expect("the message is written");
        let mut r = Cursor::new(wire);
        assert_eq!(
            read_msg::<_, HolderRequest>(&mut r).expect("first"),
            HolderRequest::Ping { nonce: 9 }
        );
        assert_eq!(
            read_msg::<_, HolderRequest>(&mut r).expect("second"),
            HolderRequest::Status
        );
        assert!(matches!(
            read_msg::<_, HolderRequest>(&mut r),
            Err(FramingError::Eof)
        ));
    }

    #[test]
    fn the_header_is_a_little_endian_length() {
        let mut wire = Vec::new();
        write_msg(&mut wire, &HolderRequest::Status).expect("the message is written");
        assert_eq!(wire, [1, 0, 0, 0, 1]);
    }

    #[test]
    fn a_stream_that_ends_inside_a_message_is_truncated() {
        for cut in 1..5 {
            let wire = [1u8, 0, 0, 0, 1];
            let mut r = Cursor::new(&wire[..cut]);
            assert!(
                matches!(
                    read_msg::<_, HolderRequest>(&mut r),
                    Err(FramingError::Truncated)
                ),
                "cut {cut}"
            );
        }
    }

    #[test]
    fn a_length_over_the_cap_is_rejected_before_any_body_is_read() {
        let wire = ((MAX_MSG_BYTES as u32) + 1).to_le_bytes();
        let mut r = Cursor::new(wire);
        assert!(matches!(
            read_msg::<_, HolderRequest>(&mut r),
            Err(FramingError::TooLarge { len }) if len == MAX_MSG_BYTES as u64 + 1
        ));
    }

    #[test]
    fn a_message_over_the_cap_is_not_written() {
        let event = HolderEvent::Output {
            offset: 0,
            stream: crate::holder::StreamId::Pty,
            bytes: vec![0; MAX_MSG_BYTES],
        };
        let mut wire = Vec::new();
        assert!(matches!(
            write_msg(&mut wire, &event),
            Err(FramingError::TooLarge { .. })
        ));
        assert!(wire.is_empty());
    }

    #[test]
    fn bytes_after_the_value_are_an_error() {
        let wire = [2u8, 0, 0, 0, 1, 1];
        let mut r = Cursor::new(wire);
        assert!(matches!(
            read_msg::<_, HolderRequest>(&mut r),
            Err(FramingError::TrailingBytes { extra: 1 })
        ));
    }

    #[test]
    fn a_body_that_is_not_a_value_is_a_codec_error() {
        let wire = [1u8, 0, 0, 0, 0xff];
        let mut r = Cursor::new(wire);
        assert!(matches!(
            read_msg::<_, HolderRequest>(&mut r),
            Err(FramingError::Codec(_))
        ));
    }
}
