//! The core of neohar: the library that owns panes, their VT engines and the render
//! server. A host embeds it: `neohar.app` (Tauri) or `nh daemon`. The core never depends on a
//! webview framework.
//!
//! The state actor owns the layout and the store, and every method of the API goes through
//! [`CoreHandle::call_as`]. The panes read the output of their holders into VT engines, and the
//! render server paces frames to the windows of the layout.
//!
//! The core runs on its own multi-threaded tokio runtime ([`Core`]). A host calls it through a
//! [`CoreHandle`], from any thread and from any runtime.

mod actor;
mod agent_calls;
mod agents;
mod api;
mod askpass;
mod authorize;
mod changes;
mod clients;
pub mod config;
#[cfg(feature = "debug")]
mod debug_calls;
#[cfg(feature = "dev-ws")]
pub mod dev_ws;
pub mod device;
mod discovery;
mod doctor;
pub mod error;
mod files;
mod focus;
mod git_view;
pub mod harness;
pub mod health;
pub mod holder;
pub mod host;
mod ids;
mod in_pane;
mod integration_calls;
mod interaction;
pub mod jobs;
mod keystrokes;
mod layout;
pub mod logging;
pub mod login_env;
pub mod machines;
mod native;
mod palette;
pub mod pane;
mod pane_calls;
mod pane_text;
mod persist;
pub mod phones;
mod policy;
mod power;
pub mod projection;
pub mod projections;
pub mod render;
mod retention;
mod rich_calls;
pub mod router;
mod runtime;
pub mod session;
pub mod shims;
pub mod sidecar;
mod subscriptions;
pub mod supervise;
pub mod tokens;
mod transcripts;
mod uploads;
pub mod watch;
mod window_calls;
mod workflows;
pub mod ws_rpc;

pub use actor::{Origin, Scope};
pub use api::SignalRecord;
pub use error::{CoreError, ErrorCode, RpcError};
pub use host::{
    ClientAttachment, ClientInfo, Host, InProcessHost, NoopHost, RecordingHost, SecretAnswered,
    SecretKind, SecretQuestion,
};
/// The redaction of secrets. The host wraps the writers of its logger with it.
pub use nh_redact as redact;
/// The rich content. A link client cleans the HTML of a remote core again with it.
pub use nh_rich as rich;
pub use projection::{MessageSink, ProjectionHub, SubscriptionId};
pub use render::FrameSink;
pub use router::HostRouter;
pub use runtime::{Core, CoreConfig, CoreHandle, QuitReport, StopReport, Tuning};

/// The largest file that a client may put in the blob store: a file dropped on a pane.
pub const UPLOAD_MAX_BYTES: u64 = uploads::MAX_BYTES;
