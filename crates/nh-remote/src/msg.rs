//! The messages of `link/1`.
//!
//! A message is one variant of [`LinkMsg`]. `postcard` writes the index of the variant (the
//! `kind`) and then its fields (the `body`). The order of the variants is the protocol: a new
//! message goes at the end, and the first three never change, so that a client and a core of
//! different versions can always read `Hello`, `Welcome` and `Refused`.
//!
//! The bodies of calls, results and projection messages are the JSON of the API, as bytes. A frame
//! is the `postcard` frame of the renderer, unchanged. Ids of windows and devices are text, so
//! that a bad id gets an error and not a message that nobody can read.

use serde::{Deserialize, Serialize};

/// The name of the protocol in `Hello`.
pub const PROTO: &str = "link/1";

/// The reason of a `Refused` when the client speaks another protocol.
pub const REFUSED_PROTO: &str = "proto";
/// The reason of a `Refused` when the major version of the API differs.
pub const REFUSED_API: &str = "api";

/// The class of a message. Each side sends the lowest class first.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Class {
    /// Control and input.
    Control = 0,
    /// The other calls, their results and the projections.
    Normal = 1,
    /// Render frames.
    Frames = 2,
    /// Blobs, and every call larger than 1 MiB.
    Bulk = 3,
}

impl Class {
    pub const ALL: [Self; 4] = [Self::Control, Self::Normal, Self::Frames, Self::Bulk];

    pub(crate) const fn index(self) -> usize {
        self as usize
    }

    pub(crate) fn from_index(index: u8) -> Option<Self> {
        Self::ALL.get(usize::from(index)).copied()
    }
}

/// The calls of class 0: input and the reports that the gate and the frame pace
/// wait for. The server runs the class-0 calls of a link in the order that they came.
pub const CONTROL_METHODS: [&str; 7] = [
    "pane.input",
    "pane.paste",
    "pane.send_bytes",
    "render.ack",
    "window.focus_report",
    "pane.focus",
    "interaction.answer",
];

/// A call of another method with more bytes of parameters than this is bulk data (class 3).
pub const BULK_CALL: usize = 1 << 20;

/// Whether a call of `method` is of class 0.
pub fn is_control_method(method: &str) -> bool {
    CONTROL_METHODS.contains(&method)
}

/// The device of a client: a ULID made at its first launch, and the host name.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Device {
    pub id: String,
    pub name: String,
}

/// The first message of a client.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hello {
    /// `link/1`.
    pub proto: String,
    /// The version of the API, for example `neohar/1`.
    pub api: String,
    pub app_version: String,
    pub device: Device,
}

/// The answer of the core to a good `Hello`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Welcome {
    pub core_version: String,
    /// The id of the machine. A client that finds another one after a reconnect stops.
    pub machine_uuid: String,
    pub session_uuid: String,
    pub hostname: String,
    /// One text for one run of the core: the time it started and its process id. A client that
    /// finds another one after a reconnect knows that the core restarted, and it does not send
    /// its input again.
    pub core_epoch: String,
}

/// The result of a call or of the start of a subscription or a stream.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Outcome {
    /// The JSON of the result.
    Ok(#[serde(with = "serde_bytes")] Vec<u8>),
    /// The JSON of the error: `{code, message, details?}`.
    Err(#[serde(with = "serde_bytes")] Vec<u8>),
}

/// The answer of a client to a host request.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum HostOutcome {
    /// The client did it.
    Done,
    /// The answer of `notification_capabilities`.
    Capabilities { actions: bool },
    /// The client refused the request. The text says why.
    Refused(String),
}

/// The end of a blob transfer.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum BlobOutcome {
    /// All parts came. `hash` is the BLAKE3 digest in hex, and `mime` the type of the blob.
    Ok { hash: String, mime: String },
    /// The JSON of the error, as in [`Outcome::Err`]. `not_found` when the store has no such blob.
    Err(#[serde(with = "serde_bytes")] Vec<u8>),
}

/// A message of `link/1`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum LinkMsg {
    Hello(Hello),
    Welcome(Welcome),
    Refused {
        reason: String,
    },
    /// A JSON-RPC call. `window` is the window id of the webview that made it. `device` names a
    /// phone that the client relays: the core puts it in the origin.
    Call {
        id: u64,
        window: Option<String>,
        device: Option<Device>,
        method: String,
        #[serde(with = "serde_bytes")]
        params: Vec<u8>,
    },
    /// The result of a call, of a `Subscribe` or of a `Stream`: the three share the space of ids
    /// of the client.
    Reply {
        id: u64,
        outcome: Outcome,
    },
    Subscribe {
        sub: u64,
        name: String,
        since: Option<u64>,
    },
    Unsubscribe {
        sub: u64,
    },
    /// A snapshot, a patch or a `resync` of a projection, as JSON.
    Projection {
        sub: u64,
        #[serde(with = "serde_bytes")]
        message: Vec<u8>,
    },
    /// A binary stream. `device` names a phone that the client relays: the core attaches the
    /// phone as a client while a stream of it is open.
    Stream {
        stream: u64,
        kind: String,
        #[serde(with = "serde_bytes")]
        params: Vec<u8>,
        window: Option<String>,
        device: Option<Device>,
    },
    StreamClose {
        stream: u64,
    },
    Frame {
        stream: u64,
        #[serde(with = "serde_bytes")]
        bytes: Vec<u8>,
    },
    /// A call of the `Host` trait. `request` is the `postcard` form of a
    /// [`crate::HostRequest`]. It is a body of its own, so that a client can refuse a request that
    /// it does not know and keep the link.
    HostRequest {
        id: u64,
        #[serde(with = "serde_bytes")]
        request: Vec<u8>,
    },
    HostReply {
        id: u64,
        result: HostOutcome,
    },
    BlobGet {
        id: u64,
        hash: String,
    },
    BlobPut {
        id: u64,
        name: String,
        size: u64,
        /// The BLAKE3 digest of the bytes in hex. The core checks it when the last part came.
        hash: String,
    },
    BlobPart {
        id: u64,
        offset: u64,
        #[serde(with = "serde_bytes")]
        bytes: Vec<u8>,
        last: bool,
    },
    BlobDone {
        id: u64,
        outcome: BlobOutcome,
    },
    Ping {
        nonce: u64,
        sent_at_ms: u64,
    },
    Pong {
        nonce: u64,
        sent_at_ms: u64,
    },
    Bye {
        reason: String,
    },
    /// The receiver of a blob wrote `parts` more parts, and the sender may send as many more
    /// (flow control).
    BlobCredit {
        id: u64,
        parts: u32,
    },
}

impl LinkMsg {
    /// The class of the message.
    pub fn class(&self) -> Class {
        match self {
            Self::Hello(_)
            | Self::Welcome(_)
            | Self::Refused { .. }
            | Self::HostRequest { .. }
            | Self::HostReply { .. }
            | Self::Ping { .. }
            | Self::Pong { .. }
            | Self::Bye { .. } => Class::Control,
            // Input keeps its order whatever its size.
            Self::Call { method, .. } if is_control_method(method) => Class::Control,
            Self::Call { params, .. } if params.len() > BULK_CALL => Class::Bulk,
            Self::Call { .. }
            | Self::Reply { .. }
            | Self::Subscribe { .. }
            | Self::Unsubscribe { .. }
            | Self::Projection { .. }
            | Self::Stream { .. }
            | Self::StreamClose { .. } => Class::Normal,
            Self::Frame { .. } => Class::Frames,
            Self::BlobGet { .. }
            | Self::BlobPut { .. }
            | Self::BlobPart { .. }
            | Self::BlobDone { .. }
            | Self::BlobCredit { .. } => Class::Bulk,
        }
    }

    /// The name of the kind, for logs.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Hello(_) => "Hello",
            Self::Welcome(_) => "Welcome",
            Self::Refused { .. } => "Refused",
            Self::Call { .. } => "Call",
            Self::Reply { .. } => "Reply",
            Self::Subscribe { .. } => "Subscribe",
            Self::Unsubscribe { .. } => "Unsubscribe",
            Self::Projection { .. } => "Projection",
            Self::Stream { .. } => "Stream",
            Self::StreamClose { .. } => "StreamClose",
            Self::Frame { .. } => "Frame",
            Self::HostRequest { .. } => "HostRequest",
            Self::HostReply { .. } => "HostReply",
            Self::BlobGet { .. } => "BlobGet",
            Self::BlobPut { .. } => "BlobPut",
            Self::BlobPart { .. } => "BlobPart",
            Self::BlobDone { .. } => "BlobDone",
            Self::Ping { .. } => "Ping",
            Self::Pong { .. } => "Pong",
            Self::Bye { .. } => "Bye",
            Self::BlobCredit { .. } => "BlobCredit",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(method: &str, size: usize) -> LinkMsg {
        LinkMsg::Call {
            id: 1,
            window: None,
            device: None,
            method: method.to_owned(),
            params: vec![b' '; size],
        }
    }

    #[test]
    fn the_classes_are_the_ones_of_the_table() {
        for method in CONTROL_METHODS {
            assert_eq!(call(method, 10).class(), Class::Control, "{method}");
        }
        assert_eq!(call("pane.resize", 10).class(), Class::Normal);
        assert_eq!(call("task.create", BULK_CALL).class(), Class::Normal);
        assert_eq!(call("task.create", BULK_CALL + 1).class(), Class::Bulk);
        // A large paste stays in the order of the input.
        assert_eq!(call("pane.paste", BULK_CALL + 1).class(), Class::Control);
        let frame = LinkMsg::Frame {
            stream: 1,
            bytes: Vec::new(),
        };
        assert_eq!(frame.class(), Class::Frames);
        assert_eq!(
            LinkMsg::BlobGet {
                id: 1,
                hash: String::new()
            }
            .class(),
            Class::Bulk
        );
        assert_eq!(
            LinkMsg::Ping {
                nonce: 1,
                sent_at_ms: 2
            }
            .class(),
            Class::Control
        );
    }

    #[test]
    fn hello_welcome_and_refused_keep_their_kinds() {
        // The first byte of the body is the kind. These three never move.
        let hello = LinkMsg::Hello(Hello {
            proto: PROTO.to_owned(),
            api: "neohar/1".to_owned(),
            app_version: "0.0.0".to_owned(),
            device: Device {
                id: "d".to_owned(),
                name: "n".to_owned(),
            },
        });
        let bytes = postcard::to_allocvec(&hello).expect("encodes");
        assert_eq!(bytes[0], 0);
        let refused = LinkMsg::Refused {
            reason: REFUSED_API.to_owned(),
        };
        assert_eq!(postcard::to_allocvec(&refused).expect("encodes")[0], 2);
    }
}
