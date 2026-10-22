//! The watcher of a pane (`ProcessWatcher`).
//!
//! One task for each pane of the layout. It wakes when the holder says that the foreground group
//! changed, and once a second otherwise. At each wake it walks the process tree from the foreground
//! group, asks the matcher for a harness, and tells the actor when a run starts or ends. It also
//! starts the probe of the version, and says when a run has been `starting` long enough. The walk
//! also reads the working directory of the foreground group, the `cwd` of the pane.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex, PoisonError},
    time::{Duration, Instant},
};

use nh_agents::{
    manifest::Loaded,
    matcher::{Detection, Matcher, is_yolo},
    process::{self, Tree, Walker},
    run::{Step, Tracker},
    version,
};
use nh_proto::{
    api::{
        common::{Harness, Source},
        ids::{PaneId, RunId},
        time::Timestamp,
    },
    holder::ExitStatus,
};
use tokio::{runtime::Handle, task::JoinHandle};
use ulid::Ulid;

use super::{AdapterHosts, runs};
use crate::{
    actor::ActorHandle,
    harness::Harnesses,
    health::Health,
    pane::{Liveness, Pane},
    persist::now_ms,
};

/// How often a watcher walks when the holder says nothing.
const POLL: Duration = Duration::from_secs(1);

/// The matcher for the manifests in force. It is built again when the manifests change.
type Compiled = (Arc<Loaded>, Arc<Matcher>);

/// The service that starts a watcher for each pane.
pub(crate) struct Agents {
    pub(super) actor: ActorHandle,
    harnesses: Arc<Harnesses>,
    health: Arc<Health>,
    hosts: Arc<AdapterHosts>,
    /// When the last hook of each harness arrived, for `integration.status`.
    last_signal: Mutex<HashMap<Harness, Timestamp>>,
    /// The login environment, for the probe of a version.
    env: Vec<(String, String)>,
    pub(super) handle: Handle,
    tasks: Mutex<HashMap<Ulid, JoinHandle<()>>>,
    /// The screen watchers, one for each pane.
    screens: Mutex<HashMap<Ulid, JoinHandle<()>>>,
    matcher: Mutex<Option<Compiled>>,
    /// The login state of the harnesses: a screen that asks for a login changes it.
    pub(super) discovery: std::sync::OnceLock<Arc<crate::discovery::Discovery>>,
}

impl Agents {
    pub fn new(
        actor: ActorHandle,
        harnesses: Arc<Harnesses>,
        health: Arc<Health>,
        hosts: Arc<AdapterHosts>,
        env: Vec<(String, String)>,
        handle: Handle,
    ) -> Arc<Self> {
        Arc::new(Self {
            actor,
            harnesses,
            health,
            hosts,
            last_signal: Mutex::new(HashMap::new()),
            env,
            handle,
            tasks: Mutex::new(HashMap::new()),
            screens: Mutex::new(HashMap::new()),
            matcher: Mutex::new(None),
            discovery: std::sync::OnceLock::new(),
        })
    }

    /// Gives the watchers the discovery of the harnesses.
    pub(crate) fn set_discovery(&self, discovery: Arc<crate::discovery::Discovery>) {
        let _ = self.discovery.set(discovery);
    }

    /// The manifests in force, and the matcher for them.
    fn compiled(&self) -> Compiled {
        let current = self.harnesses.current();
        let mut cache = self.matcher.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some((loaded, matcher)) = cache.as_ref()
            && Arc::ptr_eq(loaded, &current)
        {
            return (Arc::clone(loaded), Arc::clone(matcher));
        }
        let matcher = Arc::new(Matcher::new(&current));
        *cache = Some((Arc::clone(&current), Arc::clone(&matcher)));
        (current, matcher)
    }

    /// Watches a pane. A pane object that replaces another for the same pane (a restart) takes
    /// over the watch.
    pub fn watch(self: &Arc<Self>, pane: &Arc<Pane>) {
        let task = self
            .handle
            .spawn(watch_pane(Arc::clone(self), Arc::clone(pane)));
        let screen = super::screen::spawn(self, Arc::clone(pane));
        let olds = [
            self.tasks
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .insert(pane.id(), task),
            self.screens
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .insert(pane.id(), screen),
        ];
        for old in olds.into_iter().flatten() {
            old.abort();
        }
    }

    /// A hook arrived with the token of a pane. The run of that harness in the pane is bound by
    /// it, or made if it did not exist, and the adapter of the run reads the event. The call
    /// returns at once: the work is on the actor and on the task of the adapter.
    pub fn signal(&self, pane: Ulid, harness: Harness, event: String, payload: serde_json::Value) {
        let hosts = Arc::clone(&self.hosts);
        let at = Timestamp(now_ms());
        self.last_signal
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(harness, at);
        self.actor.update_detached(move |mut parts| {
            runs::signal(
                &mut parts,
                &hosts,
                PaneId::new(pane),
                harness,
                event,
                payload,
                at,
            );
        });
    }

    /// When the last hook of a harness arrived.
    pub fn last_signal(&self, harness: Harness) -> Option<Timestamp> {
        self.last_signal
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(&harness)
            .copied()
    }

    /// The manifests in force.
    pub fn manifests(&self) -> Arc<Loaded> {
        self.harnesses.current()
    }

    /// The version of the harness that a manifest describes, from the version command (cached for
    /// each program). It can take seconds, so a caller runs it on a blocking thread.
    pub fn probe_version(&self, manifest_id: &str) -> Option<semver::Version> {
        let loaded = self.harnesses.current();
        let rule = loaded.get(manifest_id)?.version.clone()?;
        version::probe(&rule, &self.env, self.harnesses.versions())
            .ok()
            .map(|probe| probe.version)
    }

    /// The pane is gone from the layout.
    pub fn unwatch(&self, pane: Ulid) {
        let olds = [
            self.tasks
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .remove(&pane),
            self.screens
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .remove(&pane),
        ];
        for old in olds.into_iter().flatten() {
            old.abort();
        }
        let id = PaneId::new(pane);
        self.health.clear(&tie_id(id));
        self.actor
            .update_detached(move |mut parts| runs::pane_closed(&mut parts, id));
    }
}

fn tie_id(pane: PaneId) -> String {
    format!("harness-tie:{pane}")
}

fn exit_code(status: ExitStatus) -> Option<i32> {
    match status {
        ExitStatus::Code(code) => Some(code),
        ExitStatus::Signal(_) | ExitStatus::ExecFailed(_) => None,
    }
}

async fn watch_pane(agents: Arc<Agents>, pane: Arc<Pane>) {
    let id = PaneId::new(pane.id());
    // The pane must be in the layout. A pane of `debug.spawn_panes` is not. A run that the last
    // core left for this pane continues: the tracker starts with its process.
    let start = agents
        .actor
        .update(move |parts| {
            parts.state.layout.panes.contains_key(&id).then(|| {
                let run = parts.state.runs.of_pane(id);
                (run.map(|run| run.id), run.and_then(|run| run.process))
            })
        })
        .await;
    let Ok(Some((mut current, mut resumed))) = start else {
        return;
    };
    let mut tracker = Tracker::default();
    if let Some(key) = resumed {
        tracker.resume(key);
    }
    let mut walker = Walker::new(false);
    let mut foreground = pane.foreground();
    let mut interval = tokio::time::interval(POLL);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let mut settle_at: Option<Instant> = None;
    let mut tied = false;
    loop {
        tokio::select! {
            _ = interval.tick() => {}
            changed = foreground.changed() => {
                if changed.is_err() {
                    return;
                }
            }
        }
        // The child of the pane ended: the holder is the authority (rule 1 of the arbiter).
        if let Liveness::Exited(status) = pane.info().liveness {
            tracker.gone();
            // The run of the pane may be one that a hook made after this watcher started, so the
            // actor looks it up.
            let code = exit_code(status);
            agents.actor.update_detached(move |mut parts| {
                runs::end_in_pane(&mut parts, id, Source::Holder, code);
            });
            return;
        }
        let leader = (*foreground.borrow()).unwrap_or_else(|| pane.child_pid());
        let (loaded, matcher) = agents.compiled();
        walker.set_want_env(matcher.wants_env());
        let tree = walker.walk(leader);
        publish_cwd(&pane, leader);
        let found = matcher.find(&tree);
        // A run that the last core left learns the arguments of its process from the first walk.
        if let (Some(key), Some(run)) = (resumed.take(), current)
            && let Some(best) = found.best.as_ref().filter(|best| best.key() == key)
        {
            let shared = shared_daemon(best, &tree);
            agents.actor.update_detached(move |mut parts| {
                runs::set_shared_daemon(&mut parts, run, shared);
            });
        }
        if found.tied.is_empty() == tied {
            tied = !found.tied.is_empty();
            match (&found.best, tied) {
                (Some(best), true) => agents.health.warn(
                    &tie_id(id),
                    format!(
                        "The manifests {} and {} match the same program with the priority {}.",
                        best.manifest,
                        found.tied.join(", "),
                        best.priority
                    ),
                    Some("Give one of them a higher priority.".to_owned()),
                ),
                _ => agents.health.clear(&tie_id(id)),
            }
        }
        let step = tracker.observe(found.best, |key| walker.alive(key));
        match step {
            Step::Nothing => {}
            Step::Start(detection) => {
                current = begin(&agents, id, detection, &tree, &loaded, false).await;
                settle_at = current.map(|_| Instant::now() + settle_delay());
            }
            Step::Replace { started, .. } => {
                current = begin(&agents, id, started, &tree, &loaded, true).await;
                settle_at = current.map(|_| Instant::now() + settle_delay());
            }
            Step::End { .. } => {
                if let Some(run) = current.take() {
                    agents.actor.update_detached(move |mut parts| {
                        runs::end(&mut parts, run, Source::Process, None);
                    });
                }
                settle_at = None;
            }
        }
        if let (Some(at), Some(run)) = (settle_at, current)
            && Instant::now() >= at
        {
            settle_at = None;
            agents.actor.update_detached(move |mut parts| {
                runs::settle(&mut parts, run);
            });
        }
    }
}

/// Shows the working directory of the foreground group as the `cwd` of the pane. The
/// system does not tell the directory of a process of another user (`sudo`): then the child of the
/// pane tells it, and else the pane keeps the directory that it had.
fn publish_cwd(pane: &Pane, leader: i32) {
    let Some(cwd) = process::cwd(leader)
        .or_else(|| process::cwd(pane.child_pid()))
        .map(|path| path.display().to_string())
    else {
        return;
    };
    pane.update_info(|info| {
        let changed = info.cwd.as_deref() != Some(cwd.as_str());
        info.cwd = Some(cwd);
        changed
    });
}

/// A Codex without the flag of the shim can send its hooks through the shared daemon.
fn shared_daemon(detection: &Detection, tree: &Tree) -> bool {
    detection.harness == Harness::Codex
        && !tree
            .processes
            .iter()
            .any(|process| nh_agents::codex::daemon_off(&process.argv))
}

fn settle_delay() -> Duration {
    Duration::from_millis(u64::try_from(runs::SETTLE_AFTER_MS).unwrap_or(5_000))
}

/// Starts a run (or replaces the one of the pane) and the probe of its version. Returns the run.
async fn begin(
    agents: &Arc<Agents>,
    pane: PaneId,
    detection: Detection,
    tree: &Tree,
    loaded: &Loaded,
    replace: bool,
) -> Option<RunId> {
    let manifest = loaded.get(&detection.manifest);
    let yolo = manifest
        .and_then(|manifest| manifest.yolo.as_ref())
        .is_some_and(|yolo| {
            tree.processes
                .iter()
                .any(|process| is_yolo(yolo, &process.argv))
        });
    let shared_daemon = shared_daemon(&detection, tree);
    let rule = manifest.and_then(|manifest| manifest.version.clone());
    let started = detection.clone();
    let run = agents
        .actor
        .update(move |mut parts| {
            let run = if replace {
                runs::replace(&mut parts, pane, &started, yolo)
            } else {
                runs::start(&mut parts, pane, &started, yolo)
            }?;
            runs::set_shared_daemon(&mut parts, run, shared_daemon);
            Some(run)
        })
        .await
        .ok()
        .flatten()?;
    // The probe runs a program for up to `version::TIMEOUT`, so it has a thread and a task of its
    // own.
    let task = Arc::clone(agents);
    agents.handle.spawn(async move {
        let found = tokio::task::spawn_blocking({
            let (env, harnesses) = (task.env.clone(), Arc::clone(&task.harnesses));
            move || match rule {
                Some(rule) => version::probe(&rule, &env, harnesses.versions())
                    .map(|probe| probe.version)
                    .map_err(|error| error.to_string()),
                None => Err("the manifest has no version command".to_owned()),
            }
        })
        .await
        .unwrap_or_else(|_| Err("the probe of the version ended without an answer".to_owned()));
        task.actor.update_detached(move |mut parts| {
            runs::set_version(&mut parts, run, found);
        });
    });
    tracing::debug!(%pane, %run, manifest = %detection.manifest, at = now_ms(), "a run starts");
    Some(run)
}
