//! The `holder/2` protocol.
//!
//! The core starts a holder and sends it one [`HoldConfig`] over the config pipe. After that,
//! clients talk to the holder over its socket with [`HolderRequest`] and [`HolderEvent`], framed
//! by [`crate::framing`].
//!
//! Variants and fields are only appended, never reordered or removed. When `holder/3` appears,
//! these types are copied to `holder::v2_frozen` and both suites keep running.

use serde::{Deserialize, Serialize};

use crate::answers::QueryKind;

/// The most bytes of PTY output in one [`HolderEvent::Output`].
pub const MAX_OUTPUT_CHUNK: usize = 64 * 1024;

/// What the holder runs the child on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(test, derive(proptest_derive::Arbitrary))]
pub enum HoldMode {
    /// A pseudo terminal with this size.
    Pty {
        cols: u16,
        rows: u16,
        px_width: u16,
        px_height: u16,
    },
    /// Three pipes for a headless harness process.
    Pipe,
}

/// The kind of a [`HoldMode`], without its size.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(test, derive(proptest_derive::Arbitrary))]
pub enum HoldModeKind {
    Pty,
    Pipe,
}

/// The one message from the core to a new holder, over `--config-fd`.
///
/// The secret and the environment never appear in the holder's `argv` or environment.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(proptest_derive::Arbitrary))]
pub struct HoldConfig {
    pub proto: u16,
    /// The pane's ULID, as 16 bytes.
    pub pane: [u8; 16],
    /// The per-holder secret that a client must send in [`HolderRequest::Hello`].
    pub secret: [u8; 32],
    pub mode: HoldMode,
    pub argv: Vec<String>,
    pub env: Vec<(String, String)>,
    pub cwd: String,
    pub socket_path: String,
    pub log_path: String,
    /// Size of the output ring. The default is 16 MiB.
    pub ring_bytes: u64,
    /// Idle time before the holder asks for a checkpoint. The default is 2,000 ms.
    pub checkpoint_idle_ms: u32,
    /// Longest time between checkpoints while the pane is busy. The default is 30,000 ms.
    pub checkpoint_max_interval_ms: u32,
    /// How long a queued query stays fresh. The default is 5,000 ms.
    pub query_fresh_ms: u32,
}

/// The process that receives a [`HolderRequest::Signal`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(test, derive(proptest_derive::Arbitrary))]
pub enum SignalTarget {
    /// The child process.
    Child,
    /// The foreground process group of the PTY.
    Foreground,
}

/// A signal that a client can ask the holder to send.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(test, derive(proptest_derive::Arbitrary))]
pub enum SignalKind {
    Hup,
    Int,
    Term,
    Kill,
    Usr1,
}

/// A message from a client to the holder.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(proptest_derive::Arbitrary))]
pub enum HolderRequest {
    /// The first message on a connection. The holder checks the secret and the fencing token.
    Hello {
        proto: u16,
        secret: [u8; 32],
        fencing_token: u64,
        client: String,
    },
    Status,
    /// Replay the journal from `from_offset`, then stream new output.
    Attach {
        from_offset: u64,
    },
    /// Write bytes to the child. The holder drops a repeated `input_id` (at-most-once).
    Input {
        input_id: u64,
        bytes: Vec<u8>,
    },
    Resize {
        cols: u16,
        rows: u16,
        px_width: u16,
        px_height: u16,
    },
    /// The core stored a snapshot at this journal offset.
    CheckpointTaken {
        offset: u64,
    },
    /// The core answered the queued queries up to this sequence number.
    QueriesAnswered {
        up_to_seq: u64,
    },
    Signal {
        target: SignalTarget,
        signal: SignalKind,
    },
    /// Close the child's stdin (pipe mode).
    CloseStdin,
    /// The core received [`HolderEvent::ChildExited`]. The holder may exit.
    ExitAck,
    Detach,
    Ping {
        nonce: u64,
    },
}

/// Why a holder refused a client.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(test, derive(proptest_derive::Arbitrary))]
pub enum RefuseReason {
    BadSecret,
    /// A client with a higher fencing token is attached.
    StaleFence,
    ProtoUnsupported,
    WrongUser,
}

/// Which stream a journal byte belongs to. A PTY has one stream. Pipe mode has three.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(test, derive(proptest_derive::Arbitrary))]
pub enum StreamId {
    Pty,
    Stdin,
    Stdout,
    Stderr,
}

/// A marker in the journal. It keeps the order of output, resizes and inputs on replay.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(test, derive(proptest_derive::Arbitrary))]
pub enum Marker {
    Resize {
        cols: u16,
        rows: u16,
    },
    InputAck {
        input_id: u64,
    },
    /// A safe place to start a checkpoint or a replay.
    CutPoint,
    /// The journal switches to another stream (pipe mode).
    Stream {
        id: StreamId,
    },
}

/// Whether an input was written or was a duplicate.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(test, derive(proptest_derive::Arbitrary))]
pub enum InputResult {
    Written,
    Duplicate,
}

/// Why the holder asks for a checkpoint.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(test, derive(proptest_derive::Arbitrary))]
pub enum CheckpointReason {
    /// The bytes since the last checkpoint reached half of the ring.
    RingHalf,
    Idle,
    Interval,
}

/// How the child ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(test, derive(proptest_derive::Arbitrary))]
pub enum ExitStatus {
    /// The child exited with this code.
    Code(i32),
    /// A signal ended the child.
    Signal(i32),
    /// `exec` failed with this `errno`.
    ExecFailed(i32),
}

/// The byte range that the journal holds. All values are offsets in the journal's offset space.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(test, derive(proptest_derive::Arbitrary))]
pub struct JournalInfo {
    /// The oldest offset that is still in the ring.
    pub start: u64,
    /// The offset after the newest byte.
    pub end: u64,
    pub capacity: u64,
    /// The offset of the last checkpoint that the core acknowledged.
    pub last_checkpoint: u64,
    /// The newest safe cut point.
    pub last_cut_point: u64,
}

/// The modes that the holder tracks itself, to answer queries while no core is attached.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(proptest_derive::Arbitrary))]
pub struct TrackedModes {
    pub bracketed_paste: bool,
    pub focus_events: bool,
    /// The kitty keyboard flag stack, as the program last set it.
    pub kitty_flags_stack: Vec<u8>,
}

/// A query that the holder could not answer and holds for the core.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(proptest_derive::Arbitrary))]
pub struct QueuedQuery {
    pub seq: u64,
    /// The journal offset of the query.
    pub offset: u64,
    pub received_at_ms: i64,
    pub kind: QueryKind,
    /// The query bytes, as the program sent them.
    pub raw: Vec<u8>,
}

/// The state of a holder, as [`HolderRequest::Status`] reports it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(proptest_derive::Arbitrary))]
pub struct HolderStatus {
    pub child_pid: i32,
    pub fg_pgid: i32,
    pub exited: Option<ExitStatus>,
    pub cols: u16,
    pub rows: u16,
    pub journal: JournalInfo,
    pub tracked: TrackedModes,
    pub queued_queries: Vec<QueuedQuery>,
    pub holder_version: String,
    pub started_at_ms: i64,
}

/// A message from the holder to a client.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(proptest_derive::Arbitrary))]
pub enum HolderEvent {
    HelloOk {
        proto: u16,
        pane: [u8; 16],
        mode: HoldModeKind,
        child_pid: i32,
        journal: JournalInfo,
        fencing_token: u64,
    },
    Refused {
        reason: RefuseReason,
    },
    StatusReply(HolderStatus),
    /// Output bytes at `offset`. At most [`MAX_OUTPUT_CHUNK`] bytes.
    Output {
        offset: u64,
        stream: StreamId,
        bytes: Vec<u8>,
    },
    Marker {
        offset: u64,
        marker: Marker,
    },
    /// The requested offset is no longer in the ring. The journal now starts at `start`.
    Truncated {
        requested: u64,
        start: u64,
    },
    InputAck {
        input_id: u64,
        offset: u64,
        result: InputResult,
    },
    CheckpointWanted {
        offset: u64,
        reason: CheckpointReason,
    },
    ForegroundChanged {
        pgid: i32,
    },
    ChildExited {
        status: ExitStatus,
    },
    /// Another client with a higher fencing token took the lease.
    LeaseLost {
        by_fencing_token: u64,
    },
    Pong {
        nonce: u64,
    },
}
