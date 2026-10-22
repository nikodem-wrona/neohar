//! The VT engine behind a trait, and the frame builder.
//!
//! `nh-term` turns the bytes of a pane into the frames of `nh-proto`. The engine keeps the
//! screen model. The frame builder keeps what each client of a pane has seen.

mod alacritty;
#[cfg(feature = "test-support")]
pub mod client_grid;
mod dump;
mod effects;
mod engine;
mod engine_snapshot;
mod frame_builder;
mod history;
pub mod input;
mod intercept;
mod intern;
mod mirror;
mod palette;
mod selection;
mod snapshot;

pub use alacritty::AlacrittyEngine;
pub use dump::{DumpCell, DumpDiff, DumpRun, ScreenDump};
pub use effects::{Effect, EffectSink, Recorder, Replaying};
pub use engine::{CellView, Damage, EngineConfig, HistoryLine, StyleRun, VtEngine};
#[cfg(feature = "test-support")]
pub use engine_snapshot::Scrollback;
pub use frame_builder::{Absorbed, ClientState, FrameBuilder};
pub use history::{HIGH_WATER as HISTORY_HIGH_WATER, MIN_CAPACITY as MIN_HISTORY};
pub use palette::xterm_palette;
pub use snapshot::{
    HistoryState, RestoreError, Run, SNAPSHOT_VERSION, Screen, SnapCell, SnapColor, SnapCursor,
    Snapshot, SnapshotError,
};
