//! The holder: one process per pane. It owns the PTY master, is the parent of the
//! pane's child process, and keeps both alive when the app quits, crashes or updates.
//!
//! The holder is small on purpose. It has no VT parser beyond the cut-point scanner, no async
//! runtime and no logging framework. `mise run check:holder-size` limits the code in this crate.

mod auth;
mod checkpoint;
mod config;
mod cutpoint;
mod daemon;
mod event_loop;
mod feed;
mod journal;
#[macro_use]
mod log;
mod pty;
mod sequences;
mod server;
mod spawn;

pub use cutpoint::CutScanner;

use std::fmt;

/// An error that stops the holder. It carries a message for the ready pipe and the log.
#[derive(Debug)]
pub struct Error(String);

impl Error {
    /// An error with context: `"bind socket: address in use"`.
    pub(crate) fn new(context: &str, cause: impl fmt::Display) -> Self {
        Self(format!("{context}: {cause}"))
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Runs `nh hold <args>`: reads the config, detaches from the caller, starts the child and serves
/// the socket until the child's exit is acknowledged. It never returns.
///
/// `args` are the arguments after `hold`: `--pane <ulid> --config-fd <n>`.
pub fn main(args: Vec<String>) -> ! {
    let code = match daemon::run(&args) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("nh hold: {e}");
            1
        }
    };
    std::process::exit(code)
}
