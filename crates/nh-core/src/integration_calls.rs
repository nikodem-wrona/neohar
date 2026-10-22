//! The methods `integration.*`: the installers of the hooks of the harnesses,
//! and what the core knows about them.
//!
//! Both harnesses have an installer: Claude Code writes `settings.json`, and Codex writes
//! `hooks.json`. `integration.doctor`, the round trip with a live harness, is not built yet.

use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use nh_agents::{
    capabilities::{self, Context, Coverage, TUI, Transports},
    harness::name as harness_name,
    integration::{
        self, claude, codex,
        files::{self, Action},
        hooks::{Presence, Spec},
    },
};
use nh_proto::api::{
    calls::system::{
        CapabilityRow, FileAction, FileChange, IntegrationCapabilitiesResult, IntegrationChanges,
        IntegrationInfo, IntegrationListResult, IntegrationScope, IntegrationStatusEntry,
        IntegrationStatusResult, IntegrationTarget, LaunchMode,
    },
    common::Harness,
    methods::{Call, Method},
};
use serde::Serialize;
use serde_json::{Value, to_value};

use crate::{ErrorCode, error::RpcError, runtime::Inner};

/// Whether this module runs the method.
pub(crate) fn handles(method: &str) -> bool {
    matches!(
        Method::from_name(method),
        Some(
            Method::IntegrationList
                | Method::IntegrationInstall
                | Method::IntegrationStatus
                | Method::IntegrationUninstall
                | Method::IntegrationCapabilities
                | Method::IntegrationDoctor
        )
    )
}

fn json<T: Serialize>(value: &T) -> Result<Value, RpcError> {
    to_value(value).map_err(|e| RpcError::internal(e.to_string()))
}

fn failed(error: integration::Error) -> RpcError {
    match error {
        integration::Error::Format(message) => RpcError::invalid_params(message),
        integration::Error::Changed(_) => RpcError::new(ErrorCode::StaleVersion, error.to_string()),
        integration::Error::Io(message) => RpcError::internal(message),
    }
}

/// The harnesses that a target names.
fn harnesses(target: IntegrationTarget) -> Vec<Harness> {
    match target {
        IntegrationTarget::Claude => vec![Harness::Claude],
        IntegrationTarget::Codex => vec![Harness::Codex],
        IntegrationTarget::All => vec![Harness::Claude, Harness::Codex],
    }
}

/// What the user must do once after the hooks of Codex are in `hooks.json`.
const CODEX_TRUST: &str = "Codex runs a new hook only after you trust it. Start Codex and choose \"Trust all and continue\" at the start, or run /hooks once. neohar never writes the trust.";

/// The hooks file and the spec of a harness.
fn spec_of(harness: Harness) -> &'static Spec {
    match harness {
        Harness::Claude => &claude::SPEC,
        Harness::Codex => &codex::SPEC,
    }
}

/// The command that the hooks call: the symlink of the app, or the sidecar in a test run.
fn nh_bin(inner: &Inner) -> String {
    let factory = &inner.factory;
    let path = if factory.paths.has_overrides() {
        None
    } else {
        factory.paths.nh_symlink().ok()
    };
    path.unwrap_or_else(|| factory.nh_path.clone())
        .display()
        .to_string()
}

/// The directory of the configuration of a harness: `CLAUDE_CONFIG_DIR` or `~/.claude`, and
/// `CODEX_HOME` or `~/.codex`.
pub(crate) fn config_dir(inner: &Inner, harness: Harness) -> Result<PathBuf, RpcError> {
    let (variable, default) = match harness {
        Harness::Claude => ("CLAUDE_CONFIG_DIR", ".claude"),
        Harness::Codex => ("CODEX_HOME", ".codex"),
    };
    let from_env = inner
        .factory
        .base_env
        .iter()
        .find(|(key, _)| key == variable)
        .map(|(_, value)| PathBuf::from(value));
    match from_env {
        Some(dir) => Ok(dir),
        None => Ok(inner
            .factory
            .paths
            .home
            .clone()
            .ok_or_else(|| RpcError::internal("the home directory is not known"))?
            .join(default)),
    }
}

/// The file that holds the hooks of a harness in a scope.
fn settings_path(
    inner: &Inner,
    harness: Harness,
    scope: IntegrationScope,
    project_root: Option<&str>,
) -> Result<PathBuf, RpcError> {
    match scope {
        IntegrationScope::Project => {
            let root = project_root.ok_or_else(|| {
                RpcError::invalid_params("the scope project needs project_root: the repository")
            })?;
            Ok(match harness {
                Harness::Claude => Path::new(root).join(".claude").join("settings.local.json"),
                Harness::Codex => Path::new(root).join(".codex").join("hooks.json"),
            })
        }
        IntegrationScope::User => Ok(config_dir(inner, harness)?.join(match harness {
            Harness::Claude => "settings.json",
            Harness::Codex => "hooks.json",
        })),
    }
}

fn file_action(action: Action) -> FileAction {
    match action {
        Action::Created => FileAction::Created,
        Action::Modified => FileAction::Modified,
        Action::Removed => FileAction::Removed,
        Action::Unchanged => FileAction::Unchanged,
    }
}

fn change_of(outcome: &files::Outcome, dry_run: bool, notes: &mut Vec<String>) -> FileChange {
    if dry_run && !outcome.diff.is_empty() {
        notes.push(outcome.diff.clone());
    }
    FileChange {
        path: outcome.path.display().to_string(),
        action: file_action(outcome.action),
        backup: outcome.backup.as_ref().map(|p| p.display().to_string()),
    }
}

/// Runs a blocking job of the installer on a thread of its own.
async fn blocking<T: Send + 'static>(
    job: impl FnOnce() -> Result<T, RpcError> + Send + 'static,
) -> Result<T, RpcError> {
    tokio::task::spawn_blocking(job)
        .await
        .map_err(|e| RpcError::internal(format!("the installer task failed: {e}")))?
}

/// Runs a call that [`handles`] accepted.
pub(crate) async fn run(inner: &Arc<Inner>, call: Call) -> Result<Value, RpcError> {
    match call {
        Call::IntegrationInstall(p) => install(inner, &p).await,
        Call::IntegrationUninstall(p) => uninstall(inner, &p).await,
        Call::IntegrationStatus(p) => status(inner, p.target).await,
        Call::IntegrationList(_) => list(inner).await,
        Call::IntegrationCapabilities(_) => capabilities_matrix(inner).await,
        Call::IntegrationDoctor(_) => Err(RpcError::new(
            ErrorCode::Unsupported,
            "integration.doctor, the round trip with a live harness, is not built yet; use doctor.run",
        )),
        other => Err(RpcError::internal(format!(
            "not an integration call: {other:?}"
        ))),
    }
}

async fn install(
    inner: &Arc<Inner>,
    p: &nh_proto::api::calls::system::IntegrationInstall,
) -> Result<Value, RpcError> {
    let mut notes = Vec::new();
    let mut changes = Vec::new();
    for harness in harnesses(p.target) {
        let path = settings_path(inner, harness, p.scope, p.project_root.as_deref())?;
        let bin = nh_bin(inner);
        let (spec, dry_run) = (spec_of(harness), p.dry_run);
        let outcome =
            blocking(move || files::install_hooks(spec, &path, &bin, dry_run).map_err(failed))
                .await?;
        changes.push(change_of(&outcome, dry_run, &mut notes));
        if dry_run || outcome.action == Action::Unchanged {
            continue;
        }
        match harness {
            Harness::Claude => notes.push(
                "Claude Code reads its hooks when a session starts. Start a new session to use them."
                    .to_owned(),
            ),
            Harness::Codex => notes.push(CODEX_TRUST.to_owned()),
        }
    }
    json(&IntegrationChanges { changes, notes })
}

async fn uninstall(
    inner: &Arc<Inner>,
    p: &nh_proto::api::calls::system::IntegrationUninstall,
) -> Result<Value, RpcError> {
    let mut notes = Vec::new();
    let mut changes = Vec::new();
    for harness in harnesses(p.target) {
        let path = settings_path(inner, harness, p.scope, p.project_root.as_deref())?;
        let spec = spec_of(harness);
        let outcome =
            blocking(move || files::uninstall_hooks(spec, &path, false).map_err(failed)).await?;
        changes.push(change_of(&outcome, false, &mut notes));
    }
    json(&IntegrationChanges { changes, notes })
}

/// The hooks of a harness at the user scope: the file, and how each event stands.
async fn hooks_state(
    inner: &Arc<Inner>,
    harness: Harness,
) -> Result<(PathBuf, Vec<integration::hooks::EventState>), RpcError> {
    let path = settings_path(inner, harness, IntegrationScope::User, None)?;
    let bin = nh_bin(inner);
    let (file, spec) = (path.clone(), spec_of(harness));
    // A file that cannot be read is not an installation.
    let states =
        blocking(move || Ok(files::inspect_hooks(spec, &file, &bin).unwrap_or_default())).await?;
    Ok((path, states))
}

/// Whether every hook of the harness at the user scope is current.
async fn installed(inner: &Arc<Inner>, harness: Harness) -> Result<(PathBuf, bool), RpcError> {
    let (path, states) = hooks_state(inner, harness).await?;
    let installed = !states.is_empty()
        && states
            .iter()
            .all(|state| state.presence == Presence::Current);
    Ok((path, installed))
}

/// The version of a harness and whether a golden recording covers it.
async fn version_of(inner: &Arc<Inner>, harness: Harness) -> (Option<semver::Version>, bool) {
    let agents = Arc::clone(&inner.agents);
    let manifest = harness_name(harness).to_owned();
    let version = tokio::task::spawn_blocking(move || agents.probe_version(&manifest))
        .await
        .ok()
        .flatten();
    let validated = version.as_ref().is_some_and(|version| {
        let loaded = inner.agents.manifests();
        loaded.get(harness_name(harness)).is_some_and(|manifest| {
            capabilities::resolve(
                &Context {
                    manifest,
                    harness,
                    version: Some(version),
                    mode: TUI,
                    transports: Transports { hooks: true },
                    bound: true,
                    screen_validates: false,
                },
                &Coverage::builtin(),
            )
            .unvalidated
            .is_none()
        })
    });
    (version, validated)
}

async fn status(inner: &Arc<Inner>, target: Option<IntegrationTarget>) -> Result<Value, RpcError> {
    let mut status = Vec::new();
    for harness in harnesses(target.unwrap_or(IntegrationTarget::All)) {
        let (path, states) = hooks_state(inner, harness).await?;
        let installed = !states.is_empty()
            && states
                .iter()
                .all(|state| state.presence == Presence::Current);
        let (_, validated) = version_of(inner, harness).await;
        // Codex keeps the trust of each hook in its `config.toml`. neohar only reads it.
        let hooks_trusted = if harness == Harness::Codex && installed {
            let config = config_dir(inner, harness)?.join("config.toml");
            let text =
                blocking(move || Ok(files::read_text(&config).ok().flatten().unwrap_or_default()))
                    .await?;
            let trusted = codex::trust(&text, &path.display().to_string(), &states);
            Some(!trusted.is_empty() && trusted.iter().all(|(_, trusted)| *trusted))
        } else {
            None
        };
        status.push(IntegrationStatusEntry {
            harness,
            installed,
            files: vec![path.display().to_string()],
            hooks_trusted,
            version_validated: validated,
            last_signal_at: inner.agents.last_signal(harness),
        });
    }
    json(&IntegrationStatusResult { status })
}

async fn list(inner: &Arc<Inner>) -> Result<Value, RpcError> {
    let mut integrations = Vec::new();
    for harness in [Harness::Claude, Harness::Codex] {
        let installed = installed(inner, harness).await?.1;
        let (version, _) = version_of(inner, harness).await;
        // The transports in use by the live runs of the harness.
        let in_use = inner
            .actor
            .update(move |parts| {
                let runs: Vec<_> = parts
                    .state
                    .runs
                    .by_id
                    .values()
                    .filter(|run| run.live() && run.harness == harness)
                    .map(|run| run.hooks)
                    .collect();
                let mut transports = Vec::new();
                if runs.iter().any(|hooks| *hooks) {
                    transports.push("hooks".to_owned());
                }
                if !runs.is_empty() {
                    transports.push("process".to_owned());
                }
                transports
            })
            .await
            .map_err(|e| RpcError::internal(e.to_string()))?;
        integrations.push(IntegrationInfo {
            harness,
            installed,
            version_seen: version.map(|v| v.to_string()),
            transports: in_use,
        });
    }
    json(&IntegrationListResult { integrations })
}

/// The capability matrix as it resolves for the versions that are installed.
async fn capabilities_matrix(inner: &Arc<Inner>) -> Result<Value, RpcError> {
    let mut rows = Vec::new();
    let coverage = Coverage::builtin();
    for harness in [Harness::Claude, Harness::Codex] {
        let (version, _) = version_of(inner, harness).await;
        let Some(version) = version else {
            continue;
        };
        let loaded = inner.agents.manifests();
        let Some(manifest) = loaded.get(harness_name(harness)) else {
            continue;
        };
        // A bound run that has the hooks, which is what the integration makes.
        let resolved = capabilities::resolve(
            &Context {
                manifest,
                harness,
                version: Some(&version),
                mode: TUI,
                transports: Transports { hooks: true },
                bound: true,
                screen_validates: false,
            },
            &coverage,
        );
        rows.push(CapabilityRow {
            harness,
            version: version.to_string(),
            mode: LaunchMode::Tui,
            capabilities: resolved.capabilities,
        });
    }
    json(&IntegrationCapabilitiesResult { rows })
}
