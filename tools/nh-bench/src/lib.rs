//! The tools of the render tests.
//!
//! `gen-frames` writes frame sequences for the render benches and the render tests.
//!
//! ## Frame sequences
//!
//! `gen-frames` drives one VT engine for each pane with a scenario, builds the frames that the
//! core would send, and writes them to a file that the bench page plays. Everything is
//! deterministic: the same arguments give the same bytes.
//!
//! The file has a header and records. All numbers are little-endian `u32`.
//!
//! ```text
//! "NHFRAMES"  version  panes  cols  rows  count
//! count × ( tick  pane  length  frame bytes )
//! ```
//!
//! Frames of one `tick` are for the same display frame. Panes have the ids 1 to `panes`.

pub mod container;
pub mod scenes;

pub use container::{Container, Record, read, write};
pub use scenes::{Scenario, generate};
