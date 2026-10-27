//! The inputs of a pane that the holder did not acknowledge yet ("Input").
//!
//! Every input has an id from the client. The holder writes it, records an `InputAck` in the
//! journal and answers with an ack. The semantics are at-most-once: an input that the holder did
//! not acknowledge may be lost, and the client is told (`input_unconfirmed`). This type holds the
//! ids that went out and have no ack yet, and the ids that were acknowledged lately.

use std::{
    collections::VecDeque,
    time::{Duration, Instant},
};

use super::INTERNAL_INPUT;

/// The most ids that the pane remembers in each list. It is also the number of ids that the holder
/// remembers to drop duplicates.
pub(super) const KEPT: usize = 4096;

/// Why an input is reported as unconfirmed (`pane.input_unconfirmed`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnconfirmedReason {
    /// The connection to the holder ended before the ack.
    HolderLost,
    /// The ack did not come in time.
    NotWritten,
}

impl UnconfirmedReason {
    /// The reason as the event of the log has it.
    pub fn event_reason(self) -> nh_proto::api::events::UnconfirmedReason {
        use nh_proto::api::events::UnconfirmedReason as Event;
        match self {
            Self::HolderLost => Event::HolderLost,
            Self::NotWritten => Event::NotWritten,
        }
    }

    /// The name in the API.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::HolderLost => "holder_lost",
            Self::NotWritten => "not_written",
        }
    }
}

/// An input that the core reports as unconfirmed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Unconfirmed {
    pub input_id: u64,
    pub reason: UnconfirmedReason,
}

/// The ids of the inputs of one pane.
#[derive(Default)]
pub(super) struct Inputs {
    /// Sent and not acknowledged, the oldest first.
    pending: VecDeque<(u64, Instant)>,
    /// Acknowledged, the oldest first. The number is the sequence number of the ack.
    acked: VecDeque<(u64, u64)>,
    next_seq: u64,
}

impl Inputs {
    /// An input went to the holder. Returns whether it is the only one that waits for an ack, so
    /// that the pane task must start a timer for it.
    pub fn sent(&mut self, id: u64, at: Instant) -> bool {
        if self.pending.len() == KEPT {
            self.pending.pop_front();
        }
        self.pending.push_back((id, at));
        self.pending.len() == 1
    }

    /// An input could not be queued for the holder after all.
    pub fn unsent(&mut self, id: u64) {
        self.pending.retain(|(pending, _)| *pending != id);
    }

    /// The holder acknowledged an input. Returns how long the ack took, when the core sent the
    /// input and had not given it up. The list of acks keeps the ids of clients only: the replies
    /// of the core to queries are many, and they would push those ids out.
    pub fn acked(&mut self, id: u64) -> Option<Duration> {
        let took = self
            .pending
            .iter()
            .position(|(pending, _)| *pending == id)
            .and_then(|at| self.pending.remove(at))
            .map(|(_, sent)| sent.elapsed());
        if id & INTERNAL_INPUT == 0 {
            if self.acked.len() == KEPT {
                self.acked.pop_front();
            }
            self.acked.push_back((self.next_seq, id));
            self.next_seq += 1;
        }
        took
    }

    /// Takes the inputs that have waited for an ack longer than `timeout`.
    pub fn expired(&mut self, now: Instant, timeout: Duration) -> Vec<u64> {
        let mut out = Vec::new();
        while let Some(&(id, sent)) = self.pending.front() {
            if now.duration_since(sent) < timeout {
                break;
            }
            self.pending.pop_front();
            out.push(id);
        }
        out
    }

    /// The time at which the oldest pending input expires.
    pub fn deadline(&self, timeout: Duration) -> Option<Instant> {
        self.pending.front().map(|(_, sent)| *sent + timeout)
    }

    /// Takes every pending input. The connection to the holder ended.
    pub fn take_pending(&mut self) -> Vec<u64> {
        self.pending.drain(..).map(|(id, _)| id).collect()
    }

    /// How many inputs wait for an ack.
    pub fn pending_len(&self) -> usize {
        self.pending.len()
    }

    /// The ids acknowledged with a sequence number of `since` or more, and the number to ask for
    /// next time. A client that asks with the number it got misses no id, unless more than
    /// [`KEPT`] acks came between two calls.
    pub fn acked_since(&self, since: u64) -> (Vec<u64>, u64) {
        let ids = self
            .acked
            .iter()
            .filter(|(seq, _)| *seq >= since)
            .map(|(_, id)| *id)
            .collect();
        (ids, self.next_seq)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECOND: Duration = Duration::from_secs(1);

    #[test]
    fn an_ack_moves_the_id_from_pending_to_acked() {
        let mut inputs = Inputs::default();
        inputs.sent(7, Instant::now());
        assert_eq!(inputs.pending_len(), 1);
        assert!(inputs.acked(7).is_some());
        assert_eq!(inputs.pending_len(), 0);
        assert_eq!(inputs.acked_since(0), (vec![7], 1));
    }

    #[test]
    fn an_ack_of_an_unknown_id_is_kept_without_a_duration() {
        // After a restart of the core, the holder acknowledges ids that this core never sent.
        let mut inputs = Inputs::default();
        assert!(inputs.acked(9).is_none());
        assert_eq!(inputs.acked_since(0).0, [9]);
    }

    #[test]
    fn a_client_that_passes_the_next_number_gets_each_ack_once() {
        let mut inputs = Inputs::default();
        for id in [1, 2, 3] {
            inputs.acked(id);
        }
        let (first, next) = inputs.acked_since(0);
        assert_eq!(first, [1, 2, 3]);
        inputs.acked(4);
        let (second, next) = inputs.acked_since(next);
        assert_eq!(second, [4]);
        assert!(inputs.acked_since(next).0.is_empty());
    }

    #[test]
    fn an_ack_of_an_input_of_the_core_leaves_the_list_of_acks_alone() {
        let mut inputs = Inputs::default();
        inputs.acked(1);
        let keystroke = INTERNAL_INPUT | 1;
        inputs.sent(keystroke, Instant::now());
        for n in 0..KEPT as u64 {
            inputs.acked(INTERNAL_INPUT | (n + 2));
        }
        assert!(inputs.acked(keystroke).is_some(), "it is no longer pending");
        assert_eq!(inputs.pending_len(), 0);
        assert_eq!(inputs.acked_since(0), (vec![1], 1));
    }

    #[test]
    fn only_the_inputs_older_than_the_timeout_expire() {
        let mut inputs = Inputs::default();
        let start = Instant::now();
        inputs.sent(1, start);
        inputs.sent(2, start + SECOND);
        inputs.sent(3, start + 2 * SECOND);
        let expired = inputs.expired(start + SECOND + SECOND / 2, SECOND);
        assert_eq!(expired, [1]);
        assert_eq!(inputs.pending_len(), 2);
        assert_eq!(inputs.deadline(SECOND), Some(start + 2 * SECOND));
    }

    #[test]
    fn a_lost_connection_takes_every_pending_input() {
        let mut inputs = Inputs::default();
        inputs.sent(1, Instant::now());
        inputs.sent(2, Instant::now());
        inputs.acked(1);
        assert_eq!(inputs.take_pending(), [2]);
        assert_eq!(inputs.deadline(SECOND), None);
    }

    #[test]
    fn an_input_that_was_not_queued_is_forgotten() {
        let mut inputs = Inputs::default();
        inputs.sent(1, Instant::now());
        inputs.unsent(1);
        assert_eq!(inputs.pending_len(), 0);
    }

    #[test]
    fn the_lists_are_bounded() {
        let mut inputs = Inputs::default();
        for id in 0..(KEPT as u64 + 10) {
            inputs.sent(id, Instant::now());
            inputs.acked(id + 100_000);
        }
        assert_eq!(inputs.pending_len(), KEPT);
        let (ids, next) = inputs.acked_since(0);
        assert_eq!(ids.len(), KEPT);
        assert_eq!(next, KEPT as u64 + 10);
    }
}
