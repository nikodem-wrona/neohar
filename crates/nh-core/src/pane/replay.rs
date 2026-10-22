//! The replay of the journal after a restart ("Restart flow").
//!
//! The engine reads the bytes that the holder kept after the snapshot. While it reads old output,
//! its side effects are off: no notification, bell, clipboard write or terminal reply happens.
//! One kind of reply is wanted: the holder could not answer some queries while no core was
//! attached (cursor position, colours, window size), so it queued them. When the replay passes
//! such a query, the engine reads its bytes with the replies on and the core sends the answer, if
//! the query is still fresh.

use nh_proto::holder::QueuedQuery;
use nh_term::{EffectSink, Replaying, VtEngine};

/// A query that the holder queued, as a range of the journal.
struct ReplayQuery {
    seq: u64,
    /// The offset of the first byte of the query.
    start: u64,
    /// The offset after the last byte.
    end: u64,
    received_at_ms: i64,
    /// What the engine wrote when it read the query.
    reply: Vec<u8>,
}

/// The state of a replay. The task drops it when the replay ends.
pub(super) struct Replay {
    /// The journal offset at which the replay ends.
    pub end: u64,
    queries: Vec<ReplayQuery>,
    /// How long a query stays fresh, in milliseconds.
    fresh_ms: i64,
    /// How many bytes the replay fed.
    pub bytes: u64,
    /// The output is new, and its effects happen. A new pane reads what its child printed before
    /// the core attached: nobody has seen it, so a notification or a clipboard write in it is
    /// real. The replies do not happen, because the holder answered some queries and queued the
    /// others. The output of a restart is old, and its effects are off.
    effects_on: bool,
}

/// A sink that keeps the replies of the engine and drops every other effect.
struct Capture<'a>(&'a mut Vec<u8>);

impl EffectSink for Capture<'_> {
    fn pty_write(&mut self, bytes: &[u8]) {
        self.0.extend_from_slice(bytes);
    }
    fn bell(&mut self) {}
    fn title_changed(&mut self) {}
    fn clipboard_store(&mut self, _: u8, _: &str) {}
    fn clipboard_load_requested(&mut self) {}
    fn notify(&mut self, _: &str, _: &str) {}
}

/// A sink that passes every effect to another sink, except the replies to queries.
struct NoReplies<'a>(&'a mut dyn EffectSink);

impl EffectSink for NoReplies<'_> {
    fn pty_write(&mut self, _: &[u8]) {}
    fn bell(&mut self) {
        self.0.bell();
    }
    fn title_changed(&mut self) {
        self.0.title_changed();
    }
    fn clipboard_store(&mut self, clipboard: u8, text: &str) {
        self.0.clipboard_store(clipboard, text);
    }
    fn clipboard_load_requested(&mut self) {
        self.0.clipboard_load_requested();
    }
    fn notify(&mut self, title: &str, body: &str) {
        self.0.notify(title, body);
    }
}

impl Replay {
    /// A replay from `from` to `end`. `queued` are the queries that the holder reported.
    pub fn new(from: u64, end: u64, queued: &[QueuedQuery], fresh_ms: i64) -> Self {
        let mut queries: Vec<ReplayQuery> = queued
            .iter()
            .map(|q| ReplayQuery {
                seq: q.seq,
                start: q.offset.saturating_sub(q.raw.len() as u64),
                end: q.offset,
                received_at_ms: q.received_at_ms,
                reply: Vec::new(),
            })
            // A query before the snapshot was seen by the core that made it.
            .filter(|q| q.end > from)
            .collect();
        queries.sort_by_key(|q| q.start);
        Self {
            end,
            queries,
            fresh_ms,
            bytes: 0,
            effects_on: false,
        }
    }

    /// The replay of what the child of a new pane printed before the core attached. See
    /// [`Replay::effects_on`] for the difference from the replay of a restart.
    pub fn new_pane(end: u64, queued: &[QueuedQuery], fresh_ms: i64) -> Self {
        Self {
            effects_on: true,
            ..Self::new(0, end, queued, fresh_ms)
        }
    }

    /// Feeds the output at `offset` to the engine. The side effects are off, except for the
    /// replies to the queued queries. A replay of new output sends its effects to `fx`, except
    /// the replies.
    pub fn feed(
        &mut self,
        engine: &mut impl VtEngine,
        offset: u64,
        bytes: &[u8],
        fx: &mut dyn EffectSink,
    ) {
        self.bytes += bytes.len() as u64;
        let chunk_end = offset + bytes.len() as u64;
        let mut at = offset;
        while at < chunk_end {
            // The next place where the sink changes: the start or the end of a query.
            let query = self.queries.iter_mut().find(|q| q.end > at);
            let (until, capture) = match query {
                Some(q) if q.start <= at => (chunk_end.min(q.end), Some(&mut q.reply)),
                Some(q) => (chunk_end.min(q.start), None),
                None => (chunk_end, None),
            };
            let slice = &bytes[(at - offset) as usize..(until - offset) as usize];
            match capture {
                Some(reply) => engine.advance(slice, &mut Capture(reply)),
                None if self.effects_on => engine.advance(slice, &mut NoReplies(fx)),
                None => engine.advance(slice, &mut Replaying::default()),
            }
            at = until;
        }
    }

    /// The highest sequence number of the queued queries, to put in `QueriesAnswered`. The holder
    /// forgets every query up to it, also the ones that were too old to answer.
    pub fn up_to_seq(&self) -> Option<u64> {
        self.queries.iter().map(|q| q.seq).max()
    }

    /// The replies to the queries that are still fresh at `now_ms` (unix milliseconds), in the
    /// order of the queries.
    pub fn replies(&self, now_ms: i64) -> Vec<Vec<u8>> {
        self.queries
            .iter()
            .filter(|q| !q.reply.is_empty() && now_ms - q.received_at_ms <= self.fresh_ms)
            .map(|q| q.reply.clone())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use nh_proto::answers::QueryKind;
    use nh_term::{AlacrittyEngine, MIN_HISTORY, xterm_palette};

    use super::*;

    fn engine() -> AlacrittyEngine {
        AlacrittyEngine::new(20, 5, MIN_HISTORY, xterm_palette())
    }

    fn queued(seq: u64, end: u64, raw: &[u8], received_at_ms: i64) -> QueuedQuery {
        QueuedQuery {
            seq,
            offset: end,
            received_at_ms,
            kind: QueryKind::CursorPosition,
            raw: raw.to_vec(),
        }
    }

    const DSR: &[u8] = b"\x1b[6n";

    #[test]
    fn the_cursor_report_is_computed_where_the_query_stood() {
        // "abc" is 3 bytes and the query ends at 7. Then "xyz" moves the cursor on, and the answer
        // is still the position after "abc".
        let mut replay = Replay::new(0, 10, &[queued(1, 7, DSR, 1000)], 5000);
        let mut e = engine();
        replay.feed(&mut e, 0, b"abc\x1b[6nxyz", &mut Replaying::default());
        assert_eq!(replay.replies(1000), [b"\x1b[1;4R".to_vec()]);
        assert_eq!(replay.up_to_seq(), Some(1));
    }

    #[test]
    fn a_reply_to_a_query_that_the_holder_answered_is_dropped() {
        // DA1 is not queued, because the holder answered it. Reading it again must stay silent.
        let mut replay = Replay::new(0, 9, &[queued(1, 9, DSR, 0)], 5000);
        let mut e = engine();
        replay.feed(&mut e, 0, b"\x1b[c\x1b[6n", &mut Replaying::default());
        assert_eq!(replay.replies(0), [b"\x1b[1;1R".to_vec()]);
    }

    #[test]
    fn a_query_that_is_split_between_two_chunks_is_answered_once() {
        let mut replay = Replay::new(0, 6, &[queued(1, 6, DSR, 0)], 5000);
        let mut e = engine();
        replay.feed(&mut e, 0, b"ab\x1b[", &mut Replaying::default());
        replay.feed(&mut e, 4, b"6n", &mut Replaying::default());
        assert_eq!(replay.replies(0), [b"\x1b[1;3R".to_vec()]);
    }

    #[test]
    fn two_queries_are_answered_in_order() {
        let queries = [queued(4, 4, DSR, 0), queued(5, 12, DSR, 0)];
        let mut replay = Replay::new(0, 12, &queries, 5000);
        let mut e = engine();
        replay.feed(
            &mut e,
            0,
            b"\x1b[6n\r\nab\x1b[6n",
            &mut Replaying::default(),
        );
        assert_eq!(
            replay.replies(0),
            [b"\x1b[1;1R".to_vec(), b"\x1b[2;3R".to_vec()]
        );
        assert_eq!(replay.up_to_seq(), Some(5));
    }

    #[test]
    fn a_stale_query_gets_no_reply_but_is_still_acknowledged() {
        let mut replay = Replay::new(0, 4, &[queued(3, 4, DSR, 1000)], 5000);
        let mut e = engine();
        replay.feed(&mut e, 0, DSR, &mut Replaying::default());
        assert!(replay.replies(1000 + 5001).is_empty());
        assert_eq!(
            replay.replies(1000 + 5000).len(),
            1,
            "the limit is inclusive"
        );
        assert_eq!(replay.up_to_seq(), Some(3));
    }

    #[test]
    fn a_query_before_the_start_of_the_replay_is_ignored() {
        let replay = Replay::new(100, 120, &[queued(1, 50, DSR, 0)], 5000);
        assert_eq!(replay.up_to_seq(), None);
    }

    #[test]
    fn plain_output_gets_no_reply_and_counts_its_bytes() {
        let mut replay = Replay::new(0, 5, &[], 5000);
        let mut e = engine();
        replay.feed(&mut e, 0, b"hello", &mut Replaying::default());
        assert!(replay.replies(0).is_empty());
        assert_eq!(replay.bytes, 5);
        assert_eq!(replay.up_to_seq(), None);
    }

    /// A sink that counts what the engine asks of the world.
    #[derive(Default)]
    struct Counting {
        bells: usize,
        notes: Vec<(String, String)>,
        replies: usize,
    }

    impl EffectSink for Counting {
        fn pty_write(&mut self, _: &[u8]) {
            self.replies += 1;
        }
        fn bell(&mut self) {
            self.bells += 1;
        }
        fn title_changed(&mut self) {}
        fn clipboard_store(&mut self, _: u8, _: &str) {}
        fn clipboard_load_requested(&mut self) {}
        fn notify(&mut self, title: &str, body: &str) {
            self.notes.push((title.to_owned(), body.to_owned()));
        }
    }

    #[test]
    fn the_output_of_a_new_pane_has_its_effects_but_no_replies() {
        let output = b"\x07\x1b]9;built it\x07\x1b[c";
        let mut fresh = Replay::new_pane(output.len() as u64, &[], 5000);
        let mut seen = Counting::default();
        fresh.feed(&mut engine(), 0, output, &mut seen);
        assert_eq!(seen.bells, 1);
        assert_eq!(seen.notes, [(String::new(), "built it".to_owned())]);
        assert_eq!(seen.replies, 0, "the holder answered the query");

        // The replay of a restart reads old output. Nothing happens.
        let mut old = Replay::new(0, output.len() as u64, &[], 5000);
        let mut seen = Counting::default();
        old.feed(&mut engine(), 0, output, &mut seen);
        assert_eq!((seen.bells, seen.notes.len(), seen.replies), (0, 0, 0));
    }
}
