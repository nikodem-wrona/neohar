//! The output journal: a byte ring with absolute offsets, and a ring of markers.
//!
//! A marker sits at an offset between two bytes. Replay sends the markers of an offset before the
//! byte at that offset, so output, resizes and inputs keep their order.

use std::collections::VecDeque;

use nh_proto::holder::{JournalInfo, Marker, StreamId};

/// The most markers the journal keeps. The oldest marker goes first.
const MAX_MARKERS: usize = 1 << 16;

/// A marker and its place in the stream.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Entry {
    pub offset: u64,
    pub marker: Marker,
}

/// A byte ring and its markers.
pub struct Journal {
    buf: Vec<u8>,
    capacity: u64,
    start: u64,
    end: u64,
    markers: VecDeque<Entry>,
    /// The sequence number of `markers[0]`. A client keeps the sequence number of the next marker.
    first_seq: u64,
    last_cut_point: u64,
    last_checkpoint: u64,
    /// The stream of the oldest byte, when the marker that set it has left the ring.
    base_stream: Option<StreamId>,
}

impl Journal {
    /// An empty journal that keeps the last `capacity` bytes.
    pub fn new(capacity: u64) -> Self {
        let capacity = capacity.max(1);
        let len = usize::try_from(capacity).expect("the ring size fits in memory");
        Self {
            buf: vec![0; len],
            capacity,
            start: 0,
            end: 0,
            markers: VecDeque::new(),
            first_seq: 0,
            last_cut_point: 0,
            last_checkpoint: 0,
            base_stream: None,
        }
    }

    /// The offset after the newest byte.
    pub fn end(&self) -> u64 {
        self.end
    }

    /// The oldest offset that is still in the ring.
    pub fn start(&self) -> u64 {
        self.start
    }

    /// The range, the capacity and the last checkpoint and cut point.
    pub fn info(&self) -> JournalInfo {
        JournalInfo {
            start: self.start,
            end: self.end,
            capacity: self.capacity,
            last_checkpoint: self.last_checkpoint,
            last_cut_point: self.last_cut_point,
        }
    }

    /// Records the offset of a checkpoint that the core acknowledged.
    pub fn set_checkpoint(&mut self, offset: u64) {
        self.last_checkpoint = offset.clamp(self.start, self.end).max(self.last_checkpoint);
    }

    /// Appends bytes. When the ring is full, the oldest bytes and the markers before them go.
    pub fn append(&mut self, bytes: &[u8]) {
        let cap = self.buf.len();
        // Only the last `cap` bytes can stay.
        let skipped = bytes.len().saturating_sub(cap);
        let bytes = &bytes[skipped..];
        self.end += skipped as u64;
        let at = (self.end % self.capacity) as usize;
        let first = bytes.len().min(cap - at);
        self.buf[at..at + first].copy_from_slice(&bytes[..first]);
        self.buf[..bytes.len() - first].copy_from_slice(&bytes[first..]);
        self.end += bytes.len() as u64;
        self.start = self.start.max(self.end.saturating_sub(self.capacity));
        while self.markers.front().is_some_and(|e| e.offset < self.start) {
            self.drop_front_marker();
        }
    }

    /// Adds a marker at `offset`, which must lie between the last marker and the end.
    pub fn mark_at(&mut self, offset: u64, marker: Marker) {
        debug_assert!(offset <= self.end && self.markers.back().is_none_or(|e| e.offset <= offset));
        if marker == Marker::CutPoint {
            self.last_cut_point = offset;
        }
        self.markers.push_back(Entry { offset, marker });
        if self.markers.len() > MAX_MARKERS {
            self.drop_front_marker();
        }
    }

    /// Adds a marker at the end of the journal and returns its offset.
    pub fn mark(&mut self, marker: Marker) -> u64 {
        self.mark_at(self.end, marker);
        self.end
    }

    fn drop_front_marker(&mut self) {
        if let Some(Entry {
            marker: Marker::Stream { id },
            ..
        }) = self.markers.pop_front()
        {
            self.base_stream = Some(id);
        }
        self.first_seq += 1;
    }

    /// The stream of the byte before `offset`, in pipe mode. The markers at `offset` do not count.
    pub fn stream_before(&self, offset: u64) -> Option<StreamId> {
        let switches = self.markers.iter().take_while(|e| e.offset < offset);
        switches
            .filter_map(|e| match e.marker {
                Marker::Stream { id } => Some(id),
                _ => None,
            })
            .last()
            .or(self.base_stream)
    }

    /// Up to `max` bytes from `from`, as two slices because the ring wraps. `from` must be in
    /// `start..=end`.
    pub fn read(&self, from: u64, max: usize) -> (&[u8], &[u8]) {
        debug_assert!(from >= self.start && from <= self.end);
        let len = usize::try_from(self.end - from).map_or(max, |n| n.min(max));
        let at = (from % self.capacity) as usize;
        let first = len.min(self.buf.len() - at);
        (&self.buf[at..at + first], &self.buf[..len - first])
    }

    /// The sequence number of the first marker at or after `offset`.
    pub fn marker_seq_from(&self, offset: u64) -> u64 {
        self.first_seq + self.markers.partition_point(|e| e.offset < offset) as u64
    }

    /// The marker with sequence number `seq`. A number that is too old gives the oldest marker.
    pub fn marker(&self, seq: u64) -> Option<(u64, Entry)> {
        let seq = seq.max(self.first_seq);
        let index = usize::try_from(seq - self.first_seq).ok()?;
        self.markers.get(index).map(|e| (seq, *e))
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    fn joined(j: &Journal, from: u64) -> Vec<u8> {
        let (a, b) = j.read(from, usize::MAX);
        [a, b].concat()
    }

    #[test]
    fn it_keeps_what_fits_and_reads_across_the_wrap() {
        let mut j = Journal::new(8);
        j.append(b"abcdef");
        assert_eq!((j.start(), j.end()), (0, 6));
        assert_eq!(joined(&j, 0), b"abcdef");
        j.append(b"ghij");
        assert_eq!((j.start(), j.end()), (2, 10));
        assert_eq!(joined(&j, 2), b"cdefghij");
        assert_eq!(joined(&j, 7), b"hij");
        assert_eq!(joined(&j, 10), b"");
    }

    #[test]
    fn an_append_larger_than_the_ring_keeps_the_tail() {
        let mut j = Journal::new(4);
        j.append(b"0123456789");
        assert_eq!((j.start(), j.end()), (6, 10));
        assert_eq!(joined(&j, 6), b"6789");
    }

    #[test]
    fn markers_before_the_start_are_dropped_and_sequence_numbers_stay_valid() {
        let mut j = Journal::new(4);
        j.append(b"ab");
        j.mark(Marker::Resize { cols: 1, rows: 1 });
        j.append(b"cd");
        j.mark(Marker::CutPoint);
        let seq = j.marker_seq_from(2);
        assert_eq!(seq, 0);
        j.append(b"efgh");
        // The ring holds bytes 4..8. The resize at 2 is gone. The cut point at 4 stays.
        let (kept, entry) = j.marker(seq).expect("the cut point remains");
        assert_eq!((kept, entry.offset, entry.marker), (1, 4, Marker::CutPoint));
        assert_eq!(j.marker_seq_from(4), 1);
        assert_eq!(j.info().last_cut_point, 4);
        j.append(b"ij");
        assert!(j.marker(seq).is_none());
        assert_eq!(j.marker_seq_from(6), 2);
    }

    #[test]
    fn a_marker_ring_over_its_cap_drops_the_oldest() {
        let mut j = Journal::new(16);
        for i in 0..(MAX_MARKERS + 5) {
            j.mark(Marker::InputAck { input_id: i as u64 });
        }
        let (seq, first) = j.marker(0).expect("a marker remains");
        assert_eq!(seq, 5);
        assert_eq!(first.marker, Marker::InputAck { input_id: 5 });
    }

    proptest! {
        #[test]
        fn the_ring_always_holds_the_tail_of_the_stream(
            cap in 1u64..64,
            chunks in proptest::collection::vec(proptest::collection::vec(any::<u8>(), 0..40), 0..20),
        ) {
            let mut j = Journal::new(cap);
            let mut all = Vec::new();
            for c in &chunks {
                j.append(c);
                all.extend_from_slice(c);
            }
            let keep = all.len().min(cap as usize);
            prop_assert_eq!(j.end(), all.len() as u64);
            prop_assert_eq!(j.start(), (all.len() - keep) as u64);
            prop_assert_eq!(joined(&j, j.start()), &all[all.len() - keep..]);
        }
    }

    #[test]
    fn the_stream_before_an_offset_survives_the_loss_of_its_marker() {
        let mut j = Journal::new(8);
        j.mark(Marker::Stream {
            id: StreamId::Stdout,
        });
        j.append(b"abcd");
        j.mark(Marker::Stream {
            id: StreamId::Stderr,
        });
        j.append(b"efgh");
        assert_eq!(j.stream_before(0), None);
        assert_eq!(j.stream_before(2), Some(StreamId::Stdout));
        assert_eq!(j.stream_before(4), Some(StreamId::Stdout));
        assert_eq!(j.stream_before(5), Some(StreamId::Stderr));
        // The ring drops the first marker. The stream of the oldest byte is still known.
        j.append(b"ij");
        assert_eq!(
            (j.start(), j.stream_before(j.start())),
            (2, Some(StreamId::Stdout))
        );
    }
}
