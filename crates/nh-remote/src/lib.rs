//! Remote machines: the protocol `link/1`, the server of `link.sock` and its client.
//!
//! - [`msg`]: the messages of `link/1` and their classes.
//! - [`codec`]: framing, parts of 16 KiB, one queue for each class, zstd. No I/O.
//! - [`serve`]: the server of `link.sock`. Both hosts of a core call it after `Core::start`: the
//!   app and `nh daemon`.
//! - [`LinkClient`]: a client over any pair of byte streams, with the methods that a host needs
//!   of a core. It answers the host requests of the core within the limits of [`HostLimits`].
//! - [`ssh`]: the link over the system `ssh`, `nh askpass` for its questions, and the version rule.
//! - [`machines`]: the links to the machines of `config.toml`, as the local core owns them.
//! - [`LinkHost`]: the [`nh_core::Host`] of one link client on the side of the core.

mod client;
pub mod codec;
#[cfg(feature = "dev-ws")]
pub mod dev_ws;
mod feed;
pub mod forward;
mod host_req;
pub mod link;
pub mod machines;
pub mod msg;
pub mod netwatch;
mod persistent;
pub mod reconnect;
mod server;
pub mod settings;
pub mod ssh;
mod wire;

pub use client::{ClientHello, ClientOptions, LinkClient, LinkError, MAX_BLOB, link_down};
pub use feed::LinkFeed;
pub use host_req::{
    Action, HostLimits, HostRequest, Level, Notification, Rect, Tray, TrayLine, TrayLineV2, TrayV2,
    Window,
};
pub use link::Link;
pub use msg::{Class, Device, Hello, HostOutcome, LinkMsg, Welcome};
pub use persistent::{
    ConnectError, Connector, INPUT_RESEND_WITHIN, LinkStatus, PersistentLink, UnixConnector,
};
pub use server::{
    BLOB_CREDITS, BLOB_PART, LinkDropper, LinkHost, LinkServer, OpenLink, ServeError, new_ulid,
    serve, serve_with,
};
pub use settings::{SettingsFeed, Side};
pub use wire::LinkTiming;
