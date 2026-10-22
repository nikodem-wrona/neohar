//! The links to the remote machines.
//!
//! [`MachineManager`] implements [`MachineLinks`] of the core. It keeps one entry for each
//! `[machines.<label>]`: the state (`disconnected`, `connecting`, `connected`), the reason of the
//! last disconnect, the version of the remote core, the round-trip time and the summary of the
//! harnesses of the machine. A link starts with [`connect_ssh`].
//!
//! - **Launch.** `configure` connects each machine with `connect = "launch"` that is not up.
//! - **Change.** A machine that the table removes is disconnected. A machine whose target changed
//!   is disconnected, and connected again when it is a `launch` machine or was up.
//! - **Summary.** While a machine is up, the manager follows the projection `harnesses` of its
//!   core and copies `installed` and `login` of each harness into the row.
//! - **Reconnect.** A link that was up and goes down comes back by itself: the
//!   row says `connecting` or `disconnected` with the reason while it is down, and `connected`
//!   again after. A wake from sleep or a change of the network brings the next try forward
//!   ([`crate::netwatch`]). A link that cannot come back (a changed host key, a different
//!   machine) stays `disconnected` with its reason until the person connects it again. A first
//!   connection that fails is not retried: the person sees why and asks again.
//!
//! The manager works on the runtime of its caller. Its functions never wait, except `connect`,
//! which waits for the result of the attempt.

use std::{
    collections::BTreeMap,
    ffi::OsString,
    path::PathBuf,
    sync::{Arc, Mutex, MutexGuard, PoisonError},
    time::Duration,
};

use async_trait::async_trait;
use nh_core::{
    ErrorCode, Host, MessageSink, RpcError,
    machines::{MachineConfig, MachineLinks},
};
use nh_proto::api::model::{
    LoginState, MachineHarness, MachineState, MachineUpdate, MachineValue, PortForward,
};
use serde_json::Value;
use tokio::{sync::watch, task::AbortHandle};
use ulid::Ulid;

use crate::{
    client::{ClientHello, ClientOptions, LinkClient},
    forward,
    link::Link,
    netwatch,
    persistent::{ConnectError, Connector, LinkStatus, PersistentLink},
    ssh::{Askpass, ForwardOp, SshError, SshSpec, VersionCheck, connect_ssh, version_check},
};

/// The path of `nh` on a remote machine when the table does not say.
pub const DEFAULT_REMOTE_NH: &str = "~/.local/bin/nh";

/// How often the manager looks at the round-trip time of a link.
const RTT_EVERY: Duration = Duration::from_secs(2);

/// Makes the host that answers the requests of the core of one machine, from its label.
pub type HostFor = Arc<dyn Fn(&str) -> Arc<dyn Host> + Send + Sync>;

/// What the manager needs from the app.
#[derive(Clone)]
pub struct ManagerConfig {
    /// The runtime directory of this Mac: `<runtime>/ssh/` holds the control sockets.
    pub runtime_dir: PathBuf,
    /// The `nh` that runs as `askpass`: the sidecar of the app.
    pub nh: PathBuf,
    /// The `api.sock` of the core that shows the sheets of ssh, and its session.
    pub api_socket: PathBuf,
    pub session: String,
    /// The device of this app, its name, and its version.
    pub device: Ulid,
    pub device_name: String,
    pub app_version: String,
    /// The host that answers the requests of the core of a machine. The label tells which machine,
    /// so each machine gets a host of its own.
    pub host_for: HostFor,
    pub options: ClientOptions,
    /// The `ssh` program, when it is not the system one (tests).
    pub ssh_program: Option<PathBuf>,
    /// More variables for the ssh child (tests).
    pub ssh_env: Vec<(OsString, OsString)>,
}

impl std::fmt::Debug for ManagerConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ManagerConfig")
            .field("runtime_dir", &self.runtime_dir)
            .field("session", &self.session)
            .field("device", &self.device)
            .finish_non_exhaustive()
    }
}

/// The result of an attempt to connect, for the callers that wait for it.
type Outcome = Option<Result<(), RpcError>>;

struct Entry {
    config: MachineConfig,
    row: MachineValue,
    client: Option<PersistentLink>,
    /// Changes at each start and each stop. A task that finds another number ends.
    generation: u64,
    /// The attempt that runs now, for a second caller of `connect`.
    attempt: Option<watch::Receiver<Outcome>>,
    /// The tasks of the attempt or of the link. They end with it.
    tasks: Vec<AbortHandle>,
}

struct Shared {
    config: ManagerConfig,
    runtime: tokio::runtime::Handle,
    entries: Mutex<BTreeMap<String, Entry>>,
    order: Mutex<Vec<String>>,
    notify: Mutex<Option<Arc<dyn Fn() + Send + Sync>>>,
    /// The wake and network tasks. They end with the manager.
    net: Mutex<Option<netwatch::Watch>>,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The links of one app to its remote machines.
#[derive(Clone)]
pub struct MachineManager {
    shared: Arc<Shared>,
}

impl std::fmt::Debug for MachineManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MachineManager").finish_non_exhaustive()
    }
}

impl MachineManager {
    /// A manager whose tasks run on `runtime`.
    pub fn new(config: ManagerConfig, runtime: tokio::runtime::Handle) -> Self {
        let shared = Arc::new(Shared {
            config,
            runtime: runtime.clone(),
            entries: Mutex::new(BTreeMap::new()),
            order: Mutex::new(Vec::new()),
            notify: Mutex::new(None),
            net: Mutex::new(None),
        });
        // A wake or a change of the network makes each link that is up close and try again at
        // once.
        let weak = Arc::downgrade(&shared);
        let _enter = runtime.enter();
        *lock(&shared.net) = Some(netwatch::watch(Arc::new(move |cause| {
            if let Some(shared) = weak.upgrade() {
                shared.nudge(cause);
            }
        })));
        Self { shared }
    }

    /// Brings the next try of each link forward, as a wake or a change of the network does.
    /// Adds a forward to a port of a machine, or returns the one that exists.
    pub async fn add_forward(
        &self,
        label: &str,
        remote_port: u16,
    ) -> Result<PortForward, RpcError> {
        self.shared.add_forward(label, remote_port).await
    }

    pub fn nudge(&self) {
        self.shared.nudge(netwatch::Cause::Network);
    }

    /// Ends every link with `Bye`. The remote machines keep running. The control
    /// master of ssh stays.
    pub fn shutdown(&self) {
        let labels: Vec<String> = lock(&self.shared.entries).keys().cloned().collect();
        for label in labels {
            self.shared.stop(&label, "the app quit");
        }
    }

    /// The link of a machine, for the code that routes calls.
    pub fn client(&self, label: &str) -> Option<PersistentLink> {
        self.client_of(label).map(|(client, _)| client)
    }

    /// The link of a machine and its number. The link survives the loss of a connection, so the
    /// number changes only when the person connects the machine again. A cache that belongs to
    /// the machine (the blobs) can tell a new link from the old.
    pub fn client_of(&self, label: &str) -> Option<(PersistentLink, u64)> {
        lock(&self.shared.entries)
            .get(label)
            .and_then(|entry| Some((entry.client.clone()?, entry.generation)))
    }
}

impl Shared {
    fn nudge(&self, cause: netwatch::Cause) {
        let links: Vec<PersistentLink> = lock(&self.entries)
            .values()
            .filter_map(|entry| entry.client.clone())
            .collect();
        if !links.is_empty() {
            tracing::info!(?cause, links = links.len(), "the links try again at once");
        }
        for link in links {
            link.nudge();
        }
    }

    fn changed(&self) {
        let notify = lock(&self.notify).clone();
        if let Some(notify) = notify {
            notify();
        }
    }

    /// Ends the link or the attempt of a machine, and leaves it `disconnected` with `reason`.
    fn stop(&self, label: &str, reason: &str) {
        let (client, lost) = {
            let mut entries = lock(&self.entries);
            let Some(entry) = entries.get_mut(label) else {
                return;
            };
            entry.generation += 1;
            for task in entry.tasks.drain(..) {
                task.abort();
            }
            entry.attempt = None;
            let lost = (self.spec(label, &entry.config), entry.row.forwards.clone());
            entry.row = disconnected_row(label, &entry.row, reason);
            (entry.client.take(), lost)
        };
        // The control master outlives the link for ten minutes, so the forwards end here.
        self.cancel_forwards(lost.0, lost.1);
        if let Some(client) = client {
            client.close(reason);
        }
        self.changed();
    }

    /// Ends the forwards of a link that ended. The work goes on in a task, and a failure only
    /// means that the master was gone already.
    fn cancel_forwards(&self, spec: SshSpec, forwards: Vec<PortForward>) {
        if forwards.is_empty() {
            return;
        }
        self.runtime.spawn(async move {
            for forward in forwards {
                let _ = spec
                    .forward(ForwardOp::Cancel, forward.local_port, forward.remote_port)
                    .await;
            }
        });
    }

    /// Starts an attempt for `label`. Returns the receiver of its outcome. The caller holds no lock.
    fn start(self: &Arc<Self>, label: &str) -> Option<watch::Receiver<Outcome>> {
        let (tx, rx) = watch::channel(None);
        let (config, generation) = {
            let mut entries = lock(&self.entries);
            let entry = entries.get_mut(label)?;
            entry.generation += 1;
            for task in entry.tasks.drain(..) {
                task.abort();
            }
            entry.row.state = MachineState::Connecting;
            entry.row.reason = None;
            entry.row.latency_ms = None;
            entry.attempt = Some(rx.clone());
            (entry.config.clone(), entry.generation)
        };
        self.changed();
        let shared = Arc::clone(self);
        let label_owned = label.to_owned();
        let task = self.runtime.spawn(async move {
            let result = shared.attempt(&label_owned, &config, generation).await;
            let _ = tx.send(Some(result));
        });
        if let Some(entry) = lock(&self.entries).get_mut(label)
            && entry.generation == generation
        {
            entry.tasks.push(task.abort_handle());
        }
        Some(rx)
    }

    fn spec(&self, label: &str, config: &MachineConfig) -> SshSpec {
        let machine = &config.machine;
        let remote_nh = machine
            .remote_nh
            .clone()
            .unwrap_or_else(|| DEFAULT_REMOTE_NH.to_owned());
        let mut spec = SshSpec::new(
            machine.ssh.clone(),
            machine.session.clone(),
            remote_nh,
            self.config.runtime_dir.clone(),
        );
        if let Some(program) = &self.config.ssh_program {
            spec.program.clone_from(program);
        }
        spec.askpass = Some(Askpass {
            nh: self.config.nh.clone(),
            socket: self.config.api_socket.clone(),
            session: self.config.session.clone(),
            machine: label.to_owned(),
        });
        spec.env.clone_from(&self.config.ssh_env);
        spec
    }

    /// One attempt: the ssh start, then the state of the row.
    async fn attempt(
        self: &Arc<Self>,
        label: &str,
        config: &MachineConfig,
        generation: u64,
    ) -> Result<(), RpcError> {
        let connector = Arc::new(SshConnector {
            spec: self.spec(label, config),
            hello: ClientHello {
                device: self.config.device,
                name: self.config.device_name.clone(),
                app_version: self.config.app_version.clone(),
            },
            host: (self.config.host_for)(label),
            options: self.config.options,
            last: Mutex::new(None),
        });
        match PersistentLink::connect(Arc::clone(&connector) as Arc<dyn Connector>).await {
            Ok(link) => self.connected(label, generation, link),
            Err(error) => {
                let error = match lock(&connector.last).take() {
                    Some(error) => error,
                    None => SshError::Ssh {
                        status: None,
                        stderr: match error {
                            ConnectError::Retry(m) | ConnectError::Fatal(m) => m,
                        },
                    },
                };
                self.failed(label, generation, &error)
            }
        }
    }

    fn connected(
        self: &Arc<Self>,
        label: &str,
        generation: u64,
        link: PersistentLink,
    ) -> Result<(), RpcError> {
        {
            let mut entries = lock(&self.entries);
            let Some(entry) = entries
                .get_mut(label)
                .filter(|entry| entry.generation == generation)
            else {
                // The person disconnected, or the target changed, while ssh worked.
                drop(entries);
                link.close("the attempt was replaced");
                return Err(RpcError::new(
                    ErrorCode::LinkDown,
                    "the attempt was cancelled",
                ));
            };
            entry.row.state = MachineState::Connected;
            entry.row.reason = None;
            self.fill_up(&mut entry.row, &link);
            entry.client = Some(link.clone());
            entry.attempt = None;
            let shared = Arc::clone(self);
            let owned = label.to_owned();
            let task = self.runtime.spawn(async move {
                shared.follow(&owned, generation, link).await;
            });
            entry.tasks.push(task.abort_handle());
        }
        self.changed();
        Ok(())
    }

    /// The parts of the row that a live link knows: the version, the note about it, and the
    /// round-trip time.
    fn fill_up(&self, row: &mut MachineValue, link: &PersistentLink) {
        let welcome = link.welcome();
        row.update = match version_check(&welcome, &self.config.app_version) {
            VersionCheck::UpdateAvailable => Some(MachineUpdate::UpdateAvailable),
            VersionCheck::UpdateApp => Some(MachineUpdate::UpdateApp),
            VersionCheck::Mismatch => Some(MachineUpdate::VersionMismatch),
            VersionCheck::Current | VersionCheck::Unknown => None,
        };
        row.version = Some(welcome.core_version);
        row.latency_ms = link.rtt().map(millis);
    }

    fn failed(&self, label: &str, generation: u64, error: &SshError) -> Result<(), RpcError> {
        let (reason, update, code) = describe(error);
        {
            let mut entries = lock(&self.entries);
            if let Some(entry) = entries
                .get_mut(label)
                .filter(|entry| entry.generation == generation)
            {
                entry.row = disconnected_row(label, &entry.row, &reason);
                entry.row.update = update;
                entry.row.version = None;
                entry.attempt = None;
                entry.client = None;
            }
        }
        self.changed();
        Err(RpcError::new(code, reason))
    }

    /// While the machine is linked: the summary of the harnesses, the state of the link and its
    /// round-trip time. The link keeps its subscriptions across a reconnect, so this task
    /// subscribes once.
    async fn follow(self: &Arc<Self>, label: &str, generation: u64, link: PersistentLink) {
        let summary = Arc::new(Mutex::new(Summary::default()));
        let sink = SummarySink {
            shared: Arc::downgrade(self),
            label: label.to_owned(),
            generation,
            summary: Arc::clone(&summary),
        };
        if let Err(error) = link.subscribe("harnesses", None, Box::new(sink)).await {
            tracing::warn!(machine = label, %error, "the harnesses of a machine are not followed");
        }
        let mut status = link.status();
        let mut tick = tokio::time::interval(RTT_EVERY);
        let mut was_up = true;
        loop {
            tokio::select! {
                changed = status.changed() => {
                    if changed.is_err() {
                        return;
                    }
                    let now = status.borrow_and_update().clone();
                    if self.status(label, generation, &link, &now) {
                        return;
                    }
                    // A link that comes back may have a new master, and a new master has none
                    // of the forwards.
                    let up = matches!(now, LinkStatus::Up);
                    if up && !was_up {
                        self.reapply_forwards(label, generation).await;
                    }
                    was_up = up;
                }
                _ = tick.tick() => self.latency(label, generation, &link),
            }
        }
    }

    /// The state of the link changed. Returns true when the link is gone for good.
    fn status(
        &self,
        label: &str,
        generation: u64,
        link: &PersistentLink,
        now: &LinkStatus,
    ) -> bool {
        let mut stopped = false;
        let mut lost = None;
        {
            let mut entries = lock(&self.entries);
            let Some(entry) = entries
                .get_mut(label)
                .filter(|entry| entry.generation == generation)
            else {
                return true;
            };
            match now {
                LinkStatus::Up => {
                    entry.row.state = MachineState::Connected;
                    entry.row.reason = None;
                    self.fill_up(&mut entry.row, link);
                }
                LinkStatus::Reconnecting { trying, .. } => {
                    entry.row.state = if *trying {
                        MachineState::Connecting
                    } else {
                        MachineState::Disconnected
                    };
                    entry.row.latency_ms = None;
                    entry.row.reason = now.reason_text();
                }
                LinkStatus::Stopped(reason) => {
                    lost = Some((self.spec(label, &entry.config), entry.row.forwards.clone()));
                    entry.row = disconnected_row(label, &entry.row, reason);
                    entry.client = None;
                    entry.tasks.clear();
                    stopped = true;
                }
            }
        }
        if let Some((spec, forwards)) = lost {
            self.cancel_forwards(spec, forwards);
        }
        self.changed();
        stopped
    }

    fn latency(&self, label: &str, generation: u64, link: &PersistentLink) {
        let latency = link.rtt().map(millis);
        let changed = {
            let mut entries = lock(&self.entries);
            match entries
                .get_mut(label)
                .filter(|entry| entry.generation == generation)
            {
                Some(entry)
                    if entry.row.state == MachineState::Connected
                        && entry.row.latency_ms != latency =>
                {
                    entry.row.latency_ms = latency;
                    true
                }
                _ => false,
            }
        };
        if changed {
            self.changed();
        }
    }

    /// Adds a forward from this Mac to `remote_port` of the machine, or
    /// returns the one that exists. It needs a link that is up: `ssh -O forward` talks to the
    /// master of that link.
    async fn add_forward(&self, label: &str, remote_port: u16) -> Result<PortForward, RpcError> {
        let (spec, generation) = {
            let entries = lock(&self.entries);
            let entry = entries.get(label).ok_or_else(|| {
                RpcError::new(ErrorCode::NotFound, format!("no machine {label:?}"))
            })?;
            if entry.row.state != MachineState::Connected {
                return Err(RpcError::new(
                    ErrorCode::LinkDown,
                    format!("the link to {label} is not up"),
                ));
            }
            if let Some(known) = entry
                .row
                .forwards
                .iter()
                .find(|forward| forward.remote_port == remote_port)
            {
                return Ok(*known);
            }
            (self.spec(label, &entry.config), entry.generation)
        };
        let local_port = forward::pick_local_port(remote_port)
            .ok_or_else(|| RpcError::new(ErrorCode::ForwardFailed, "no free port on this Mac"))?;
        spec.forward(ForwardOp::Forward, local_port, remote_port)
            .await
            .map_err(|error| RpcError::new(ErrorCode::ForwardFailed, error.to_string()))?;
        let added = PortForward {
            remote_port,
            local_port,
        };
        let kept = {
            let mut entries = lock(&self.entries);
            match entries
                .get_mut(label)
                .filter(|entry| entry.generation == generation)
            {
                Some(entry) => {
                    entry.row.forwards.push(added);
                    true
                }
                None => false,
            }
        };
        if kept {
            self.changed();
        } else {
            // The link ended while ssh worked: the forward would outlive it.
            let _ = spec
                .forward(ForwardOp::Cancel, local_port, remote_port)
                .await;
            return Err(RpcError::new(ErrorCode::LinkDown, "the link ended"));
        }
        Ok(added)
    }

    /// Adds the forwards of a row again after the link came back. A forward that ssh refuses
    /// leaves the row, so the row does not list what does not work.
    async fn reapply_forwards(&self, label: &str, generation: u64) {
        let (spec, forwards) = {
            let entries = lock(&self.entries);
            match entries
                .get(label)
                .filter(|entry| entry.generation == generation)
            {
                Some(entry) if !entry.row.forwards.is_empty() => {
                    (self.spec(label, &entry.config), entry.row.forwards.clone())
                }
                _ => return,
            }
        };
        let mut failed = Vec::new();
        for forward in forwards {
            // The old master may still have it.
            let _ = spec
                .forward(ForwardOp::Cancel, forward.local_port, forward.remote_port)
                .await;
            if let Err(error) = spec
                .forward(ForwardOp::Forward, forward.local_port, forward.remote_port)
                .await
            {
                tracing::warn!(machine = label, %error, "a forward did not come back");
                failed.push(forward);
            }
        }
        if failed.is_empty() {
            return;
        }
        if let Some(entry) = lock(&self.entries)
            .get_mut(label)
            .filter(|entry| entry.generation == generation)
        {
            entry
                .row
                .forwards
                .retain(|forward| !failed.contains(forward));
        }
        self.changed();
    }

    /// Copies the summary of the harnesses into the row of the machine.
    fn set_harnesses(&self, label: &str, generation: u64, harnesses: Vec<MachineHarness>) {
        let changed = {
            let mut entries = lock(&self.entries);
            match entries
                .get_mut(label)
                .filter(|entry| entry.generation == generation)
            {
                Some(entry) if entry.row.harnesses != harnesses => {
                    entry.row.harnesses = harnesses;
                    true
                }
                _ => false,
            }
        };
        if changed {
            self.changed();
        }
    }
}

/// The ssh start as a [`Connector`]: the first connection and each reconnection.
struct SshConnector {
    spec: SshSpec,
    hello: ClientHello,
    host: Arc<dyn Host>,
    options: ClientOptions,
    /// The error of the last try, for the row of the machine.
    last: Mutex<Option<SshError>>,
}

#[async_trait]
impl Connector for SshConnector {
    async fn connect(&self) -> Result<LinkClient, ConnectError> {
        match connect_ssh(
            &self.spec,
            self.hello.clone(),
            Arc::clone(&self.host),
            self.options,
        )
        .await
        {
            Ok(client) => Ok(client),
            Err(error) => {
                let answer = if retryable(&error) {
                    ConnectError::Retry(error.to_string())
                } else {
                    ConnectError::Fatal(describe(&error).0)
                };
                *lock(&self.last) = Some(error);
                Err(answer)
            }
        }
    }
}

/// Whether another try can give another answer. A network that is not up, a machine that
/// restarts and an ssh that cannot start are over soon. A host key that changed, a key that is
/// refused, a missing or old `nh` and a version that does not match are not: they stay until the
/// person acts.
fn retryable(error: &SshError) -> bool {
    match error {
        SshError::Spawn(_) | SshError::Link(_) => true,
        SshError::Ssh { stderr, .. } => {
            let fixed = [
                "Host key verification failed",
                "REMOTE HOST IDENTIFICATION HAS CHANGED",
                "Permission denied",
            ];
            !error.nh_is_missing() && !fixed.iter().any(|text| stderr.contains(text))
        }
        SshError::Setup { .. }
        | SshError::Invalid(_)
        | SshError::BridgeTooOld { .. }
        | SshError::Refused(_) => false,
    }
}

/// A round-trip time in milliseconds with one decimal, so that a small change is not a patch.
fn millis(rtt: Duration) -> f32 {
    let ms = rtt.as_secs_f32() * 1000.0;
    (ms * 10.0).round() / 10.0
}

fn new_row(label: &str) -> MachineValue {
    MachineValue {
        label: label.to_owned(),
        state: MachineState::Disconnected,
        latency_ms: None,
        version: None,
        reason: None,
        forwards: Vec::new(),
        harnesses: Vec::new(),
        update: None,
    }
}

/// The row of a machine that is down. It keeps the version and the install note (the person
/// needs them), and drops what only a live link knows.
fn disconnected_row(label: &str, before: &MachineValue, reason: &str) -> MachineValue {
    MachineValue {
        state: MachineState::Disconnected,
        reason: Some(reason.to_owned()),
        version: before.version.clone(),
        update: before.update,
        harnesses: before.harnesses.clone(),
        ..new_row(label)
    }
}

/// The reason for the person, the note for the row, and the code of the error of `machine.connect`.
fn describe(error: &SshError) -> (String, Option<MachineUpdate>, ErrorCode) {
    match error {
        SshError::BridgeTooOld { version, .. } => (
            format!(
                "the neohar on that machine is too old (version {version}): install or update it"
            ),
            Some(MachineUpdate::InstallNeeded),
            ErrorCode::LinkDown,
        ),
        e if e.nh_is_missing() => (
            "neohar is not installed on that machine".to_owned(),
            Some(MachineUpdate::InstallNeeded),
            ErrorCode::LinkDown,
        ),
        e if e.is_version_mismatch() => (
            "the neohar on that machine has another major version of the API".to_owned(),
            Some(MachineUpdate::VersionMismatch),
            ErrorCode::VersionMismatch,
        ),
        other => (other.to_string(), None, ErrorCode::LinkDown),
    }
}

/// The harnesses of a machine as its projection says: `id → (installed, login)`.
#[derive(Default)]
struct Summary {
    items: BTreeMap<String, MachineHarness>,
}

impl Summary {
    /// Applies a message of the projection `harnesses`. Returns false for a `resync`.
    fn apply(&mut self, message: &Value) -> bool {
        match message["type"].as_str() {
            Some("snapshot") => {
                self.items.clear();
                if let Some(items) = message["data"]["items"].as_object() {
                    for (key, value) in items {
                        self.set(key, value);
                    }
                }
                true
            }
            Some("patch") => {
                for op in message["ops"].as_array().into_iter().flatten() {
                    match op["op"].as_str() {
                        Some("set") => {
                            if let Some(key) = op["key"].as_str() {
                                self.set(key, &op["value"]);
                            }
                        }
                        Some("remove") => {
                            if let Some(key) = op["key"].as_str() {
                                self.items.remove(key);
                            }
                        }
                        _ => {}
                    }
                }
                true
            }
            Some("resync") => false,
            _ => true,
        }
    }

    fn set(&mut self, key: &str, value: &Value) {
        let installed = value["installed"].as_bool();
        let login = value["login"]
            .as_str()
            .and_then(|login| serde_json::from_value::<LoginState>(Value::from(login)).ok());
        // The key `_` holds the warnings, and not a harness.
        if let (Some(installed), Some(login)) = (installed, login) {
            self.items.insert(
                key.to_owned(),
                MachineHarness {
                    id: key.to_owned(),
                    installed,
                    login,
                },
            );
        } else {
            self.items.remove(key);
        }
    }

    fn rows(&self) -> Vec<MachineHarness> {
        self.items.values().cloned().collect()
    }
}

struct SummarySink {
    shared: std::sync::Weak<Shared>,
    label: String,
    generation: u64,
    summary: Arc<Mutex<Summary>>,
}

impl MessageSink for SummarySink {
    fn send(&self, message: Value) -> bool {
        let Some(shared) = self.shared.upgrade() else {
            return false;
        };
        let rows = {
            let mut summary = lock(&self.summary);
            if !summary.apply(&message) {
                // A gap: the next snapshot replaces the summary.
                summary.items.clear();
            }
            summary.rows()
        };
        shared.set_harnesses(&self.label, self.generation, rows);
        true
    }
}

#[async_trait]
impl MachineLinks for MachineManager {
    fn configure(&self, machines: Vec<MachineConfig>) {
        let shared = &self.shared;
        *lock(&shared.order) = machines.iter().map(|m| m.label.clone()).collect();
        let wanted: Vec<String> = machines.iter().map(|m| m.label.clone()).collect();
        // Machines that the table dropped.
        let gone: Vec<String> = lock(&shared.entries)
            .keys()
            .filter(|label| !wanted.contains(label))
            .cloned()
            .collect();
        for label in &gone {
            shared.stop(label, "the machine is not in config.toml any more");
            lock(&shared.entries).remove(label);
        }
        let mut start = Vec::new();
        for machine in machines {
            let mut entries = lock(&shared.entries);
            match entries.get_mut(&machine.label) {
                None => {
                    let launch = machine.machine.connect == nh_core::config::ConnectMode::Launch;
                    entries.insert(
                        machine.label.clone(),
                        Entry {
                            row: new_row(&machine.label),
                            config: machine.clone(),
                            client: None,
                            generation: 0,
                            attempt: None,
                            tasks: Vec::new(),
                        },
                    );
                    if launch {
                        start.push(machine.label);
                    }
                }
                Some(entry) if entry.config == machine => {}
                Some(entry) => {
                    let target_changed = entry.config.machine.ssh != machine.machine.ssh
                        || entry.config.machine.session != machine.machine.session
                        || entry.config.machine.remote_nh != machine.machine.remote_nh;
                    let was_up = entry.row.state != MachineState::Disconnected;
                    let launch = machine.machine.connect == nh_core::config::ConnectMode::Launch;
                    let wasnt_launch =
                        entry.config.machine.connect != nh_core::config::ConnectMode::Launch;
                    entry.config = machine.clone();
                    drop(entries);
                    if target_changed {
                        shared.stop(&machine.label, "the target in config.toml changed");
                        if was_up || launch {
                            start.push(machine.label);
                        }
                    } else if launch && wasnt_launch && !was_up {
                        start.push(machine.label);
                    }
                }
            }
        }
        shared.changed();
        for label in start {
            // Nobody waits for a launch: the row says how it went.
            let _ = shared.start(&label);
        }
    }

    async fn connect(&self, label: &str) -> Result<(), RpcError> {
        let mut rx = {
            let state = {
                let entries = lock(&self.shared.entries);
                let entry = entries.get(label).ok_or_else(|| {
                    RpcError::new(ErrorCode::NotFound, format!("no machine {label:?}"))
                })?;
                (entry.row.state, entry.attempt.clone())
            };
            match state {
                (MachineState::Connected, _) => return Ok(()),
                (MachineState::Connecting, Some(rx)) => rx,
                _ => self.shared.start(label).ok_or_else(|| {
                    RpcError::new(ErrorCode::NotFound, format!("no machine {label:?}"))
                })?,
            }
        };
        let outcome = rx
            .wait_for(Option::is_some)
            .await
            .map_err(|_| RpcError::new(ErrorCode::LinkDown, "the attempt was cancelled"))?
            .clone();
        outcome.unwrap_or_else(|| {
            Err(RpcError::new(
                ErrorCode::LinkDown,
                "the attempt was cancelled",
            ))
        })
    }

    async fn open_window(&self, label: &str) -> Result<(), RpcError> {
        self.connect(label).await?;
        let client = self
            .client(label)
            .ok_or_else(|| RpcError::new(ErrorCode::LinkDown, "the link ended"))?;
        let opened = client
            .call(None, "window.open", serde_json::json!({}))
            .await?;
        // The core asks the host for the window at once, before the row comes.
        if let Some(id) = opened["window"]
            .as_str()
            .and_then(|id| id.parse::<nh_proto::api::ids::WindowId>().ok())
        {
            client.own_window(id, true);
        }
        Ok(())
    }

    fn disconnect(&self, label: &str) -> Result<(), RpcError> {
        if !lock(&self.shared.entries).contains_key(label) {
            return Err(RpcError::new(
                ErrorCode::NotFound,
                format!("no machine {label:?}"),
            ));
        }
        self.shared.stop(label, "you disconnected");
        Ok(())
    }

    fn rows(&self) -> Vec<MachineValue> {
        let entries = lock(&self.shared.entries);
        let order = lock(&self.shared.order);
        order
            .iter()
            .filter_map(|label| entries.get(label).map(|entry| entry.row.clone()))
            .collect()
    }

    fn on_change(&self, notify: Arc<dyn Fn() + Send + Sync>) {
        *lock(&self.shared.notify) = Some(notify);
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn the_summary_follows_a_snapshot_and_its_patches() {
        let mut summary = Summary::default();
        summary.apply(&json!({"type": "snapshot", "version": 1, "data": {
            "items": {
                "_": {"warnings": []},
                "claude": {"id": "claude", "installed": true, "login": "logged_in"},
                "codex": {"id": "codex", "installed": false, "login": "unknown"}
            },
            "order": ["claude", "codex"]
        }}));
        let rows = summary.rows();
        assert_eq!(rows.len(), 2, "{rows:?}");
        assert_eq!(rows[0].id, "claude");
        assert_eq!(rows[0].login, LoginState::LoggedIn);
        assert!(!rows[1].installed);

        summary.apply(&json!({"type": "patch", "version": 2, "ops": [
            {"op": "set", "key": "codex", "value":
                {"id": "codex", "installed": true, "login": "logged_out"}},
            {"op": "remove", "key": "claude"},
            {"op": "order", "keys": ["codex"]}
        ]}));
        let rows = summary.rows();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].login, LoginState::LoggedOut);
        assert!(rows[0].installed);
        assert!(!summary.apply(&json!({"type": "resync"})));
    }

    #[test]
    fn a_failure_says_what_the_person_can_do() {
        let (reason, update, code) = describe(&SshError::BridgeTooOld {
            version: "0.0.1".to_owned(),
            protos: "link/0".to_owned(),
        });
        assert!(reason.contains("0.0.1"));
        assert_eq!(update, Some(MachineUpdate::InstallNeeded));
        assert_eq!(code, ErrorCode::LinkDown);
        let (reason, update, _) = describe(&SshError::Ssh {
            status: Some(127),
            stderr: "zsh: no such file".to_owned(),
        });
        assert!(reason.contains("not installed"));
        assert_eq!(update, Some(MachineUpdate::InstallNeeded));
        let (_, update, code) = describe(&SshError::Refused("api".to_owned()));
        assert_eq!(update, Some(MachineUpdate::VersionMismatch));
        assert_eq!(code, ErrorCode::VersionMismatch);
        let (reason, update, _) = describe(&SshError::Ssh {
            status: Some(255),
            stderr: "Host key verification failed.".to_owned(),
        });
        assert!(reason.contains("Host key verification failed."));
        assert_eq!(update, None);
    }

    #[test]
    fn a_round_trip_time_has_one_decimal() {
        assert_eq!(millis(Duration::from_micros(14_249)), 14.2);
        assert_eq!(millis(Duration::from_micros(14_251)), 14.3);
    }
}
