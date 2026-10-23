//! The projections `agents` and `attention`.

use nh_proto::api::{
    common::{Risk, Urgency},
    ids::{RunId, WindowId},
    model::{
        AgentValue, AttentionReason, AttentionValue, Execution, InteractionKind, InteractionState,
        Liveness,
    },
    projections::Projection,
};

use super::runs::{Run, Runs};
use crate::{
    interaction::{Interactions, model::Interaction},
    projection::ProjectionHub,
};

/// The row of `agents` for a run.
pub(crate) fn agent_value(run: &Run, windows: &[WindowId]) -> AgentValue {
    AgentValue {
        id: run.id,
        name: run.name.clone(),
        harness: run.harness,
        harness_version: run.version.as_ref().map(ToString::to_string),
        pane: run.pane,
        binding: run.binding.clone(),
        session_id: run.sessions.current().map(str::to_owned),
        facets: run.arbiter.facets(run.read_state(windows)),
        capabilities: run.capabilities.clone(),
        model: run.model.clone(),
        usage: run.usage.clone(),
        rate_limit: run.rate_limit.clone(),
        yolo: run.yolo || run.yolo_mode,
        isolation: None,
        resumable: false,
        started_at: run.started_at,
    }
}

/// The urgency that an interaction gives its run: the notification urgency from the risk.
pub(crate) fn interaction_urgency(risk: Risk) -> Urgency {
    match risk {
        Risk::High => Urgency::High,
        Risk::Medium | Risk::Unknown => Urgency::Normal,
        Risk::Low => Urgency::Low,
    }
}

/// The row of `attention` for a run, or `None` when the run needs nothing. A run counts
/// when it has an open interaction that a client can answer, when it is in `error` or
/// `rate_limited`, or when its turn ended while nobody looked (`turn_ended`). A
/// run that ended counts for nothing.
pub(crate) fn attention_value(run: &Run, interactions: &Interactions) -> Option<AttentionValue> {
    if matches!(run.arbiter.liveness().value, Liveness::Exited { .. }) {
        return None;
    }
    let execution = run.arbiter.execution();
    let mut reasons = Vec::new();
    let mut urgency: Option<Urgency> = None;
    let mut since = execution.provenance.at;
    let mut ids = Vec::new();
    let raise = |value: Urgency, urgency: &mut Option<Urgency>| {
        if urgency.is_none_or(|current| urgency_rank(value) < urgency_rank(current)) {
            *urgency = Some(value);
        }
    };
    let waiting: Vec<&Interaction> = interactions
        .open_of_run(run.id)
        .into_iter()
        .filter(|i| i.answerable && i.state == InteractionState::Open)
        .collect();
    for interaction in &waiting {
        let reason = match interaction.kind {
            InteractionKind::Approval => AttentionReason::Approval,
            InteractionKind::Question => AttentionReason::Question,
            InteractionKind::PlanReview => AttentionReason::PlanReview,
            InteractionKind::Notice => continue,
        };
        if !reasons.contains(&reason) {
            reasons.push(reason);
        }
        raise(interaction_urgency(interaction.risk), &mut urgency);
        ids.push(interaction.id);
        since = since.min(interaction.opened_at);
    }
    match execution.value {
        Execution::Error => {
            reasons.push(AttentionReason::Error);
            raise(Urgency::Normal, &mut urgency);
        }
        Execution::RateLimited { .. } => {
            reasons.push(AttentionReason::RateLimited);
            raise(Urgency::Low, &mut urgency);
        }
        Execution::Idle if run.turn.ended.is_some() => {
            reasons.push(AttentionReason::TurnEnded);
            raise(Urgency::Low, &mut urgency);
        }
        _ => {}
    }
    Some(AttentionValue {
        run: run.id,
        urgency: urgency?,
        reasons,
        interactions: ids,
        since,
    })
}

fn urgency_rank(urgency: Urgency) -> u8 {
    match urgency {
        Urgency::High => 0,
        Urgency::Normal => 1,
        Urgency::Low => 2,
    }
}

/// Publishes one run in `agents` and in `attention`, and puts `attention` in the order of urgency:
/// the more urgent first, and among equals the one that waited longer.
pub(crate) fn publish_run(
    hub: &ProjectionHub,
    runs: &Runs,
    interactions: &Interactions,
    id: RunId,
    windows: &[WindowId],
) {
    let Some(run) = runs.by_id.get(&id) else {
        return;
    };
    if let Ok(value) = serde_json::to_value(agent_value(run, windows)) {
        hub.set(Projection::Agents, &id.to_string(), value);
    }
    // Each window reads the run with its own read state.
    for window in windows {
        if let Ok(value) = serde_json::to_value(agent_value(run, &[*window])) {
            hub.set(Projection::AgentsFor(*window), &id.to_string(), value);
        }
    }
    match attention_value(run, interactions).and_then(|value| serde_json::to_value(value).ok()) {
        Some(value) => hub.set(Projection::Attention, &id.to_string(), value),
        None => hub.remove(Projection::Attention, &id.to_string()),
    }
    let mut waiting: Vec<_> = runs
        .by_id
        .values()
        .filter_map(|run| attention_value(run, interactions))
        .collect();
    waiting.sort_by_key(|value| (urgency_rank(value.urgency), value.since, value.run));
    let order: Vec<String> = waiting.iter().map(|value| value.run.to_string()).collect();
    hub.set_order(Projection::Attention, &order);
}

/// Takes a run out of the projections.
pub(crate) fn remove_run(hub: &ProjectionHub, id: RunId, windows: &[WindowId]) {
    hub.remove(Projection::Agents, &id.to_string());
    hub.remove(Projection::Attention, &id.to_string());
    for window in windows {
        hub.remove(Projection::AgentsFor(*window), &id.to_string());
    }
}

/// Fills the projection of a window that is new with every run, in the order of the board.
pub(crate) fn publish_window(hub: &ProjectionHub, runs: &Runs, window: WindowId) {
    for run in runs.board().iter().filter_map(|id| runs.by_id.get(id)) {
        if let Ok(value) = serde_json::to_value(agent_value(run, &[window])) {
            hub.set(Projection::AgentsFor(window), &run.id.to_string(), value);
        }
    }
    publish_board(hub, runs, &[window]);
}

/// Puts the runs of each window in the order of the board.
pub(crate) fn publish_board(hub: &ProjectionHub, runs: &Runs, windows: &[WindowId]) {
    let order: Vec<String> = runs.board().iter().map(ToString::to_string).collect();
    for window in windows {
        hub.set_order(Projection::AgentsFor(*window), &order);
    }
}

/// A window is gone: its projection goes with it.
pub(crate) fn forget_window(hub: &ProjectionHub, window: WindowId) {
    hub.forget(Projection::AgentsFor(window));
}
