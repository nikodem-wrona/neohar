//! A link that comes back.
//!
//! A [`PersistentLink`] is the link of one machine. It holds the subscriptions, the streams and
//! the input in flight of the host. When the link goes down it opens a new one, and then does
//! these steps, in this order:
//!
//! 1. It checks `Welcome`. If `machine_uuid` is not the one of the first link, it stops: the
//!    target is a different machine. If `core_epoch` changed, the core restarted.
//! 2. It subscribes again to each live projection, with `since` set to the last version that it
//!    gave to the host. After a restart of the core it takes a new snapshot. A core that cannot
//!    catch up sends `resync`, and the host takes a snapshot as it always does.
//! 3. It opens each render stream again with the same params and sink. The core starts with
//!    keyframes.
//! 4. It sends again each `pane.input`, `pane.paste` and `pane.send_bytes` that got no reply, with
//!    the same `input_id`, if the input is less than 30 s old and the core did not restart. The
//!    holder drops a repeated id. Else the call fails with `input_unconfirmed`, and the
//!    reason is `link_lost` or `core_restarted`.
//! 5. Every other call that got no reply failed with `link_down` when the link went down. Its
//!    outcome is unknown.
//!
//! While the link is down a new call fails at once with `link_down`.
//!
//! The pause between two tries is [`crate::reconnect::Backoff`]. [`PersistentLink::nudge`] makes
//! the next try at once: the Mac woke up, or the network path changed.

use std::{
    sync::{
        Arc, Mutex, MutexGuard, PoisonError,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

use async_trait::async_trait;
use nh_core::{ErrorCode, FrameSink, MessageSink, RpcError};
use nh_proto::api::ids::WindowId;
use serde_json::{Value, json};
use tokio::sync::{Notify, watch};

use crate::{
    Device, Welcome,
    client::{BlobResult, LinkClient, link_down},
    link::Link,
    reconnect::Backoff,
};

/// How long an input that got no reply is kept for a new link.
pub const INPUT_RESEND_WITHIN: Duration = Duration::from_secs(30);

/// The methods that send input to a pane. Their ids make a second send harmless.
const INPUT_METHODS: [&str; 3] = ["pane.input", "pane.paste", "pane.send_bytes"];

/// Why a try to open a link failed, and whether another try can help.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConnectError {
    /// The network, the machine or ssh is not ready: try again.
    Retry(String),
    /// Another try gives the same answer: a host key, a key that is refused, a missing `nh`, a
    /// version that does not match.
    Fatal(String),
}

/// Opens a link. The ssh start is one; a test has its own.
#[async_trait]
pub trait Connector: Send + Sync + 'static {
    async fn connect(&self) -> Result<LinkClient, ConnectError>;
}

/// The state of a [`PersistentLink`], for the row of the machine and for the banner.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LinkStatus {
    /// A link is up.
    Up,
    /// The link was lost. `trying` says that a try runs now, else the next try comes in `retry_in`.
    Reconnecting {
        reason: String,
        trying: bool,
        retry_in: Option<Duration>,
    },
    /// The link is gone for good: the person ended it, or no try can help.
    Stopped(String),
}

impl LinkStatus {
    /// What the person reads about a link that is not up: why, and when the next try comes.
    pub fn reason_text(&self) -> Option<String> {
        match self {
            Self::Up => None,
            Self::Reconnecting {
                reason,
                trying: true,
                ..
            } => Some(format!("{reason}. Connecting again.")),
            Self::Reconnecting {
                reason,
                retry_in: Some(pause),
                ..
            } if !pause.is_zero() => Some(format!(
                "{reason}. Trying again in {:.1} s.",
                pause.as_secs_f32()
            )),
            Self::Reconnecting { reason, .. } => Some(format!("{reason}. Trying again.")),
            Self::Stopped(reason) => Some(reason.clone()),
        }
    }
}

/// Opens a link to `link.sock` of a core on this Mac: the link of an app that is the
/// client of a daemon, and the dev WebSocket of the tests.
pub struct UnixConnector {
    pub path: std::path::PathBuf,
    pub hello: crate::client::ClientHello,
    pub host: Arc<dyn nh_core::Host>,
    pub options: crate::client::ClientOptions,
}

#[async_trait]
impl Connector for UnixConnector {
    async fn connect(&self) -> Result<LinkClient, ConnectError> {
        use crate::client::LinkError;
        LinkClient::connect_unix(
            &self.path,
            self.hello.clone(),
            Arc::clone(&self.host),
            self.options,
        )
        .await
        .map_err(|error| match error {
            // No listener: the core is not up yet, or is up again soon.
            LinkError::Io(_) | LinkError::Timeout | LinkError::Closed(_) => {
                ConnectError::Retry(error.to_string())
            }
            LinkError::Refused(_) | LinkError::Unexpected(_) => {
                ConnectError::Fatal(error.to_string())
            }
        })
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// A subscription of the host, which the link restores.
struct SubRecord {
    id: String,
    name: String,
    tracking: Arc<Tracking>,
}

/// What the host gets from a subscription, and the last version it got.
struct Tracking {
    sink: Box<dyn MessageSink>,
    /// The last version of a snapshot or a patch. 0: none, so the next subscribe takes a snapshot.
    version: AtomicU64,
    alive: AtomicBool,
    /// The subscription on the current link.
    link_sub: Mutex<Option<String>>,
}

struct TrackingSink(Arc<Tracking>);

impl MessageSink for TrackingSink {
    fn send(&self, message: Value) -> bool {
        match message["type"].as_str() {
            Some("snapshot" | "patch") => {
                if let Some(version) = message["version"].as_u64() {
                    self.0.version.store(version, Ordering::SeqCst);
                }
            }
            Some("resync") => self.0.version.store(0, Ordering::SeqCst),
            _ => {}
        }
        let kept = self.0.sink.send(message);
        if !kept {
            self.0.alive.store(false, Ordering::SeqCst);
        }
        kept
    }
}

/// A stream of the host, which the link opens again. `device` is a phone that the host relays.
struct StreamRecord {
    kind: String,
    params: Value,
    window: Option<WindowId>,
    device: Option<Device>,
    sink: Arc<LiveFrames>,
}

struct LiveFrames {
    sink: Arc<dyn FrameSink>,
    alive: AtomicBool,
}

impl FrameSink for LiveFrames {
    fn send(&self, frame: Vec<u8>) -> bool {
        let kept = self.sink.send(frame);
        if !kept {
            self.alive.store(false, Ordering::SeqCst);
        }
        kept
    }

    fn is_closed(&self) -> bool {
        self.sink.is_closed() || !self.alive.load(Ordering::SeqCst)
    }
}

struct State {
    /// The link that is up, and the epoch of its core.
    current: Option<(LinkClient, String)>,
    /// Counts the links that were installed. A call that lost its link waits for a newer one.
    generation: u64,
    /// The last `Welcome`, for the code that asks while the link is down.
    welcome: Welcome,
    subs: Vec<SubRecord>,
    streams: Vec<StreamRecord>,
    owned: Vec<WindowId>,
}

struct Shared {
    connector: Arc<dyn Connector>,
    state: Mutex<State>,
    status: watch::Sender<LinkStatus>,
    /// Wakes the supervisor for a nudge or a stop.
    wake: Notify,
    nudges: AtomicU64,
    stopped: AtomicBool,
    next: AtomicU64,
    /// The machine of the first link (step 1 of `install`).
    machine_uuid: String,
}

/// Ends the supervisor when the last handle goes.
struct Owner(Arc<Shared>);

impl Drop for Owner {
    fn drop(&mut self) {
        self.0.stop("the client ended");
    }
}

/// The link of one machine, which survives the loss of a link. Cloning is cheap. The link ends
/// when the last clone drops.
#[derive(Clone)]
pub struct PersistentLink {
    shared: Arc<Shared>,
    _owner: Arc<Owner>,
}

impl std::fmt::Debug for PersistentLink {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PersistentLink")
            .field("status", &*self.shared.status.borrow())
            .finish_non_exhaustive()
    }
}

impl Shared {
    fn stop(&self, reason: &str) {
        if self.stopped.swap(true, Ordering::SeqCst) {
            return;
        }
        let current = lock(&self.state).current.take();
        if let Some((client, _)) = current {
            client.close(reason);
        }
        self.status
            .send_replace(LinkStatus::Stopped(reason.to_owned()));
        self.wake.notify_waiters();
        self.wake.notify_one();
    }

    fn set(&self, status: LinkStatus) {
        if !self.stopped.load(Ordering::SeqCst) {
            self.status.send_replace(status);
        }
    }
}

impl PersistentLink {
    /// Opens the first link. An error here is the answer to the person who asked for the link:
    /// nothing is retried.
    pub async fn connect(connector: Arc<dyn Connector>) -> Result<Self, ConnectError> {
        let client = connector.connect().await?;
        let welcome = client.welcome().clone();
        let epoch = welcome.core_epoch.clone();
        let (status, _) = watch::channel(LinkStatus::Up);
        let shared = Arc::new(Shared {
            connector,
            state: Mutex::new(State {
                current: Some((client, epoch)),
                generation: 0,
                welcome: welcome.clone(),
                subs: Vec::new(),
                streams: Vec::new(),
                owned: Vec::new(),
            }),
            status,
            wake: Notify::new(),
            nudges: AtomicU64::new(0),
            stopped: AtomicBool::new(false),
            next: AtomicU64::new(1),
            machine_uuid: welcome.machine_uuid,
        });
        tokio::spawn(supervise(Arc::clone(&shared)));
        Ok(Self {
            _owner: Arc::new(Owner(Arc::clone(&shared))),
            shared,
        })
    }

    /// The state of the link. The value changes when the link goes down and when it comes back.
    pub fn status(&self) -> watch::Receiver<LinkStatus> {
        self.shared.status.subscribe()
    }

    /// The Mac woke up, or the network path changed: close the link if one is up, and try to open
    /// a new one at once.
    pub fn nudge(&self) {
        self.shared.nudges.fetch_add(1, Ordering::SeqCst);
        self.shared.wake.notify_one();
    }

    /// The link that is up, with the epoch of its core. Else `link_down`.
    fn current(&self) -> Result<(LinkClient, String), RpcError> {
        lock(&self.shared.state).current.clone().ok_or_else(|| {
            match &*self.shared.status.borrow() {
                LinkStatus::Stopped(reason) => RpcError::new(
                    ErrorCode::LinkDown,
                    format!("the link to the machine ended: {reason}"),
                ),
                LinkStatus::Reconnecting { reason, .. } => RpcError::new(
                    ErrorCode::LinkDown,
                    format!("the link to the machine is down: {reason}"),
                ),
                LinkStatus::Up => link_down(),
            }
        })
    }

    /// Waits for a link newer than `generation`: the one that replaces the link that a call lost.
    /// Fails when the link stops or `deadline` passes. Comparing the number and not the status
    /// matters: just after a loss the status can still say `Up`.
    async fn wait_newer(
        &self,
        generation: u64,
        deadline: Instant,
    ) -> Result<(LinkClient, String, u64), ()> {
        let mut status = self.shared.status.subscribe();
        loop {
            {
                let state = lock(&self.shared.state);
                if state.generation > generation
                    && let Some((client, epoch)) = state.current.clone()
                {
                    return Ok((client, epoch, state.generation));
                }
            }
            if matches!(*status.borrow_and_update(), LinkStatus::Stopped(_)) {
                return Err(());
            }
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() || tokio::time::timeout(left, status.changed()).await.is_err() {
                return Err(());
            }
        }
    }

    /// An input call: it is sent again on a new link, with the same id, for 30 s.
    async fn call_input(
        &self,
        device: Option<&Device>,
        window: Option<WindowId>,
        method: &str,
        params: Value,
    ) -> Result<Value, RpcError> {
        let deadline = Instant::now() + INPUT_RESEND_WITHIN;
        let input_id = params["input_id"].as_u64();
        let unconfirmed = |reason: &str| {
            let mut error = RpcError::new(
                ErrorCode::InputUnconfirmed,
                "the link went down while this input was in flight, and it is not known if it arrived",
            );
            error.details = json!({"input_id": input_id, "reason": reason});
            error
        };
        // A first send needs a link. The calls after it wait for the next one.
        let (mut client, epoch) = self.current()?;
        let mut generation = lock(&self.shared.state).generation;
        loop {
            match client
                .call_for(device.cloned(), window, method, params.clone())
                .await
            {
                Err(error) if error.code == ErrorCode::LinkDown => {
                    let Ok((next, next_epoch, next_generation)) =
                        self.wait_newer(generation, deadline).await
                    else {
                        return Err(unconfirmed("link_lost"));
                    };
                    if next_epoch != epoch {
                        // The holder knows the ids of one run of the core: do not send again.
                        return Err(unconfirmed("core_restarted"));
                    }
                    (client, generation) = (next, next_generation);
                }
                other => return other,
            }
        }
    }
}

/// What opens a stream again on a new link: the kind, the params, the window and the phone.
type StreamOpen = (String, Value, Option<WindowId>, Option<Device>);

impl PersistentLink {
    /// Opens a stream and keeps it for the next link. A stream whose receiver is gone is forgotten
    /// here, so the list does not grow.
    async fn open_stream(
        &self,
        device: Option<Device>,
        kind: &str,
        params: Value,
        window: Option<WindowId>,
        sink: Arc<dyn FrameSink>,
    ) -> Result<(), RpcError> {
        let (client, _) = self.current()?;
        let live = Arc::new(LiveFrames {
            sink,
            alive: AtomicBool::new(true),
        });
        client
            .stream_for(
                device.clone(),
                kind,
                params.clone(),
                window,
                Arc::clone(&live) as Arc<dyn FrameSink>,
            )
            .await?;
        let mut state = lock(&self.shared.state);
        state
            .streams
            .retain(|s| s.sink.alive.load(Ordering::SeqCst));
        state.streams.push(StreamRecord {
            kind: kind.to_owned(),
            params,
            window,
            device,
            sink: live,
        });
        Ok(())
    }
}

/// The supervisor: waits for the link to end, then opens a new one.
async fn supervise(shared: Arc<Shared>) {
    let mut seen = 0u64;
    loop {
        let Some(client) = lock(&shared.state).current.as_ref().map(|(c, _)| c.clone()) else {
            return;
        };
        let wire = client.wire();
        let mut nudged = false;
        let reason = loop {
            tokio::select! {
                reason = wire.wait_down() => break reason,
                () = shared.wake.notified() => {
                    if shared.stopped.load(Ordering::SeqCst) {
                        return;
                    }
                    let now = shared.nudges.load(Ordering::SeqCst);
                    if now != seen {
                        seen = now;
                        nudged = true;
                        // The old link may be dead without TCP knowing. Close it and make a new
                        // one.
                        client.close("the network changed");
                    }
                }
            }
        };
        drop(client);
        if shared.stopped.load(Ordering::SeqCst) {
            return;
        }
        lock(&shared.state).current = None;
        let reason = if nudged {
            format!("the network changed ({reason})")
        } else {
            reason
        };
        match reconnect(&shared, &reason, &mut seen, nudged).await {
            Some(()) => {}
            None => return,
        }
    }
}

/// Opens a link again, with the pauses of [`Backoff`]. Returns `None` when the link is stopped.
async fn reconnect(shared: &Arc<Shared>, lost: &str, seen: &mut u64, nudged: bool) -> Option<()> {
    let mut backoff = Backoff::new();
    if nudged {
        backoff.nudge();
    }
    let mut reason = lost.to_owned();
    loop {
        let pause = backoff.pause();
        if !pause.is_zero() {
            shared.set(LinkStatus::Reconnecting {
                reason: reason.clone(),
                trying: false,
                retry_in: Some(pause),
            });
            let until = tokio::time::Instant::now() + pause;
            loop {
                tokio::select! {
                    () = tokio::time::sleep_until(until) => break,
                    () = shared.wake.notified() => {
                        if shared.stopped.load(Ordering::SeqCst) {
                            return None;
                        }
                        let now = shared.nudges.load(Ordering::SeqCst);
                        if now != *seen {
                            // A nudge brings the try forward. The pauses go on as they were.
                            *seen = now;
                            break;
                        }
                        // A stale wake: the pause goes on.
                    }
                }
            }
        }
        if shared.stopped.load(Ordering::SeqCst) {
            return None;
        }
        shared.set(LinkStatus::Reconnecting {
            reason: reason.clone(),
            trying: true,
            retry_in: None,
        });
        let attempt = shared.connector.connect();
        let result = tokio::select! {
            result = attempt => result,
            () = async {
                // A stop ends the try. A nudge during a try is no news: the try is the answer.
                loop {
                    shared.wake.notified().await;
                    if shared.stopped.load(Ordering::SeqCst) {
                        break;
                    }
                }
            } => return None,
        };
        match result {
            Ok(client) => match install(shared, client).await {
                Ok(()) => return Some(()),
                Err(Install::Stop(message)) => {
                    shared.stop(&message);
                    return None;
                }
                Err(Install::Retry(message)) => reason = message,
            },
            Err(ConnectError::Fatal(message)) => {
                shared.stop(&message);
                return None;
            }
            Err(ConnectError::Retry(message)) => reason = message,
        }
    }
}

enum Install {
    Stop(String),
    Retry(String),
}

/// The steps on a new link.
async fn install(shared: &Arc<Shared>, client: LinkClient) -> Result<(), Install> {
    let welcome = client.welcome().clone();
    if welcome.machine_uuid != shared.machine_uuid {
        client.close("the target is a different machine");
        return Err(Install::Stop(
            "the target is now a different machine".to_owned(),
        ));
    }
    let (epoch_changed, owned, subs, streams) = {
        let state = lock(&shared.state);
        let known = &state.welcome.core_epoch;
        let subs: Vec<(String, Arc<Tracking>)> = state
            .subs
            .iter()
            .filter(|s| s.tracking.alive.load(Ordering::SeqCst))
            .map(|s| (s.name.clone(), Arc::clone(&s.tracking)))
            .collect();
        let streams: Vec<(StreamOpen, Arc<LiveFrames>)> = state
            .streams
            .iter()
            .filter(|s| s.sink.alive.load(Ordering::SeqCst))
            .map(|s| {
                (
                    (s.kind.clone(), s.params.clone(), s.window, s.device.clone()),
                    Arc::clone(&s.sink),
                )
            })
            .collect();
        (
            *known != welcome.core_epoch,
            state.owned.clone(),
            subs,
            streams,
        )
    };
    client.set_owned_windows(owned);
    // Step 2: the projections, from the last version that the host got.
    for (name, tracking) in subs {
        let since = match tracking.version.load(Ordering::SeqCst) {
            0 => None,
            _ if epoch_changed => None,
            version => Some(version),
        };
        match client
            .subscribe(&name, since, Box::new(TrackingSink(Arc::clone(&tracking))))
            .await
        {
            Ok(link_sub) => *lock(&tracking.link_sub) = Some(link_sub),
            Err(error) if error.code == ErrorCode::LinkDown => {
                return Err(Install::Retry("the link went down again".to_owned()));
            }
            Err(error) => {
                // The page takes a snapshot, as it does after any `resync`.
                tracing::warn!(name, %error, "a subscription did not come back");
                tracking.sink.send(json!({"type": "resync"}));
            }
        }
    }
    // Step 3: the streams. The core starts each with keyframes.
    for ((kind, params, window, device), sink) in streams {
        match client
            .stream_for(
                device,
                &kind,
                params,
                window,
                Arc::clone(&sink) as Arc<dyn FrameSink>,
            )
            .await
        {
            Ok(()) => {}
            Err(error) if error.code == ErrorCode::LinkDown => {
                return Err(Install::Retry("the link went down again".to_owned()));
            }
            Err(error) => {
                tracing::warn!(kind, %error, "a stream did not come back");
                sink.alive.store(false, Ordering::SeqCst);
            }
        }
    }
    {
        let mut state = lock(&shared.state);
        state.welcome = welcome.clone();
        state.current = Some((client, welcome.core_epoch));
        state.generation += 1;
    }
    shared.set(LinkStatus::Up);
    Ok(())
}

#[async_trait]
impl Link for PersistentLink {
    fn welcome(&self) -> Welcome {
        lock(&self.shared.state).welcome.clone()
    }

    fn rtt(&self) -> Option<Duration> {
        lock(&self.shared.state)
            .current
            .as_ref()
            .and_then(|(client, _)| client.rtt())
    }

    fn is_up(&self) -> bool {
        matches!(*self.shared.status.borrow(), LinkStatus::Up)
    }

    async fn closed(&self) -> String {
        let mut status = self.shared.status.subscribe();
        loop {
            if let LinkStatus::Stopped(reason) = &*status.borrow_and_update() {
                return reason.clone();
            }
            if status.changed().await.is_err() {
                return "the link object ended".to_owned();
            }
        }
    }

    fn close(&self, reason: &str) {
        self.shared.stop(reason);
    }

    fn set_owned_windows(&self, windows: Vec<WindowId>) {
        let mut state = lock(&self.shared.state);
        state.owned.clone_from(&windows);
        if let Some((client, _)) = &state.current {
            client.set_owned_windows(windows);
        }
    }

    fn own_window(&self, window: WindowId, owned: bool) {
        let mut state = lock(&self.shared.state);
        state.owned.retain(|known| *known != window);
        if owned {
            state.owned.push(window);
        }
        if let Some((client, _)) = &state.current {
            client.own_window(window, owned);
        }
    }

    async fn call(
        &self,
        window: Option<WindowId>,
        method: &str,
        params: Value,
    ) -> Result<Value, RpcError> {
        if method == "projection.unsubscribe" {
            let id = params
                .get("subscription")
                .and_then(Value::as_str)
                .ok_or_else(|| RpcError::invalid_params("subscription is a string"))?;
            return if self.unsubscribe(id) {
                Ok(json!({}))
            } else {
                Err(RpcError::new(
                    ErrorCode::NotFound,
                    format!("no subscription {id}"),
                ))
            };
        }
        if INPUT_METHODS.contains(&method) {
            return self.call_input(None, window, method, params).await;
        }
        let (client, _) = self.current()?;
        client.call(window, method, params).await
    }

    async fn subscribe(
        &self,
        name: &str,
        since: Option<u64>,
        sink: Box<dyn MessageSink>,
    ) -> Result<String, RpcError> {
        let (client, _) = self.current()?;
        let tracking = Arc::new(Tracking {
            sink,
            version: AtomicU64::new(since.unwrap_or(0)),
            alive: AtomicBool::new(true),
            link_sub: Mutex::new(None),
        });
        let link_sub = client
            .subscribe(name, since, Box::new(TrackingSink(Arc::clone(&tracking))))
            .await?;
        *lock(&tracking.link_sub) = Some(link_sub);
        let id = format!(
            "{name}#r{}",
            self.shared.next.fetch_add(1, Ordering::Relaxed)
        );
        lock(&self.shared.state).subs.push(SubRecord {
            id: id.clone(),
            name: name.to_owned(),
            tracking,
        });
        Ok(id)
    }

    fn unsubscribe(&self, subscription: &str) -> bool {
        let (record, client) = {
            let mut state = lock(&self.shared.state);
            let Some(at) = state.subs.iter().position(|s| s.id == subscription) else {
                return false;
            };
            (
                state.subs.remove(at),
                state.current.as_ref().map(|(client, _)| client.clone()),
            )
        };
        record.tracking.alive.store(false, Ordering::SeqCst);
        if let (Some(client), Some(link_sub)) = (client, lock(&record.tracking.link_sub).take()) {
            client.unsubscribe(&link_sub);
        }
        true
    }

    async fn stream(
        &self,
        kind: &str,
        params: Value,
        window: Option<WindowId>,
        sink: Arc<dyn FrameSink>,
    ) -> Result<(), RpcError> {
        self.open_stream(None, kind, params, window, sink).await
    }

    async fn blob(&self, hash: &str) -> BlobResult {
        let (client, _) = self.current()?;
        client.blob(hash).await
    }

    async fn put_blob(&self, name: &str, bytes: Vec<u8>) -> Result<String, RpcError> {
        let (client, _) = self.current()?;
        client.put_blob(name, &bytes).await
    }

    async fn relay_call(
        &self,
        device: &Device,
        window: Option<WindowId>,
        method: &str,
        params: Value,
    ) -> Result<Value, RpcError> {
        if INPUT_METHODS.contains(&method) {
            return self.call_input(Some(device), window, method, params).await;
        }
        let (client, _) = self.current()?;
        client
            .call_for(Some(device.clone()), window, method, params)
            .await
    }

    async fn relay_stream(
        &self,
        device: &Device,
        kind: &str,
        params: Value,
        window: Option<WindowId>,
        sink: Arc<dyn FrameSink>,
    ) -> Result<(), RpcError> {
        self.open_stream(Some(device.clone()), kind, params, window, sink)
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A connector that fails with the given answers, one for each try, and records when each try
    /// came. The tokio clock is paused, so the times are exact.
    struct Script {
        answers: Mutex<std::collections::VecDeque<ConnectError>>,
        tries: Mutex<Vec<tokio::time::Instant>>,
    }

    #[async_trait]
    impl Connector for Script {
        async fn connect(&self) -> Result<LinkClient, ConnectError> {
            lock(&self.tries).push(tokio::time::Instant::now());
            let next = lock(&self.answers).pop_front();
            match next {
                Some(error) => Err(error),
                // The script is out of answers: the try never ends, like a stuck ssh.
                None => std::future::pending().await,
            }
        }
    }

    fn shared_for_tests(connector: Arc<dyn Connector>) -> Arc<Shared> {
        let (status, _) = watch::channel(LinkStatus::Up);
        Arc::new(Shared {
            connector,
            state: Mutex::new(State {
                current: None,
                generation: 0,
                welcome: Welcome {
                    core_version: String::new(),
                    machine_uuid: "m".to_owned(),
                    session_uuid: String::new(),
                    hostname: String::new(),
                    core_epoch: "e".to_owned(),
                },
                subs: Vec::new(),
                streams: Vec::new(),
                owned: Vec::new(),
            }),
            status,
            wake: Notify::new(),
            nudges: AtomicU64::new(0),
            stopped: AtomicBool::new(false),
            next: AtomicU64::new(1),
            machine_uuid: "m".to_owned(),
        })
    }

    fn retry() -> ConnectError {
        ConnectError::Retry("no route".to_owned())
    }

    #[tokio::test(start_paused = true)]
    async fn the_tries_come_after_0_5_1_2_4_8_8_seconds() {
        let script = Arc::new(Script {
            answers: Mutex::new((0..6).map(|_| retry()).collect()),
            tries: Mutex::new(Vec::new()),
        });
        let shared = shared_for_tests(script.clone());
        let started = tokio::time::Instant::now();
        let task = tokio::spawn({
            let shared = Arc::clone(&shared);
            async move {
                let mut seen = 0;
                reconnect(&shared, "lost", &mut seen, false).await
            }
        });
        // The sixth try fails, and a seventh starts at 23.5 s and never ends.
        tokio::time::sleep(Duration::from_secs(24)).await;
        let times: Vec<f32> = lock(&script.tries)
            .iter()
            .map(|t| t.duration_since(started).as_secs_f32())
            .collect();
        assert_eq!(times, [0.5, 1.5, 3.5, 7.5, 15.5, 23.5], "six tries by 24 s");
        // The status says that a try runs.
        assert!(matches!(
            *shared.status.borrow(),
            LinkStatus::Reconnecting { trying: true, .. } | LinkStatus::Reconnecting { .. }
        ));
        shared.stop("test over");
        assert_eq!(task.await.expect("the task ends"), None);
    }

    #[tokio::test(start_paused = true)]
    async fn a_nudge_makes_the_next_try_at_once() {
        let script = Arc::new(Script {
            answers: Mutex::new((0..3).map(|_| retry()).collect()),
            tries: Mutex::new(Vec::new()),
        });
        let shared = shared_for_tests(script.clone());
        let started = tokio::time::Instant::now();
        let task = tokio::spawn({
            let shared = Arc::clone(&shared);
            async move {
                let mut seen = 0;
                reconnect(&shared, "lost", &mut seen, false).await
            }
        });
        // The first try is due at 0.5 s. A nudge at 0.1 s brings it forward.
        tokio::time::sleep(Duration::from_millis(100)).await;
        shared.nudges.fetch_add(1, Ordering::SeqCst);
        shared.wake.notify_one();
        tokio::time::sleep(Duration::from_millis(50)).await;
        {
            let tries = lock(&script.tries);
            assert_eq!(tries.len(), 1, "the nudge made a try at once");
            assert!(tries[0].duration_since(started) < Duration::from_millis(200));
        }
        // The pauses go on as they were: the next one is 1 s after the failed try.
        tokio::time::sleep(Duration::from_millis(1100)).await;
        assert_eq!(lock(&script.tries).len(), 2);
        shared.stop("test over");
        assert_eq!(task.await.expect("the task ends"), None);
    }

    #[tokio::test(start_paused = true)]
    async fn a_fatal_answer_stops_the_link_with_its_reason() {
        let script = Arc::new(Script {
            answers: Mutex::new(
                [
                    retry(),
                    ConnectError::Fatal("Host key verification failed.".to_owned()),
                ]
                .into_iter()
                .collect(),
            ),
            tries: Mutex::new(Vec::new()),
        });
        let shared = shared_for_tests(script.clone());
        let mut seen = 0;
        let result = reconnect(&shared, "lost", &mut seen, false).await;
        assert_eq!(result, None);
        assert_eq!(
            *shared.status.borrow(),
            LinkStatus::Stopped("Host key verification failed.".to_owned())
        );
        assert_eq!(lock(&script.tries).len(), 2);
    }
}
