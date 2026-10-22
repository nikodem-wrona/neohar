//! `nh-record`: records a program under a real holder and saves the raw journal.
//!
//! The tool starts `nh hold` the way the core does, attaches to it, drives the program with a
//! step script ([`script`]) and saves what the holder sent as a journal file ([`journal`]). The
//! corpus in `tests/vt-corpus/` is made with it. A tool that replays a journal through a VT
//! engine can read the files with [`journal::read_journal`].
//!
//! Safety. Holders outlive their parent by design, so the tool stops them on every way out: a
//! normal end, an error, a panic and an interrupt. [`holder::HolderProcess`] is the guard. It
//! sends `Signal Kill` over the socket with the highest fencing token, kills the child's process
//! group, and then kills the holder by the pid from `READY`. Everything the program touches is in
//! one scratch directory under `/tmp`, which the tool removes at the end.

pub mod cli;
pub mod holder;
pub mod journal;
pub mod recorder;
pub mod script;
pub mod signals;

use std::io;

use nh_proto::framing::FramingError;

/// Why a recording failed.
#[derive(Debug, thiserror::Error)]
pub enum RecordError {
    #[error("{0}")]
    Script(#[from] script::ScriptError),
    #[error("{context}: {source}")]
    Io {
        context: String,
        #[source]
        source: io::Error,
    },
    /// The holder or the setup of a recording did not work.
    #[error("{0}")]
    Holder(String),
    #[error("holder connection: {0}")]
    Framing(#[from] FramingError),
    #[error("{0}")]
    Journal(#[from] journal::JournalError),
    /// A script step did not do what it should.
    #[error("{0}")]
    Step(String),
    #[error("interrupted")]
    Interrupted,
}

impl RecordError {
    /// A function that wraps an I/O error with what the tool was doing.
    pub fn io(context: impl Into<String>) -> impl FnOnce(io::Error) -> Self {
        let context = context.into();
        move |source| Self::Io { context, source }
    }
}
