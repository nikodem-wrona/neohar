//! A scripted model server for the tests of neohar.
//!
//! `mock-llm` speaks two wire protocols: the Anthropic Messages API (what Claude Code calls) and
//! the OpenAI Responses API (what Codex calls). It answers from a scenario file, so a run of a
//! harness is deterministic, offline and free. It logs every request, so that the recordings of
//! the harnesses show what each one calls.
//!
//! The server is a library, so that the recorder can start it in its own process, and a binary
//! (`mock-llm --scenario <file>`) for a run by hand.

pub mod anthropic;
pub mod log;
pub mod openai;
pub mod request;
pub mod scenario;
mod server;

pub use log::LogSink;
pub use scenario::{Block, Reply, Scenario, ScenarioError, Turn};
pub use server::{Report, Server, serve};
