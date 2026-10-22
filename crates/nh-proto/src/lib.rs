//! Wire types shared by the core, the holder, the CLI and the WASM decoder.
//!
//! This crate defines every binary wire type: frames, the `holder/2` protocol
//! and the shared style, cell and mode types. It also holds the one place that
//! computes directories and socket paths. It compiles for native targets and for
//! `wasm32-unknown-unknown`, and it has no async runtime.
//!
//! Wire evolution is append-only. Never reorder or remove enum variants or struct fields in
//! [`holder`] or [`frame`]. `postcard` encodes an enum discriminant by position, so a reordered
//! variant breaks the wire without any compiler error. The frozen fixtures in `tests/fixtures`
//! catch this.

pub mod answers;
#[cfg(feature = "api")]
pub mod api;
pub mod cells;
#[cfg(all(feature = "api", unix))]
pub mod doctor;
pub mod frame;
pub mod framing;
pub mod holder;
pub mod input;
pub mod modes;
pub mod paths;
pub mod style;
pub mod version;

#[cfg(test)]
mod test_support;
#[cfg(test)]
mod wire_tests;
