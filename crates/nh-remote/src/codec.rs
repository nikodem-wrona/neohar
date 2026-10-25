//! The bytes of `link/1`: framing, parts, classes and compression.
//!
//! This module does no I/O, so that its rules have tests of their own.
//!
//! - **Framing.** The bytes on the link are parts. A part is a `u32` little-endian length and a
//!   `postcard` body: a [`Part`] with one byte of flags and at most 16 KiB of the message.
//! - **Parts.** A message of at most 16 KiB is one part. A larger message goes in parts of 16 KiB.
//!   The flag `more` says that the next part of the same class continues it.
//! - **Classes.** The [`Scheduler`] keeps one queue for each class and sends a part of the lowest
//!   class that has one. A message of class 0 thus waits at most one part of another message.
//!   Messages of one class keep their order, and their parts never interleave, so the receiver
//!   keeps one buffer for each class.
//! - **Compression.** A message of class 1, 2 or 3 that is larger than 4 KiB is compressed with
//!   zstd at level 1. The sender sends the smaller form, and the flag `zstd` tells which.

use std::{collections::VecDeque, io::Read};

use serde::{Deserialize, Serialize};

use crate::msg::{Class, LinkMsg};

/// The most bytes of a message in one part.
pub const PART_SIZE: usize = 16 * 1024;

/// A message of class 1 or higher that is larger than this is compressed.
pub const COMPRESS_ABOVE: usize = 4 * 1024;

/// The largest message from a client to a core, before and after compression. A task with 20
/// files of 20 MiB in base64 fits.
pub const MAX_MESSAGE: usize = 640 << 20;

/// The largest message from a core to a client, before and after compression. A core sends
/// replies, projections, frames and parts of blobs of 64 KiB, and none of them needs more. A client
/// that does not trust the core thus keeps at most this much for each class.
pub const MAX_FROM_CORE: usize = 64 << 20;

/// The largest body of a part on the wire: the flags, the length of the bytes and the bytes.
pub const MAX_FRAME: usize = PART_SIZE + 8;

const CLASS_MASK: u8 = 0b0011;
const MORE: u8 = 0b0100;
const ZSTD: u8 = 0b1000;

/// One part on the wire.
#[derive(Serialize, Deserialize)]
struct Part<'a> {
    flags: u8,
    #[serde(borrow, with = "serde_bytes")]
    bytes: &'a [u8],
}

/// Why bytes are not `link/1`. Every one ends the link.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum CodecError {
    #[error("a part of {0} bytes is over the limit of {MAX_FRAME}")]
    FrameTooLarge(usize),
    #[error("a part is not postcard")]
    BadPart,
    #[error("a part has unknown flags {0:#04x}")]
    BadFlags(u8),
    #[error("a part changes the compression of its message")]
    MixedCompression,
    #[error("a message is over the size limit of this side")]
    MessageTooLarge,
    #[error("a compressed message does not decompress")]
    BadCompression,
    #[error("a message does not encode: {0}")]
    Encode(String),
}

/// A message that is ready to go: its class and its bytes.
#[derive(Clone, Debug)]
pub struct Outgoing {
    pub class: Class,
    pub compressed: bool,
    pub bytes: Vec<u8>,
    sent: usize,
}

/// Encodes a message and compresses it when the rules say so.
pub fn encode(msg: &LinkMsg) -> Result<Outgoing, CodecError> {
    encode_within(msg, MAX_MESSAGE)
}

/// Like [`encode`], with the size limit of the receiver.
pub fn encode_within(msg: &LinkMsg, limit: usize) -> Result<Outgoing, CodecError> {
    let raw = postcard::to_allocvec(msg).map_err(|e| CodecError::Encode(e.to_string()))?;
    if raw.len() > limit {
        return Err(CodecError::MessageTooLarge);
    }
    let class = msg.class();
    let (bytes, compressed) = if class != Class::Control && raw.len() > COMPRESS_ABOVE {
        match zstd::bulk::compress(&raw, 1) {
            Ok(small) if small.len() < raw.len() => (small, true),
            _ => (raw, false),
        }
    } else {
        (raw, false)
    };
    Ok(Outgoing {
        class,
        compressed,
        bytes,
        sent: 0,
    })
}

/// The queues of one sender, one for each class.
#[derive(Debug, Default)]
pub struct Scheduler {
    queues: [VecDeque<Outgoing>; 4],
    /// The bytes of the queued messages that are not sent yet.
    bytes: usize,
}

impl Scheduler {
    pub fn push(&mut self, out: Outgoing) {
        self.bytes = self.bytes.saturating_add(out.bytes.len());
        self.queues[out.class.index()].push_back(out);
    }

    pub fn is_empty(&self) -> bool {
        self.queues.iter().all(VecDeque::is_empty)
    }

    /// The number of messages that wait, in all classes.
    pub fn len(&self) -> usize {
        self.queues.iter().map(VecDeque::len).sum()
    }

    /// The bytes of the messages that wait and are not sent yet.
    pub fn queued_bytes(&self) -> usize {
        self.bytes
    }

    /// Appends the next part to `wire`: the length and the body. Returns `false` when no part
    /// waits.
    pub fn next_part(&mut self, wire: &mut Vec<u8>) -> bool {
        let Some(queue) = self.queues.iter_mut().find(|q| !q.is_empty()) else {
            return false;
        };
        let Some(head) = queue.front_mut() else {
            return false;
        };
        let end = (head.sent + PART_SIZE).min(head.bytes.len());
        let more = end < head.bytes.len();
        let mut flags = head.class as u8;
        if more {
            flags |= MORE;
        }
        if head.compressed {
            flags |= ZSTD;
        }
        let part = Part {
            flags,
            bytes: &head.bytes[head.sent..end],
        };
        // A part of at most 16 KiB always encodes.
        let body = postcard::to_allocvec(&part).unwrap_or_default();
        let len = u32::try_from(body.len()).unwrap_or(u32::MAX);
        wire.extend_from_slice(&len.to_le_bytes());
        wire.extend_from_slice(&body);
        self.bytes = self.bytes.saturating_sub(end - head.sent);
        head.sent = end;
        if !more {
            queue.pop_front();
        }
        true
    }
}

/// A message whose parts have not all come.
#[derive(Debug)]
struct Partial {
    compressed: bool,
    bytes: Vec<u8>,
}

/// Reads the bytes of a link into messages.
#[derive(Debug)]
pub struct Decoder {
    buf: Vec<u8>,
    partial: [Option<Partial>; 4],
    /// The largest message, before and after compression.
    limit: usize,
    /// The messages that came whole and that this version cannot read: a kind of a newer core, for
    /// example. They are dropped, and the link stays.
    pub skipped: u64,
}

/// A decoder with the limit of a core: [`MAX_MESSAGE`].
impl Default for Decoder {
    fn default() -> Self {
        Self::with_limit(MAX_MESSAGE)
    }
}

impl Decoder {
    /// A decoder that ends the link on a message larger than `limit`, before or after
    /// compression.
    pub fn with_limit(limit: usize) -> Self {
        Self {
            buf: Vec::new(),
            partial: Default::default(),
            limit,
            skipped: 0,
        }
    }

    /// Takes the next bytes of the link and returns the messages that they complete.
    pub fn feed(&mut self, data: &[u8]) -> Result<Vec<LinkMsg>, CodecError> {
        self.buf.extend_from_slice(data);
        let mut out = Vec::new();
        let mut at = 0;
        while let Some(head) = self.buf.get(at..at + 4) {
            let len = u32::from_le_bytes([head[0], head[1], head[2], head[3]]) as usize;
            if len > MAX_FRAME {
                return Err(CodecError::FrameTooLarge(len));
            }
            let Some(body) = self.buf.get(at + 4..at + 4 + len) else {
                break;
            };
            let part: Part<'_> = postcard::from_bytes(body).map_err(|_| CodecError::BadPart)?;
            if let Some(msg) = take_part(&mut self.partial, &mut self.skipped, self.limit, &part)? {
                out.push(msg);
            }
            at += 4 + len;
        }
        self.buf.drain(..at);
        Ok(out)
    }
}

fn take_part(
    partial: &mut [Option<Partial>; 4],
    skipped: &mut u64,
    limit: usize,
    part: &Part<'_>,
) -> Result<Option<LinkMsg>, CodecError> {
    if part.flags & !(CLASS_MASK | MORE | ZSTD) != 0 {
        return Err(CodecError::BadFlags(part.flags));
    }
    let class =
        Class::from_index(part.flags & CLASS_MASK).ok_or(CodecError::BadFlags(part.flags))?;
    let compressed = part.flags & ZSTD != 0;
    let slot = &mut partial[class.index()];
    let message = slot.get_or_insert_with(|| Partial {
        compressed,
        bytes: Vec::new(),
    });
    if message.compressed != compressed {
        return Err(CodecError::MixedCompression);
    }
    if message.bytes.len() + part.bytes.len() > limit {
        return Err(CodecError::MessageTooLarge);
    }
    message.bytes.extend_from_slice(part.bytes);
    if part.flags & MORE != 0 {
        return Ok(None);
    }
    let Some(whole) = slot.take() else {
        return Ok(None);
    };
    let bytes = if whole.compressed {
        decompress(&whole.bytes, limit)?
    } else {
        whole.bytes
    };
    match postcard::from_bytes::<LinkMsg>(&bytes) {
        Ok(msg) => Ok(Some(msg)),
        Err(error) => {
            *skipped += 1;
            tracing::debug!(%error, "a message of the link is of a kind that this version does not read");
            Ok(None)
        }
    }
}

/// Decompresses a message, with a limit on the size, so that a small body cannot fill the memory.
fn decompress(bytes: &[u8], limit: usize) -> Result<Vec<u8>, CodecError> {
    let decoder =
        zstd::stream::read::Decoder::new(bytes).map_err(|_| CodecError::BadCompression)?;
    let mut out = Vec::new();
    decoder
        .take(limit as u64 + 1)
        .read_to_end(&mut out)
        .map_err(|_| CodecError::BadCompression)?;
    if out.len() > limit {
        return Err(CodecError::MessageTooLarge);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    fn call(id: u64, method: &str, params: Vec<u8>) -> LinkMsg {
        LinkMsg::Call {
            id,
            window: None,
            device: None,
            method: method.to_owned(),
            params,
        }
    }

    fn frame(stream: u64, bytes: Vec<u8>) -> LinkMsg {
        LinkMsg::Frame { stream, bytes }
    }

    fn ping(nonce: u64) -> LinkMsg {
        LinkMsg::Ping {
            nonce,
            sent_at_ms: nonce,
        }
    }

    /// Bytes that zstd cannot make smaller.
    fn noise(len: usize, seed: u64) -> Vec<u8> {
        let mut x = seed.wrapping_mul(6_364_136_223_846_793_005) | 1;
        (0..len)
            .map(|_| {
                x ^= x << 13;
                x ^= x >> 7;
                x ^= x << 17;
                (x >> 24) as u8
            })
            .collect()
    }

    /// Sends the messages through a scheduler and returns the wire, one part at a time.
    fn wire_of(messages: &[LinkMsg]) -> Vec<Vec<u8>> {
        let mut scheduler = Scheduler::default();
        for msg in messages {
            scheduler.push(encode(msg).expect("encodes"));
        }
        let mut parts = Vec::new();
        loop {
            let mut part = Vec::new();
            if !scheduler.next_part(&mut part) {
                return parts;
            }
            parts.push(part);
        }
    }

    fn decode_all(parts: &[Vec<u8>]) -> Vec<LinkMsg> {
        let mut decoder = Decoder::default();
        parts
            .iter()
            .flat_map(|part| decoder.feed(part).expect("decodes"))
            .collect()
    }

    #[test]
    fn a_small_message_is_one_part_and_comes_back() {
        let msg = call(7, "pane.resize", br#"{"cols":80}"#.to_vec());
        let parts = wire_of(std::slice::from_ref(&msg));
        assert_eq!(parts.len(), 1);
        let len = u32::from_le_bytes(parts[0][..4].try_into().expect("four bytes")) as usize;
        assert_eq!(len, parts[0].len() - 4);
        assert_eq!(decode_all(&parts), [msg]);
    }

    #[test]
    fn a_large_message_goes_in_parts_of_16_kib() {
        let msg = frame(1, noise(100_000, 3));
        let encoded = encode(&msg).expect("encodes");
        // Noise does not compress, so the message keeps its size.
        assert!(!encoded.compressed);
        let parts = wire_of(std::slice::from_ref(&msg));
        assert_eq!(parts.len(), encoded.bytes.len().div_ceil(PART_SIZE));
        assert!(parts.iter().all(|p| p.len() <= 4 + MAX_FRAME));
        assert_eq!(decode_all(&parts), [msg]);
    }

    #[test]
    fn class_0_waits_at_most_one_part_of_a_large_message() {
        let mut scheduler = Scheduler::default();
        scheduler.push(encode(&frame(1, noise(200_000, 5))).expect("encodes"));
        let mut first = Vec::new();
        assert!(scheduler.next_part(&mut first));
        // Input comes while the frame is on its way.
        scheduler.push(encode(&ping(9)).expect("encodes"));
        let mut second = Vec::new();
        assert!(scheduler.next_part(&mut second));
        let mut decoder = Decoder::default();
        assert!(decoder.feed(&first).expect("decodes").is_empty());
        assert_eq!(decoder.feed(&second).expect("decodes"), [ping(9)]);
        let mut rest = Vec::new();
        while scheduler.next_part(&mut rest) {}
        let tail = decoder.feed(&rest).expect("decodes");
        assert_eq!(tail.len(), 1);
        assert!(matches!(&tail[0], LinkMsg::Frame { stream: 1, .. }));
    }

    #[test]
    fn a_large_message_of_class_1_and_higher_is_compressed_when_smaller() {
        let text = b"{\"type\":\"patch\",\"ops\":[]}".repeat(400);
        assert!(text.len() > COMPRESS_ABOVE);
        let projection = LinkMsg::Projection {
            sub: 1,
            message: text.clone(),
        };
        let out = encode(&projection).expect("encodes");
        assert!(out.compressed);
        assert!(out.bytes.len() < text.len() / 4);
        assert_eq!(
            decode_all(&wire_of(std::slice::from_ref(&projection))),
            [projection]
        );

        // Noise is larger after zstd, so it goes as it is.
        assert!(
            !encode(&frame(1, noise(10_000, 1)))
                .expect("encodes")
                .compressed
        );
        // Small messages and class 0 are never compressed.
        assert!(
            !encode(&LinkMsg::Projection {
                sub: 1,
                message: b"a".repeat(COMPRESS_ABOVE - 16),
            })
            .expect("encodes")
            .compressed
        );
        assert!(
            !encode(&call(1, "pane.paste", b"a".repeat(100_000)))
                .expect("encodes")
                .compressed
        );
    }

    #[test]
    fn the_flag_of_compression_is_in_every_part_of_the_message() {
        let msg = LinkMsg::Projection {
            sub: 2,
            // Hex text of noise: half of it goes away, and the rest is many parts.
            message: noise(100_000, 9)
                .iter()
                .flat_map(|b| format!("{b:02x}").into_bytes())
                .collect(),
        };
        let parts = wire_of(std::slice::from_ref(&msg));
        assert!(parts.len() > 1, "{}", parts.len());
        for part in &parts {
            let body: Part<'_> = postcard::from_bytes(&part[4..]).expect("a part");
            assert_ne!(body.flags & ZSTD, 0);
            assert_eq!(body.flags & CLASS_MASK, Class::Normal as u8);
        }
        assert_eq!(decode_all(&parts), [msg]);
    }

    #[test]
    fn bytes_that_come_one_at_a_time_decode() {
        // In the order of their classes, which is the order on the wire.
        let messages = [
            ping(1),
            call(3, "x", vec![1; 10]),
            frame(2, noise(40_000, 2)),
        ];
        let wire: Vec<u8> = wire_of(&messages).concat();
        let mut decoder = Decoder::default();
        let mut out = Vec::new();
        for byte in wire {
            out.extend(decoder.feed(&[byte]).expect("decodes"));
        }
        assert_eq!(out, messages);
    }

    #[test]
    fn a_part_over_the_limit_ends_the_link() {
        let mut decoder = Decoder::default();
        let len = u32::try_from(MAX_FRAME + 1).expect("fits");
        assert_eq!(
            decoder.feed(&len.to_le_bytes()),
            Err(CodecError::FrameTooLarge(MAX_FRAME + 1))
        );
    }

    #[test]
    fn unknown_flags_and_mixed_compression_end_the_link() {
        let framed = |flags: u8, bytes: &[u8]| {
            let body = postcard::to_allocvec(&Part { flags, bytes }).expect("encodes");
            let mut wire = u32::try_from(body.len())
                .expect("fits")
                .to_le_bytes()
                .to_vec();
            wire.extend(body);
            wire
        };
        assert_eq!(
            Decoder::default().feed(&framed(0x10, b"x")),
            Err(CodecError::BadFlags(0x10))
        );
        let mut decoder = Decoder::default();
        assert_eq!(decoder.feed(&framed(1 | MORE, b"x")), Ok(Vec::new()));
        assert_eq!(
            decoder.feed(&framed(1 | ZSTD, b"y")),
            Err(CodecError::MixedCompression)
        );
        assert_eq!(
            Decoder::default().feed(&framed(1 | ZSTD, b"not zstd")),
            Err(CodecError::BadCompression)
        );
    }

    #[test]
    fn a_message_of_an_unknown_kind_is_skipped_and_the_link_stays() {
        let framed = |flags: u8, bytes: &[u8]| {
            let body = postcard::to_allocvec(&Part { flags, bytes }).expect("encodes");
            let mut wire = u32::try_from(body.len())
                .expect("fits")
                .to_le_bytes()
                .to_vec();
            wire.extend(body);
            wire
        };
        let mut decoder = Decoder::default();
        let mut wire = framed(1, &[200, 1, 2, 3]);
        wire.extend(wire_of(&[ping(4)]).concat());
        assert_eq!(decoder.feed(&wire), Ok(vec![ping(4)]));
        assert_eq!(decoder.skipped, 1);
    }

    #[test]
    fn a_compressed_body_over_the_limit_is_refused() {
        // 4 MiB of zeros is a few hundred bytes after zstd.
        let small = zstd::bulk::compress(&vec![0u8; 4 << 20], 1).expect("compresses");
        assert!(small.len() < 4096);
        assert_eq!(
            decompress(&small, 1 << 20),
            Err(CodecError::MessageTooLarge)
        );
        assert_eq!(decompress(&small, 4 << 20).map(|v| v.len()), Ok(4 << 20));
    }

    #[test]
    fn a_decoder_with_a_small_limit_refuses_a_larger_message_in_both_forms() {
        // Noise does not compress: the parts alone go over the limit.
        let plain = wire_of(&[frame(1, noise(300_000, 9))]);
        let mut decoder = Decoder::with_limit(256 * 1024);
        let refused = plain.iter().try_for_each(|p| decoder.feed(p).map(drop));
        assert_eq!(refused, Err(CodecError::MessageTooLarge));
        // Zeros compress to a few hundred bytes, and the output is over the limit.
        let zeros = wire_of(&[frame(1, vec![0; 1 << 20])]);
        assert_eq!(zeros.len(), 1, "one small compressed part");
        assert_eq!(
            Decoder::with_limit(256 * 1024).feed(&zeros[0]),
            Err(CodecError::MessageTooLarge)
        );
        // The limit of a core takes both.
        assert_eq!(decode_all(&plain).len(), 1);
        assert_eq!(decode_all(&zeros).len(), 1);
        // A sender with the limit of the receiver does not send the message.
        assert_eq!(
            encode_within(&frame(1, vec![0; 1 << 20]), 256 * 1024).map(|_| ()),
            Err(CodecError::MessageTooLarge)
        );
    }

    /// A message of a class, as the property test makes them.
    fn message(class: u8, id: u64, len: usize) -> LinkMsg {
        match class {
            0 => call(id, "pane.input", noise(len, id)),
            1 => LinkMsg::Projection {
                sub: id,
                message: b"{\"op\":\"replace\"}".repeat(len / 16 + 1),
            },
            2 => frame(id, noise(len, id)),
            _ => LinkMsg::BlobPart {
                id,
                offset: 0,
                bytes: noise(len, id),
                last: true,
            },
        }
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(64))]

        /// Any mix of messages comes back whole, each class in its order, and a class-0 message
        /// that waits goes out before any part of a higher class.
        #[test]
        fn order_within_a_class_and_priority_hold(
            spec in prop::collection::vec((0u8..4, 0usize..60_000), 1..24),
            chunk in 1usize..40_000,
        ) {
            let messages: Vec<LinkMsg> = spec
                .iter()
                .enumerate()
                .map(|(i, (class, len))| message(*class, i as u64, *len))
                .collect();
            let wire = wire_of(&messages).concat();
            let mut decoder = Decoder::default();
            let mut out = Vec::new();
            for piece in wire.chunks(chunk) {
                out.extend(decoder.feed(piece).expect("decodes"));
            }
            prop_assert_eq!(out.len(), messages.len());
            for class in Class::ALL {
                let sent: Vec<&LinkMsg> = messages.iter().filter(|m| m.class() == class).collect();
                let came: Vec<&LinkMsg> = out.iter().filter(|m| m.class() == class).collect();
                prop_assert_eq!(sent, came);
            }
            // All were queued before the first part, so the classes come in their order.
            let classes: Vec<Class> = out.iter().map(LinkMsg::class).collect();
            let mut sorted = classes.clone();
            sorted.sort();
            prop_assert_eq!(classes, sorted);
        }
    }
}
