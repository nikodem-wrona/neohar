//! `nh-golden`: records real Claude Code and Codex binaries against the mock model.
//!
//! A recording runs a harness the way neohar would run it in a pane, but with nothing that comes
//! from the user or from the network:
//!
//! - **Isolation** ([`recipe`], [`sandbox`]). The harness gets a new home and config directory in
//!   a scratch tree, an environment built from nothing, and a macOS sandbox profile. The profile
//!   denies writes outside the scratch tree, denies any access to the real `~/.claude`,
//!   `~/.claude.json` and `~/.codex`, and denies network traffic except to the loopback.
//! - **A model that is a script** ([`mock_llm`]). The harness calls a mock server on 127.0.0.1
//!   that answers from the `[[turn]]` list of the scenario.
//! - **Hooks**. The harness calls `nh hook <harness> <Event>` as in the product. The shim writes a
//!   tee file and also talks to a minimal API socket ([`apisock`]).
//! - **A driver** ([`driver`]). It runs the harness in a holder pty, answers the terminal queries
//!   like the core does, feeds the output to a VT engine and runs the `[[step]]` list of the
//!   scenario ([`scenario`], [`keys`]).
//!
//! A recording is a directory with `session.cast` (asciicast v2), `signals.jsonl`,
//! `llm-requests.jsonl`, `screens/*.txt`, `scenario.toml` and `meta.toml`. [`compare`] says whether
//! two recordings of one scenario are the same, and [`guard`] proves that the real configuration
//! of the user stayed as it was. [`scrub`] keeps the system prompt and the tool descriptions of the
//! harness out of the files.

pub mod apisock;
pub mod cast;
pub mod chain;
pub mod compare;
pub mod corectl;
pub mod driver;
pub mod guard;
pub mod keys;
pub mod mcp;
pub mod meta;
pub mod procs;
pub mod recipe;
pub mod sandbox;
pub mod scenario;
pub mod scrub;

use std::io;

/// Why a recording or a comparison failed.
#[derive(Debug, thiserror::Error)]
pub enum GoldenError {
    #[error("{context}: {source}")]
    Io {
        context: String,
        #[source]
        source: io::Error,
    },
    #[error("{0}")]
    Scenario(#[from] scenario::ScenarioError),
    /// The holder, the pty or the connection to them did not work.
    #[error("{0}")]
    Record(#[from] nh_record::RecordError),
    /// A step of the scenario did not do what it should.
    #[error("{0}")]
    Step(String),
    /// The setup of a recording did not work: a binary is missing, a version is unknown.
    #[error("{0}")]
    Setup(String),
}

impl GoldenError {
    /// A function that wraps an I/O error with what the tool was doing.
    pub fn io(context: impl Into<String>) -> impl FnOnce(io::Error) -> Self {
        let context = context.into();
        move |source| Self::Io { context, source }
    }
}
