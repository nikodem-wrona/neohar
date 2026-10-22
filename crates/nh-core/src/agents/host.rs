//! The host of the adapters.
//!
//! Each live run that a hook has spoken for has one adapter, in a task of its own. A signal goes to
//! the task through a queue, so the shim gets its receipt at once and the signals of one run keep
//! their order. The adapter reads the signal and reports what happened. The host gives that to the
//! actor, which gives it to the arbiter.
//!
//! A supervisor watches each task. An adapter that panics ends its own task and nothing else: the
//! health of the run says `degraded`, and the next signal starts a new adapter.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, PoisonError},
};

use nh_agents::{
    adapter::{Adapter, AdapterCtx, AdapterSignal, Emitter, StopReason},
    claude::ClaudeHooks,
    codex::CodexHooks,
    manifest::{Manifest, schema::VersionRule},
    version,
};
use nh_proto::api::{
    common::Harness,
    ids::{PaneId, RunId},
    time::Timestamp,
};
use tokio::{
    runtime::Handle,
    sync::{
        mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel},
        oneshot,
    },
};

use super::runs;
use crate::{
    actor::ActorHandle,
    interaction::{
        apply::{self, Prepared},
        gate::GateReply,
        model::Lease,
        render::{RenderCtx, render},
    },
};

/// What the adapter of a run needs to start.
pub(crate) struct Start {
    pub run: RunId,
    pub pane: Option<PaneId>,
    pub harness: Harness,
    pub manifest: Arc<Manifest>,
    /// The root of the workspace of the agent, for the rules of risk. `None` when the
    /// layout does not say, and then the directory of the hook counts.
    pub workspace: Option<PathBuf>,
}

/// What the host asks of the core to render the payload of an interaction.
pub(crate) trait RenderEnv: Send + Sync {
    /// The last lines of the pane, for the strip of context on a card.
    fn peek(&self, pane: PaneId) -> Vec<String>;
    /// The hosts that a command may connect to without a warning.
    fn network_allow(&self) -> Vec<String>;
    fn home(&self) -> Option<PathBuf>;
}

/// A shim that waits for the reply to its hook (`adapter.gate`).
pub(crate) struct GateTicket {
    pub lease: Lease,
    pub reply: oneshot::Sender<GateReply>,
}

enum Inbound {
    Signal {
        signal: AdapterSignal,
        at: Timestamp,
        ticket: Option<GateTicket>,
    },
    Stop(StopReason),
}

/// Makes the adapter for a run.
pub(crate) type Factory = Box<dyn Fn(&Start) -> Box<dyn Adapter> + Send + Sync>;

/// The adapters of the live runs.
/// What a probe of the version needs: the environment of the host and the cache of the versions.
#[derive(Clone)]
struct ProbeEnv {
    env: Vec<(String, String)>,
    harnesses: Arc<crate::harness::Harnesses>,
}

pub(crate) struct AdapterHosts {
    handle: Handle,
    actor: Mutex<Option<ActorHandle>>,
    env: Mutex<Option<Arc<dyn RenderEnv>>>,
    probe: Mutex<Option<ProbeEnv>>,
    running: Mutex<HashMap<RunId, UnboundedSender<Inbound>>>,
    factory: Factory,
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The adapter for a run: the one of its harness.
fn adapter_for(start: &Start) -> Box<dyn Adapter> {
    match start.harness {
        Harness::Claude => Box::new(ClaudeHooks::new()),
        Harness::Codex => Box::new(CodexHooks::new()),
    }
}

impl AdapterHosts {
    pub fn new(handle: Handle) -> Arc<Self> {
        Self::with_factory(handle, Box::new(adapter_for))
    }

    /// A host whose adapters come from `factory`. The tests use it to make an adapter that fails.
    pub fn with_factory(handle: Handle, factory: Factory) -> Arc<Self> {
        Arc::new(Self {
            handle,
            actor: Mutex::new(None),
            env: Mutex::new(None),
            probe: Mutex::new(None),
            running: Mutex::new(HashMap::new()),
            factory,
        })
    }

    /// Gives the host the state actor. Signals that arrive before it are dropped.
    pub fn attach(&self, actor: ActorHandle) {
        *lock(&self.actor) = Some(actor);
    }

    /// Gives the host what it needs to render the payloads of interactions.
    pub fn set_env(&self, env: Arc<dyn RenderEnv>) {
        *lock(&self.env) = Some(env);
    }

    /// Gives the host what it needs to ask a harness for its version.
    pub fn set_probe(&self, env: Vec<(String, String)>, harnesses: Arc<crate::harness::Harnesses>) {
        *lock(&self.probe) = Some(ProbeEnv { env, harnesses });
    }

    /// Asks the harness of a run for its version, on a thread, and tells the actor. A run that a
    /// hook made has no process walk to do it, and its capabilities follow its version.
    pub fn probe_version(&self, run: RunId, rule: VersionRule) {
        let (Some(actor), Some(probe)) = (self.actor(), lock(&self.probe).clone()) else {
            return;
        };
        self.handle.spawn(async move {
            let found = tokio::task::spawn_blocking(move || {
                version::probe(&rule, &probe.env, probe.harnesses.versions())
                    .map(|found| found.version)
                    .map_err(|error| error.to_string())
            })
            .await
            .unwrap_or_else(|_| Err("the probe of the version ended without an answer".to_owned()));
            actor.update_detached(move |mut parts| runs::set_version(&mut parts, run, found));
        });
    }

    fn actor(&self) -> Option<ActorHandle> {
        lock(&self.actor).clone()
    }

    /// Hands a signal to the adapter of the run. A run that has no adapter gets one first.
    pub fn deliver(self: &Arc<Self>, start: Start, signal: AdapterSignal, at: Timestamp) {
        self.dispatch(start, signal, at, None);
    }

    /// Like [`AdapterHosts::deliver`], for a hook that a shim holds. The ticket gets the reply.
    pub fn deliver_gate(
        self: &Arc<Self>,
        start: Start,
        signal: AdapterSignal,
        at: Timestamp,
        ticket: GateTicket,
    ) {
        self.dispatch(start, signal, at, Some(ticket));
    }

    fn dispatch(
        self: &Arc<Self>,
        start: Start,
        signal: AdapterSignal,
        at: Timestamp,
        ticket: Option<GateTicket>,
    ) {
        let run = start.run;
        let message = Inbound::Signal { signal, at, ticket };
        let running = lock(&self.running);
        if let Some(queue) = running.get(&run) {
            match queue.send(message) {
                Ok(()) => return,
                // The task ended (a panic, or a stop). The next adapter starts below.
                Err(returned) => {
                    drop(running);
                    self.spawn(start, returned.0);
                    return;
                }
            }
        }
        drop(running);
        self.spawn(start, message);
    }

    fn spawn(self: &Arc<Self>, start: Start, first: Inbound) {
        let run = start.run;
        let (queue, inbox) = unbounded_channel();
        let _ = queue.send(first);
        lock(&self.running).insert(run, queue);
        let Some(actor) = self.actor() else {
            lock(&self.running).remove(&run);
            return;
        };
        let adapter = (self.factory)(&start);
        let env = lock(&self.env).clone();
        let task = self.handle.spawn(run_adapter(
            actor.clone(),
            start,
            adapter,
            inbox,
            env,
            self.handle.clone(),
        ));
        // The supervisor: a panic ends the adapter, and the run says so.
        let hosts = Arc::clone(self);
        self.handle.spawn(async move {
            let result = task.await;
            lock(&hosts.running).remove(&run);
            if let Err(error) = result
                && error.is_panic()
            {
                tracing::error!(%run, "the adapter of a run panicked");
                actor.update_detached(move |mut parts| {
                    runs::degrade(&mut parts, run, "the adapter failed and starts again");
                });
            }
        });
    }

    /// The run ended: its adapter stops.
    pub fn stop(&self, run: RunId, reason: StopReason) {
        if let Some(queue) = lock(&self.running).remove(&run) {
            let _ = queue.send(Inbound::Stop(reason));
        }
    }

    /// How many adapters run, for the tests.
    #[cfg(test)]
    pub fn count(&self) -> usize {
        lock(&self.running).len()
    }
}

/// The largest file that a diff reads (`nh-rich` cuts its input at the same size).
const MAX_FILE: u64 = 2 * 1024 * 1024;

fn read_file(path: &Path) -> Option<String> {
    let meta = std::fs::metadata(path).ok()?;
    if !meta.is_file() || meta.len() > MAX_FILE {
        return None;
    }
    std::fs::read_to_string(path).ok()
}

/// Renders the payload of each interaction that the signals open. The work reads files and runs
/// the highlighter, so it runs on the blocking pool and never on the actor.
async fn prepare(
    start: &Start,
    env: Option<&Arc<dyn RenderEnv>>,
    hook_cwd: Option<PathBuf>,
    emitted: &[AdapterSignal],
) -> Vec<Prepared> {
    let mut prepared = Vec::new();
    for (index, signal) in emitted.iter().enumerate() {
        let AdapterSignal::InteractionOpen { kind, payload, .. } = signal else {
            continue;
        };
        let (kind, payload) = (*kind, payload.clone());
        let workspace = start
            .workspace
            .clone()
            .or_else(|| hook_cwd.clone())
            .unwrap_or_else(|| PathBuf::from("/"));
        let cwd = hook_cwd.clone().unwrap_or_else(|| workspace.clone());
        let peek = match (env, start.pane) {
            (Some(env), Some(pane)) => env.peek(pane),
            _ => Vec::new(),
        };
        let (home, allow) = match env {
            Some(env) => (env.home(), env.network_allow()),
            None => (None, Vec::new()),
        };
        let rendered = tokio::task::spawn_blocking(move || {
            let project = nh_rich::project::Project::scan(&workspace);
            let ctx = RenderCtx {
                workspace: &workspace,
                cwd: &cwd,
                home: home.as_deref(),
                project: &project,
                network_allow: &allow,
                peek,
                read_file: &read_file,
            };
            render(kind, &payload, &ctx)
        })
        .await;
        match rendered {
            Ok(rendered) => prepared.push(Prepared { index, rendered }),
            Err(error) => tracing::warn!(%error, "the payload of an interaction did not render"),
        }
    }
    prepared
}

/// The directory that a hook names in its payload.
fn cwd_of(signal: &AdapterSignal) -> Option<PathBuf> {
    let AdapterSignal::Raw { payload, .. } = signal else {
        return None;
    };
    payload
        .get("cwd")
        .and_then(serde_json::Value::as_str)
        .filter(|cwd| Path::new(cwd).is_absolute())
        .map(PathBuf::from)
}

async fn run_adapter(
    actor: ActorHandle,
    start: Start,
    mut adapter: Box<dyn Adapter>,
    mut inbox: UnboundedReceiver<Inbound>,
    env: Option<Arc<dyn RenderEnv>>,
    _handle: Handle,
) {
    let run = start.run;
    let (emitter, signals) = Emitter::channel();
    let ctx = AdapterCtx {
        run,
        pane: start.pane,
        harness: start.harness,
        manifest: Arc::clone(&start.manifest),
        emitter,
    };
    if let Err(error) = adapter.start(ctx).await {
        let reason = format!("the adapter did not start: {error}");
        actor.update_detached(move |mut parts| runs::degrade(&mut parts, run, &reason));
        return;
    }
    while let Some(message) = inbox.recv().await {
        match message {
            Inbound::Signal { signal, at, ticket } => {
                let hook_cwd = cwd_of(&signal);
                let result = adapter.on_signal(signal).await;
                let emitted: Vec<AdapterSignal> = signals.try_iter().collect();
                let prepared = prepare(&start, env.as_ref(), hook_cwd, &emitted).await;
                let failed = result
                    .as_ref()
                    .err()
                    .map(|e| format!("the adapter failed: {e}"));
                actor.update_detached(move |mut parts| {
                    let (lease, reply) = match ticket {
                        Some(ticket) => (Some(ticket.lease), Some(ticket.reply)),
                        None => (None, None),
                    };
                    // The state first, then the interactions: a signal can end the turn in the
                    // same batch that opens an interaction.
                    if !emitted.is_empty() {
                        runs::apply_signals(&mut parts, run, &emitted, at);
                    }
                    let opened = apply::signals(&mut parts, run, &emitted, prepared, at, lease);
                    if let Some(reply) = reply {
                        let _ = reply.send(match opened.first() {
                            Some(opened) if opened.held => GateReply::Wait {
                                interaction: opened.id,
                            },
                            // The interaction is open and the harness shows its dialog: the core
                            // does not hold the hook.
                            Some(_) => GateReply::NoDecision("observe"),
                            None => GateReply::NoDecision("no_interaction"),
                        });
                    }
                    if let Some(reason) = failed {
                        runs::degrade(&mut parts, run, &reason);
                    }
                });
            }
            Inbound::Stop(reason) => {
                let _ = adapter.stop(reason).await;
                return;
            }
        }
    }
}
