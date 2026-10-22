//! The debug methods of the API. They exist only in a build with the feature `debug`. The tests,
//! `nh-chaos` and `nh-golden` use them.
//!
//! | Method | Parameters | Result |
//! |---|---|---|
//! | `debug.spawn_panes` | `{count?, cols, rows, argv?, cwd?, env?, isolated_home?}` (`DebugSpawnPanes` of the contract) | `{panes: [{pane, render_id}]}`; the panes are not in the layout, and no window draws them |
//! | `debug.emit_events` | `{count, kind?, subject?, batch?}` | `{emitted}`; appends `count` events of the kind `debug.tick` (default), `batch` (default 100) in each transaction, and sends them to the subscribers of `events.subscribe` |
//! | `debug.screen` | `{pane, dump?}` (`DebugScreen` of the contract) | `DebugScreenResult`: the cells, cursor, modes and title, read with the journal offset they belong to |
//! | `debug.pane_inputs` | `{pane, since?}` | `{acked, next, pending, unconfirmed}`; the ids that the holder acknowledged since the number `since` |
//! | `debug.restore_report` | `{}` | what the restart of this core did, pane by pane |
//! | `debug.interactions` | `{}` | `{interactions}`; the interactions of the hook gate, with their state, decision and attempts |
//! | `debug.answer` | `{interaction, decision, by?, via?, hold?}` | `{interaction, rev, state}`, or the error `already_answered {by, via, decision}` or `interaction_closed` |
//! | `debug.deliver` | `{interaction}` | `{interaction, state}`; delivers a decision that `hold` kept back |
//! | `debug.release` | `{interaction, reason?}` | `{interaction, mode}`; the hook prints nothing, and the harness shows its dialog |
//!
//! `pane` is the ULID of the pane.

use std::sync::Arc;

use nh_proto::api::{
    calls::internal::{
        DebugRestoredScreen, DebugScreen, DebugScreenResult, DebugSpawnPanes, DebugSpawned,
        DebugSpawnedPane,
    },
    ids::PaneId,
};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::{
    error::RpcError,
    pane::{Pane, PaneSpec},
    runtime::Inner,
};

fn parse<T: for<'de> Deserialize<'de>>(params: &Value) -> Result<T, RpcError> {
    T::deserialize(params).map_err(|e| RpcError::invalid_params(e.to_string()))
}

/// The debug methods that do not wait. They run in the caller's thread. Returns `None` for the
/// other methods, which [`call_async`] runs.
pub(crate) fn call_sync(
    core: &Arc<Inner>,
    method: &str,
    params: &Value,
) -> Option<Result<Value, RpcError>> {
    Some(match method {
        "debug.screen" => parse::<DebugScreen>(params).and_then(|p| {
            let pane = core.pane(&p.pane.to_string())?;
            screen(&pane, p.dump)
        }),
        "debug.pane_inputs" => parse::<InputsParams>(params).and_then(|p| {
            let pane = core.pane(&p.pane)?;
            let (acked, next) = pane.acked_inputs(p.since);
            let unconfirmed: Vec<Value> = pane
                .info()
                .input_unconfirmed
                .iter()
                .map(|u| json!({ "input_id": u.input_id, "reason": u.reason.as_str() }))
                .collect();
            Ok(json!({
                "acked": acked,
                "next": next,
                "pending": pane.pending_inputs(),
                "unconfirmed": unconfirmed,
            }))
        }),
        "debug.restore_report" => Ok(core.restore.report().to_json()),
        _ => return None,
    })
}

/// The debug methods that start processes or wait for them. They run on the runtime of the core.
pub(crate) async fn call_async(
    core: &Arc<Inner>,
    method: &str,
    params: Value,
) -> Result<Value, RpcError> {
    match method {
        "debug.interactions" | "debug.answer" | "debug.deliver" | "debug.release" => {
            crate::interaction::debug::call(core, method, params).await
        }
        "debug.spawn_panes" => spawn_panes(core, &params).await,
        "debug.emit_events" => emit_events(core, &params).await,
        _ => Err(RpcError::method_not_found(method)),
    }
}

#[derive(Deserialize)]
struct EmitParams {
    count: u32,
    kind: Option<String>,
    subject: Option<String>,
    #[serde(default = "default_batch")]
    batch: u32,
}

fn default_batch() -> u32 {
    100
}

/// Appends events to the log, as the handler of a command does. The tests of the event
/// subscriptions use it.
async fn emit_events(core: &Arc<Inner>, params: &Value) -> Result<Value, RpcError> {
    let EmitParams {
        count,
        kind,
        subject,
        batch,
    } = parse(params)?;
    let kind = kind.unwrap_or_else(|| "debug.tick".to_owned());
    let batch = batch.max(1);
    let mut emitted = 0u32;
    while emitted < count {
        let n = (count - emitted).min(batch);
        let first = emitted;
        let (kind, subject, sender) = (kind.clone(), subject.clone(), core.events.clone());
        core.actor
            .store(move |store| {
                let ((), events) = store.tx(crate::persist::now_ms(), |tx| {
                    for i in 0..n {
                        tx.append_raw(&kind, 1, subject.as_deref(), json!({ "n": first + i }))?;
                    }
                    Ok(())
                })?;
                // The actor sends the events of its own commands. This job does it itself.
                for event in events {
                    let _ = sender.send(event);
                }
                Ok(())
            })
            .await
            .map_err(|e| RpcError::internal(e.to_string()))?;
        emitted += n;
    }
    Ok(json!({ "emitted": emitted }))
}

#[derive(Deserialize)]
struct InputsParams {
    pane: String,
    #[serde(default)]
    since: u64,
}

/// What the pane shows, with the journal offset it belongs to.
fn screen(pane: &Pane, with_dump: bool) -> Result<Value, RpcError> {
    let to_value = |value: &nh_term::ScreenDump| {
        serde_json::to_value(value).map_err(|e| RpcError::internal(e.to_string()))
    };
    let info = pane.info();
    let (fed_offset, dump, restored) = if with_dump {
        let (dump, fed_offset) = pane.dump();
        let restored = match pane.restored_dump() {
            Some((offset, dump)) => Some(DebugRestoredScreen {
                fed_offset: offset,
                dump: to_value(&dump)?,
            }),
            None => None,
        };
        (fed_offset, Some(to_value(&dump)?), restored)
    } else {
        (pane.fed_offset(), None, None)
    };
    serde_json::to_value(DebugScreenResult {
        pane: PaneId::new(pane.id()),
        fed_offset,
        since_resize_ms: u64::try_from(pane.since_resize().as_millis()).unwrap_or(u64::MAX),
        restoring: info.restoring,
        settled: pane.settled(),
        recovery: info.recovery.as_str().to_owned(),
        asked_size: pane.asked_size(),
        size: pane.size(),
        dump,
        restored,
    })
    .map_err(|e| RpcError::internal(e.to_string()))
}

/// Makes the three directories of one isolated home and returns the variables that name them.
fn isolated_home(core: &Inner) -> Result<(String, Vec<(String, String)>), RpcError> {
    use std::os::unix::fs::DirBuilderExt;
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis());
    let root = core
        .factory
        .paths
        .data_dir(&core.factory.session)
        .map_err(|e| RpcError::internal(e.to_string()))?
        .join("homes")
        .join(format!("{ms}-{n}"));
    let dir = |name: &str| -> Result<String, RpcError> {
        let path = root.join(name);
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(&path)
            .map_err(|e| RpcError::internal(format!("cannot make {}: {e}", path.display())))?;
        Ok(path.display().to_string())
    };
    let home = dir("home")?;
    let env = vec![
        ("HOME".to_owned(), home.clone()),
        ("CLAUDE_CONFIG_DIR".to_owned(), dir("claude")?),
        ("CODEX_HOME".to_owned(), dir("codex")?),
    ];
    Ok((home, env))
}

/// The most panes one call starts.
const MAX_PANES: u32 = 64;

/// Starts `count` panes with the same program. The panes are not in the layout, and no window
/// draws them. They start one after the other, so that their order is the order of the
/// projection.
async fn spawn_panes(core: &Arc<Inner>, params: &Value) -> Result<Value, RpcError> {
    let p: DebugSpawnPanes = parse(params)?;
    if p.count == 0 || p.count > MAX_PANES {
        return Err(RpcError::invalid_params(format!(
            "count is 1 to {MAX_PANES}, not {}",
            p.count
        )));
    }
    let argv = p.argv.unwrap_or_else(|| {
        vec![
            core.factory
                .base_env
                .iter()
                .find(|(k, _)| k == "SHELL")
                .map_or_else(|| "/bin/sh".to_owned(), |(_, v)| v.clone()),
        ]
    });
    let mut started = Vec::new();
    for _ in 0..p.count {
        let mut spec = PaneSpec {
            argv: argv.clone(),
            cwd: p.cwd.clone(),
            env: p.env.clone(),
            cols: p.cols,
            rows: p.rows,
        };
        if p.isolated_home {
            let (home, env) = isolated_home(core)?;
            spec.cwd.get_or_insert(home);
            spec.env.splice(0..0, env);
        }
        let pane = core.spawn_pane(spec).await?;
        started.push(DebugSpawnedPane {
            pane: PaneId::new(pane.id()),
            render_id: pane.render_id(),
        });
    }
    serde_json::to_value(DebugSpawned { panes: started })
        .map_err(|e| RpcError::internal(e.to_string()))
}
