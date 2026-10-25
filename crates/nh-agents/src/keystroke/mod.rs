//! Answer delivery by verified keystrokes.
//!
//! When no native channel exists, neohar can answer a dialog of a harness by typing. This is best
//! effort: the application can change between the last read of the screen and the key. The
//! verifier makes a wrong selection unlikely. It is a state machine without I/O. The core reads
//! the screen, gives the reading to [`Verifier::observe`], and does what [`Next`] says.
//!
//! The rules:
//!
//! 1. The dialog must match the request: it asks for the same command, file or question
//!    ([`Asks`]), and the labels fuzzy-match at 0.9 or more.
//! 2. One move key at a time (or one accelerator), and a reading after each key.
//! 3. Enter only when the pointer is on the target. A reading that does not fit the plan ends the
//!    sequence with no more key.
//! 4. After Enter, the dialog must go. The core decides what the end means.

mod verifier;

pub use verifier::{
    Asks, Choice, Failure, Key, Next, Outcome, Reading, Request, Verifier, shows_command,
    similarity,
};

/// The least similarity of a label of the dialog to a label that the request names.
pub const MIN_SIMILARITY: f32 = 0.9;

/// How long the core waits for the screen to change after a key.
pub const DAMAGE_WAIT_MS: u64 = 300;

/// How long the core waits for the dialog to go after Enter.
pub const CONFIRM_WAIT_MS: u64 = 2_000;

/// How many times a move key is sent again when the screen did not move.
pub const MOVE_RETRIES: u8 = 2;

#[cfg(test)]
mod tests;
