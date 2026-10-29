//! Harness manifests and the facts that they give about a harness.
//!
//! A manifest is a TOML file that describes how neohar finds a harness, runs it, resumes it,
//! learns its identity and state, and which capabilities each version has. Two manifests are built
//! in (Claude Code and Codex). A user can override one by `id`, or describe a wrapper script with
//! `extends`. This crate has no I/O besides reading the files and running a version command. The
//! core wires it to the configuration directory, the health notes and the event log.

pub mod adapter;
pub mod arbiter;
pub mod auth;
pub mod capabilities;
pub mod claude;
pub mod codex;
pub mod harness;
pub mod integration;
pub mod keystroke;
pub mod manifest;
pub mod matcher;
pub mod process;
pub mod run;
pub mod screen;
pub mod skills;
#[cfg(test)]
pub(crate) mod testing;
pub mod transcript;
pub mod version;

pub use manifest::{
    Entry, Loaded, Manifest, Source, Warning,
    loader::{builtin, load},
};
