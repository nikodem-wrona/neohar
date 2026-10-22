//! When the holder asks the core for a checkpoint.
//!
//! A checkpoint is a snapshot of the VT engine at a cut point. The holder asks when the bytes
//! since the last checkpoint fill half of the ring, when the output has been quiet for a while,
//! and when too much time has passed since the last checkpoint. It asks once, and again after
//! 10 s if the core does not answer. All times are arguments, so that tests need no clock.

use std::time::{Duration, Instant};

use nh_proto::holder::{CheckpointReason, HoldConfig, JournalInfo};

/// The time after which the holder asks again for a checkpoint that the core did not make.
const REPEAT: Duration = Duration::from_secs(10);

/// The state of the checkpoint triggers.
pub struct Checkpoints {
    idle: Duration,
    interval: Duration,
    taken_at: Instant,
    output_at: Instant,
    asked_at: Option<Instant>,
}

impl Checkpoints {
    /// Triggers with the thresholds of `config`. The holder starts at `now`.
    pub fn new(config: &HoldConfig, now: Instant) -> Self {
        Self {
            idle: Duration::from_millis(config.checkpoint_idle_ms.into()),
            interval: Duration::from_millis(config.checkpoint_max_interval_ms.into()),
            taken_at: now,
            output_at: now,
            asked_at: None,
        }
    }

    /// New output reached the journal.
    pub fn output(&mut self, now: Instant) {
        self.output_at = now;
    }

    /// The core made a checkpoint.
    pub fn taken(&mut self, now: Instant) {
        self.taken_at = now;
        self.asked_at = None;
    }

    /// A new client attached. It has made no checkpoint, so the holder can ask at once.
    pub fn forget_request(&mut self) {
        self.asked_at = None;
    }

    /// The checkpoint to ask for now, if any: the cut point to use and the reason.
    ///
    /// There is nothing to ask for until a cut point lies after the last checkpoint.
    pub fn due(&mut self, now: Instant, journal: &JournalInfo) -> Option<(u64, CheckpointReason)> {
        if journal.last_cut_point <= journal.last_checkpoint {
            return None;
        }
        if self.asked_at.is_some_and(|asked| now < asked + REPEAT) {
            return None;
        }
        let reason = if journal.end - journal.last_checkpoint >= journal.capacity / 2 {
            CheckpointReason::RingHalf
        } else if now >= self.output_at + self.idle {
            CheckpointReason::Idle
        } else if now >= self.taken_at + self.interval {
            CheckpointReason::Interval
        } else {
            return None;
        };
        self.asked_at = Some(now);
        Some((journal.last_cut_point, reason))
    }

    /// The time at which [`Checkpoints::due`] can next say yes because of the clock. A full ring
    /// is not a time: the holder checks that after each read.
    pub fn deadline(&self, journal: &JournalInfo) -> Option<Instant> {
        if journal.last_cut_point <= journal.last_checkpoint {
            return None;
        }
        let by_clock = (self.output_at + self.idle).min(self.taken_at + self.interval);
        Some(
            self.asked_at
                .map_or(by_clock, |asked| by_clock.max(asked + REPEAT)),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECOND: Duration = Duration::from_secs(1);

    fn config() -> HoldConfig {
        HoldConfig {
            proto: 2,
            pane: [0; 16],
            secret: [0; 32],
            mode: nh_proto::holder::HoldMode::Pipe,
            argv: vec![],
            env: vec![],
            cwd: String::new(),
            socket_path: String::new(),
            log_path: String::new(),
            ring_bytes: 1000,
            checkpoint_idle_ms: 2_000,
            checkpoint_max_interval_ms: 30_000,
            query_fresh_ms: 5_000,
        }
    }

    fn journal(end: u64, cut: u64, checkpoint: u64) -> JournalInfo {
        JournalInfo {
            start: 0,
            end,
            capacity: 1000,
            last_checkpoint: checkpoint,
            last_cut_point: cut,
        }
    }

    #[test]
    fn nothing_is_due_before_a_cut_point_lies_after_the_last_checkpoint() {
        let t0 = Instant::now();
        let mut c = Checkpoints::new(&config(), t0);
        assert_eq!(c.due(t0 + 100 * SECOND, &journal(900, 0, 0)), None);
        assert_eq!(c.due(t0 + 100 * SECOND, &journal(900, 500, 500)), None);
        assert_eq!(c.deadline(&journal(900, 500, 500)), None);
    }

    #[test]
    fn a_ring_half_full_of_new_bytes_asks_at_once() {
        let t0 = Instant::now();
        let mut c = Checkpoints::new(&config(), t0);
        assert_eq!(c.due(t0, &journal(499, 400, 0)), None);
        assert_eq!(
            c.due(t0, &journal(500, 400, 0)),
            Some((400, CheckpointReason::RingHalf))
        );
    }

    #[test]
    fn quiet_output_asks_after_the_idle_time() {
        let t0 = Instant::now();
        let mut c = Checkpoints::new(&config(), t0);
        c.output(t0 + 5 * SECOND);
        let info = journal(100, 100, 0);
        assert_eq!(c.due(t0 + 6 * SECOND, &info), None);
        assert_eq!(
            c.due(t0 + 7 * SECOND, &info),
            Some((100, CheckpointReason::Idle))
        );
    }

    #[test]
    fn busy_output_asks_after_the_interval() {
        let t0 = Instant::now();
        let mut c = Checkpoints::new(&config(), t0);
        let info = journal(100, 100, 0);
        // Output keeps the idle trigger away.
        for s in 1..=30 {
            c.output(t0 + s * SECOND);
        }
        assert_eq!(
            c.due(t0 + 30 * SECOND, &info),
            Some((100, CheckpointReason::Interval))
        );
    }

    #[test]
    fn it_asks_once_repeats_after_ten_seconds_and_stops_when_the_core_answers() {
        let t0 = Instant::now();
        let mut c = Checkpoints::new(&config(), t0);
        let info = journal(600, 600, 0);
        assert!(c.due(t0, &info).is_some());
        assert_eq!(c.due(t0 + SECOND, &info), None);
        assert_eq!(c.due(t0 + 9 * SECOND, &info), None);
        assert!(c.due(t0 + 10 * SECOND, &info).is_some());
        c.taken(t0 + 11 * SECOND);
        assert_eq!(c.due(t0 + 100 * SECOND, &journal(600, 600, 600)), None);
        // More output after the checkpoint gives a new request.
        assert!(c.due(t0 + 100 * SECOND, &journal(900, 900, 600)).is_some());
    }

    #[test]
    fn the_deadline_is_the_first_clock_trigger_or_the_repeat() {
        let t0 = Instant::now();
        let mut c = Checkpoints::new(&config(), t0);
        let info = journal(100, 100, 0);
        c.output(t0 + SECOND);
        assert_eq!(c.deadline(&info), Some(t0 + 3 * SECOND));
        let _ = c.due(t0 + 3 * SECOND, &info);
        assert_eq!(c.deadline(&info), Some(t0 + 13 * SECOND));
        c.forget_request();
        assert_eq!(c.deadline(&info), Some(t0 + 3 * SECOND));
    }
}
