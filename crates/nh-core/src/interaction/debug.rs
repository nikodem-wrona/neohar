//! The debug methods for interactions: `debug.interactions`, `debug.answer`, `debug.deliver` and
//! `debug.release`. A test or a recording of a harness uses them to control the order of events.
//! They exist in a build with the feature `debug` only.

use nh_proto::api::{
    calls::internal::{DebugAnswer, DebugAnswered},
    ids::{InteractionId, Ref},
};
use serde::Deserialize;
use serde_json::{Value, json, to_value};

use super::{
    apply,
    model::{Effect, Interaction},
    rows::name,
};
use crate::ErrorCode;
use std::sync::Arc;

use crate::{error::RpcError, runtime::Inner};

#[derive(Deserialize)]
struct InteractionParams {
    interaction: Ref<InteractionId>,
}

#[derive(Deserialize)]
struct ReleaseParams {
    interaction: Ref<InteractionId>,
    #[allow(dead_code, reason = "the reason is for the log of the recording")]
    reason: Option<String>,
}

fn parse<T: for<'de> Deserialize<'de>>(params: Value) -> Result<T, RpcError> {
    serde_json::from_value(params).map_err(|e| RpcError::invalid_params(e.to_string()))
}

fn row(interaction: &Interaction) -> Value {
    let delivery = &interaction.delivery;
    json!({
        "id": interaction.id.to_string(),
        "handle": interaction.handle,
        "kind": name(&interaction.kind),
        "state": name(&interaction.state),
        "mode": name(&interaction.mode),
        "native_ref": interaction.native_ref,
        "answerable": interaction.answerable,
        "channels": interaction.channels.iter().map(name).collect::<Vec<_>>(),
        "risk": name(&interaction.risk),
        "lease": delivery.lease.is_some(),
        "conn": delivery.lease.map(|l| l.conn),
        "holder": delivery.lease.map(|l| l.holder.as_str()),
        "shim_pid": delivery.lease.and_then(|l| l.pid),
        "acked": delivery.acked,
        "rev": delivery.rev,
        "decision": delivery.decision,
        "decided_by": delivery.decided_by,
        "via": delivery.via.map(|v| name(&v)),
        "key": delivery.key,
        "channel": delivery.channel.map(|c| name(&c)),
        "attempts": delivery.attempts.iter().map(|a| a.outcome).collect::<Vec<_>>(),
        "fail_reason": delivery.fail_reason,
        "deadline": interaction.deadline.map(|d| d.0),
    })
}

/// Runs a debug method of the interactions.
pub(crate) async fn call(
    core: &Arc<Inner>,
    method: &str,
    params: Value,
) -> Result<Value, RpcError> {
    let internal = |e: crate::error::CoreError| RpcError::internal(e.to_string());
    match method {
        "debug.interactions" => core
            .actor
            .update(|parts| {
                let table = &parts.state.interactions;
                let all: Vec<Value> = table
                    .open()
                    .into_iter()
                    .chain(table.ended())
                    .map(row)
                    .collect();
                json!({"interactions": all})
            })
            .await
            .map_err(internal),
        "debug.answer" => {
            let p: DebugAnswer = parse(params)?;
            core.actor
                .update(move |mut parts| {
                    let id = parts
                        .state
                        .interactions
                        .resolve(&p.interaction)
                        .ok_or_else(|| RpcError::new(ErrorCode::NotFound, "no such interaction"))?;
                    let hold = p.hold;
                    let answered = apply::transition(&mut parts, id, |interaction| {
                        let before = interaction.clone();
                        match interaction.answer(p.decision, &p.by, p.via) {
                            Ok(mut step) => {
                                if hold {
                                    step.effects.retain(|e| *e != Effect::Deliver);
                                }
                                (
                                    Ok(DebugAnswered {
                                        rev: interaction.delivery.rev,
                                        state: interaction.state,
                                    }),
                                    step,
                                )
                            }
                            Err(refusal) => (
                                Err(super::calls::refuse(refusal, &before)),
                                super::model::Step::default(),
                            ),
                        }
                    })
                    .ok_or_else(|| RpcError::new(ErrorCode::NotFound, "no such interaction"))??;
                    to_value(answered).map_err(|e| RpcError::internal(e.to_string()))
                })
                .await
                .map_err(internal)?
        }
        "debug.deliver" => {
            let p: InteractionParams = parse(params)?;
            core.actor
                .update(move |mut parts| {
                    let id = parts
                        .state
                        .interactions
                        .resolve(&p.interaction)
                        .ok_or_else(|| RpcError::new(ErrorCode::NotFound, "no such interaction"))?;
                    apply::deliver(&mut parts, id);
                    Ok(json!({}))
                })
                .await
                .map_err(internal)?
        }
        "debug.release" => {
            let p: ReleaseParams = parse(params)?;
            core.actor
                .update(move |mut parts| {
                    let id = parts
                        .state
                        .interactions
                        .resolve(&p.interaction)
                        .ok_or_else(|| RpcError::new(ErrorCode::NotFound, "no such interaction"))?;
                    apply::release(&mut parts, id);
                    Ok(json!({}))
                })
                .await
                .map_err(internal)?
        }
        other => Err(RpcError::method_not_found(other)),
    }
}
