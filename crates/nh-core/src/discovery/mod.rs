//! The discovery and the login state of the harnesses of this machine.
//!
//! For each manifest the core looks for the program of `[launch] argv` on the `PATH` of the login
//! environment, reads its version with a cache (`VersionCache`), and runs `[auth] status`.
//! The result is one row of the projection `harnesses`. The core keeps the state and the method of
//! the login, and never the output of the probe, which can name the account.
//!
//! The probes run on the blocking pool of the runtime, never on the state actor. They run at the
//! start of the core, every 30 minutes, on `harness.refresh`, and before `agent.start`,
//! `agent.resume` and `task.start` when the last state is not `logged_in`. A run that shows the
//! screen rule `login_required` makes the state `logged_out` with the source `run`, and the next
//! probe that gives `logged_in` wins.
//!
//! What the person sees: one warning of `health` for each harness that is not installed or not
//! logged in, and one notification when a state becomes `logged_out`. A start with a harness that
//! is not installed, or that a new probe finds logged out, fails with `harness_unavailable`.

use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex, MutexGuard, PoisonError},
    time::{Duration, Instant},
};

use nh_agents::{
    auth::{self, Verdict},
    harness::name as harness_name,
    manifest::Manifest,
    version::{self, VersionCache, find_exe, path_of},
};
use nh_proto::api::{
    common::{Harness, Urgency},
    model::{HarnessValue, LoginSource, LoginState},
    projections::Projection,
    time::Timestamp,
};
use serde_json::json;
use tokio::runtime::Handle;

use crate::{
    ErrorCode,
    error::RpcError,
    harness::Harnesses,
    health::Health,
    host::{Host, HostNotification},
    persist::now_ms,
    projection::ProjectionHub,
};

/// How often the core asks every harness again.
pub const PERIOD: Duration = Duration::from_secs(30 * 60);

/// The prefix of the warnings of `health`. The group `harness` is the notes of the manifests.
const WARNING: &str = "harnesses:";

/// The mode whose capability ranges say that a version is validated.
const MODE: &str = "tui";

/// What the core knows of one harness.
#[derive(Clone, Debug)]
struct Known {
    value: HarnessValue,
    /// When a probe of the program ran last.
    probed: Instant,
    /// A notification said that the harness is logged out, and the state has not been
    /// `logged_in` since.
    notified: bool,
}

/// The service. The API, the launches and the screen watchers reach it.
pub(crate) struct Discovery {
    handle: Handle,
    env: Vec<(String, String)>,
    harnesses: Arc<Harnesses>,
    health: Arc<Health>,
    hub: Arc<ProjectionHub>,
    host: Arc<dyn Host>,
    limit: Duration,
    known: Mutex<BTreeMap<String, Known>>,
    /// One round of probes at a time.
    round: tokio::sync::Mutex<()>,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The error of a start with a harness that is not there. `details` has
/// `{harness, reason, hint}`, and `reason` is `not_installed` or `logged_out`.
pub(crate) fn unavailable(value: &HarnessValue, reason: &str) -> RpcError {
    let what = match reason {
        "not_installed" => "is not installed",
        _ => "is not logged in",
    };
    let mut message = format!("{} {what} on this machine.", value.name);
    if let Some(hint) = &value.hint {
        message = format!("{message} {hint}");
    }
    let mut error = RpcError::new(ErrorCode::HarnessUnavailable, message);
    error.details = json!({ "harness": value.id, "reason": reason, "hint": value.hint });
    error
}

/// Looks for the harness of a manifest and asks it for its login state. It blocks: call it on the
/// blocking pool.
fn probe(
    manifest: &Manifest,
    env: &[(String, String)],
    versions: &VersionCache,
    limit: Duration,
) -> HarnessValue {
    let name = manifest.name.clone().unwrap_or_else(|| manifest.id.clone());
    let checked_at = Some(Timestamp(now_ms()));
    let program = manifest
        .launch
        .as_ref()
        .and_then(|launch| launch.argv.first())
        .cloned();
    let login_hint = manifest
        .auth
        .as_ref()
        .and_then(|auth| auth.login_hint.clone());
    let exe = program
        .as_deref()
        .and_then(|program| find_exe(program, &path_of(env)));
    let Some(exe) = exe else {
        let hint = match &program {
            Some(program) => format!(
                "`{program}` is not on the PATH of the login shell of this machine. Install \
                 {name}, or add its directory to PATH in the profile of the shell."
            ),
            None => format!("The manifest {} has no [launch] argv.", manifest.id),
        };
        return HarnessValue {
            id: manifest.id.clone(),
            name,
            installed: false,
            path: None,
            version: None,
            validated: false,
            login: LoginState::Unknown,
            login_source: None,
            login_method: None,
            available: false,
            checked_at,
            hint: Some(hint),
        };
    };
    let found = manifest
        .version
        .as_ref()
        .and_then(|rule| version::probe(rule, env, versions).ok())
        .map(|probe| probe.version);
    let validated = found
        .as_ref()
        .is_some_and(|v| !manifest.capability_ranges(v, MODE).is_empty());
    let verdict = manifest.auth.as_ref().map_or(Verdict::UNKNOWN, |rule| {
        auth::probe_with_limit(rule, env, limit)
    });
    HarnessValue {
        id: manifest.id.clone(),
        name,
        installed: true,
        path: Some(exe.display().to_string()),
        version: found.map(|v| v.to_string()),
        validated,
        login: verdict.state,
        login_source: manifest.auth.as_ref().map(|_| LoginSource::Probe),
        login_method: verdict.method,
        available: verdict.state != LoginState::LoggedOut,
        checked_at,
        hint: (verdict.state != LoginState::LoggedIn)
            .then_some(login_hint)
            .flatten(),
    }
}

impl Discovery {
    pub(crate) fn new(
        handle: Handle,
        env: Vec<(String, String)>,
        harnesses: Arc<Harnesses>,
        health: Arc<Health>,
        hub: Arc<ProjectionHub>,
        host: Arc<dyn Host>,
    ) -> Arc<Self> {
        Self::with_limit(handle, env, harnesses, health, hub, host, auth::TIMEOUT)
    }

    pub(crate) fn with_limit(
        handle: Handle,
        env: Vec<(String, String)>,
        harnesses: Arc<Harnesses>,
        health: Arc<Health>,
        hub: Arc<ProjectionHub>,
        host: Arc<dyn Host>,
        limit: Duration,
    ) -> Arc<Self> {
        Arc::new(Self {
            handle,
            env,
            harnesses,
            health,
            hub,
            host,
            limit,
            known: Mutex::new(BTreeMap::new()),
            round: tokio::sync::Mutex::new(()),
        })
    }

    /// The rows as the projection has them, by id.
    #[cfg(test)]
    pub(crate) fn rows(&self) -> BTreeMap<String, HarnessValue> {
        lock(&self.known)
            .iter()
            .map(|(id, known)| (id.clone(), known.value.clone()))
            .collect()
    }

    /// Starts the probes at the start of the core and every [`PERIOD`] after it.
    pub(crate) fn spawn_schedule(self: &Arc<Self>) {
        let this = Arc::clone(self);
        self.handle.spawn(async move {
            loop {
                if let Err(error) = this.refresh(None).await {
                    tracing::warn!(%error, "the harnesses are not probed");
                }
                tokio::time::sleep(PERIOD).await;
            }
        });
    }

    /// Asks one harness, or every harness, again (`harness.refresh`). It returns when the probes
    /// are done.
    pub(crate) async fn refresh(&self, only: Option<&str>) -> Result<(), RpcError> {
        self.probe_since(only, None).await.map(drop)
    }

    /// The row of the harness `id`. Before the first probe of it, the call asks the harness now,
    /// and a round that ends after the call started is enough: the call does not wait for a
    /// round of the schedule and then probe again.
    pub(crate) async fn row_or_probe(&self, id: &str) -> Option<HarnessValue> {
        if let Some(known) = lock(&self.known).get(id) {
            return Some(known.value.clone());
        }
        let asked = Instant::now();
        let _ = self.probe_since(Some(id), Some(asked)).await;
        lock(&self.known).get(id).map(|known| known.value.clone())
    }

    /// Runs the probes of `only`, or of every manifest. A probe that ran after `since` is not run
    /// again. Gives the result of each probe, before the merge with what a run showed.
    async fn probe_since(
        &self,
        only: Option<&str>,
        since: Option<Instant>,
    ) -> Result<Vec<HarnessValue>, RpcError> {
        let loaded = self.harnesses.current();
        if let Some(id) = only
            && loaded.get(id).is_none()
        {
            return Err(RpcError::new(
                ErrorCode::NotFound,
                format!("no harness {id}"),
            ));
        }
        let _round = self.round.lock().await;
        let mut results = Vec::new();
        for (id, entry) in &loaded.manifests {
            if only.is_some_and(|only| only != id) {
                continue;
            }
            let fresh = since.and_then(|since| {
                lock(&self.known)
                    .get(id)
                    .filter(|known| known.probed >= since)
                    .map(|known| known.value.clone())
            });
            if let Some(value) = fresh {
                results.push(value);
                continue;
            }
            let manifest = entry.manifest.clone();
            let env = self.env.clone();
            let harnesses = Arc::clone(&self.harnesses);
            let limit = self.limit;
            let value = tokio::task::spawn_blocking(move || {
                probe(&manifest, &env, harnesses.versions(), limit)
            })
            .await
            .map_err(|e| RpcError::internal(e.to_string()))?;
            self.apply(value.clone(), true);
            results.push(value);
        }
        if only.is_none() {
            // A manifest that is gone has no row.
            let gone: Vec<String> = lock(&self.known)
                .keys()
                .filter(|id| loaded.get(id).is_none())
                .cloned()
                .collect();
            for id in gone {
                lock(&self.known).remove(&id);
                self.hub.remove(Projection::Harnesses, &id);
                self.health.clear(&format!("{WARNING}{id}"));
            }
            self.publish_order();
        }
        Ok(results)
    }

    /// Checks the harness of a start. When its last state is not `logged_in`, the core
    /// asks it again. A harness that is not installed, or that the new probe finds logged out,
    /// gives `harness_unavailable`. With `unknown`, the start goes on.
    pub(crate) async fn ensure(&self, harness: Harness) -> Result<(), RpcError> {
        let id = harness_name(harness);
        let ready = lock(&self.known).get(id).is_some_and(|known| {
            known.value.installed && known.value.login == LoginState::LoggedIn
        });
        if ready {
            return Ok(());
        }
        let asked = Instant::now();
        let results = match self.probe_since(Some(id), Some(asked)).await {
            Ok(results) => results,
            // A harness without a manifest is the error of the launch, which says it better.
            Err(error) if error.code == ErrorCode::NotFound => return Ok(()),
            Err(error) => return Err(error),
        };
        let Some(value) = results.into_iter().next() else {
            return Ok(());
        };
        if !value.installed {
            return Err(unavailable(&value, "not_installed"));
        }
        if value.login == LoginState::LoggedOut {
            return Err(unavailable(&value, "logged_out"));
        }
        Ok(())
    }

    /// A run of the harness `id` showed that it is not logged in (the screen rule
    /// `login_required`).
    pub(crate) fn run_logged_out(&self, id: &str) {
        let Some(mut value) = lock(&self.known).get(id).map(|known| known.value.clone()) else {
            return;
        };
        if value.login == LoginState::LoggedOut {
            return;
        }
        let hint = self.harnesses.current().get(id).and_then(|manifest| {
            manifest
                .auth
                .as_ref()
                .and_then(|auth| auth.login_hint.clone())
        });
        value.login = LoginState::LoggedOut;
        value.login_source = Some(LoginSource::Run);
        value.login_method = None;
        value.available = false;
        value.checked_at = Some(Timestamp(now_ms()));
        value.hint = hint.or(value.hint);
        self.apply(value, false);
    }

    /// Keeps a new state of a harness, publishes its row, and tells the person what changed.
    fn apply(&self, mut value: HarnessValue, probed: bool) {
        let mut known = lock(&self.known);
        let before = known.get(&value.id).cloned();
        // A probe that says nothing does not undo what a run showed: only `logged_in` wins.
        if let Some(before) = &before
            && probed
            && value.installed
            && value.login == LoginState::Unknown
            && before.value.login == LoginState::LoggedOut
            && before.value.login_source == Some(LoginSource::Run)
        {
            value.login = LoginState::LoggedOut;
            value.login_source = Some(LoginSource::Run);
            value.available = false;
        }
        let mut notified = before.as_ref().is_some_and(|known| known.notified);
        if value.login == LoginState::LoggedIn || !value.installed {
            notified = false;
        } else if value.login == LoginState::LoggedOut && !notified {
            notified = true;
            self.host.notify(HostNotification {
                title: format!("{} is not logged in", value.name),
                body: value.hint.clone().unwrap_or_default(),
                urgency: Urgency::Normal,
                interaction: None,
                pane: None,
                actions: Vec::new(),
            });
        }
        let warning = format!("{WARNING}{}", value.id);
        if !value.installed {
            self.health.warn(
                &warning,
                format!("{} is not installed", value.name),
                value.hint.clone(),
            );
        } else if value.login == LoginState::LoggedOut {
            self.health.warn(
                &warning,
                format!("{} is not logged in", value.name),
                value.hint.clone(),
            );
        } else {
            self.health.clear(&warning);
        }
        self.hub.set(
            Projection::Harnesses,
            &value.id,
            serde_json::to_value(&value).unwrap_or_default(),
        );
        let probed_at = if probed {
            Instant::now()
        } else {
            before
                .as_ref()
                .map_or_else(Instant::now, |known| known.probed)
        };
        let fresh_id = !known.contains_key(&value.id);
        known.insert(
            value.id.clone(),
            Known {
                value,
                probed: probed_at,
                notified,
            },
        );
        drop(known);
        if fresh_id {
            self.publish_order();
        }
    }

    /// The rows in the order of their ids. The webview does not sort.
    fn publish_order(&self) {
        let keys: Vec<String> = lock(&self.known).keys().cloned().collect();
        self.hub.set_order(Projection::Harnesses, &keys);
    }
}

#[cfg(test)]
mod tests;
