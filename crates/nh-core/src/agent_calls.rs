//! The methods `agent.*` that act on a run that exists:
//! `agent.list`, `show`, `prompt`, `reply`, `interrupt`, `stop`, `link`, `unlink`, `move` and
//! `wait`. Also
//! `agent.choices`, the models and the effort levels that a launch can choose, with the state of
//! each harness, and `harness.refresh`.
//!
//! A call that types into a pane or ends a program needs a **bound** run: a run that
//! only looks like an agent, by its process or by its screen, is never given a prompt or a key.
//! The keys and the prompt go through the same lock as an answer by keystrokes, so a person who
//! types at the same moment wins and the call fails with `busy`.

use std::{sync::Arc, time::Duration};

use nh_agents::{
    codex::models::{CATALOG_FILE, catalog},
    harness::name as harness_name,
    manifest::{
        choices::{self, CODEX_CATALOG},
        schema::Launch,
    },
};
use nh_proto::api::{
    calls::agent::{
        AgentChoicesResult, AgentLinked, AgentListResult, AgentShown, AgentWaitResult, WaitFor,
    },
    common::{Empty, Harness, JobStarted},
    ids::{Ref, RunId},
    methods::{Call, Method},
    model::{AgentValue, Execution, Liveness},
};
use serde::Serialize;
use serde_json::{Value, to_value};
use tokio::time::Instant;

use crate::{
    ErrorCode,
    actor::{Origin, Parts, Scope},
    agents::{project, runs},
    error::RpcError,
    runtime::Inner,
};

/// How often `agent.wait` looks at the run.
const WAIT_POLL: Duration = Duration::from_millis(100);

/// Whether this module runs the method.
pub(crate) fn handles(method: &str) -> bool {
    matches!(
        Method::from_name(method),
        Some(
            Method::AgentList
                | Method::AgentShow
                | Method::AgentPrompt
                | Method::AgentReply
                | Method::AgentInterrupt
                | Method::AgentStop
                | Method::AgentLink
                | Method::AgentUnlink
                | Method::AgentMove
                | Method::AgentWait
                | Method::AgentStart
                | Method::AgentResume
                | Method::AgentChoices
                | Method::HarnessRefresh
        )
    )
}

fn json<T: Serialize>(value: &T) -> Result<Value, RpcError> {
    to_value(value).map_err(|e| RpcError::internal(e.to_string()))
}

fn gone(error: impl std::fmt::Display) -> RpcError {
    RpcError::internal(error.to_string())
}

fn not_found(reference: &Ref<RunId>) -> RpcError {
    RpcError::new(ErrorCode::NotFound, format!("no agent {reference}"))
}

/// The row of `agents` for a run, as the API shows it.
fn value(parts: &Parts<'_>, id: RunId) -> Option<AgentValue> {
    let windows: Vec<_> = parts.state.layout.windows.iter().map(|w| w.id).collect();
    let run = parts.state.runs.by_id.get(&id)?;
    Some(project::agent_value(run, &windows))
}

fn reached(parts: &Parts<'_>, id: RunId, until: WaitFor) -> bool {
    let Some(run) = parts.state.runs.by_id.get(&id) else {
        return true;
    };
    match until {
        WaitFor::Idle => matches!(run.arbiter.execution().value, Execution::Idle),
        WaitFor::Exited => matches!(run.arbiter.liveness().value, Liveness::Exited { .. }),
        WaitFor::Attention => project::attention_value(run, &parts.state.interactions).is_some(),
    }
}

async fn wait(
    inner: &Arc<Inner>,
    reference: Ref<RunId>,
    until: WaitFor,
    timeout_ms: Option<u64>,
) -> Result<Value, RpcError> {
    let deadline = timeout_ms.map(|ms| Instant::now() + Duration::from_millis(ms));
    loop {
        let reference = reference.clone();
        let (done, row) = inner
            .actor
            .update(move |parts| {
                let id = parts
                    .state
                    .runs
                    .resolve(&reference)
                    .ok_or_else(|| not_found(&reference))?;
                Ok::<_, RpcError>((reached(&parts, id, until), value(&parts, id)))
            })
            .await
            .map_err(gone)??;
        let expired = deadline.is_some_and(|at| Instant::now() >= at);
        if done || expired {
            let agent =
                row.ok_or_else(|| RpcError::new(ErrorCode::NotFound, "the agent is gone"))?;
            return json(&AgentWaitResult {
                reached: done,
                agent,
            });
        }
        tokio::time::sleep(WAIT_POLL).await;
    }
}

/// Runs a call that [`handles`] accepted.
pub(crate) async fn run(inner: &Arc<Inner>, call: Call, origin: Origin) -> Result<Value, RpcError> {
    match call {
        Call::AgentList(p) => {
            let agents =
                inner
                    .actor
                    .update(move |parts| {
                        let pane = match &p.pane {
                            Some(reference) => {
                                Some(parts.state.layout.resolve_pane(reference).map_err(|e| {
                                    RpcError::new(ErrorCode::NotFound, e.to_string())
                                })?)
                            }
                            None => None,
                        };
                        let workspace = match &p.workspace {
                            Some(reference) => {
                                let at = parts.state.layout.resolve_workspace(reference).map_err(
                                    |e| RpcError::new(ErrorCode::NotFound, e.to_string()),
                                )?;
                                Some(parts.state.layout.workspaces[at].id)
                            }
                            None => None,
                        };
                        let mut ids: Vec<(i64, RunId)> = parts
                            .state
                            .runs
                            .by_id
                            .values()
                            .filter(|run| run.live())
                            .filter(|run| p.harness.is_none_or(|h| run.harness == h))
                            .filter(|run| pane.is_none_or(|pane| run.pane == Some(pane)))
                            .filter(|run| {
                                workspace.is_none_or(|ws| {
                                    run.pane
                                        .and_then(|pane| parts.state.layout.panes.get(&pane))
                                        .is_some_and(|entry| entry.workspace == ws)
                                })
                            })
                            .map(|run| (run.started_at.0, run.id))
                            .collect();
                        ids.sort_unstable();
                        Ok::<_, RpcError>(
                            ids.into_iter()
                                .filter_map(|(_, id)| value(&parts, id))
                                .collect::<Vec<_>>(),
                        )
                    })
                    .await
                    .map_err(gone)??;
            json(&AgentListResult { agents })
        }
        Call::AgentShow(p) => {
            let agent = inner
                .actor
                .update(move |parts| {
                    let id = parts
                        .state
                        .runs
                        .resolve(&p.agent)
                        .ok_or_else(|| not_found(&p.agent))?;
                    value(&parts, id).ok_or_else(|| not_found(&p.agent))
                })
                .await
                .map_err(gone)??;
            json(&AgentShown { agent })
        }
        Call::AgentPrompt(p) => {
            inner.control.prompt(p.agent, p.text, p.submit).await?;
            json(&Empty {})
        }
        Call::AgentReply(p) => {
            let written = inner.control.reply(p.run, p.input_id, p.text).await?;
            // A reply from a phone counts for the metrics (the event `phone.replied`).
            if let (Some(run), Scope::Phone, Some(device)) = (written, origin.scope, origin.device)
            {
                inner.actor.update_detached(move |mut parts| {
                    crate::phones::replied(&mut parts, device, run)
                });
            }
            json(&Empty {})
        }
        Call::AgentInterrupt(p) => {
            inner.control.interrupt(p.agent).await?;
            json(&Empty {})
        }
        Call::AgentStop(p) => {
            inner.control.stop(p.agent, p.force).await?;
            json(&Empty {})
        }
        Call::AgentStart(p) => {
            let job = inner.control.start(p).await?;
            json(&JobStarted { job })
        }
        Call::AgentResume(p) => {
            let job = inner.control.resume(p).await?;
            json(&JobStarted { job })
        }
        Call::AgentWait(p) => wait(inner, p.agent, p.until, p.timeout_ms).await,
        Call::AgentChoices(p) => agent_choices(inner, p.harness).await,
        Call::HarnessRefresh(p) => {
            inner.discovery.refresh(p.harness.as_deref()).await?;
            json(&Empty {})
        }
        Call::AgentLink(p) => {
            let run = inner
                .actor
                .update(move |mut parts| {
                    let pane = parts
                        .state
                        .layout
                        .resolve_pane(&p.pane)
                        .map_err(|e| RpcError::new(ErrorCode::NotFound, e.to_string()))?;
                    runs::link(&mut parts, pane, p.harness, p.session_id)
                })
                .await
                .map_err(gone)??;
            json(&AgentLinked { run })
        }
        Call::AgentUnlink(p) => {
            inner
                .actor
                .update(move |mut parts| {
                    let id = parts
                        .state
                        .runs
                        .resolve(&p.agent)
                        .ok_or_else(|| not_found(&p.agent))?;
                    runs::unlink(&mut parts, id);
                    Ok::<_, RpcError>(())
                })
                .await
                .map_err(gone)??;
            json(&Empty {})
        }
        Call::AgentMove(p) => {
            inner
                .actor
                .update(move |mut parts| {
                    let id = parts
                        .state
                        .runs
                        .resolve(&p.agent)
                        .ok_or_else(|| not_found(&p.agent))?;
                    let index = usize::try_from(p.index).unwrap_or(usize::MAX);
                    runs::move_on_board(&mut parts, id, index);
                    Ok::<_, RpcError>(())
                })
                .await
                .map_err(gone)??;
            json(&Empty {})
        }
        _ => Err(RpcError::internal("not an agent call")),
    }
}

/// `agent.choices`: the menus of each harness, from its manifest and the catalog that the manifest
/// names. A catalog that is not there or not right leaves the list of the manifest.
async fn agent_choices(inner: &Arc<Inner>, only: Option<Harness>) -> Result<Value, RpcError> {
    let wanted: Vec<Harness> =
        only.map_or_else(|| vec![Harness::Claude, Harness::Codex], |h| vec![h]);
    let launches: Vec<(Harness, Option<Launch>)> = inner
        .actor
        .update(move |parts| {
            let loaded = runs::loaded_manifests(&parts);
            wanted
                .into_iter()
                .map(|harness| {
                    let launch = loaded
                        .get(harness_name(harness))
                        .and_then(|manifest| manifest.launch.clone());
                    (harness, launch)
                })
                .collect()
        })
        .await
        .map_err(gone)?;
    let mut harnesses = Vec::with_capacity(launches.len());
    for (harness, launch) in launches {
        let models = match launch.as_ref().and_then(|l| l.model_catalog.as_deref()) {
            Some(CODEX_CATALOG) => {
                let path =
                    crate::integration_calls::config_dir(inner, Harness::Codex)?.join(CATALOG_FILE);
                tokio::task::spawn_blocking(move || std::fs::read_to_string(path))
                    .await
                    .ok()
                    .and_then(Result::ok)
                    .and_then(|text| catalog(&text))
            }
            _ => None,
        };
        let mut menus = choices::of(harness, launch.as_ref(), models);
        // The state of the harness on this machine. Before the first probe, the call
        // asks the harness now, or takes the round that runs.
        if let Some(row) = inner.discovery.row_or_probe(harness_name(harness)).await {
            menus.installed = row.installed;
            menus.login = row.login;
            menus.available = row.available;
            menus.hint = (!row.available).then_some(row.hint).flatten();
        }
        harnesses.push(menus);
    }
    json(&AgentChoicesResult { harnesses })
}
