//! The client of a link.
//!
//! [`LinkClient`] runs over any pair of byte streams: a Unix socket ([`LinkClient::connect_unix`]),
//! or the stdio of ssh. Its methods mirror what a host needs of a [`nh_core::CoreHandle`]:
//! `call`, `subscribe`, `stream` and `blob`. It answers the host requests of the core through the
//! local [`Host`] that it gets at connect, within the limits of its [`HostLimits`]. It cleans the
//! HTML in each result and projection message again, because the core can be hostile.
//!
//! The client keeps nothing of the remote machine on disk. For a reconnect, the client host makes
//! a new `LinkClient`, and these calls fail with `link_down` until it does.

use std::{
    collections::HashMap,
    path::Path,
    sync::{
        Arc, Mutex, PoisonError,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

use nh_core::{ErrorCode, FrameSink, Host, MessageSink, RpcError};
use nh_proto::{api::ids::WindowId, version::API_VERSION};
use serde_json::{Value, json};
use tokio::{
    io::{AsyncRead, AsyncWrite},
    net::UnixStream,
    sync::{mpsc, oneshot},
};
use ulid::Ulid;

use crate::{
    codec::{MAX_FROM_CORE, MAX_MESSAGE},
    host_req::{HostGuard, HostLimits},
    msg::{BlobOutcome, Device, Hello, LinkMsg, Outcome, PROTO, Welcome},
    wire::{Incoming, LinkTiming, Sizes, Wire, start},
};

/// Who the client is.
#[derive(Clone, Debug)]
pub struct ClientHello {
    /// The device id: a ULID made at the first launch of the app.
    pub device: Ulid,
    /// The device name: the host name.
    pub name: String,
    pub app_version: String,
}

/// The settings of a client.
#[derive(Clone, Copy, Debug)]
pub struct ClientOptions {
    pub timing: LinkTiming,
    pub limits: HostLimits,
    /// The largest message from the core, before and after compression. A larger one ends the
    /// link.
    pub max_message: usize,
    /// The largest blob. A larger one fails with `too_large`.
    pub max_blob: usize,
    /// The most messages that wait to go to the core. A core that stops reading and keeps
    /// sending (pings, host requests) makes the client queue an answer to each; one more ends
    /// the link.
    pub max_queued: usize,
}

/// The largest blob that a client takes: the app keeps at most this much in its cache of blobs.
pub const MAX_BLOB: usize = 64 << 20;

impl Default for ClientOptions {
    fn default() -> Self {
        Self {
            timing: LinkTiming::default(),
            limits: HostLimits::default(),
            max_message: MAX_FROM_CORE,
            max_blob: MAX_BLOB,
            max_queued: crate::wire::MAX_QUEUED,
        }
    }
}

/// The longest texts of a `Welcome`. The app shows the version and the host name, and keeps them
/// in the projection `machines`.
const WELCOME_LIMITS: [(&str, usize); 5] = [
    ("core_version", 64),
    ("machine_uuid", 64),
    ("session_uuid", 64),
    ("hostname", 255),
    ("core_epoch", 64),
];

/// Whether each text of a `Welcome` is within its limit and has no control character.
fn welcome_fits(welcome: &Welcome) -> bool {
    let texts = [
        &welcome.core_version,
        &welcome.machine_uuid,
        &welcome.session_uuid,
        &welcome.hostname,
        &welcome.core_epoch,
    ];
    texts
        .iter()
        .zip(WELCOME_LIMITS)
        .all(|(text, (_, limit))| text.len() <= limit && !text.chars().any(char::is_control))
}

/// Why a link does not open.
#[derive(Debug, thiserror::Error)]
pub enum LinkError {
    #[error("cannot connect: {0}")]
    Io(#[from] std::io::Error),
    /// The core refused the `Hello`: `proto` or `api`.
    #[error("the core refused the link: {0}")]
    Refused(String),
    #[error("the link ended before Welcome: {0}")]
    Closed(String),
    #[error("no Welcome came in time")]
    Timeout,
    #[error("the core answered Hello with {0}")]
    Unexpected(&'static str),
}

/// The error of a call when the link is down.
pub fn link_down() -> RpcError {
    RpcError::new(ErrorCode::LinkDown, "the link to the core is down")
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// A blob: its bytes and its type, or `None` when the core has none with the hash.
pub type BlobResult = Result<Option<(Vec<u8>, String)>, RpcError>;

/// An upload on its way to the core: its credits, and where its end goes.
struct PutOut {
    credits: Arc<tokio::sync::Semaphore>,
    done: Option<oneshot::Sender<Result<String, RpcError>>>,
}

/// A blob on its way.
struct BlobIn {
    hash: String,
    bytes: Vec<u8>,
    /// `None` once the blob failed for its size: its later parts get credit and are dropped, so
    /// that the core can end it.
    done: Option<oneshot::Sender<BlobResult>>,
}

struct Inner {
    wire: Arc<Wire>,
    welcome: Welcome,
    next: AtomicU64,
    /// The calls, subscriptions and streams that wait for their `Reply`.
    pending: Mutex<HashMap<u64, oneshot::Sender<Outcome>>>,
    subs: Mutex<HashMap<u64, Arc<dyn MessageSink>>>,
    /// The id that the core gave each subscription, and its number on the link.
    sub_ids: Mutex<HashMap<String, u64>>,
    streams: Mutex<HashMap<u64, Arc<dyn FrameSink>>>,
    blobs: Mutex<HashMap<u64, BlobIn>>,
    /// The uploads on their way to the core.
    puts: Mutex<HashMap<u64, PutOut>>,
    max_blob: usize,
    guard: HostGuard,
}

/// Ends the link when the last handle goes.
struct Owner(Arc<Wire>);

impl Drop for Owner {
    fn drop(&mut self) {
        self.0.close("the client ended");
    }
}

/// A client of a core over a link. Cloning is cheap. The link ends with `Bye` when the last clone
/// drops.
#[derive(Clone)]
pub struct LinkClient {
    inner: Arc<Inner>,
    _owner: Arc<Owner>,
}

impl std::fmt::Debug for LinkClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LinkClient")
            .field("welcome", &self.inner.welcome)
            .finish_non_exhaustive()
    }
}

impl LinkClient {
    /// Opens a link on `link.sock`.
    pub async fn connect_unix(
        path: &Path,
        hello: ClientHello,
        host: Arc<dyn Host>,
        options: ClientOptions,
    ) -> Result<Self, LinkError> {
        let stream = UnixStream::connect(path).await?;
        let (read, write) = stream.into_split();
        Self::connect(read, write, hello, host, options).await
    }

    /// Opens a link over a pair of byte streams: sends `Hello` and waits for `Welcome`. Call it on
    /// a tokio runtime: the tasks of the link run there.
    pub async fn connect<R, W>(
        read: R,
        write: W,
        hello: ClientHello,
        host: Arc<dyn Host>,
        options: ClientOptions,
    ) -> Result<Self, LinkError>
    where
        R: AsyncRead + Unpin + Send + 'static,
        W: AsyncWrite + Unpin + Send + 'static,
    {
        let sizes = Sizes {
            recv: options.max_message,
            send: MAX_MESSAGE,
            queue: options.max_queued,
        };
        let (wire, mut rx) = start(read, write, options.timing, sizes);
        wire.send(&LinkMsg::Hello(Hello {
            proto: PROTO.to_owned(),
            api: API_VERSION.to_owned(),
            app_version: hello.app_version,
            device: Device {
                id: hello.device.to_string(),
                name: hello.name,
            },
        }));
        let guard = HostGuard::new(host, options.limits);
        // One time for the whole wait: messages that this version cannot read do not extend it.
        let deadline = tokio::time::Instant::now() + options.timing.hello_within;
        let welcome = loop {
            let first = tokio::time::timeout_at(deadline, rx.recv()).await;
            match first {
                Ok(Some(Incoming::Msg(LinkMsg::Welcome(welcome)))) => {
                    if !welcome_fits(&welcome) {
                        wire.close("a text of Welcome is over its limit");
                        return Err(LinkError::Unexpected(
                            "a Welcome with a text over its limit",
                        ));
                    }
                    break welcome;
                }
                Ok(Some(Incoming::Msg(LinkMsg::Refused { reason }))) => {
                    wire.set_down("refused");
                    return Err(LinkError::Refused(reason));
                }
                // The core sends its host requests after `Welcome`, so another message is a
                // mistake of the core.
                Ok(Some(Incoming::Msg(other))) => {
                    wire.close("the first message is Welcome");
                    return Err(LinkError::Unexpected(other.kind()));
                }
                Ok(Some(Incoming::Unreadable)) => {}
                Ok(None) => return Err(LinkError::Closed(wire.wait_down().await)),
                Err(_) => {
                    wire.close("no Welcome came in time");
                    return Err(LinkError::Timeout);
                }
            }
        };
        let inner = Arc::new(Inner {
            wire: Arc::clone(&wire),
            welcome,
            next: AtomicU64::new(1),
            pending: Mutex::default(),
            subs: Mutex::default(),
            sub_ids: Mutex::default(),
            streams: Mutex::default(),
            blobs: Mutex::default(),
            puts: Mutex::default(),
            max_blob: options.max_blob,
            guard,
        });
        tokio::spawn(dispatch(Arc::clone(&inner), rx, options.timing.ping_every));
        Ok(Self {
            inner,
            _owner: Arc::new(Owner(wire)),
        })
    }

    /// What the core said in `Welcome`.
    pub fn welcome(&self) -> &Welcome {
        &self.inner.welcome
    }

    /// The last round-trip time of a ping, if one came back.
    pub fn rtt(&self) -> Option<Duration> {
        self.inner.wire.rtt()
    }

    /// Whether the link is up.
    pub fn is_up(&self) -> bool {
        !self.inner.wire.is_down()
    }

    /// Waits until the link is down and returns the reason.
    pub async fn closed(&self) -> String {
        self.inner.wire.wait_down().await
    }

    /// The wire of the link, for a task that waits for its end. It does not keep the link open:
    /// only a [`LinkClient`] does.
    pub(crate) fn wire(&self) -> Arc<Wire> {
        Arc::clone(&self.inner.wire)
    }

    /// Ends the link with `Bye`.
    pub fn close(&self, reason: &str) {
        self.inner.wire.close(reason);
    }

    /// The windows that this client owns. A window request of the core for another
    /// window is refused.
    pub fn set_owned_windows(&self, windows: impl IntoIterator<Item = WindowId>) {
        self.inner.guard.set_owned(windows);
    }

    /// Adds a window to the windows that this client owns, or removes it.
    pub fn own_window(&self, window: WindowId, owned: bool) {
        self.inner.guard.own(window, owned);
    }

    /// The host of this client, which answers the host requests of the core.
    pub fn host(&self) -> &Arc<dyn Host> {
        self.inner.guard.host()
    }

    fn next_id(&self) -> u64 {
        self.inner.next.fetch_add(1, Ordering::Relaxed)
    }

    /// Sends a message that gets a `Reply` with the id `id`, and waits for it.
    async fn ask(&self, id: u64, msg: LinkMsg) -> Result<Value, RpcError> {
        let (tx, rx) = oneshot::channel();
        lock(&self.inner.pending).insert(id, tx);
        if !self.inner.wire.send(&msg) {
            lock(&self.inner.pending).remove(&id);
            return Err(link_down());
        }
        match rx.await {
            Ok(Outcome::Ok(bytes)) => serde_json::from_slice(&bytes)
                .map(untrusted)
                .map_err(|e| RpcError::internal(e.to_string())),
            Ok(Outcome::Err(bytes)) => Err(error_of(&bytes)),
            Err(_) => Err(link_down()),
        }
    }

    /// Calls a method of the core. `window` is the window of the webview that
    /// made the call. `projection.unsubscribe {subscription}` ends a subscription of this client,
    /// as `CoreHandle::call_as` does for the UI.
    pub async fn call(
        &self,
        window: Option<WindowId>,
        method: &str,
        params: Value,
    ) -> Result<Value, RpcError> {
        self.call_for(None, window, method, params).await
    }

    /// [`LinkClient::call`] for a phone that this client relays. The core
    /// puts the device of the phone in the origin of the call.
    pub async fn call_for(
        &self,
        device: Option<Device>,
        window: Option<WindowId>,
        method: &str,
        params: Value,
    ) -> Result<Value, RpcError> {
        if method == "projection.unsubscribe" {
            let subscription = params
                .get("subscription")
                .and_then(Value::as_str)
                .ok_or_else(|| RpcError::invalid_params("subscription is a string"))?;
            return if self.unsubscribe(subscription) {
                Ok(json!({}))
            } else {
                Err(RpcError::new(
                    ErrorCode::NotFound,
                    format!("no subscription {subscription}"),
                ))
            };
        }
        // A call of the person at this Mac (not of a relayed phone) can let the core write the
        // clipboard or focus a window once, just after it.
        if device.is_none() {
            self.inner.guard.local_call(method, &params);
        }
        let id = self.next_id();
        let params = serde_json::to_vec(&params).map_err(|e| RpcError::internal(e.to_string()))?;
        self.ask(
            id,
            LinkMsg::Call {
                id,
                window: window.map(|w| w.to_string()),
                device,
                method: method.to_owned(),
                params,
            },
        )
        .await
    }

    /// Subscribes to a projection. The sink gets the snapshot and then
    /// the patches. Returns the id of the subscription, which `projection.unsubscribe` takes.
    pub async fn subscribe(
        &self,
        name: &str,
        since: Option<u64>,
        sink: Box<dyn MessageSink>,
    ) -> Result<String, RpcError> {
        let sub = self.next_id();
        // The snapshot can come before the reply.
        lock(&self.inner.subs).insert(sub, Arc::from(sink));
        let answer = self
            .ask(
                sub,
                LinkMsg::Subscribe {
                    sub,
                    name: name.to_owned(),
                    since,
                },
            )
            .await;
        match answer {
            Ok(value) => {
                let id = value["subscription"]
                    .as_str()
                    .map_or_else(|| format!("{name}#link{sub}"), str::to_owned);
                lock(&self.inner.sub_ids).insert(id.clone(), sub);
                Ok(id)
            }
            Err(error) => {
                lock(&self.inner.subs).remove(&sub);
                Err(error)
            }
        }
    }

    /// Ends a subscription of this client. Returns whether it existed.
    pub fn unsubscribe(&self, subscription: &str) -> bool {
        let Some(sub) = lock(&self.inner.sub_ids).remove(subscription) else {
            return false;
        };
        lock(&self.inner.subs).remove(&sub);
        self.inner.wire.send(&LinkMsg::Unsubscribe { sub });
        true
    }

    /// Opens a stream. The frames go to the sink until it says that its
    /// receiver is gone, or the link ends.
    pub async fn stream(
        &self,
        kind: &str,
        params: Value,
        window: Option<WindowId>,
        sink: Arc<dyn FrameSink>,
    ) -> Result<(), RpcError> {
        self.stream_for(None, kind, params, window, sink).await
    }

    /// [`LinkClient::stream`] for a phone that this client relays. The core attaches the phone as
    /// a client while a stream of it is open.
    pub async fn stream_for(
        &self,
        device: Option<Device>,
        kind: &str,
        params: Value,
        window: Option<WindowId>,
        sink: Arc<dyn FrameSink>,
    ) -> Result<(), RpcError> {
        let stream = self.next_id();
        lock(&self.inner.streams).insert(stream, sink);
        let params = serde_json::to_vec(&params).map_err(|e| RpcError::internal(e.to_string()))?;
        let answer = self
            .ask(
                stream,
                LinkMsg::Stream {
                    stream,
                    kind: kind.to_owned(),
                    params,
                    window: window.map(|w| w.to_string()),
                    device,
                },
            )
            .await;
        if answer.is_err() {
            lock(&self.inner.streams).remove(&stream);
        }
        answer.map(|_| ())
    }

    /// Puts a file in the blob store of the core and returns its hash. The
    /// bytes go in parts of [`crate::BLOB_PART`], with at most [`crate::BLOB_CREDITS`] parts in
    /// flight: the core gives a credit for each part that it wrote. A file over the limit of
    /// [`nh_core::UPLOAD_MAX_BYTES`] fails before the first byte goes. The core checks the
    /// digest, and this call checks that the hash it answers is the digest of the bytes.
    pub async fn put_blob(&self, name: &str, bytes: &[u8]) -> Result<String, RpcError> {
        if bytes.len() as u64 > nh_core::UPLOAD_MAX_BYTES {
            return Err(RpcError::new(
                ErrorCode::TooLarge,
                format!("a file has at most {} bytes", nh_core::UPLOAD_MAX_BYTES),
            ));
        }
        let hash = blake3::hash(bytes).to_hex().to_string();
        let id = self.next_id();
        let credits = Arc::new(tokio::sync::Semaphore::new(crate::server::BLOB_CREDITS));
        let (done, finished) = oneshot::channel();
        lock(&self.inner.puts).insert(
            id,
            PutOut {
                credits: Arc::clone(&credits),
                done: Some(done),
            },
        );
        let sent = self.inner.wire.send(&LinkMsg::BlobPut {
            id,
            name: name.to_owned(),
            size: bytes.len() as u64,
            hash: hash.clone(),
        });
        if !sent {
            lock(&self.inner.puts).remove(&id);
            return Err(link_down());
        }
        let parts: Vec<&[u8]> = if bytes.is_empty() {
            vec![&[][..]]
        } else {
            bytes.chunks(crate::server::BLOB_PART).collect()
        };
        let count = parts.len();
        let mut finished = std::pin::pin!(finished);
        for (n, part) in parts.into_iter().enumerate() {
            // A refusal of the core ends the upload with an answer and no credit, and the end of
            // the link closes the wait.
            let permit = tokio::select! {
                permit = credits.acquire() => permit,
                early = &mut finished => return Self::put_answer(early, &hash),
                _ = self.inner.wire.wait_down() => {
                    lock(&self.inner.puts).remove(&id);
                    return Err(link_down());
                }
            };
            let Ok(permit) = permit else { break };
            permit.forget();
            if !self.inner.wire.send(&LinkMsg::BlobPart {
                id,
                offset: (n * crate::server::BLOB_PART) as u64,
                bytes: part.to_vec(),
                last: n + 1 == count,
            }) {
                lock(&self.inner.puts).remove(&id);
                return Err(link_down());
            }
        }
        Self::put_answer((&mut finished).await, &hash)
    }

    /// The end of an upload: the hash that the core answers must be the digest of the bytes.
    fn put_answer(
        answer: Result<Result<String, RpcError>, oneshot::error::RecvError>,
        hash: &str,
    ) -> Result<String, RpcError> {
        let stored = answer.unwrap_or_else(|_| Err(link_down()))?;
        if stored == hash {
            Ok(stored)
        } else {
            Err(RpcError::new(
                ErrorCode::BlobCorrupt,
                "the core stored other bytes than the ones sent",
            ))
        }
    }

    /// A blob of the core: its bytes and its type. `None` when the core has no blob with this
    /// hash. The client checks the BLAKE3 digest of the bytes.
    pub async fn blob(&self, hash: &str) -> BlobResult {
        let id = self.next_id();
        let (done, rx) = oneshot::channel();
        lock(&self.inner.blobs).insert(
            id,
            BlobIn {
                hash: hash.to_ascii_lowercase(),
                bytes: Vec::new(),
                done: Some(done),
            },
        );
        if !self.inner.wire.send(&LinkMsg::BlobGet {
            id,
            hash: hash.to_owned(),
        }) {
            lock(&self.inner.blobs).remove(&id);
            return Err(link_down());
        }
        rx.await.unwrap_or_else(|_| Err(link_down()))
    }
}

/// A result or a projection message of the core, as the client passes it on. The core can be on
/// another machine, and the webview inserts the text of each field `html` as HTML. Thus the client
/// cleans each one again with the rules of the core (`nh_rich::markdown::clean`), and a core
/// cannot put a form, a style or a refresh into a window of this Mac.
fn untrusted(mut value: Value) -> Value {
    nh_core::rich::markdown::clean_html_fields(&mut value);
    value
}

/// The error in the JSON of a `Reply`.
fn error_of(bytes: &[u8]) -> RpcError {
    let value: Value = serde_json::from_slice(bytes).unwrap_or(Value::Null);
    RpcError::from_wire(
        value["code"].as_str().unwrap_or("internal"),
        value["message"]
            .as_str()
            .unwrap_or("the core sent no message"),
        value.get("details").cloned().unwrap_or(Value::Null),
    )
}

/// Reads the messages of the core until the link ends.
async fn dispatch(inner: Arc<Inner>, mut rx: mpsc::UnboundedReceiver<Incoming>, sweep: Duration) {
    // A stream whose receiver is gone sends nothing, so the client asks the sinks now and then.
    let mut sweeper = tokio::time::interval(sweep.min(Duration::from_secs(1)));
    loop {
        let incoming = tokio::select! {
            incoming = rx.recv() => incoming,
            _ = sweeper.tick() => {
                sweep_streams(&inner);
                continue;
            }
        };
        let Some(incoming) = incoming else {
            break;
        };
        let Incoming::Msg(msg) = incoming else {
            continue;
        };
        handle(&inner, msg);
    }
    // The link is down: every call that waits gets `link_down`.
    lock(&inner.pending).clear();
    lock(&inner.subs).clear();
    lock(&inner.sub_ids).clear();
    lock(&inner.streams).clear();
    lock(&inner.puts).clear();
    for (_, blob) in lock(&inner.blobs).drain() {
        if let Some(done) = blob.done {
            let _ = done.send(Err(link_down()));
        }
    }
}

fn sweep_streams(inner: &Inner) {
    let closed: Vec<u64> = lock(&inner.streams)
        .iter()
        .filter(|(_, sink)| sink.is_closed())
        .map(|(id, _)| *id)
        .collect();
    for stream in closed {
        lock(&inner.streams).remove(&stream);
        inner.wire.send(&LinkMsg::StreamClose { stream });
    }
}

fn handle(inner: &Inner, msg: LinkMsg) {
    match msg {
        LinkMsg::Reply { id, outcome } => {
            if let Some(tx) = lock(&inner.pending).remove(&id) {
                let _ = tx.send(outcome);
            }
        }
        LinkMsg::Projection { sub, message } => {
            let sink = lock(&inner.subs).get(&sub).cloned();
            let Some(sink) = sink else {
                return;
            };
            let Ok(message) = serde_json::from_slice::<Value>(&message) else {
                tracing::debug!("a projection message of the link is not JSON");
                return;
            };
            if !sink.send(untrusted(message)) {
                lock(&inner.subs).remove(&sub);
                lock(&inner.sub_ids).retain(|_, s| *s != sub);
                inner.wire.send(&LinkMsg::Unsubscribe { sub });
            }
        }
        LinkMsg::Frame { stream, bytes } => {
            let sink = lock(&inner.streams).get(&stream).cloned();
            if let Some(sink) = sink
                && !sink.send(bytes)
            {
                lock(&inner.streams).remove(&stream);
                inner.wire.send(&LinkMsg::StreamClose { stream });
            }
        }
        LinkMsg::HostRequest { id, request } => {
            let result = inner.guard.handle(&request);
            inner.wire.send(&LinkMsg::HostReply { id, result });
        }
        LinkMsg::BlobPart {
            id, offset, bytes, ..
        } => {
            let mut blobs = lock(&inner.blobs);
            let Some(blob) = blobs.get_mut(&id) else {
                return;
            };
            if blob.done.is_none() {
                // The blob failed for its size: the part is dropped.
            } else if offset != blob.bytes.len() as u64 {
                if let Some(done) = blobs.remove(&id).and_then(|blob| blob.done) {
                    let _ = done.send(Err(RpcError::internal(
                        "a part of a blob came out of order",
                    )));
                }
                return;
            } else if blob.bytes.len() + bytes.len() > inner.max_blob {
                if let Some(done) = blob.done.take() {
                    let _ = done.send(Err(RpcError::new(
                        ErrorCode::TooLarge,
                        format!("a blob has at most {} bytes over a link", inner.max_blob),
                    )));
                }
                blob.bytes = Vec::new();
            } else {
                blob.bytes.extend_from_slice(&bytes);
            }
            drop(blobs);
            // The part is written: the core may send one more.
            inner.wire.send(&LinkMsg::BlobCredit { id, parts: 1 });
        }
        LinkMsg::BlobCredit { id, parts } => {
            if let Some(put) = lock(&inner.puts).get(&id) {
                put.credits
                    .add_permits((parts as usize).min(crate::server::BLOB_CREDITS));
            }
        }
        LinkMsg::BlobDone { id, outcome } if lock(&inner.puts).contains_key(&id) => {
            let Some(done) = lock(&inner.puts).remove(&id).and_then(|put| put.done) else {
                return;
            };
            let _ = done.send(match outcome {
                BlobOutcome::Ok { hash, .. } => Ok(hash),
                BlobOutcome::Err(bytes) => Err(error_of(&bytes)),
            });
        }
        LinkMsg::BlobDone { id, outcome } => {
            let Some(blob) = lock(&inner.blobs).remove(&id) else {
                return;
            };
            let Some(done) = blob.done else {
                return;
            };
            let result = match outcome {
                BlobOutcome::Ok { mime, .. } => {
                    // The digest of the bytes, not the word of the core, decides.
                    let digest = blake3::hash(&blob.bytes).to_hex();
                    if digest.as_str() == blob.hash {
                        Ok(Some((blob.bytes, mime)))
                    } else {
                        Err(RpcError::new(
                            ErrorCode::BlobCorrupt,
                            "the bytes of the blob do not match its hash",
                        ))
                    }
                }
                BlobOutcome::Err(bytes) => {
                    let error = error_of(&bytes);
                    if error.code == ErrorCode::NotFound {
                        Ok(None)
                    } else {
                        Err(error)
                    }
                }
            };
            let _ = done.send(result);
        }
        other => tracing::debug!(kind = other.kind(), "a core sent a message for a core"),
    }
}
