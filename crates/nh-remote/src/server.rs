//! The server of `link.sock`. Both hosts of a core start it: the app and
//! `nh daemon`.
//!
//! The socket has mode 0600 in the 0700 runtime directory of the session, and the server checks
//! the UID of each peer, as the API socket does. A link thus gets the scope `ui`.
//!
//! A connection starts with `Hello`. The server answers `Welcome`, or `Refused {proto | api}` and
//! the end. Then:
//!
//! - **Calls** run as `Origin::ui()` with the window of the call and the device of the client.
//!   The class-0 calls of one link run one after the other, in the order that they
//!   came, because the order of input matters. The other calls run at the same time and reply when
//!   they are done. The calls wait until the host of the link is attached (see below),
//!   because a call such as `pane.copy` needs the host of its device. An answer larger than
//!   [`MAX_FROM_CORE`] becomes the error `too_large`.
//! - **Subscriptions** and **streams** send `Projection` and `Frame`. They all end with the link.
//! - **A relayed phone**. A call with `device` runs as that phone: the
//!   origin has the scope `phone` and its device, so `phone.window` gives the window of the phone
//!   and the refusals of a phone hold. The other `phone.*` methods are for the Mac that the phone
//!   is paired with, and they answer `unsupported` here. A stream with `device` attaches the phone
//!   as a client, with a host that does nothing, until its last stream on the link closes.
//! - **Blobs**: `BlobGet` sends the blob in parts of 64 KiB with at most 8 parts in flight.
//!   `BlobPut` receives an upload in parts, with a credit for each part. A link has at most 4
//!   uploads at the same time. The last part ends the upload: the server checks the size and the
//!   hash, and the core stores the file (`put_upload`). `BlobDone` gives the result.
//! - **Host requests**: after `Welcome` the server asks the client for its notification
//!   capabilities, then attaches a [`LinkHost`] to the core with [`CoreHandle::attach_client`].
//!   The attachment lives as long as the link. When the link ends, the render streams of the link
//!   say that they are closed, and the render server drops them at the next pass of their windows.

use std::{
    collections::HashMap,
    io,
    os::unix::fs::{FileTypeExt, MetadataExt, PermissionsExt},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex, PoisonError,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::Duration,
};

use nh_core::{
    ClientInfo, CoreHandle, ErrorCode, FrameSink, Host, MessageSink, Origin, RpcError,
    host::{
        FolderChosen, FolderPick, HostNotification, NotificationCapabilities, TrayModel, WindowSpec,
    },
};
use nh_proto::{
    api::ids::{NotificationId, WindowId},
    paths::Env,
    version::API_VERSION,
};
use serde_json::{Value, json};
use tokio::{
    net::{UnixListener, UnixStream},
    sync::{Semaphore, mpsc, oneshot},
};
use ulid::Ulid;

use crate::{
    codec::{MAX_FROM_CORE, MAX_MESSAGE},
    host_req::{HostRequest, Notification, Tray, Window},
    msg::{
        BlobOutcome, Device, HostOutcome, LinkMsg, Outcome, PROTO, REFUSED_API, REFUSED_PROTO,
        Welcome, is_control_method,
    },
    wire::{Incoming, LinkTiming, Sizes, Wire, start},
};

/// The size of one part of a blob.
pub const BLOB_PART: usize = 64 * 1024;

/// The most parts of one blob in flight.
pub const BLOB_CREDITS: usize = 8;

/// How long the server waits for the notification capabilities of a client.
const CAPABILITIES_WITHIN: Duration = Duration::from_secs(5);

/// Why the server does not start.
#[derive(Debug, thiserror::Error)]
pub enum ServeError {
    #[error(transparent)]
    Paths(#[from] nh_proto::paths::PathsError),
    #[error("cannot prepare {path}: {source}")]
    Io { path: PathBuf, source: io::Error },
    #[error("another core serves {0}")]
    InUse(PathBuf),
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// A link that is open, as [`LinkServer::links`] shows it.
#[derive(Clone, Debug)]
pub struct OpenLink {
    pub client: ClientInfo,
    /// The host that sends the requests of the [`Host`] trait to this client.
    pub host: Arc<LinkHost>,
}

/// A connection, with its client once the client is attached.
type Connection = (Arc<Wire>, Option<OpenLink>);

#[derive(Default)]
struct Shared {
    next: AtomicU64,
    conns: Mutex<HashMap<u64, Connection>>,
}

/// The server of `link.sock`. Dropping it stops the accept loop, ends every link with `Bye` and
/// removes the socket file, if it is still the one that this server made.
pub struct LinkServer {
    path: PathBuf,
    inode: u64,
    shared: Arc<Shared>,
    accept: tokio::task::JoinHandle<()>,
}

impl std::fmt::Debug for LinkServer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LinkServer")
            .field("path", &self.path)
            .finish_non_exhaustive()
    }
}

impl LinkServer {
    /// The path of the socket.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// A handle that closes every link of this server, for the tests of the reconnect.
    pub fn dropper(&self) -> LinkDropper {
        LinkDropper(Arc::clone(&self.shared))
    }

    /// The links that said `Hello` and got `Welcome`.
    pub fn links(&self) -> Vec<OpenLink> {
        lock(&self.shared.conns)
            .values()
            .filter_map(|(_, link)| link.clone())
            .collect()
    }
}

/// Closes the links of a [`LinkServer`] and leaves the server listening: a client sees a link that
/// ends and opens a new one.
#[derive(Clone)]
pub struct LinkDropper(Arc<Shared>);

impl LinkDropper {
    /// Closes each link with `Bye` and `reason`.
    pub fn drop_links(&self, reason: &str) {
        for (wire, _) in lock(&self.0.conns).values() {
            wire.close(reason);
        }
    }
}

impl Drop for LinkServer {
    fn drop(&mut self) {
        self.accept.abort();
        for (wire, _) in lock(&self.shared.conns).values() {
            wire.close("the core stops");
        }
        let ours = std::fs::symlink_metadata(&self.path).is_ok_and(|m| m.ino() == self.inode);
        if ours {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

/// Binds `link.sock` of `session` and serves it on the runtime of the core. Call it after
/// `Core::start`, which made the runtime directory.
pub fn serve(core: CoreHandle, paths: &Env, session: &str) -> Result<LinkServer, ServeError> {
    serve_with(core, paths, session, LinkTiming::default())
}

/// Like [`serve`], with other times for the pings and the handshake.
pub fn serve_with(
    core: CoreHandle,
    paths: &Env,
    session: &str,
    timing: LinkTiming,
) -> Result<LinkServer, ServeError> {
    let path = paths.link_socket(session)?;
    let dir = paths.runtime_dir(session)?;
    let io = |path: &Path| {
        let path = path.to_owned();
        move |source| ServeError::Io { path, source }
    };
    {
        use std::os::unix::fs::DirBuilderExt;
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(&dir)
            .map_err(io(&dir))?;
    }
    remove_stale(&path)?;
    let listener = std::os::unix::net::UnixListener::bind(&path).map_err(io(&path))?;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).map_err(io(&path))?;
    listener.set_nonblocking(true).map_err(io(&path))?;
    let inode = std::fs::symlink_metadata(&path).map_err(io(&path))?.ino();
    let runtime = core.runtime();
    let listener = {
        let _guard = runtime.enter();
        UnixListener::from_std(listener).map_err(io(&path))?
    };
    let shared = Arc::new(Shared::default());
    let accept = runtime.spawn(accept_loop(
        listener,
        paths.uid,
        core,
        Arc::clone(&shared),
        timing,
    ));
    Ok(LinkServer {
        path,
        inode,
        shared,
        accept,
    })
}

/// Removes a socket file that no core listens on. Refuses a live socket and a file that is no
/// socket.
fn remove_stale(path: &Path) -> Result<(), ServeError> {
    let error = |source| ServeError::Io {
        path: path.to_owned(),
        source,
    };
    match std::fs::symlink_metadata(path) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(error(e)),
        Ok(meta) if !meta.file_type().is_socket() => Err(error(io::Error::other(
            "a file that is not a socket is in the way",
        ))),
        Ok(_) => match std::os::unix::net::UnixStream::connect(path) {
            Ok(_) => Err(ServeError::InUse(path.to_owned())),
            Err(e) if e.kind() == io::ErrorKind::ConnectionRefused => {
                std::fs::remove_file(path).map_err(error)
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(error(e)),
        },
    }
}

async fn accept_loop(
    listener: UnixListener,
    uid: u32,
    core: CoreHandle,
    shared: Arc<Shared>,
    timing: LinkTiming,
) {
    loop {
        match listener.accept().await {
            Ok((stream, _)) => {
                let peer = stream.peer_cred();
                if peer.as_ref().is_ok_and(|cred| cred.uid() == uid) {
                    tokio::spawn(connection(
                        core.clone(),
                        stream,
                        Arc::clone(&shared),
                        timing,
                    ));
                } else if peer.is_err() {
                    tracing::debug!("a client left link.sock before its user was read");
                } else {
                    tracing::warn!("a client of another user connected to link.sock");
                }
            }
            Err(error) => {
                tracing::warn!(%error, "accept on link.sock failed");
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        }
    }
}

/// The major version of an API version: `neohar/1` of `neohar/1.4`.
fn api_major(version: &str) -> &str {
    version.split('.').next().unwrap_or(version)
}

/// The JSON bytes of an error.
pub(crate) fn error_bytes(error: &RpcError) -> Vec<u8> {
    serde_json::to_vec(error).unwrap_or_default()
}

fn outcome_of(result: Result<Value, RpcError>) -> Outcome {
    match result {
        Ok(value) => Outcome::Ok(serde_json::to_vec(&value).unwrap_or_default()),
        Err(error) => Outcome::Err(error_bytes(&error)),
    }
}

/// The JSON of the parameters of a call or a stream. No bytes are `null`.
fn params_of(bytes: &[u8]) -> Result<Value, RpcError> {
    if bytes.is_empty() {
        return Ok(Value::Null);
    }
    serde_json::from_slice(bytes).map_err(|e| RpcError::invalid_params(e.to_string()))
}

async fn connection(core: CoreHandle, stream: UnixStream, shared: Arc<Shared>, timing: LinkTiming) {
    let (read, write) = stream.into_split();
    let sizes = Sizes {
        recv: MAX_MESSAGE,
        send: MAX_FROM_CORE,
    };
    let (wire, mut rx) = start(read, write, timing, sizes);
    let id = shared.next.fetch_add(1, Ordering::Relaxed);
    lock(&shared.conns).insert(id, (Arc::clone(&wire), None));
    if let Some(conn) = handshake(&core, &wire, &mut rx, timing).await {
        let conn = Arc::new(conn);
        let attached = attach(&core, &wire, &conn, &shared, id);
        serve_link(&conn, &mut rx, attached).await;
        conn.end();
    }
    lock(&shared.conns).remove(&id);
    wire.set_down("the link ended");
}

/// Reads `Hello` and answers `Welcome` or `Refused`. `None` when the link ends here.
async fn handshake(
    core: &CoreHandle,
    wire: &Arc<Wire>,
    rx: &mut mpsc::UnboundedReceiver<Incoming>,
    timing: LinkTiming,
) -> Option<Conn> {
    let refuse = |reason: &str| {
        wire.send(&LinkMsg::Refused {
            reason: reason.to_owned(),
        });
        wire.close("refused");
    };
    let hello = match tokio::time::timeout(timing.hello_within, rx.recv()).await {
        Ok(Some(Incoming::Msg(LinkMsg::Hello(hello)))) => hello,
        // A `Hello` of another protocol may not decode as this one.
        Ok(Some(Incoming::Unreadable)) => {
            refuse(REFUSED_PROTO);
            return None;
        }
        Ok(Some(Incoming::Msg(other))) => {
            tracing::debug!(kind = other.kind(), "a link did not start with Hello");
            wire.close("the first message is Hello");
            return None;
        }
        Ok(None) => return None,
        Err(_) => {
            wire.close("no Hello came in time");
            return None;
        }
    };
    if hello.proto != PROTO {
        refuse(REFUSED_PROTO);
        return None;
    }
    if api_major(&hello.api) != api_major(API_VERSION) {
        refuse(REFUSED_API);
        return None;
    }
    let Ok(device) = hello.device.id.parse::<Ulid>() else {
        wire.close("the device id is not a ULID");
        return None;
    };
    let welcome = match welcome(core).await {
        Ok(welcome) => welcome,
        Err(error) => {
            tracing::warn!(%error, "the core cannot say who it is to a link");
            wire.close("the core cannot answer");
            return None;
        }
    };
    wire.send(&LinkMsg::Welcome(welcome));
    tracing::info!(device = %device, name = hello.device.name, app = hello.app_version, "a client linked");
    Some(Conn {
        core: core.clone(),
        wire: Arc::clone(wire),
        info: ClientInfo {
            device,
            name: hello.device.name,
        },
        host: Arc::new(LinkHost::new(Arc::clone(wire))),
        subs: Mutex::default(),
        streams: Mutex::default(),
        phones: Mutex::default(),
        puts: Mutex::default(),
        blobs: Mutex::default(),
        control: Mutex::new(None),
    })
}

/// What the core says about itself in `Welcome`: `session.info` and `machine.info`.
async fn welcome(core: &CoreHandle) -> Result<Welcome, RpcError> {
    let session = core
        .call_as(Origin::ui(), "session.info", json!({}))
        .await?;
    let machine = core
        .call_as(Origin::ui(), "machine.info", json!({}))
        .await?;
    let text = |value: &Value| value.as_str().unwrap_or_default().to_owned();
    Ok(Welcome {
        core_version: text(&session["version"]),
        machine_uuid: text(&session["machine_uuid"]),
        session_uuid: text(&session["session_uuid"]),
        hostname: text(&machine["hostname"]),
        core_epoch: format!("{}-{}", text(&session["started_at"]), session["pid"]),
    })
}

/// Asks the client for its capabilities, then attaches its host to the core until the link ends.
/// The receiver gets a value when the host is attached, and an error when the link ended first.
fn attach(
    core: &CoreHandle,
    wire: &Arc<Wire>,
    conn: &Arc<Conn>,
    shared: &Arc<Shared>,
    id: u64,
) -> oneshot::Receiver<()> {
    let (core, wire, conn, shared) = (
        core.clone(),
        Arc::clone(wire),
        Arc::clone(conn),
        Arc::clone(shared),
    );
    let (done, attached) = oneshot::channel();
    tokio::spawn(async move {
        match tokio::time::timeout(
            CAPABILITIES_WITHIN,
            conn.host.ask(&HostRequest::NotificationCapabilities),
        )
        .await
        {
            Ok(Some(HostOutcome::Capabilities { actions })) => {
                conn.host.actions.store(actions, Ordering::Relaxed);
            }
            other => tracing::debug!(
                ?other,
                "a client did not tell its notification capabilities"
            ),
        }
        if wire.is_down() {
            return;
        }
        let host: Arc<dyn Host> = conn.host.clone();
        let attachment = core.attach_client(conn.info.clone(), host);
        if let Some((_, link)) = lock(&shared.conns).get_mut(&id) {
            *link = Some(OpenLink {
                client: conn.info.clone(),
                host: Arc::clone(&conn.host),
            });
        }
        let _ = done.send(());
        wire.wait_down().await;
        drop(attachment);
    });
    attached
}

/// A call of a link, as it waits for the attach or in the queue of class 0.
struct LinkCall {
    id: u64,
    window: Option<String>,
    device: Option<Device>,
    method: String,
    params: Vec<u8>,
}

/// A stream of a link: whether it was closed, and the phone that it was relayed for.
struct StreamIn {
    closed: Arc<AtomicBool>,
    phone: Option<Ulid>,
}

/// A relayed phone that is attached: its streams that are open, and its attachment.
struct PhoneIn {
    streams: usize,
    _attachment: nh_core::host::ClientAttachment,
}

/// The device of a relayed phone, as a ULID.
fn phone_device(device: &Device) -> Result<Ulid, RpcError> {
    device
        .id
        .parse::<Ulid>()
        .map_err(|e| RpcError::invalid_params(format!("the device of a phone: {e}")))
}

/// One link after `Welcome`.
struct Conn {
    core: CoreHandle,
    wire: Arc<Wire>,
    info: ClientInfo,
    host: Arc<LinkHost>,
    /// The subscriptions of the link: the number of the client and the id of the core.
    subs: Mutex<HashMap<u64, String>>,
    /// The streams of the link.
    streams: Mutex<HashMap<u64, StreamIn>>,
    /// The relayed phones that a stream of the link attaches.
    phones: Mutex<HashMap<Ulid, PhoneIn>>,
    /// The credits of the blobs on their way to the client.
    blobs: Mutex<HashMap<u64, Arc<Semaphore>>>,
    /// The blobs on their way from the client (`BlobPut`).
    puts: Mutex<HashMap<u64, PutIn>>,
    /// The queue of the class-0 calls. The task that runs them starts with the first one.
    control: Mutex<Option<mpsc::UnboundedSender<LinkCall>>>,
}

impl Conn {
    fn origin(&self, window: Option<&str>, device: Option<&Device>) -> Result<Origin, RpcError> {
        let origin = match device {
            Some(device) => Origin::phone(phone_device(device)?),
            None => Origin::ui().from_device(self.info.device),
        };
        match window {
            None => Ok(origin),
            Some(text) => text
                .parse::<WindowId>()
                .map(|window| origin.from_window(window))
                .map_err(|e| RpcError::invalid_params(e.to_string())),
        }
    }

    async fn run_call(&self, call: &LinkCall) -> Outcome {
        let run = async {
            if call.device.is_some()
                && call.method.starts_with("phone.")
                && call.method != "phone.window"
            {
                return Err(RpcError::new(
                    ErrorCode::Unsupported,
                    format!(
                        "{} runs on the Mac that the phone is paired with",
                        call.method
                    ),
                ));
            }
            let origin = self.origin(call.window.as_deref(), call.device.as_ref())?;
            let params = params_of(&call.params)?;
            self.core.call_as(origin, &call.method, params).await
        };
        outcome_of(run.await)
    }

    /// A stream of a relayed phone opened: the phone is a client while one is open.
    fn phone_stream_opened(&self, device: &Device) -> Result<Ulid, RpcError> {
        let id = phone_device(device)?;
        let mut phones = lock(&self.phones);
        match phones.get_mut(&id) {
            Some(phone) => phone.streams += 1,
            None => {
                let attachment = self.core.attach_phone(id, &device.name);
                phones.insert(
                    id,
                    PhoneIn {
                        streams: 1,
                        _attachment: attachment,
                    },
                );
            }
        }
        Ok(id)
    }

    /// A stream of a relayed phone closed: the last one detaches the phone.
    fn phone_stream_closed(&self, id: Ulid) {
        let mut phones = lock(&self.phones);
        if let Some(phone) = phones.get_mut(&id) {
            phone.streams = phone.streams.saturating_sub(1);
            if phone.streams == 0 {
                phones.remove(&id);
            }
        }
    }

    /// Sends the answer to a call. An answer that is larger than a client reads
    /// ([`MAX_FROM_CORE`]) becomes the error `too_large`, so that the caller does not wait.
    fn reply(&self, id: u64, outcome: Outcome) {
        if self.wire.send(&LinkMsg::Reply { id, outcome }) || self.wire.is_down() {
            return;
        }
        let error = RpcError::new(
            ErrorCode::TooLarge,
            format!("the answer is larger than {MAX_FROM_CORE} bytes, the limit of a link"),
        );
        self.wire.send(&LinkMsg::Reply {
            id,
            outcome: Outcome::Err(error_bytes(&error)),
        });
    }

    /// Ends everything of the link: its subscriptions, its streams and its host requests.
    fn end(&self) {
        for (_, id) in lock(&self.subs).drain() {
            self.core.unsubscribe_tracked(&id);
        }
        for (_, stream) in lock(&self.streams).drain() {
            stream.closed.store(true, Ordering::SeqCst);
        }
        lock(&self.phones).clear();
        for (_, credits) in lock(&self.blobs).drain() {
            credits.close();
        }
        lock(&self.puts).clear();
        lock(&self.control).take();
        self.host.fail_all();
        self.wire.set_down("the link ended");
    }
}

/// Runs a call of the link: a class-0 call in the queue, another one on its own task.
fn serve_call(conn: &Arc<Conn>, call: LinkCall) {
    if is_control_method(&call.method) {
        let _ = control_queue(conn).send(call);
    } else {
        let conn = Arc::clone(conn);
        tokio::spawn(async move {
            let outcome = conn.run_call(&call).await;
            conn.reply(call.id, outcome);
        });
    }
}

/// Serves the messages of a link until it ends. The calls wait until the host of the link is
/// attached (`attached`), in the order that they came: a call such as `pane.copy` or
/// `palette.execute {settings.open}` needs the host of its device, and the attach waits for the
/// capabilities of the client. Subscriptions, streams and blobs need no host.
async fn serve_link(
    conn: &Arc<Conn>,
    rx: &mut mpsc::UnboundedReceiver<Incoming>,
    mut attached: oneshot::Receiver<()>,
) {
    let mut held: Option<Vec<LinkCall>> = Some(Vec::new());
    loop {
        let incoming = tokio::select! {
            incoming = rx.recv() => incoming,
            // An error is a link that ended before the attach: the calls run, and they find no
            // host.
            _ = &mut attached, if held.is_some() => {
                for call in held.take().unwrap_or_default() {
                    serve_call(conn, call);
                }
                continue;
            }
        };
        let Some(incoming) = incoming else {
            break;
        };
        let Incoming::Msg(msg) = incoming else {
            continue;
        };
        match msg {
            LinkMsg::Call {
                id,
                window,
                device,
                method,
                params,
            } => {
                let call = LinkCall {
                    id,
                    window,
                    device,
                    method,
                    params,
                };
                match held.as_mut() {
                    Some(held) => held.push(call),
                    None => serve_call(conn, call),
                }
            }
            LinkMsg::Subscribe { sub, name, since } => subscribe(conn, sub, &name, since),
            LinkMsg::Unsubscribe { sub } => {
                if let Some(id) = lock(&conn.subs).remove(&sub) {
                    conn.core.unsubscribe_tracked(&id);
                }
            }
            LinkMsg::Stream {
                stream,
                kind,
                params,
                window: _,
                device,
            } => open_stream(conn, stream, &kind, &params, device.as_ref()),
            LinkMsg::StreamClose { stream } => {
                let closed = lock(&conn.streams).remove(&stream);
                if let Some(stream) = closed {
                    stream.closed.store(true, Ordering::SeqCst);
                    if let Some(phone) = stream.phone {
                        conn.phone_stream_closed(phone);
                    }
                }
            }
            LinkMsg::HostReply { id, result } => conn.host.resolve(id, result),
            LinkMsg::BlobGet { id, hash } => send_blob(conn, id, hash),
            LinkMsg::BlobCredit { id, parts } => {
                if let Some(credits) = lock(&conn.blobs).get(&id) {
                    credits.add_permits((parts as usize).min(BLOB_CREDITS));
                }
            }
            LinkMsg::BlobPut {
                id,
                name,
                size,
                hash,
            } => start_put(conn, id, name, size, hash),
            LinkMsg::BlobPart {
                id,
                offset,
                bytes,
                last,
            } => put_part(conn, id, offset, &bytes, last),
            other => tracing::debug!(kind = other.kind(), "a link sent a message for a client"),
        }
    }
}

/// The queue of the class-0 calls of a link. Its task runs them one after the other.
fn control_queue(conn: &Arc<Conn>) -> mpsc::UnboundedSender<LinkCall> {
    let mut slot = lock(&conn.control);
    if let Some(queue) = slot.as_ref() {
        return queue.clone();
    }
    let (tx, mut rx) = mpsc::unbounded_channel::<LinkCall>();
    let runner = Arc::clone(conn);
    tokio::spawn(async move {
        while let Some(call) = rx.recv().await {
            let outcome = runner.run_call(&call).await;
            runner.reply(call.id, outcome);
        }
    });
    *slot = Some(tx.clone());
    tx
}

/// The sink of a subscription of a link.
struct ProjectionOut {
    wire: Arc<Wire>,
    sub: u64,
}

impl MessageSink for ProjectionOut {
    fn send(&self, message: Value) -> bool {
        let message = serde_json::to_vec(&message).unwrap_or_default();
        self.wire.send(&LinkMsg::Projection {
            sub: self.sub,
            message,
        })
    }
}

fn subscribe(conn: &Arc<Conn>, sub: u64, name: &str, since: Option<u64>) {
    if lock(&conn.subs).contains_key(&sub) {
        let error = RpcError::invalid_params(format!("the subscription {sub} exists"));
        conn.reply(sub, Outcome::Err(error_bytes(&error)));
        return;
    }
    let sink = ProjectionOut {
        wire: Arc::clone(&conn.wire),
        sub,
    };
    match conn.core.subscribe_tracked(name, since, Box::new(sink)) {
        Ok(id) => {
            lock(&conn.subs).insert(sub, id.clone());
            conn.reply(sub, outcome_of(Ok(json!({ "subscription": id }))));
        }
        Err(error) => conn.reply(sub, Outcome::Err(error_bytes(&error))),
    }
}

/// The sink of a stream of a link.
struct FrameOut {
    wire: Arc<Wire>,
    stream: u64,
    closed: Arc<AtomicBool>,
}

impl FrameSink for FrameOut {
    fn send(&self, frame: Vec<u8>) -> bool {
        !self.closed.load(Ordering::SeqCst)
            && self.wire.send(&LinkMsg::Frame {
                stream: self.stream,
                bytes: frame,
            })
    }

    fn is_closed(&self) -> bool {
        self.closed.load(Ordering::SeqCst) || self.wire.is_down()
    }

    /// The pings of the link measure the round trip, and the pacer of the window gives the pane
    /// more frames in flight when it is long.
    fn round_trip(&self) -> Option<std::time::Duration> {
        self.wire.rtt()
    }
}

fn open_stream(conn: &Arc<Conn>, stream: u64, kind: &str, params: &[u8], device: Option<&Device>) {
    let closed = Arc::new(AtomicBool::new(false));
    let opened = params_of(params).and_then(|params| {
        if lock(&conn.streams).contains_key(&stream) {
            return Err(RpcError::invalid_params(format!(
                "the stream {stream} exists"
            )));
        }
        // A relayed phone is a client before its first frame, so its window is present.
        let phone = device.map(|d| conn.phone_stream_opened(d)).transpose()?;
        // The stream is known before its first frame.
        lock(&conn.streams).insert(
            stream,
            StreamIn {
                closed: Arc::clone(&closed),
                phone,
            },
        );
        let sink = FrameOut {
            wire: Arc::clone(&conn.wire),
            stream,
            closed: Arc::clone(&closed),
        };
        let opened = conn.core.stream(kind, params, Arc::new(sink));
        if opened.is_err() {
            lock(&conn.streams).remove(&stream);
            if let Some(phone) = phone {
                conn.phone_stream_closed(phone);
            }
        }
        opened.map(|()| json!({}))
    });
    conn.reply(stream, outcome_of(opened));
}

/// The most uploads that one link has at the same time. Each holds up to a file in memory.
const MAX_PUTS: usize = 4;

/// A blob that the client sends.
struct PutIn {
    name: String,
    size: u64,
    hash: String,
    bytes: Vec<u8>,
}

fn put_failed(conn: &Conn, id: u64, error: &RpcError) {
    lock(&conn.puts).remove(&id);
    conn.wire.send(&LinkMsg::BlobDone {
        id,
        outcome: BlobOutcome::Err(error_bytes(error)),
    });
}

/// `BlobPut`: the client will send `size` bytes in parts. The limit of a file is checked now, so a
/// file that is too large is refused before its parts come.
fn start_put(conn: &Arc<Conn>, id: u64, name: String, size: u64, hash: String) {
    if size > nh_core::UPLOAD_MAX_BYTES {
        let error = RpcError::new(
            ErrorCode::TooLarge,
            format!("a file has at most {} bytes", nh_core::UPLOAD_MAX_BYTES),
        );
        return put_failed(conn, id, &error);
    }
    let mut puts = lock(&conn.puts);
    if puts.len() >= MAX_PUTS || puts.contains_key(&id) {
        drop(puts);
        let error = RpcError::new(
            ErrorCode::Busy,
            format!("a link has at most {MAX_PUTS} uploads"),
        );
        return put_failed(conn, id, &error);
    }
    let capacity = usize::try_from(size).unwrap_or(0).min(1 << 20);
    puts.insert(
        id,
        PutIn {
            name,
            size,
            hash,
            bytes: Vec::with_capacity(capacity),
        },
    );
}

/// `BlobPart` of an upload: the part is written, the client gets a credit for it, and the last
/// part ends the upload with the check of the digest.
fn put_part(conn: &Arc<Conn>, id: u64, offset: u64, bytes: &[u8], last: bool) {
    let finished = {
        let mut puts = lock(&conn.puts);
        let Some(put) = puts.get_mut(&id) else {
            return;
        };
        let wrong_offset = offset != put.bytes.len() as u64;
        if wrong_offset || put.bytes.len() as u64 + bytes.len() as u64 > put.size {
            drop(puts);
            let error = RpcError::invalid_params("a part of an upload is out of order or too long");
            return put_failed(conn, id, &error);
        }
        put.bytes.extend_from_slice(bytes);
        conn.wire.send(&LinkMsg::BlobCredit { id, parts: 1 });
        if last { puts.remove(&id) } else { None }
    };
    let Some(put) = finished else {
        return;
    };
    if put.bytes.len() as u64 != put.size || blake3::hash(&put.bytes).to_hex().as_str() != put.hash
    {
        let error = RpcError::new(
            ErrorCode::BlobCorrupt,
            "the bytes of the upload do not match its size and its hash",
        );
        return put_failed(conn, id, &error);
    }
    let conn = Arc::clone(conn);
    tokio::spawn(async move {
        let outcome = match conn.core.put_upload(&put.name, put.bytes).await {
            Ok((hash, mime)) => BlobOutcome::Ok { hash, mime },
            Err(error) => BlobOutcome::Err(error_bytes(&error)),
        };
        conn.wire.send(&LinkMsg::BlobDone { id, outcome });
    });
}

fn send_blob(conn: &Arc<Conn>, id: u64, hash: String) {
    let credits = Arc::new(Semaphore::new(BLOB_CREDITS));
    lock(&conn.blobs).insert(id, Arc::clone(&credits));
    let conn = Arc::clone(conn);
    tokio::spawn(async move {
        let outcome = match conn.core.blob(&hash).await {
            Ok(Some((bytes, mime))) => {
                let mut sent = true;
                let parts: Vec<&[u8]> = if bytes.is_empty() {
                    Vec::new()
                } else {
                    bytes.chunks(BLOB_PART).collect()
                };
                let count = parts.len();
                for (n, part) in parts.into_iter().enumerate() {
                    let Ok(permit) = credits.acquire().await else {
                        sent = false;
                        break;
                    };
                    permit.forget();
                    if !conn.wire.send(&LinkMsg::BlobPart {
                        id,
                        offset: (n * BLOB_PART) as u64,
                        bytes: part.to_vec(),
                        last: n + 1 == count,
                    }) {
                        sent = false;
                        break;
                    }
                }
                if !sent {
                    lock(&conn.blobs).remove(&id);
                    return;
                }
                BlobOutcome::Ok {
                    hash: blake3::hash(&bytes).to_hex().to_string(),
                    mime,
                }
            }
            Ok(None) => BlobOutcome::Err(error_bytes(&RpcError::new(
                ErrorCode::NotFound,
                format!("no blob {hash}"),
            ))),
            Err(error) => BlobOutcome::Err(error_bytes(&error)),
        };
        lock(&conn.blobs).remove(&id);
        conn.wire.send(&LinkMsg::BlobDone { id, outcome });
    });
}

/// The [`Host`] of one link client. Each method sends a `HostRequest`. The client
/// answers each one, and a refusal is only logged: the trait has no answer.
pub struct LinkHost {
    wire: Arc<Wire>,
    next: AtomicU64,
    /// The requests whose answer the core waits for.
    pending: Mutex<HashMap<u64, oneshot::Sender<HostOutcome>>>,
    /// The notification capabilities that the client told after `Welcome`.
    actions: AtomicBool,
}

impl std::fmt::Debug for LinkHost {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LinkHost")
            .field("actions", &self.actions.load(Ordering::Relaxed))
            .finish_non_exhaustive()
    }
}

impl LinkHost {
    fn new(wire: Arc<Wire>) -> Self {
        Self {
            wire,
            next: AtomicU64::new(1),
            pending: Mutex::default(),
            actions: AtomicBool::new(false),
        }
    }

    /// Sends a request and returns its id.
    fn send(&self, request: &HostRequest) -> Option<u64> {
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        let bytes = postcard::to_allocvec(request).ok()?;
        self.wire
            .send(&LinkMsg::HostRequest { id, request: bytes })
            .then_some(id)
    }

    /// Sends a request and waits for the answer. `None` when the link ends first.
    async fn ask(&self, request: &HostRequest) -> Option<HostOutcome> {
        let (tx, rx) = oneshot::channel();
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        lock(&self.pending).insert(id, tx);
        let bytes = postcard::to_allocvec(request).ok()?;
        if !self.wire.send(&LinkMsg::HostRequest { id, request: bytes }) {
            lock(&self.pending).remove(&id);
            return None;
        }
        rx.await.ok()
    }

    fn resolve(&self, id: u64, outcome: HostOutcome) {
        if let Some(tx) = lock(&self.pending).remove(&id) {
            let _ = tx.send(outcome);
        } else if let HostOutcome::Refused(why) = outcome {
            tracing::debug!(why, "a link client refused a host request");
        }
    }

    fn fail_all(&self) {
        lock(&self.pending).clear();
    }

    /// Whether the link of this host is down.
    pub fn is_down(&self) -> bool {
        self.wire.is_down()
    }

    fn fire(&self, request: &HostRequest) {
        if self.send(request).is_none() {
            tracing::debug!("a host request found its link down");
        }
    }
}

/// A new ULID: the id of a notification, or the device id of a client that has none of its own.
pub fn new_ulid() -> Ulid {
    let mut random = [0u8; 16];
    let _ = getrandom::fill(&mut random);
    let ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(0));
    Ulid::from_parts(ms, u128::from_be_bytes(random) & ((1u128 << 80) - 1))
}

impl Host for LinkHost {
    fn notify(&self, notification: HostNotification) -> NotificationId {
        let id = NotificationId::new(new_ulid());
        self.fire(&HostRequest::Notify {
            id: id.to_string(),
            notification: Notification::from(&notification),
        });
        id
    }

    /// One `NotifyOpen` for all of them: the client counts it once.
    fn notify_open(&self, notifications: Vec<HostNotification>) -> Vec<NotificationId> {
        let shown: Vec<(NotificationId, Notification)> = notifications
            .iter()
            .map(|n| (NotificationId::new(new_ulid()), Notification::from(n)))
            .collect();
        let ids = shown.iter().map(|(id, _)| *id).collect();
        self.fire(&HostRequest::NotifyOpen {
            notifications: shown
                .into_iter()
                .map(|(id, n)| (id.to_string(), n))
                .collect(),
        });
        ids
    }

    fn update_notification(&self, id: NotificationId, notification: HostNotification) {
        self.fire(&HostRequest::UpdateNotification {
            id: id.to_string(),
            notification: Notification::from(&notification),
        });
    }

    fn withdraw_notification(&self, id: NotificationId) {
        self.fire(&HostRequest::WithdrawNotification { id: id.to_string() });
    }

    /// The answer that the client gave after `Welcome`.
    fn notification_capabilities(&self) -> NotificationCapabilities {
        NotificationCapabilities {
            actions: self.actions.load(Ordering::Relaxed),
        }
    }

    fn set_badge(&self, count: u32) {
        self.fire(&HostRequest::SetBadge { count });
    }

    fn set_tray(&self, tray: TrayModel) {
        self.fire(&HostRequest::SetTray {
            tray: Tray::from(&tray),
        });
    }

    fn open_window(&self, spec: WindowSpec) {
        self.fire(&HostRequest::OpenWindow {
            window: Window::from(&spec),
        });
    }

    fn close_window(&self, id: WindowId) {
        self.fire(&HostRequest::CloseWindow {
            window: id.to_string(),
        });
    }

    fn focus_window(&self, id: WindowId) {
        self.fire(&HostRequest::FocusWindow {
            window: id.to_string(),
        });
    }

    /// Never sent over a link: the sidecar is the one of the machine of the core.
    fn sidecar_path(&self) -> PathBuf {
        PathBuf::new()
    }

    fn clipboard_write(&self, text: String) {
        self.fire(&HostRequest::ClipboardWrite { text });
    }

    /// Never sent over a link: a client opens a URL only after a click of the person.
    fn open_url(&self, _: String) {
        tracing::debug!("a URL is not opened over a link");
    }

    fn open_settings(&self) {
        self.fire(&HostRequest::OpenSettings);
    }

    /// Only an in-process host shows the folder panel. `done` is dropped, and the core
    /// answers `unsupported`.
    fn pick_folder(&self, _: FolderPick, done: FolderChosen) {
        drop(done);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_major_version_of_the_api_ignores_the_minor() {
        assert_eq!(api_major("neohar/1"), "neohar/1");
        assert_eq!(api_major("neohar/1.4"), "neohar/1");
        assert_ne!(api_major("neohar/2"), api_major(API_VERSION));
    }
}
