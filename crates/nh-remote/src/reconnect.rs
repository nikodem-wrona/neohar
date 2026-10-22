//! The pauses between two tries to open a link again.
//!
//! After a loss the client waits 0.5 s, and doubles the pause up to 8 s. A wake from sleep or a
//! change of the network path makes the next try at once. The code here has no clock: the
//! supervisor of [`crate::PersistentLink`] sleeps for the pause, so a test with a paused clock
//! sees the pauses.

use std::time::Duration;

/// The first pause.
pub const FIRST: Duration = Duration::from_millis(500);

/// The longest pause.
pub const LONGEST: Duration = Duration::from_secs(8);

/// The pause before each try.
#[derive(Clone, Debug)]
pub struct Backoff {
    next: Duration,
    nudged: bool,
}

impl Default for Backoff {
    fn default() -> Self {
        Self::new()
    }
}

impl Backoff {
    pub fn new() -> Self {
        Self {
            next: FIRST,
            nudged: false,
        }
    }

    /// The pause before the next try. A nudge gives no pause once, and the pauses go on as they
    /// were.
    pub fn pause(&mut self) -> Duration {
        if std::mem::take(&mut self.nudged) {
            return Duration::ZERO;
        }
        let pause = self.next;
        self.next = (self.next * 2).min(LONGEST);
        pause
    }

    /// The Mac woke up, or the network path changed: try at once.
    pub fn nudge(&mut self) {
        self.nudged = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_pauses_double_up_to_eight_seconds() {
        let mut backoff = Backoff::new();
        let pauses: Vec<f32> = (0..7).map(|_| backoff.pause().as_secs_f32()).collect();
        assert_eq!(pauses, [0.5, 1.0, 2.0, 4.0, 8.0, 8.0, 8.0]);
    }

    #[test]
    fn a_nudge_tries_at_once_and_the_pauses_go_on() {
        let mut backoff = Backoff::new();
        assert_eq!(backoff.pause(), Duration::from_millis(500));
        backoff.nudge();
        assert_eq!(backoff.pause(), Duration::ZERO);
        // The nudge did not reset the pauses: the next pause is the one that was due.
        assert_eq!(backoff.pause(), Duration::from_secs(1));
        // Two nudges before a try are one.
        backoff.nudge();
        backoff.nudge();
        assert_eq!(backoff.pause(), Duration::ZERO);
        assert_eq!(backoff.pause(), Duration::from_secs(2));
    }
}
