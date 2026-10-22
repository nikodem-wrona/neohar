//! Child output on its way into the journal.
//!
//! `Feed` owns the journal. For a PTY it records a cut point after each read, where the stream
//! is between sequences, and it follows the few sequences that the holder acts on. For pipes it
//! records which stream each byte came from, and a cut point where every stream is at a line end.

use nh_proto::{
    answers::QueryKind,
    holder::{Marker, StreamId, TrackedModes},
};

use crate::{
    cutpoint::{CutScanner, Found},
    journal::Journal,
    sequences::Sequence,
};

/// Kitty keyboard flags nest at most this deep.
const MAX_KITTY_STACK: usize = 64;

/// A query found in the output, with the journal offset after it.
pub struct Query {
    pub offset: u64,
    pub kind: QueryKind,
    pub raw: Vec<u8>,
}

/// The journal and what is learned from the bytes that go into it.
pub struct Feed {
    pub journal: Journal,
    /// The modes that the holder follows to answer queries.
    pub tracked: TrackedModes,
    pipe: bool,
    scanner: CutScanner,
    found: Vec<Found>,
    /// The stream of the last byte, in pipe mode.
    stream: Option<StreamId>,
    /// For stdin, stdout and stderr: the last byte was not a line end.
    partial: [bool; 3],
}

impl Feed {
    /// A feed for a PTY, or for pipes when `pipe` is true.
    pub fn new(journal: Journal, pipe: bool) -> Self {
        Self {
            journal,
            tracked: TrackedModes::default(),
            pipe,
            scanner: CutScanner::new(),
            found: Vec::new(),
            stream: None,
            partial: [false; 3],
        }
    }

    /// Adds bytes of `stream` to the journal. Returns the queries in them.
    pub fn push(&mut self, stream: StreamId, bytes: &[u8]) -> Vec<Query> {
        if bytes.is_empty() {
            return Vec::new();
        }
        if self.pipe {
            self.push_pipe(stream, bytes);
            return Vec::new();
        }
        let base = self.journal.end();
        self.journal.append(bytes);
        let cut = self.scanner.scan(bytes, &mut self.found);
        let mut queries = Vec::new();
        for Found { end, seq } in std::mem::take(&mut self.found) {
            match seq {
                Sequence::Query { kind, raw } => queries.push(Query {
                    offset: base + end as u64,
                    kind,
                    raw,
                }),
                other => self.track(other),
            }
        }
        if let Some(k) = cut {
            self.journal.mark_at(base + k as u64, Marker::CutPoint);
        }
        queries
    }

    fn push_pipe(&mut self, stream: StreamId, bytes: &[u8]) {
        if self.stream != Some(stream) {
            self.journal.mark(Marker::Stream { id: stream });
            self.stream = Some(stream);
        }
        self.journal.append(bytes);
        let index = match stream {
            StreamId::Stdin | StreamId::Pty => 0,
            StreamId::Stdout => 1,
            StreamId::Stderr => 2,
        };
        self.partial[index] = bytes.last() != Some(&b'\n');
        if !self.partial.contains(&true) {
            self.journal.mark(Marker::CutPoint);
        }
    }

    fn track(&mut self, seq: Sequence) {
        let tracked = &mut self.tracked;
        match seq {
            Sequence::Mode { number: 2004, set } => tracked.bracketed_paste = set,
            Sequence::Mode { number: 1004, set } => tracked.focus_events = set,
            Sequence::KittyPush(flags) => {
                if tracked.kitty_flags_stack.len() == MAX_KITTY_STACK {
                    tracked.kitty_flags_stack.remove(0);
                }
                tracked.kitty_flags_stack.push(flags);
            }
            Sequence::KittyPop(n) => {
                let keep = tracked
                    .kitty_flags_stack
                    .len()
                    .saturating_sub(usize::from(n));
                tracked.kitty_flags_stack.truncate(keep);
            }
            Sequence::KittySet { flags, mode } => {
                if tracked.kitty_flags_stack.is_empty() {
                    tracked.kitty_flags_stack.push(0);
                }
                if let Some(top) = tracked.kitty_flags_stack.last_mut() {
                    match mode {
                        2 => *top |= flags,
                        3 => *top &= !flags,
                        _ => *top = flags,
                    }
                }
            }
            Sequence::Mode { .. } | Sequence::Query { .. } => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pty() -> Feed {
        Feed::new(Journal::new(1 << 16), false)
    }

    #[test]
    fn modes_are_tracked_across_reads() {
        let mut f = pty();
        f.push(StreamId::Pty, b"x\x1b[?20");
        f.push(StreamId::Pty, b"04h\x1b[?1004h");
        assert!(f.tracked.bracketed_paste && f.tracked.focus_events);
        f.push(StreamId::Pty, b"\x1b[?2004l");
        assert!(!f.tracked.bracketed_paste && f.tracked.focus_events);
    }

    #[test]
    fn the_kitty_stack_pushes_pops_and_sets_the_top() {
        let mut f = pty();
        f.push(StreamId::Pty, b"\x1b[>1u\x1b[>5u");
        assert_eq!(f.tracked.kitty_flags_stack, [1, 5]);
        f.push(StreamId::Pty, b"\x1b[=2;2u");
        assert_eq!(f.tracked.kitty_flags_stack, [1, 7]);
        f.push(StreamId::Pty, b"\x1b[=1;3u");
        assert_eq!(f.tracked.kitty_flags_stack, [1, 6]);
        f.push(StreamId::Pty, b"\x1b[=9u");
        assert_eq!(f.tracked.kitty_flags_stack, [1, 9]);
        f.push(StreamId::Pty, b"\x1b[<u");
        assert_eq!(f.tracked.kitty_flags_stack, [1]);
        f.push(StreamId::Pty, b"\x1b[<5u");
        assert!(f.tracked.kitty_flags_stack.is_empty());
        f.push(StreamId::Pty, b"\x1b[=4u");
        assert_eq!(f.tracked.kitty_flags_stack, [4]);
    }

    #[test]
    fn a_query_gets_the_offset_after_it() {
        let mut f = pty();
        let queries = f.push(StreamId::Pty, b"abc\x1b[6n");
        assert_eq!(queries.len(), 1);
        assert_eq!(
            (queries[0].offset, queries[0].kind),
            (7, QueryKind::CursorPosition)
        );
        assert_eq!(queries[0].raw, b"\x1b[6n");
    }

    #[test]
    fn pty_output_gets_a_cut_point_at_the_last_safe_offset_of_each_read() {
        let mut f = pty();
        f.push(StreamId::Pty, b"ab\x1b[31");
        assert_eq!(f.journal.info().last_cut_point, 2);
        f.push(StreamId::Pty, b"mcd");
        assert_eq!(f.journal.info().last_cut_point, 9);
    }

    #[test]
    fn pipe_output_marks_stream_switches_and_cuts_only_at_line_ends() {
        let mut f = Feed::new(Journal::new(1 << 16), true);
        f.push(StreamId::Stdout, b"out1\n");
        f.push(StreamId::Stderr, b"er");
        assert_eq!(f.journal.info().last_cut_point, 5);
        f.push(StreamId::Stdout, b"out2\n");
        // stderr still has a partial line, so no cut point after `out2`.
        assert_eq!(f.journal.info().last_cut_point, 5);
        f.push(StreamId::Stderr, b"r1\n");
        assert_eq!(f.journal.info().last_cut_point, 15);
        let streams: Vec<_> = (0..)
            .map_while(|seq| f.journal.marker(seq).filter(|(s, _)| *s == seq))
            .filter_map(|(_, e)| match e.marker {
                Marker::Stream { id } => Some((e.offset, id)),
                _ => None,
            })
            .collect();
        assert_eq!(
            streams,
            [
                (0, StreamId::Stdout),
                (5, StreamId::Stderr),
                (7, StreamId::Stdout),
                (12, StreamId::Stderr)
            ]
        );
    }
}
