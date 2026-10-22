//! Safe cut points: offsets in the PTY byte stream that are not inside a UTF-8
//! sequence or an escape sequence.
//!
//! The scanner mirrors the state table of the `vte` crate that `alacritty_terminal` uses, so
//! that a cut point is always a place where the VT engine's parser is in its ground state. It has
//! no screen model. A test checks it against `vte` itself.
//!
//! The scanner also collects the body of each CSI and OSC sequence, and passes it to
//! [`crate::sequences`], so that the holder can answer queries while no core is attached.

use crate::sequences::{Sequence, parse_csi, parse_osc};

/// The most bytes of a sequence body that the scanner keeps. A longer sequence is ignored.
const MAX_BODY: usize = 64;

/// A sequence that ended in a chunk, and the number of bytes of the chunk up to its end.
#[derive(Debug, PartialEq, Eq)]
pub struct Found {
    pub end: usize,
    pub seq: Sequence,
}

/// The parser states of `vte`, merged where they leave the same way.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum State {
    #[default]
    Ground,
    Escape,
    EscapeIntermediate,
    /// `CsiEntry`, `CsiParam`, `CsiIntermediate` and `CsiIgnore`: all end on a final byte.
    Csi,
    DcsEntry,
    DcsParam,
    DcsIntermediate,
    DcsIgnore,
    DcsPassthrough,
    Osc,
    /// SOS, PM and APC strings.
    SosPmApc,
}

/// Tracks the state of the byte stream between reads.
#[derive(Clone, Debug, Default)]
pub struct CutScanner {
    state: State,
    /// Continuation bytes still missing from a multi-byte UTF-8 character.
    utf8_left: u8,
    /// The body of the CSI or OSC sequence that is open.
    body: Vec<u8>,
    body_overflow: bool,
}

impl CutScanner {
    /// A scanner at the start of a stream.
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether the next byte starts a new character or sequence.
    pub fn at_cut_point(&self) -> bool {
        self.state == State::Ground && self.utf8_left == 0
    }

    #[cfg(test)]
    /// Feeds `bytes`. Returns `k` when the stream was at a cut point after the first `k` bytes of
    /// `bytes` and after no later byte (`1 <= k <= bytes.len()`), or `None` when there was none.
    pub fn feed(&mut self, bytes: &[u8]) -> Option<usize> {
        self.scan(bytes, &mut Vec::new())
    }

    /// Like [`CutScanner::feed`]. It also adds each sequence that ends in `bytes` to `found`.
    pub fn scan(&mut self, bytes: &[u8], found: &mut Vec<Found>) -> Option<usize> {
        let mut last = None;
        for (i, &byte) in bytes.iter().enumerate() {
            found.extend(
                self.step(byte)
                    .into_iter()
                    .map(|seq| Found { end: i + 1, seq }),
            );
            if self.at_cut_point() {
                last = Some(i + 1);
            }
        }
        last
    }

    fn step(&mut self, b: u8) -> Vec<Sequence> {
        use State::*;
        if self.utf8_left > 0 {
            if (0x80..=0xBF).contains(&b) {
                self.utf8_left -= 1;
                return Vec::new();
            }
            // An invalid sequence ends here. `vte` prints a replacement character and
            // handles this byte as if it had no sequence before it.
            self.utf8_left = 0;
        }
        // The bytes that every non-ground state treats the same way (`vte` calls it "anywhere").
        let anywhere = |state| match b {
            0x18 | 0x1A => Ground,
            0x1B => Escape,
            _ => state,
        };
        let before = self.state;
        self.state = match self.state {
            Ground => match b {
                0x1B => Escape,
                0xC2..=0xDF => self.pending(1),
                0xE0..=0xEF => self.pending(2),
                0xF0..=0xF4 => self.pending(3),
                _ => Ground,
            },
            Escape => match b {
                0x20..=0x2F => EscapeIntermediate,
                0x30..=0x4F | 0x51..=0x57 | 0x59 | 0x5A | 0x5C | 0x60..=0x7E => Ground,
                0x50 => DcsEntry,
                0x58 | 0x5E | 0x5F => SosPmApc,
                0x5B => Csi,
                0x5D => Osc,
                _ => anywhere(Escape),
            },
            EscapeIntermediate => match b {
                0x30..=0x7E => Ground,
                _ => anywhere(EscapeIntermediate),
            },
            Csi => match b {
                0x40..=0x7E => Ground,
                _ => anywhere(Csi),
            },
            DcsEntry => match b {
                0x20..=0x2F => DcsIntermediate,
                0x30..=0x3F => DcsParam,
                0x40..=0x7E => DcsPassthrough,
                _ => anywhere(DcsEntry),
            },
            DcsParam => match b {
                0x20..=0x2F => DcsIntermediate,
                0x3C..=0x3F => DcsIgnore,
                0x40..=0x7E => DcsPassthrough,
                _ => anywhere(DcsParam),
            },
            DcsIntermediate => match b {
                0x30..=0x3F => DcsIgnore,
                0x40..=0x7E => DcsPassthrough,
                _ => anywhere(DcsIntermediate),
            },
            DcsIgnore => anywhere(DcsIgnore),
            DcsPassthrough => match b {
                // `vte` ends a DCS string at a raw 0x9C byte, also inside UTF-8 text.
                0x9C => Ground,
                _ => anywhere(DcsPassthrough),
            },
            Osc => match b {
                0x07 => Ground,
                _ => anywhere(Osc),
            },
            SosPmApc => anywhere(SosPmApc),
        };
        self.collect(before, b)
    }

    /// Keeps the body of an open CSI or OSC sequence. Returns the sequences that `b` completes.
    fn collect(&mut self, before: State, b: u8) -> Vec<Sequence> {
        use State::*;
        match (before, self.state) {
            (Escape, Csi | Osc) => {
                self.body.clear();
                self.body_overflow = false;
            }
            (Csi, Csi) if (0x20..=0x3F).contains(&b) => self.keep(b),
            (Osc, Osc) if b >= 0x20 => self.keep(b),
            (Csi, Ground) if (0x40..=0x7E).contains(&b) && !self.body_overflow => {
                return parse_csi(&self.body, b);
            }
            // BEL ends an OSC string. So does ESC, which starts the two-byte `ESC \`.
            (Osc, Ground | Escape) if (b == 0x07 || b == 0x1B) && !self.body_overflow => {
                return parse_osc(&self.body, b == 0x07).into_iter().collect();
            }
            _ => {}
        }
        Vec::new()
    }

    /// Adds a byte to the body of the open sequence. A body that is too long is dropped.
    fn keep(&mut self, b: u8) {
        if self.body.len() < MAX_BODY {
            self.body.push(b);
        } else {
            self.body_overflow = true;
        }
    }

    /// Starts a multi-byte character. The state stays `Ground`.
    fn pending(&mut self, continuation_bytes: u8) -> State {
        self.utf8_left = continuation_bytes;
        State::Ground
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;
    use vte::{Params, Parser, Perform};

    use super::*;

    /// Records every `vte` action as text.
    #[derive(Default)]
    struct Actions(Vec<String>);

    impl Perform for Actions {
        fn print(&mut self, c: char) {
            self.0.push(format!("print {c:?}"));
        }
        fn execute(&mut self, byte: u8) {
            self.0.push(format!("execute {byte:#x}"));
        }
        fn hook(&mut self, params: &Params, i: &[u8], ignore: bool, c: char) {
            let p: Vec<_> = params.iter().map(|p| p.to_vec()).collect();
            self.0.push(format!("hook {p:?} {i:?} {ignore} {c:?}"));
        }
        fn put(&mut self, byte: u8) {
            self.0.push(format!("put {byte:#x}"));
        }
        fn unhook(&mut self) {
            self.0.push("unhook".into());
        }
        fn osc_dispatch(&mut self, params: &[&[u8]], bell: bool) {
            self.0.push(format!("osc {params:?} {bell}"));
        }
        fn csi_dispatch(&mut self, params: &Params, i: &[u8], ignore: bool, c: char) {
            let p: Vec<_> = params.iter().map(|p| p.to_vec()).collect();
            self.0.push(format!("csi {p:?} {i:?} {ignore} {c:?}"));
        }
        fn esc_dispatch(&mut self, i: &[u8], ignore: bool, byte: u8) {
            self.0.push(format!("esc {i:?} {ignore} {byte:#x}"));
        }
    }

    fn actions(stream: &[u8]) -> Vec<String> {
        let mut performer = Actions::default();
        Parser::new().advance(&mut performer, stream);
        performer.0
    }

    /// A stream built from the pieces that real programs send, and some that they should not.
    fn stream() -> impl Strategy<Value = Vec<u8>> {
        let piece = prop_oneof![
            3 => "[ -~]{0,6}".prop_map(String::into_bytes),
            2 => "\\PC{0,3}".prop_map(String::into_bytes),
            2 => Just(b"\x1b[1;31m".to_vec()),
            1 => Just(b"\x1b[?2004h".to_vec()),
            2 => Just(b"\x1b]0;title\x07".to_vec()),
            2 => Just(b"\x1b]8;;http://x\x1b\\".to_vec()),
            1 => Just(b"\x1bP1$r0m\x1b\\".to_vec()),
            1 => Just(b"\x1bPq#0;2;0;0;0\x1b\\".to_vec()),
            1 => Just(b"\x1b_Gf=24;AAAA\x1b\\".to_vec()),
            1 => Just(b"\x1bX data\x1b\\".to_vec()),
            1 => Just(b"\x1b(B".to_vec()),
            1 => Just(vec![0x18]),
            1 => Just(vec![0x1a]),
            1 => Just(vec![0x1b]),
            1 => Just(vec![0x07]),
            1 => Just(vec![0x9c]),
            3 => proptest::collection::vec(any::<u8>(), 0..4),
        ];
        proptest::collection::vec(piece, 0..12).prop_map(|v| v.concat())
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(512))]

        /// A cut point is a place where a fresh parser gives the same result as the old one.
        #[test]
        fn every_cut_point_matches_vte(stream in stream()) {
            let whole = actions(&stream);
            let mut scanner = CutScanner::new();
            for (i, &byte) in stream.iter().enumerate() {
                if scanner.feed(&[byte]).is_some() {
                    let mut split = actions(&stream[..=i]);
                    split.extend(actions(&stream[i + 1..]));
                    prop_assert_eq!(&split, &whole, "cut after byte {} of {:02x?}", i, stream);
                }
            }
        }

        /// How a stream is cut into reads does not change where the cut points are.
        #[test]
        fn read_boundaries_do_not_move_cut_points(stream in stream(), cut in any::<prop::sample::Index>()) {
            let mut one = CutScanner::new();
            let at = one.feed(&stream);
            let split = if stream.is_empty() { 0 } else { cut.index(stream.len()) };
            let mut two = CutScanner::new();
            let a = two.feed(&stream[..split]);
            let b = two.feed(&stream[split..]);
            let expected = b.map(|k| split + k).or(a);
            prop_assert_eq!(expected, at);
        }
    }

    #[test]
    fn plain_text_is_a_cut_point_after_every_byte() {
        let mut s = CutScanner::new();
        assert_eq!(s.feed(b"hello"), Some(5));
    }

    #[test]
    fn a_cut_is_not_inside_an_escape_sequence_or_a_character() {
        let mut s = CutScanner::new();
        assert_eq!(s.feed(b"ab\x1b[31"), Some(2));
        assert_eq!(s.feed(b"m!"), Some(2));
        assert_eq!(s.feed("é".as_bytes().get(..1).expect("one byte")), None);
        assert_eq!(s.feed(&"é".as_bytes()[1..]), Some(1));
    }

    #[test]
    fn an_osc_ends_at_bel_or_st() {
        let mut s = CutScanner::new();
        assert_eq!(s.feed(b"\x1b]0;title"), None);
        assert_eq!(s.feed(b"\x07"), Some(1));
        assert_eq!(s.feed(b"\x1b]0;title\x1b"), None);
        assert_eq!(s.feed(b"\\"), Some(1));
    }

    #[test]
    fn can_and_sub_abort_a_sequence() {
        for abort in [0x18u8, 0x1a] {
            let mut s = CutScanner::new();
            assert_eq!(s.feed(b"\x1b[3"), None);
            assert_eq!(s.feed(&[abort]), Some(1));
        }
    }

    fn found(chunks: &[&[u8]]) -> Vec<(usize, usize, Sequence)> {
        let mut scanner = CutScanner::new();
        let mut out = Vec::new();
        for (n, chunk) in chunks.iter().enumerate() {
            let mut list = Vec::new();
            scanner.scan(chunk, &mut list);
            out.extend(list.into_iter().map(|f| (n, f.end, f.seq)));
        }
        out
    }

    #[test]
    fn sequences_are_reported_with_the_end_of_their_final_byte() {
        let out = found(&[b"ab\x1b[?2004h\x1b[c"]);
        assert_eq!(
            out,
            vec![
                (
                    0,
                    10,
                    Sequence::Mode {
                        number: 2004,
                        set: true
                    }
                ),
                (
                    0,
                    13,
                    Sequence::Query {
                        kind: nh_proto::answers::QueryKind::Da1,
                        raw: b"\x1b[c".to_vec()
                    }
                ),
            ]
        );
    }

    #[test]
    fn a_sequence_split_between_reads_is_found_in_the_read_that_ends_it() {
        let out = found(&[b"\x1b[?20", b"04", b"h"]);
        assert_eq!(
            out,
            vec![(
                2,
                1,
                Sequence::Mode {
                    number: 2004,
                    set: true
                }
            )]
        );
    }

    #[test]
    fn an_osc_query_ends_at_bel_or_at_the_esc_of_st() {
        let bell = found(&[b"\x1b]11;?\x07"]);
        assert!(
            matches!(&bell[..], [(0, 7, Sequence::Query { raw, .. })] if raw == b"\x1b]11;?\x07")
        );
        let st = found(&[b"\x1b]11;?\x1b\\"]);
        assert!(
            matches!(&st[..], [(0, 7, Sequence::Query { raw, .. })] if raw == b"\x1b]11;?\x1b\\")
        );
    }

    #[test]
    fn an_aborted_or_overlong_sequence_gives_nothing() {
        assert!(found(&[b"\x1b[6\x18n"]).is_empty());
        let mut long = b"\x1b[?".to_vec();
        long.extend(std::iter::repeat_n(b'1', 100));
        long.push(b'h');
        assert!(found(&[&long]).is_empty());
        // The next sequence is not affected.
        assert_eq!(found(&[&long, b"\x1b[?2004l"]).len(), 1);
    }
}
