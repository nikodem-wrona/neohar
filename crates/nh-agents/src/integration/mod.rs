//! The installers of the integrations.
//!
//! An installer puts the hooks of neohar in the configuration of a harness, and takes them out
//! again. Its rules:
//!
//! 1. **It is idempotent.** A second run gives the same bytes. A hook of neohar is told by its
//!    command, `<nh> hook <id> <Event>`, and has no marker key: Claude Code drops an unknown key
//!    when it saves its settings.
//! 2. **It never clobbers.** The edit is **text surgery** on the file: the installer finds the
//!    places in the text, and inserts or removes only its own objects. Everything else stays byte
//!    for byte: the order of keys, the indentation and the odd spacing. A file that is not JSON is
//!    refused.
//! 3. **It writes atomically.** A backup `<file>.neohar-bak-<seconds>` comes before the first change,
//!    the new content goes to a temporary file in the same directory, and the write stops when the
//!    file changed after it was read.
//!
//! The modules: [`json`] reads the places of a JSON text and makes the edits, [`hooks`] installs the
//! hooks of a harness from a [`hooks::Spec`], [`claude`] and [`codex`] hold the specs, [`diff`]
//! prints the change for `--dry-run`, and [`files`] does the I/O.

pub mod claude;
pub mod codex;
pub mod diff;
pub mod files;
pub mod hooks;
pub mod json;

/// What the installers can fail on.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum Error {
    /// The text is not JSON, or not the JSON that the installer expects. The file stays as it is.
    #[error("{0}")]
    Format(String),
    /// The file changed between the read and the write.
    #[error("{0} changed while neohar edited it, so nothing was written; run the command again")]
    Changed(String),
    #[error("{0}")]
    Io(String),
}

impl From<std::io::Error> for Error {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error.to_string())
    }
}

pub type Result<T> = std::result::Result<T, Error>;
