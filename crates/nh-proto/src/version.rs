//! Protocol and format versions.

/// Version of the frame wire format. Checked at `render.attach`.
pub const FRAME_PROTO: u16 = 1;

/// Version of the holder protocol. A holder refuses a client with another version.
pub const HOLDER_PROTO: u16 = 2;

/// Version of the JSON API. Additive within a major version.
pub const API_VERSION: &str = "neohar/1";
