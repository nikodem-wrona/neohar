//! `nh doctor` and `nh debug bundle`.
//!
//! The core gathers the facts of its install in [`gather`], and the pure functions of [`checks`]
//! turn them into a report. The same facts feed the projection `health`. A bundle is a zip with
//! the logs, the report, the configuration, statistics of the database and the recent events, and
//! every text in it has passed the redaction (`nh-redact`).
//!
//! Without a core, [`offline`] gives the checks of the install: the symlink, the runtime
//! directory and the socket.

mod bundle;
mod zipfile;

use std::{sync::Arc, time::Duration};

use nh_proto::{
    api::{
        calls::system::{
            DebugBundled, DoctorReport, IntegrationStatusParams, IntegrationStatusResult,
        },
        common::Harness,
        methods::{Call, Method},
    },
    doctor::{self as checks, Facts, Integration, PowerFacts, Shadow},
};
use serde::Serialize;
use serde_json::{Value, to_value};

use crate::{error::RpcError, integration_calls, pane::Liveness, runtime::Inner};

/// How long the doctor waits for the login shell of the person.
const SHELL_WAIT: Duration = Duration::from_secs(4);

/// How long the doctor waits for `pmset`.
const PMSET_WAIT: Duration = Duration::from_secs(2);

/// Whether this module runs the method.
pub(crate) fn handles(method: &str) -> bool {
    matches!(
        Method::from_name(method),
        Some(Method::DoctorRun | Method::DebugBundle)
    )
}

fn json<T: Serialize>(value: &T) -> Result<Value, RpcError> {
    to_value(value).map_err(|e| RpcError::internal(e.to_string()))
}

/// Runs a call that [`handles`] accepted.
pub(crate) async fn run(inner: &Arc<Inner>, call: Call) -> Result<Value, RpcError> {
    match call {
        Call::DoctorRun(_) => json(&report(inner).await),
        Call::DebugBundle(p) => {
            let DebugBundled { path, bytes, files } =
                bundle::write(inner, p.out.as_deref(), p.include_scrollback).await?;
            json(&DebugBundled { path, bytes, files })
        }
        other => Err(RpcError::internal(format!("not a doctor call: {other:?}"))),
    }
}

/// The report of a running core.
pub(crate) async fn report(inner: &Arc<Inner>) -> DoctorReport {
    let facts = gather(inner).await;
    DoctorReport {
        checks: checks::evaluate(&facts),
    }
}

/// How the login shell resolves the names of the harnesses.
async fn shadows(shell: &str) -> Vec<Shadow> {
    let names = ["claude", "codex"];
    let script = names
        .iter()
        .map(|name| format!("type -a {name} 2>/dev/null"))
        .collect::<Vec<_>>()
        .join("; ");
    let mut command = tokio::process::Command::new(shell);
    command
        .args(["-ilc", &script])
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true);
    let output = match tokio::time::timeout(SHELL_WAIT, command.output()).await {
        Ok(Ok(output)) => String::from_utf8_lossy(&output.stdout).into_owned(),
        _ => return Vec::new(),
    };
    names
        .iter()
        .map(|name| Shadow {
            harness: (*name).to_owned(),
            kind: checks::shadow_kind(&output, name),
        })
        .collect()
}

async fn integrations(inner: &Arc<Inner>) -> Vec<Integration> {
    let call = Call::IntegrationStatus(IntegrationStatusParams { target: None });
    let Ok(value) = integration_calls::run(inner, call).await else {
        return Vec::new();
    };
    let Ok(result) = serde_json::from_value::<IntegrationStatusResult>(value) else {
        return Vec::new();
    };
    result
        .status
        .into_iter()
        .map(|entry| Integration {
            harness: match entry.harness {
                Harness::Claude => "claude",
                Harness::Codex => "codex",
            }
            .to_owned(),
            installed: entry.installed,
            trusted: entry.hooks_trusted,
        })
        .collect()
}

/// What `pmset` prints for `args`, from its output and its errors: `pmset -g` writes the settings
/// to its errors. Empty when it does not run. It only reads.
async fn pmset(args: &[&str]) -> String {
    let mut command = tokio::process::Command::new("/usr/bin/pmset");
    command
        .args(args)
        .stdin(std::process::Stdio::null())
        .kill_on_drop(true);
    match tokio::time::timeout(PMSET_WAIT, command.output()).await {
        Ok(Ok(output)) => format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ),
        _ => String::new(),
    }
}

/// The sleep settings and the power source, for a paired phone. Only macOS has
/// `pmset`.
async fn power(inner: &Arc<Inner>) -> PowerFacts {
    let subscribed = inner
        .actor
        .update(|parts| parts.state.phones.subscribed())
        .await
        .unwrap_or(false);
    let (settings, battery) = if cfg!(target_os = "macos") {
        (pmset(&["-g"]).await, pmset(&["-g", "batt"]).await)
    } else {
        (String::new(), String::new())
    };
    PowerFacts {
        settings,
        battery,
        keep_awake: inner.config.config().phone.keep_awake,
        subscribed,
    }
}

/// The facts of a running core.
async fn gather(inner: &Arc<Inner>) -> Facts {
    let factory = &inner.factory;
    let mut facts = checks::static_facts(&factory.paths, &factory.session, &factory.nh_path);
    let panes: Vec<_> = crate::pane::lock(&inner.panes)
        .iter()
        .map(|(id, pane)| (*id, Arc::clone(pane)))
        .collect();
    for (id, pane) in &panes {
        if pane.info().liveness != Liveness::Running {
            continue;
        }
        facts.holders_live += 1;
        let present = factory
            .paths
            .holder_socket(&factory.session, *id)
            .is_ok_and(|socket| socket.exists());
        if !present {
            facts.holders_missing.push(id.to_string());
        }
    }
    facts.health = Some(inner.health.value());
    facts.integrations = integrations(inner).await;
    facts.machines = crate::machines::rows(inner)
        .into_iter()
        .filter(|row| row.state != nh_proto::api::model::MachineState::Local)
        .collect();
    let shell = factory
        .base_env
        .iter()
        .find(|(name, _)| name == "SHELL")
        .map_or_else(|| "/bin/sh".to_owned(), |(_, value)| value.clone());
    facts.shadows = shadows(&shell).await;
    inner.phones.refresh().await;
    let phone = inner.phones.status();
    if inner.config.config().phone.enabled || phone.listening {
        facts.phone = Some(phone);
        facts.power = Some(power(inner).await);
    }
    facts
}

#[cfg(test)]
mod tests;
