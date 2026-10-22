//! The async client of the holder socket.
//!
//! The synchronous codec is in `nh_proto::framing`; this module reads and writes the same layout
//! with tokio. [`connect`] says `Hello`, waits for `HelloOk`, and then runs two tasks: a reader
//! that turns the socket into a queue of [`HolderEvent`]s, and a writer that sends the requests
//! in the order they were queued. A pane task can select on the queue, which a `read_exact` in the
//! middle of a message would not allow.

use std::{io, path::Path, time::Duration};

use nh_proto::{
    framing::{self, FramingError, MAX_MSG_BYTES},
    holder::{HoldModeKind, HolderEvent, HolderRequest, JournalInfo, RefuseReason},
    version::HOLDER_PROTO,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{
        UnixStream,
        unix::{OwnedReadHalf, OwnedWriteHalf},
    },
    sync::mpsc,
    time::timeout,
};

pub mod spawn;

/// How long a connection or the answer to `Hello` may take.
const STEP_TIMEOUT: Duration = Duration::from_secs(5);

/// The events that the reader holds before it stops reading the socket. The holder then keeps the
/// bytes in its own buffer and in its journal.
const EVENT_QUEUE: usize = 256;

/// An error from talking to a holder.
#[derive(Debug, thiserror::Error)]
pub enum HolderError {
    #[error("cannot connect to the holder at {path}: {source}")]
    Connect {
        path: std::path::PathBuf,
        source: io::Error,
    },
    #[error("the holder refused the connection: {0:?}")]
    Refused(RefuseReason),
    #[error("the holder answered Hello with {0}")]
    UnexpectedReply(String),
    #[error("the holder did not answer in time")]
    Timeout,
    #[error("the holder closed the connection")]
    Closed,
    #[error(transparent)]
    Framing(#[from] FramingError),
    #[error("i/o error: {0}")]
    Io(#[from] io::Error),
}

/// What `HelloOk` said.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HelloInfo {
    /// The protocol version that the holder speaks.
    pub proto: u16,
    pub mode: HoldModeKind,
    pub child_pid: i32,
    pub journal: JournalInfo,
    pub fencing_token: u64,
}

/// The sending end of a holder connection. Cloning is cheap.
///
/// Requests go out in the order of the calls. The queue has no limit: the requests of one pane are
/// small and a person types them, or the engine answers a terminal query.
#[derive(Clone, Debug)]
pub struct HolderTx(mpsc::UnboundedSender<HolderRequest>);

impl HolderTx {
    /// Queues a request. It fails when the connection is closed.
    pub fn send(&self, request: HolderRequest) -> Result<(), HolderError> {
        self.0.send(request).map_err(|_| HolderError::Closed)
    }

    /// Whether the connection is closed.
    pub fn is_closed(&self) -> bool {
        self.0.is_closed()
    }
}

/// A connection after `HelloOk`.
pub struct HolderLink {
    pub tx: HolderTx,
    /// The events of the holder, in order. `None` means that the connection ended.
    pub events: mpsc::Receiver<HolderEvent>,
    pub hello: HelloInfo,
}

/// Connects to the socket of a holder and says `Hello`.
///
/// `fencing_token` must be higher than any token that the holder accepted before.
pub async fn connect(
    socket: &Path,
    secret: [u8; 32],
    fencing_token: u64,
    client: &str,
) -> Result<HolderLink, HolderError> {
    let stream = timeout(STEP_TIMEOUT, UnixStream::connect(socket))
        .await
        .map_err(|_| HolderError::Timeout)?
        .map_err(|source| HolderError::Connect {
            path: socket.to_owned(),
            source,
        })?;
    let (mut reader, mut writer) = stream.into_split();
    let hello = HolderRequest::Hello {
        proto: HOLDER_PROTO,
        secret,
        fencing_token,
        client: client.to_owned(),
    };
    write_request(&mut writer, &hello).await?;
    let mut body = Vec::new();
    let first = timeout(STEP_TIMEOUT, read_event(&mut reader, &mut body))
        .await
        .map_err(|_| HolderError::Timeout)??;
    let info = match first {
        HolderEvent::HelloOk {
            proto,
            mode,
            child_pid,
            journal,
            fencing_token,
            ..
        } => HelloInfo {
            proto,
            mode,
            child_pid,
            journal,
            fencing_token,
        },
        HolderEvent::Refused { reason } => return Err(HolderError::Refused(reason)),
        other => return Err(HolderError::UnexpectedReply(format!("{other:?}"))),
    };

    let (event_tx, event_rx) = mpsc::channel(EVENT_QUEUE);
    tokio::spawn(async move {
        loop {
            match read_event(&mut reader, &mut body).await {
                Ok(event) => {
                    if event_tx.send(event).await.is_err() {
                        return;
                    }
                }
                Err(HolderError::Closed) => return,
                Err(error) => {
                    tracing::warn!(%error, "the holder connection broke");
                    return;
                }
            }
        }
    });
    let (request_tx, mut request_rx) = mpsc::unbounded_channel::<HolderRequest>();
    tokio::spawn(async move {
        while let Some(request) = request_rx.recv().await {
            if let Err(error) = write_request(&mut writer, &request).await {
                tracing::warn!(%error, "cannot write to the holder");
                return;
            }
        }
    });
    Ok(HolderLink {
        tx: HolderTx(request_tx),
        events: event_rx,
        hello: info,
    })
}

/// Reads one message. `body` is a buffer that the caller keeps between calls, so that a stream of
/// output chunks does not allocate for the framing.
async fn read_event(
    reader: &mut OwnedReadHalf,
    body: &mut Vec<u8>,
) -> Result<HolderEvent, HolderError> {
    let mut header = [0u8; 4];
    reader
        .read_exact(&mut header)
        .await
        .map_err(|e| match e.kind() {
            io::ErrorKind::UnexpectedEof => HolderError::Closed,
            _ => e.into(),
        })?;
    let len = u32::from_le_bytes(header) as usize;
    if len > MAX_MSG_BYTES {
        return Err(FramingError::TooLarge { len: len as u64 }.into());
    }
    body.resize(len, 0);
    reader.read_exact(body).await.map_err(|e| match e.kind() {
        io::ErrorKind::UnexpectedEof => HolderError::Framing(FramingError::Truncated),
        _ => e.into(),
    })?;
    let (event, rest) = postcard::take_from_bytes(body).map_err(FramingError::from)?;
    if !rest.is_empty() {
        return Err(FramingError::TrailingBytes { extra: rest.len() }.into());
    }
    Ok(event)
}

async fn write_request(
    writer: &mut OwnedWriteHalf,
    request: &HolderRequest,
) -> Result<(), HolderError> {
    let mut buf = Vec::new();
    framing::write_msg(&mut buf, request)?;
    writer.write_all(&buf).await?;
    Ok(())
}
