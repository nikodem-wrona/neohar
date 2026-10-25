//! The core that the app speaks to: the core in this process, or a core behind
//! `link.sock` on this Mac.
//!
//! At launch the app tries `link.sock` of its session before it starts a core. When a core answers
//! `Welcome` (for example `nh daemon`), the app starts no core. It is a client of the machine
//! `local`, with the device of `<data>/device-id`. Its [`crate::host::TauriHost`] answers
//! the host requests of that core, and the [`LinkClient`] applies its limits. Else the app starts
//! its own core, as before, and serves `link.sock` for other clients.
//!
//! Every module of the app uses a [`Backend`] and never a `CoreHandle` directly. The webview does
//! not change: its three commands go to the backend with the window of the webview.

use std::{
    collections::{HashMap, VecDeque},
    sync::{Arc, Mutex, PoisonError},
    time::Duration,
};

use nh_core::{CoreHandle, FrameSink, Host, MessageSink, Origin, RpcError};
use nh_proto::{
    api::ids::WindowId,
    paths::{self, Env},
};
use nh_remote::{
    ClientHello, ClientOptions, Device, Link, LinkClient, LinkError, LinkTiming, PersistentLink,
};
use serde_json::{Value, json};
use tauri::{AppHandle, Manager};
use tokio::net::UnixStream;
use ulid::Ulid;

/// How long the app waits for `link.sock` to take the connection. A socket with no listener
/// refuses at once, so this only limits a core that hangs.
const CONNECT_WITHIN: Duration = Duration::from_millis(500);

/// How long the app waits for `Welcome` after the socket took the connection. The core asks
/// `session.info` and `machine.info` first, and a core that restores its panes can be slow.
const WELCOME_WITHIN: Duration = Duration::from_secs(5);

/// The most bytes of blobs of a link that the app keeps in memory.
pub const BLOB_CACHE_BYTES: usize = 64 << 20;

/// How long a quit waits for the `Bye` to go out.
const BYE_WITHIN: Duration = Duration::from_secs(1);

/// The core of the app. Cloning is cheap.
#[derive(Clone)]
pub enum Backend {
    /// The core runs in this process.
    Local(CoreHandle),
    /// A core behind `link.sock` serves the session.
    Link(Linked),
}

/// A client of a core through `link.sock`.
#[derive(Clone)]
pub struct Linked {
    /// One link to a daemon on this Mac, or the link of a remote machine, which comes back.
    pub client: Arc<dyn Link>,
    /// The device of this app, as it said in `Hello`.
    device: Ulid,
    blobs: Arc<Mutex<BlobCache>>,
}

impl Linked {
    pub fn new(client: LinkClient, device: Ulid) -> Self {
        Self::of(Arc::new(client), device)
    }

    /// The link of a remote machine.
    pub fn persistent(link: PersistentLink, device: Ulid) -> Self {
        Self::of(Arc::new(link), device)
    }

    fn of(client: Arc<dyn Link>, device: Ulid) -> Self {
        Self {
            client,
            device,
            blobs: Arc::new(Mutex::new(BlobCache::new(BLOB_CACHE_BYTES))),
        }
    }
}

/// The backend of the app, once setup has chosen it. `None` before that, and when the app did not
/// start because a core refused it.
pub fn of(app: &AppHandle) -> Option<Backend> {
    app.try_state::<Backend>()
        .map(|state| state.inner().clone())
}

impl Backend {
    /// The device of this app.
    pub fn device(&self) -> Option<Ulid> {
        match self {
            Self::Local(core) => core.local_device(),
            Self::Link(linked) => Some(linked.device),
        }
    }

    /// The link client, in client mode.
    pub fn link(&self) -> Option<&Arc<dyn Link>> {
        match self {
            Self::Local(_) => None,
            Self::Link(linked) => Some(&linked.client),
        }
    }

    /// Calls a method as the UI of this app (`call` of the transport). `window` is the window of
    /// the layout that made the call. The core of a link puts the device of `Hello` in the origin.
    pub async fn call(
        &self,
        window: Option<WindowId>,
        method: &str,
        params: Value,
    ) -> Result<Value, RpcError> {
        match self {
            Self::Local(core) => {
                let mut origin = match core.local_device() {
                    Some(device) => Origin::ui().from_device(device),
                    None => Origin::ui(),
                };
                if let Some(window) = window {
                    origin = origin.from_window(window);
                }
                core.call_as(origin, method, params).await
            }
            Self::Link(linked) => linked.client.call(window, method, params).await,
        }
    }

    /// [`Backend::call`] for a phone that the listener relays to a machine. The core of
    /// the link puts the device of the phone in the origin. The core in this process serves its
    /// phones itself, so it gets no relayed phone.
    pub async fn call_for(
        &self,
        phone: Option<&Device>,
        window: Option<WindowId>,
        method: &str,
        params: Value,
    ) -> Result<Value, RpcError> {
        match (self, phone) {
            (_, None) => self.call(window, method, params).await,
            (Self::Link(linked), Some(phone)) => {
                linked
                    .client
                    .relay_call(phone, window, method, params)
                    .await
            }
            (Self::Local(_), Some(_)) => Err(relayed_to_local()),
        }
    }

    /// Calls a method as the UI of this app, for no window.
    pub async fn rpc(&self, method: &str, params: Value) -> Result<Value, RpcError> {
        self.call(None, method, params).await
    }

    /// Subscribes to a projection and returns the id that `projection.unsubscribe` takes.
    pub async fn subscribe_tracked(
        &self,
        name: &str,
        since: Option<u64>,
        sink: Box<dyn MessageSink>,
    ) -> Result<String, RpcError> {
        match self {
            Self::Local(core) => core.subscribe_tracked(name, since, sink),
            Self::Link(linked) => linked.client.subscribe(name, since, sink).await,
        }
    }

    /// Subscribes to a projection for the host itself. It ends when the sink says that its
    /// receiver is gone.
    pub async fn subscribe(
        &self,
        name: &str,
        since: Option<u64>,
        sink: Box<dyn MessageSink>,
    ) -> Result<(), RpcError> {
        match self {
            Self::Local(core) => core.subscribe(name, since, sink),
            Self::Link(linked) => linked.client.subscribe(name, since, sink).await.map(|_| ()),
        }
    }

    /// Opens a stream (`stream` of the transport). The frames go to the sink as bytes. The host
    /// does not decode them.
    pub async fn stream(
        &self,
        kind: &str,
        params: Value,
        window: Option<WindowId>,
        sink: Arc<dyn FrameSink>,
    ) -> Result<(), RpcError> {
        match self {
            Self::Local(core) => core.stream(kind, params, sink),
            Self::Link(linked) => linked.client.stream(kind, params, window, sink).await,
        }
    }

    /// [`Backend::stream`] for a phone that the listener relays to a machine. The core of the
    /// link attaches the phone while a stream of it is open.
    pub async fn stream_for(
        &self,
        phone: Option<&Device>,
        kind: &str,
        params: Value,
        window: Option<WindowId>,
        sink: Arc<dyn FrameSink>,
    ) -> Result<(), RpcError> {
        match (self, phone) {
            (_, None) => self.stream(kind, params, window, sink).await,
            (Self::Link(linked), Some(phone)) => {
                linked
                    .client
                    .relay_stream(phone, kind, params, window, sink)
                    .await
            }
            (Self::Local(_), Some(_)) => Err(relayed_to_local()),
        }
    }

    /// A blob for `nh://blob/<hash>`: its bytes and its type. A blob of a link comes from the
    /// cache in memory or with `BlobGet`, and only with the type of an image.
    pub async fn blob(&self, hash: &str) -> Result<Option<(Vec<u8>, String)>, RpcError> {
        let linked = match self {
            Self::Local(core) => return core.blob(hash).await,
            Self::Link(linked) => linked,
        };
        let key = hash.to_ascii_lowercase();
        if let Some(hit) = lock(&linked.blobs).get(&key) {
            return Ok(Some(hit));
        }
        let Some((bytes, mime)) = linked.client.blob(&key).await? else {
            return Ok(None);
        };
        let Some(mime) = image_type(&mime) else {
            tracing::warn!(
                mime,
                "a blob of the link is not an image, and the app refuses it"
            );
            return Ok(None);
        };
        lock(&linked.blobs).put(&key, &bytes, mime);
        Ok(Some((bytes, mime.to_owned())))
    }

    /// What the system lets the app do with notifications. The core in this process
    /// keeps it in the health. A core of a link has no method for it yet, so the app only writes
    /// it to its log, and its host drops the notifications that the system refuses.
    pub fn notification_permission(&self, permission: nh_core::host::NotificationPermission) {
        match self {
            Self::Local(core) => core.notification_permission(permission),
            Self::Link(_) => tracing::info!(
                ?permission,
                "client mode: the core of the link does not get the notification permission"
            ),
        }
    }

    /// "Quit and stop everything". The core in this process stops its
    /// children. A core of a link gets `session.stop_everything`. The text is for the log.
    pub async fn stop_everything(&self, grace: Duration) -> Result<String, RpcError> {
        match self {
            Self::Local(core) => Ok(format!("{:?}", core.stop_everything(grace).await)),
            Self::Link(linked) => linked
                .client
                .call(None, "session.stop_everything", json!({}))
                .await
                .map(|report| report.to_string()),
        }
    }

    /// Ends the link with `Bye`, and waits a short time for it to go out. The core of the link
    /// and its panes keep running (Quit).
    pub fn end_link_blocking(&self) {
        if let Self::Link(linked) = self {
            linked.client.close("the app quit");
            let client = linked.client.clone();
            let ended = tauri::async_runtime::block_on(async move {
                tokio::time::timeout(BYE_WITHIN, client.closed()).await
            });
            match ended {
                Ok(reason) => tracing::info!(reason, "client mode: the link ended"),
                Err(_) => tracing::warn!("client mode: the link did not end in time"),
            }
        }
    }
}

/// The routing of `config.*` for a window of a remote machine calls both cores the same way.
#[async_trait::async_trait]
impl nh_remote::settings::Calls for Backend {
    async fn call(
        &self,
        window: Option<WindowId>,
        method: &str,
        params: Value,
    ) -> Result<Value, RpcError> {
        Backend::call(self, window, method, params).await
    }
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The error of a relayed phone for the core in this process, which its own listener serves.
fn relayed_to_local() -> RpcError {
    RpcError::internal("the core in this process serves its phones itself")
}

/// What the app does at launch.
#[derive(Debug)]
pub enum Launch<T> {
    /// A core answered `Welcome`: the app is its client and starts no core.
    Client(T),
    /// No core listens on `link.sock`: the app starts its own core. The text says why.
    StartCore(String),
    /// A core listens and does not take this app. The app must not start a second core for the
    /// session, so it stops with this message.
    Refuse(String),
}

/// The choice of the launch from the result of the connection to `link.sock`.
///
/// An error of the socket (no file, no listener, a file of a crashed core) means that no core
/// serves the session. Every other result means that a core listens.
pub fn launch_of<T>(outcome: Result<T, LinkError>, session: &str) -> Launch<T> {
    match outcome {
        Ok(client) => Launch::Client(client),
        Err(LinkError::Io(error)) if error.kind() == std::io::ErrorKind::PermissionDenied => {
            Launch::Refuse(unsafe_socket(session, &error))
        }
        Err(LinkError::Io(error)) => Launch::StartCore(error.to_string()),
        Err(LinkError::Refused(reason)) => Launch::Refuse(format!(
            "the core that serves the session {session:?} refused this app ({reason}): their \
             versions do not match. Update neohar, or stop the core that serves the session."
        )),
        Err(LinkError::Timeout) => Launch::Refuse(format!(
            "a core listens on link.sock of the session {session:?} and does not answer. Stop \
             it, or wait until it answers."
        )),
        Err(error @ (LinkError::Closed(_) | LinkError::Unexpected(_))) => Launch::Refuse(format!(
            "the core that serves the session {session:?} ended the link: {error}"
        )),
    }
}

/// The message when the runtime directory or `link.sock` of the session is not safe: another user
/// of this Mac can have made it to get the keys of the app. The app neither connects nor starts a
/// core there.
fn unsafe_socket(session: &str, error: &std::io::Error) -> String {
    format!(
        "neohar does not use the session {session:?}: {error}. Another user of this Mac can have \
         made it. Remove it, or set NEOHAR_RUNTIME_DIR to a directory of yours, and open neohar \
         again."
    )
}

/// How long the app tries `link.sock` again when its own core does not start because another
/// core holds the session: a core that starts (for example `nh daemon`) holds the lock of the
/// session before it serves `link.sock`.
pub const BUSY_RETRY_FOR: Duration = Duration::from_secs(5);

/// The time between two tries of `link.sock` after [`BUSY_RETRY_FOR`] began.
pub const BUSY_RETRY_EVERY: Duration = Duration::from_millis(100);

/// The choice after the core of the app did not start because another core holds the session
/// (`ApiSocketInUse`), from one more try of `link.sock`. `None`: try again. `Some(Ok)`: the app is
/// the client of that core. `Some(Err)`: the app shows the message and stops. An error of the
/// socket means that the other core does not serve `link.sock` yet; `last` says that the time to
/// wait for it is over.
pub fn launch_when_busy<T>(
    outcome: Result<T, LinkError>,
    session: &str,
    last: bool,
) -> Option<Result<T, String>> {
    match outcome {
        Ok(client) => Some(Ok(client)),
        Err(LinkError::Io(error)) if error.kind() == std::io::ErrorKind::PermissionDenied => {
            Some(Err(unsafe_socket(session, &error)))
        }
        Err(LinkError::Io(_)) if !last => None,
        Err(LinkError::Io(error)) => Some(Err(format!(
            "another core holds the session {session:?}, and its link.sock does not take a \
             connection ({error}). Wait until that core is up and open neohar again, or stop it."
        ))),
        Err(error) => match launch_of::<T>(Err(error), session) {
            Launch::Client(client) => Some(Ok(client)),
            Launch::StartCore(message) | Launch::Refuse(message) => Some(Err(message)),
        },
    }
}

/// Tries `link.sock` with `connect` until a core answers, or until `within` passed. The app calls
/// it when its own core did not start because another core holds the session. An error is the
/// message for the dialog.
pub async fn wait_for_link<T, F, Fut>(
    mut connect: F,
    session: &str,
    within: Duration,
    every: Duration,
) -> Result<T, String>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, LinkError>>,
{
    let until = tokio::time::Instant::now() + within;
    loop {
        let outcome = connect().await;
        let last = tokio::time::Instant::now() + every > until;
        if let Some(result) = launch_when_busy(outcome, session, last) {
            return result;
        }
        tokio::time::sleep(every).await;
    }
}

/// Connects to `link.sock` of `session`. `host` runs only when the socket takes the connection,
/// and gives the host that answers the host requests of the core. The core asks it for its
/// notification capabilities at once, so the host must be ready.
///
/// The app connects before its own core checks the runtime root, and the root can be in `/tmp`,
/// where another user can make it first and serve a fake core. So the root must pass the check of
/// [`Env::verify_runtime_root`], the directory of the socket must be closed to other users, and a
/// process of this user must serve the socket. A refusal is a [`LinkError::Io`] of kind
/// `PermissionDenied`, and [`launch_of`] stops the app with it. A missing root is `NotFound`: no
/// core ran.
pub async fn connect(
    paths: &Env,
    session: &str,
    hello: ClientHello,
    host: impl FnOnce() -> Arc<dyn Host>,
) -> Result<LinkClient, LinkError> {
    paths.verify_runtime_root().map_err(std::io::Error::from)?;
    let socket = paths.link_socket(session).map_err(std::io::Error::from)?;
    paths::check_socket_parent(&socket).map_err(std::io::Error::from)?;
    let stream = match tokio::time::timeout(CONNECT_WITHIN, UnixStream::connect(&socket)).await {
        Ok(Ok(stream)) => stream,
        Ok(Err(error)) => return Err(LinkError::Io(error)),
        Err(_) => return Err(LinkError::Timeout),
    };
    paths::check_peer(&socket, stream.peer_cred()?.uid()).map_err(std::io::Error::from)?;
    let host = host();
    let (read, write) = stream.into_split();
    let options = ClientOptions {
        timing: LinkTiming {
            hello_within: WELCOME_WITHIN,
            ..LinkTiming::default()
        },
        ..ClientOptions::default()
    };
    LinkClient::connect(read, write, hello, host, options).await
}

/// The windows of the layout that this client owns: the rows whose `client` is its device.
/// The [`LinkClient`] lets a window request of the core through only for these.
pub fn owned_windows<'a>(
    rows: impl IntoIterator<Item = (&'a String, Option<&'a str>)>,
    device: &str,
) -> Vec<WindowId> {
    rows.into_iter()
        .filter(|(_, client)| *client == Some(device))
        .filter_map(|(id, _)| id.parse::<WindowId>().ok())
        .collect()
}

/// The content type of an image that a blob of a link may have, or `None`. Parameters
/// and case do not count.
pub fn image_type(mime: &str) -> Option<&'static str> {
    let base = mime.split(';').next().unwrap_or_default().trim();
    ["image/png", "image/jpeg", "image/gif", "image/webp"]
        .into_iter()
        .find(|known| base.eq_ignore_ascii_case(known))
}

/// The blobs of a link in memory, with a bound on their bytes. The one used last stays. A blob
/// larger than the bound is not kept.
pub struct BlobCache {
    max: usize,
    used: usize,
    entries: HashMap<String, (Arc<[u8]>, &'static str)>,
    /// The hashes from the one used first to the one used last.
    order: VecDeque<String>,
}

impl BlobCache {
    pub fn new(max: usize) -> Self {
        Self {
            max,
            used: 0,
            entries: HashMap::new(),
            order: VecDeque::new(),
        }
    }

    /// The bytes of all the blobs that the cache keeps.
    #[cfg(test)]
    pub fn used(&self) -> usize {
        self.used
    }

    fn touch(&mut self, hash: &str) {
        if let Some(at) = self.order.iter().position(|h| h == hash)
            && let Some(hash) = self.order.remove(at)
        {
            self.order.push_back(hash);
        }
    }

    pub fn get(&mut self, hash: &str) -> Option<(Vec<u8>, String)> {
        let (bytes, mime) = self.entries.get(hash)?.clone();
        self.touch(hash);
        Some((bytes.to_vec(), mime.to_owned()))
    }

    pub fn put(&mut self, hash: &str, bytes: &[u8], mime: &'static str) {
        if bytes.len() > self.max || self.entries.contains_key(hash) {
            return;
        }
        while self.used + bytes.len() > self.max {
            let Some(oldest) = self.order.pop_front() else {
                break;
            };
            if let Some((gone, _)) = self.entries.remove(&oldest) {
                self.used -= gone.len();
            }
        }
        self.used += bytes.len();
        self.entries
            .insert(hash.to_owned(), (Arc::from(bytes), mime));
        self.order.push_back(hash.to_owned());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_launch_follows_the_answer_of_link_sock() {
        let session = "dev";
        assert!(matches!(launch_of(Ok(7), session), Launch::Client(7)));
        // No file, or a file that no core listens on: the app starts its core.
        for kind in [
            std::io::ErrorKind::NotFound,
            std::io::ErrorKind::ConnectionRefused,
        ] {
            let outcome: Result<(), _> = Err(LinkError::Io(kind.into()));
            assert!(matches!(launch_of(outcome, session), Launch::StartCore(_)));
        }
        // A core listens: the app never starts a second core for the session.
        let refused: Result<(), _> = Err(LinkError::Refused("api".to_owned()));
        match launch_of(refused, session) {
            Launch::Refuse(message) => {
                assert!(
                    message.contains("api") && message.contains("\"dev\""),
                    "{message}"
                );
            }
            other => panic!("{other:?}"),
        }
        for error in [
            LinkError::Timeout,
            LinkError::Closed("bye".to_owned()),
            LinkError::Unexpected("Pong"),
        ] {
            let outcome: Result<(), _> = Err(error);
            assert!(matches!(launch_of(outcome, session), Launch::Refuse(_)));
        }
    }

    #[test]
    fn a_socket_that_is_not_safe_stops_the_app_and_starts_no_core() {
        let session = "dev";
        let unsafe_dir = || {
            Err::<u8, _>(LinkError::Io(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "the runtime directory /tmp/neohar-501 is not safe",
            )))
        };
        match launch_of(unsafe_dir(), session) {
            Launch::Refuse(message) => assert!(
                message.contains("/tmp/neohar-501") && message.contains("Another user"),
                "{message}"
            ),
            other => panic!("{other:?}"),
        }
        // Also while the app waits for a busy session: no retry.
        assert!(matches!(
            launch_when_busy(unsafe_dir(), session, false),
            Some(Err(message)) if message.contains("not safe")
        ));
    }

    /// A directory under `/tmp` (a socket path holds 103 bytes), removed on drop.
    struct Scratch(std::path::PathBuf);

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// The runtime root with this mode in a new scratch directory, and an `Env` that uses it.
    fn env_with_root(mode: u32) -> (Scratch, Env) {
        use std::os::unix::fs::PermissionsExt;

        static N: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let scratch = Scratch(std::path::PathBuf::from(format!(
            "/tmp/nhb-{}-{}",
            std::process::id(),
            N.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        )));
        let _ = std::fs::remove_dir_all(&scratch.0);
        let root = scratch.0.join("run");
        std::fs::create_dir_all(&root).expect("the root");
        std::fs::set_permissions(&root, std::fs::Permissions::from_mode(mode)).expect("the mode");
        let mut env = Env::from_process();
        env.runtime_override = Some(root);
        (scratch, env)
    }

    fn hello() -> ClientHello {
        ClientHello {
            device: Ulid::nil(),
            name: "test".to_owned(),
            app_version: "0.0.0".to_owned(),
        }
    }

    #[tokio::test]
    async fn the_app_does_not_connect_to_a_link_sock_that_another_user_can_have_made() {
        // A root that other users can write: another user could serve a fake core in it.
        let (_dir, env) = env_with_root(0o777);
        let socket = env.link_socket("dev").expect("a path");
        std::fs::create_dir_all(socket.parent().expect("a parent")).expect("the session dir");
        let listener = std::os::unix::net::UnixListener::bind(&socket).expect("a listener");
        listener.set_nonblocking(true).expect("non-blocking");
        let outcome = connect(&env, "dev", hello(), || {
            panic!("the host is never made for a socket that is not safe")
        })
        .await;
        match outcome {
            Err(LinkError::Io(error)) => {
                assert_eq!(
                    error.kind(),
                    std::io::ErrorKind::PermissionDenied,
                    "{error}"
                );
            }
            other => panic!("{other:?}"),
        }
        assert!(listener.accept().is_err(), "the app did not connect");
        assert!(matches!(
            launch_of(
                connect(&env, "dev", hello(), || panic!("no host")).await,
                "dev"
            ),
            Launch::Refuse(_)
        ));
    }

    #[tokio::test]
    async fn a_missing_root_means_that_no_core_runs() {
        let (_dir, mut env) = env_with_root(0o700);
        env.runtime_override = env.runtime_override.map(|root| root.join("absent"));
        let outcome = connect(&env, "dev", hello(), || panic!("no host")).await;
        assert!(matches!(launch_of(outcome, "dev"), Launch::StartCore(_)));
    }

    #[test]
    fn a_busy_session_waits_for_link_sock_and_never_starts_a_core() {
        let session = "dev";
        let io = || Err::<u8, _>(LinkError::Io(std::io::ErrorKind::ConnectionRefused.into()));
        assert_eq!(launch_when_busy(Ok(7), session, false), Some(Ok(7)));
        assert_eq!(launch_when_busy(Ok(7), session, true), Some(Ok(7)));
        // The other core does not serve link.sock yet: try again while there is time.
        assert_eq!(launch_when_busy(io(), session, false), None);
        match launch_when_busy(io(), session, true) {
            Some(Err(message)) => assert!(
                message.contains("another core holds the session \"dev\""),
                "{message}"
            ),
            other => panic!("{other:?}"),
        }
        // A core that answers and refuses is the refusal at once, as at the launch.
        for error in [
            LinkError::Refused("api".to_owned()),
            LinkError::Timeout,
            LinkError::Closed("bye".to_owned()),
        ] {
            assert!(matches!(
                launch_when_busy(Err::<u8, _>(error), session, false),
                Some(Err(_))
            ));
        }
    }

    #[tokio::test]
    async fn the_wait_for_link_sock_ends_with_the_client_or_a_message() {
        let every = Duration::from_millis(5);
        // The core serves link.sock after three tries.
        let mut tries = 0;
        let found = wait_for_link(
            || {
                tries += 1;
                let outcome = if tries < 4 {
                    Err(LinkError::Io(std::io::ErrorKind::NotFound.into()))
                } else {
                    Ok(tries)
                };
                std::future::ready(outcome)
            },
            "dev",
            Duration::from_secs(5),
            every,
        )
        .await;
        assert_eq!(found, Ok(4));
        // It never does: the wait ends at its limit with the message of the dialog.
        let started = std::time::Instant::now();
        let never = wait_for_link(
            || {
                std::future::ready(Err::<u8, _>(LinkError::Io(
                    std::io::ErrorKind::NotFound.into(),
                )))
            },
            "dev",
            Duration::from_millis(100),
            every,
        )
        .await;
        assert!(never.is_err());
        assert!(started.elapsed() < Duration::from_secs(2));
        // A refusal ends the wait at once.
        let mut calls = 0;
        let refused = wait_for_link(
            || {
                calls += 1;
                std::future::ready(Err::<u8, _>(LinkError::Refused("api".to_owned())))
            },
            "dev",
            Duration::from_secs(5),
            every,
        )
        .await;
        assert!(refused.is_err());
        assert_eq!(calls, 1);
    }

    #[test]
    fn the_client_owns_the_rows_of_its_device_only() {
        let me = "01ARZ3NDEKTSV4RRFFQ69G5FAX";
        let mine = "01ARZ3NDEKTSV4RRFFQ69G5FAV".to_owned();
        let other = "01ARZ3NDEKTSV4RRFFQ69G5FAW".to_owned();
        let unowned = "01ARZ3NDEKTSV4RRFFQ69G5FAY".to_owned();
        let bad = "not-a-window".to_owned();
        let rows = [
            (&mine, Some(me)),
            (&other, Some("01ARZ3NDEKTSV4RRFFQ69G5FAZ")),
            (&unowned, None),
            (&bad, Some(me)),
        ];
        let owned = owned_windows(rows, me);
        assert_eq!(owned.len(), 1);
        assert_eq!(owned[0].to_string(), mine);
    }

    #[test]
    fn a_blob_of_a_link_is_served_only_as_an_image() {
        assert_eq!(image_type("image/png"), Some("image/png"));
        assert_eq!(image_type("IMAGE/JPEG"), Some("image/jpeg"));
        assert_eq!(image_type("image/gif; charset=binary"), Some("image/gif"));
        assert_eq!(image_type("image/webp"), Some("image/webp"));
        for refused in [
            "image/svg+xml",
            "text/html",
            "application/pdf",
            "application/octet-stream",
            "",
            "image/png-but-not",
        ] {
            assert_eq!(image_type(refused), None, "{refused:?}");
        }
    }

    #[test]
    fn the_blob_cache_keeps_its_bound_and_drops_the_one_used_first() {
        let mut cache = BlobCache::new(10);
        cache.put("a", &[1; 4], "image/png");
        cache.put("b", &[2; 4], "image/png");
        assert_eq!(cache.used(), 8);
        // A read makes `a` the one used last.
        assert_eq!(cache.get("a").map(|(b, _)| b), Some(vec![1; 4]));
        cache.put("c", &[3; 4], "image/gif");
        assert_eq!(cache.used(), 8, "b went to make room");
        assert!(cache.get("b").is_none());
        assert!(cache.get("a").is_some());
        assert_eq!(cache.get("c"), Some((vec![3; 4], "image/gif".to_owned())));
        // A blob larger than the bound is never kept, and it removes nothing.
        cache.put("big", &[0; 11], "image/png");
        assert!(cache.get("big").is_none());
        assert_eq!(cache.used(), 8);
        // The same hash is kept once.
        cache.put("c", &[3; 4], "image/gif");
        assert_eq!(cache.used(), 8);
        // A blob as large as the bound takes the whole cache.
        cache.put("full", &[9; 10], "image/webp");
        assert_eq!(cache.used(), 10);
        assert!(cache.get("a").is_none() && cache.get("c").is_none());
    }

    #[test]
    fn the_bound_of_the_cache_is_the_one_of_the_spec() {
        assert_eq!(BLOB_CACHE_BYTES, 64 * 1024 * 1024);
    }
}
