//! The interactions on the state actor.
//!
//! A function here takes the interaction out of the table, lets the model make one transition,
//! and then does the three things that every change needs: it writes the row and the events in
//! one transaction, it puts the interaction in the table and the projections, and it hands the
//! effects (deliver a decision, release a hook) to the gates. This is the pattern of the runs.

use nh_agents::{
    adapter::{AdapterSignal, Resolution},
    arbiter::{Input, Observation},
};
use nh_proto::api::{
    calls::{
        agent::{ApprovalDecision, Behavior, Decision},
        system::PolicyDecision,
    },
    common::{Source as ArbiterSource, Via},
    ids::{InteractionId, PaneId, RunId},
    model::{
        AnswerChannel, Binding, GateMode, InteractionKind, InteractionPayload, InteractionState,
    },
    time::Timestamp,
};

use super::{
    gate::{GateOutcome, GateReply},
    model::{Interaction, Lease, OpenSpec, Source, Step},
    output::hook_output,
    project,
    render::{Rendered, Subject},
    rows,
};
use crate::{
    actor::Parts,
    agents::runs,
    config::ApprovalMode,
    ids::new_interaction_id,
    persist::now_ms,
    policy::{self, fingerprint},
};

/// The rendered payload of the signal at `index` of a batch.
pub(crate) struct Prepared {
    pub index: usize,
    pub rendered: Rendered,
}

/// The interactions that a batch of signals opened, for the reply of a held hook.
pub(crate) struct Opened {
    pub id: InteractionId,
    /// The core holds the hook that this batch comes from. A hook that it does not hold gets "no
    /// decision" at once.
    pub held: bool,
}

/// The signals that the adapter of a run reported. The interactions open, and end by a native
/// resolution, in the order of the signals. `lease` is the hook that this batch comes from, when a
/// shim holds one: the first interaction it opens takes it.
pub(crate) fn signals(
    parts: &mut Parts<'_>,
    run: RunId,
    emitted: &[AdapterSignal],
    mut prepared: Vec<Prepared>,
    at: Timestamp,
    mut lease: Option<Lease>,
) -> Vec<Opened> {
    let mut opened = Vec::new();
    for (index, signal) in emitted.iter().enumerate() {
        match signal {
            AdapterSignal::InteractionOpen {
                kind,
                native_ref,
                gate,
                ..
            } => {
                let Some(at_index) = prepared.iter().position(|p| p.index == index) else {
                    continue;
                };
                let rendered = prepared.swap_remove(at_index).rendered;
                if let Some(found) = open(parts, run, *kind, native_ref, *gate, rendered, at, lease)
                {
                    lease = None;
                    opened.push(found);
                }
            }
            AdapterSignal::InteractionResolved {
                native_ref,
                resolution,
            } => resolved(parts, run, native_ref, resolution, at),
            // The end of a tool call ends the request that it made. The adapter says so with
            // `InteractionResolved` when it opened the interaction, but an adapter that started
            // after a restart of the core never saw the request.
            AdapterSignal::ToolEnded { call_id, ok, .. } => {
                let Some(id) = parts.state.interactions.open_by_native(run, call_id) else {
                    continue;
                };
                let answered = parts
                    .state
                    .interactions
                    .get(id)
                    .is_some_and(|i| i.kind == InteractionKind::Question);
                let resolution = match (ok, answered) {
                    (true, true) => Resolution::Answered,
                    (true, false) => Resolution::Allowed,
                    (false, _) => Resolution::Unknown,
                };
                resolved(parts, run, call_id, &resolution, at);
            }
            _ => {}
        }
    }
    opened
}

/// The channels that carry an answer for a run. The native one needs the hook to be
/// held and a capability for the kind. The keystroke one needs `keystroke` (a bound run whose
/// capabilities let keys answer the kind) and a hook that the core does not hold, so that the
/// harness shows its own dialog.
fn channels_of(
    caps: &nh_proto::api::model::Capabilities,
    kind: InteractionKind,
    mode: GateMode,
    keystroke: bool,
) -> Vec<AnswerChannel> {
    let mut channels = Vec::new();
    if mode == GateMode::Gate && caps.gate && caps.answer_native.contains(&kind) {
        channels.push(AnswerChannel::Native);
    }
    // The harness shows its own dialog when the core does not hold the hook, and keys can answer
    // it.
    if mode == GateMode::Observe && keystroke {
        channels.push(AnswerChannel::Keystroke);
    }
    channels
}

/// The mode of a new interaction. The core holds the hook only when a shim waits, the
/// run is bound, the adapter and the capabilities allow it, and the setting asks for it: `gate`
/// always, `auto` when the pane is not focused, `observe` never.
fn mode_of(
    parts: &Parts<'_>,
    run: &crate::agents::runs::Run,
    declared: GateMode,
    held: bool,
    bound: bool,
) -> GateMode {
    let wanted = match parts.state.config.config().approvals.mode_for(run.harness) {
        ApprovalMode::Observe => false,
        ApprovalMode::Gate => true,
        ApprovalMode::Auto => !run
            .pane
            .is_some_and(|pane| parts.state.focus.is_focused(pane)),
    };
    if held && bound && wanted && declared == GateMode::Gate && run.capabilities.gate {
        GateMode::Gate
    } else {
        GateMode::Observe
    }
}

/// Opens an interaction for a native request of a run. A request that is open already is the same
/// interaction.
#[allow(clippy::too_many_arguments)]
fn open(
    parts: &mut Parts<'_>,
    run_id: RunId,
    kind: InteractionKind,
    native_ref: &str,
    declared: GateMode,
    rendered: Rendered,
    at: Timestamp,
    lease: Option<Lease>,
) -> Option<Opened> {
    if let Some(existing) = parts.state.interactions.open_by_native(run_id, native_ref) {
        // A shim that holds the hook again takes the lease of the interaction that exists, when
        // the core still holds hooks for it.
        let gate = parts
            .state
            .interactions
            .get(existing)
            .is_some_and(|i| i.mode == GateMode::Gate && i.state == InteractionState::Open);
        if let Some(lease) = lease
            && gate
        {
            hold(parts, existing, lease);
        }
        return Some(Opened {
            id: existing,
            held: lease.is_some() && gate,
        });
    }
    let run = parts.state.runs.by_id.get(&run_id)?;
    if !run.live() {
        return None;
    }
    // Identity check: a run that is only suggested cannot be answered.
    let bound = matches!(run.binding, Binding::Bound);
    let (harness, pane, caps) = (run.harness, run.pane, run.capabilities.clone());
    let mut rendered = rendered;
    // Policy: a rule that fits decides at once, when the core holds the hook and can answer it.
    // Else the rule only annotates the card.
    let facts = rendered
        .subject
        .as_ref()
        .map(|s| fingerprint::facts_of(&subject_of(harness, s), rendered.risk, &s.argv));
    let verdict = policy_verdict(parts, harness, rendered.subject.as_ref());
    let configured = parts.state.config.config().approvals.mode_for(harness);
    let can_decide = verdict.is_some()
        && kind == InteractionKind::Approval
        && lease.is_some()
        && bound
        && declared == GateMode::Gate
        && caps.gate
        && caps.answer_native.contains(&kind)
        && configured != ApprovalMode::Observe;
    if let Some(found) = verdict.as_ref().filter(|_| !can_decide)
        && let InteractionPayload::Approval(payload) = &mut rendered.payload
    {
        payload.policy_note = Some(found.note());
    }
    let run = parts.state.runs.by_id.get(&run_id)?;
    let mode = if can_decide {
        GateMode::Gate
    } else {
        mode_of(parts, run, declared, lease.is_some(), bound)
    };
    // Keys answer a dialog of a bound run whose screen manifest validates.
    let keystroke = bound
        && caps.answer_keystroke
        && matches!(
            kind,
            InteractionKind::Approval | InteractionKind::Question | InteractionKind::PlanReview
        );
    let channels = if bound {
        channels_of(&caps, kind, mode, keystroke)
    } else {
        Vec::new()
    };
    let gate_timeout = parts
        .state
        .gates
        .as_ref()
        .and_then(|gates| gates.timeout())
        .unwrap_or_else(|| parts.state.config.config().approvals.gate_timeout.0);
    let deadline = (mode == GateMode::Gate)
        .then(|| Timestamp(at.0 + i64::try_from(gate_timeout.as_millis()).unwrap_or(i64::MAX / 2)));
    let spec = OpenSpec {
        kind,
        run: Some(run_id),
        pane,
        native_ref: Some(native_ref.to_owned()),
        title: rendered.title,
        risk: rendered.risk,
        payload: rendered.payload,
        mode,
        answerable: bound && !channels.is_empty(),
        channels,
        source: Source::Hook,
        confidence: 1.0,
        deadline,
        facts,
        by_policy: can_decide,
        keystroke,
    };
    let id = new_interaction_id().ok()?;
    let stored = parts.commit(|tx| {
        let number = tx.next_handle("interaction")?;
        let (mut interaction, step) = Interaction::open(id, format!("i{number}"), spec, at);
        // A hook that the core does not hold is not recorded: the shim got "no decision".
        if let Some(lease) = lease
            && mode == GateMode::Gate
        {
            interaction.hold(lease);
        }
        rows::write(tx, &interaction, &step.events)?;
        Ok(interaction)
    });
    let interaction = match stored {
        Ok(interaction) => interaction,
        Err(error) => {
            tracing::warn!(run = %run_id, %error, "an interaction is not stored");
            return None;
        }
    };
    let (mode, deadline) = (interaction.mode, interaction.deadline);
    let evicted = parts.state.interactions.put(interaction);
    if let Some(interaction) = parts.state.interactions.get(id) {
        project::publish(
            parts.projections,
            &parts.state.interactions,
            interaction,
            &evicted,
        );
    }
    // A rule decides at once, and that needs no notification.
    if !can_decide {
        crate::native::changed(parts.state, id, true);
    }
    runs::input(
        parts,
        run_id,
        Input::new(
            ArbiterSource::Structured,
            at,
            Observation::Opened { id, kind },
        ),
    );
    runs::publish(parts, run_id);
    if mode == GateMode::Gate
        && let (Some(gates), Some(deadline)) = (parts.state.gates.clone(), deadline)
    {
        gates.arm_deadline(id, deadline);
    }
    if can_decide && let Some(found) = verdict {
        decide_by_policy(parts, id, &found);
    }
    Some(Opened {
        id,
        held: mode == GateMode::Gate && lease.is_some(),
    })
}

/// A rule that fits a request, with what the decision needs.
struct Found {
    id: String,
    decision: PolicyDecision,
    reason: Option<String>,
}

impl Found {
    /// The note for a card whose request the core cannot answer.
    fn note(&self) -> String {
        let verb = match self.decision {
            PolicyDecision::Allow => "allow",
            PolicyDecision::Deny => "deny",
        };
        match &self.reason {
            Some(reason) => format!("The rule `{}` would {verb} this: {reason}", self.id),
            None => format!("The rule `{}` would {verb} this.", self.id),
        }
    }
}

fn subject_of(harness: nh_proto::api::common::Harness, s: &Subject) -> policy::Subject {
    policy::Subject {
        harness,
        tool: s.tool.clone(),
        command: s.command.clone(),
        paths: s.paths.clone(),
        workspace: s.workspace.clone(),
    }
}

/// What the rules say about a request.
fn policy_verdict(
    parts: &mut Parts<'_>,
    harness: nh_proto::api::common::Harness,
    subject: Option<&Subject>,
) -> Option<Found> {
    let subject = subject_of(harness, subject?);
    let config = parts.state.config.config();
    let home = policy::calls::home();
    let rules = parts.state.policy.rules(&config, home.as_deref());
    let rule = policy::matcher::evaluate(rules, &subject).rule()?;
    Some(Found {
        id: rule.id.clone(),
        decision: rule.decision,
        reason: rule.reason.clone(),
    })
}

/// A rule decides: the decision is recorded for `policy` and goes through delivery like any
/// other. The answered approval gets its fingerprint.
fn decide_by_policy(parts: &mut Parts<'_>, id: InteractionId, found: &Found) {
    let allow = found.decision == PolicyDecision::Allow;
    let decision = Decision::Approval(ApprovalDecision {
        behavior: if allow {
            Behavior::Allow
        } else {
            Behavior::Deny
        },
        always: false,
        persist: false,
        message: (!allow).then(|| match &found.reason {
            Some(reason) => format!("Denied by the policy rule {}: {reason}", found.id),
            None => format!("Denied by the policy rule {}", found.id),
        }),
        stop: false,
    });
    let decided = transition(parts, id, |interaction| {
        match interaction.answer(decision, "policy", Via::Policy) {
            Ok(step) => (true, step),
            Err(_) => (false, Step::default()),
        }
    });
    if decided == Some(true) {
        record_fingerprint(parts, id);
    }
}

/// Stores the fingerprint of an answered approval.
pub(crate) fn record_fingerprint(parts: &mut Parts<'_>, id: InteractionId) {
    let Some(row) = parts
        .state
        .interactions
        .get(id)
        .and_then(|i| fingerprint_row(i, Timestamp(now_ms())))
    else {
        return;
    };
    if let Err(error) = parts.commit(|tx| tx.put_fingerprint(&row)) {
        tracing::warn!(interaction = %id, %error, "a fingerprint is not stored");
    }
}

/// The row for the fingerprint of an approval that has a decision.
pub(crate) fn fingerprint_row(
    interaction: &Interaction,
    at: Timestamp,
) -> Option<nh_store::FingerprintRow> {
    let facts = interaction.facts.as_ref()?;
    let Some(Decision::Approval(decision)) = &interaction.delivery.decision else {
        return None;
    };
    Some(nh_store::FingerprintRow {
        fingerprint: facts.fingerprint.clone(),
        decision: match decision.behavior {
            Behavior::Allow => "allow",
            Behavior::Deny => "deny",
        }
        .to_owned(),
        interaction: Some(interaction.id.ulid()),
        at: at.0,
        harness: Some(
            match facts.harness {
                nh_proto::api::common::Harness::Claude => "claude",
                nh_proto::api::common::Harness::Codex => "codex",
            }
            .to_owned(),
        ),
        tool: Some(facts.tool.clone()),
        pattern: facts.pattern.clone(),
        workspace: Some(facts.workspace.clone()),
        eligible: facts.eligible,
    })
}

/// A shim holds the hook of an interaction (again).
pub(crate) fn hold(parts: &mut Parts<'_>, id: InteractionId, lease: Lease) {
    transition(parts, id, |interaction| {
        interaction.hold(lease);
        ((), Step::default())
    });
}

/// The harness reports the end of a request (`InteractionResolved`).
fn resolved(
    parts: &mut Parts<'_>,
    run: RunId,
    native_ref: &str,
    resolution: &Resolution,
    at: Timestamp,
) {
    let Some(id) = parts.state.interactions.open_by_native(run, native_ref) else {
        return;
    };
    transition(parts, id, |interaction| {
        let step = interaction.native_resolved(resolution, at);
        ((), step)
    });
}

/// The arbiter closes interactions without an answer.
pub(crate) fn cancel(parts: &mut Parts<'_>, ids: &[InteractionId], reason: &str) {
    let at = Timestamp(now_ms());
    for id in ids {
        transition(parts, *id, |interaction| {
            let step = interaction.cancel(reason, at);
            ((), step)
        });
    }
}

/// The deadline of an interaction passed.
pub(crate) fn deadline(parts: &mut Parts<'_>, id: InteractionId) {
    let at = Timestamp(now_ms());
    transition(parts, id, |interaction| {
        let step = interaction.deadline_passed(at);
        ((), step)
    });
}

/// The decision of an interaction went to its holder (`delivery_started`).
pub(crate) fn sent(parts: &mut Parts<'_>, id: InteractionId) {
    let at = Timestamp(now_ms());
    transition(parts, id, |interaction| {
        let step = interaction.sent(at);
        ((), step)
    });
}

/// The shim printed the decision (`adapter.delivery_ack`).
pub(crate) fn acked(
    parts: &mut Parts<'_>,
    id: InteractionId,
    key: &str,
    applied: bool,
) -> Option<bool> {
    let at = Timestamp(now_ms());
    transition(parts, id, |interaction| {
        (interaction.acked(key, applied, at), Step::default())
    })
}

/// The hook of an interaction lost its connection. A shim that comes back in time resumes it.
pub(crate) fn lease_lost(parts: &mut Parts<'_>, id: InteractionId, conn: u64) {
    let at = Timestamp(now_ms());
    transition(parts, id, |interaction| {
        if interaction.delivery.lease.map(|l| l.conn) != Some(conn) {
            return ((), Step::default());
        }
        let step = interaction.lease_lost(at);
        ((), step)
    });
    let waiting =
        parts.state.interactions.get(id).is_some_and(|i| {
            !i.is_final() && i.mode == GateMode::Gate && i.delivery.lease.is_none()
        });
    if waiting && let Some(gates) = parts.state.gates.clone() {
        gates.arm_grace(id);
    }
}

/// The connection of a shim ended: the interactions that it held lose their lease.
pub(crate) fn connection_closed(parts: &mut Parts<'_>, conn: u64) {
    let held: Vec<InteractionId> = parts
        .state
        .interactions
        .open()
        .into_iter()
        .filter(|i| i.delivery.lease.map(|l| l.conn) == Some(conn))
        .map(|i| i.id)
        .collect();
    for id in held {
        lease_lost(parts, id, conn);
    }
}

/// The shim did not come back in time. A hook that nobody holds cannot carry a decision.
pub(crate) fn grace_over(parts: &mut Parts<'_>, id: InteractionId) {
    let at = Timestamp(now_ms());
    let Some(state) = parts
        .state
        .interactions
        .get(id)
        .filter(|i| i.delivery.lease.is_none())
        .map(|i| i.state)
    else {
        return;
    };
    match state {
        InteractionState::Open => {
            transition(parts, id, |interaction| {
                let step = interaction.release_to_observe(at);
                ((), step)
            });
        }
        InteractionState::DecisionRecorded => failed(parts, id, "shim_gone"),
        _ => {}
    }
}

/// The pane got the focus while a shim holds the hook: the shim prints nothing, the harness
/// shows its own dialog, and the interaction goes on in `observe` mode.
pub(crate) fn release(parts: &mut Parts<'_>, id: InteractionId) {
    let at = Timestamp(now_ms());
    transition(parts, id, |interaction| {
        let step = interaction.release_to_observe(at);
        ((), step)
    });
}

/// Sends the decision of an interaction to the shim that holds its hook.
pub(crate) fn deliver(parts: &mut Parts<'_>, id: InteractionId) {
    let Some(interaction) = parts.state.interactions.get(id).cloned() else {
        return;
    };
    if !matches!(
        interaction.state,
        InteractionState::DecisionRecorded
            | InteractionState::Delivering
            | InteractionState::DeliveryUnknown
    ) {
        return;
    }
    if interaction.delivery.channel == Some(AnswerChannel::Keystroke) {
        deliver_keystrokes(parts, id, &interaction);
        return;
    }
    let (Some(key), Some(lease)) = (interaction.delivery.key.clone(), interaction.delivery.lease)
    else {
        // Nobody holds the hook. The decision waits for a shim that resumes, or for the deadline.
        return;
    };
    let Some(harness) = interaction
        .run
        .and_then(|run| parts.state.runs.by_id.get(&run))
        .map(|run| run.harness)
    else {
        failed(parts, id, "run_gone");
        return;
    };
    let persist = parts.state.config.config().approvals.claude.persist_always;
    let Some(output) = hook_output(harness, &interaction, persist) else {
        failed(parts, id, "no_native_output");
        return;
    };
    let Some(gates) = parts.state.gates.clone() else {
        return;
    };
    let outcome = GateOutcome::Decision {
        interaction: id,
        output,
        key,
    };
    if gates.send(lease.conn, outcome) {
        sent(parts, id);
    } else {
        lease_lost(parts, id, lease.conn);
    }
}

/// Types the decision into the pane of the run. The sequence runs on its own task. Its
/// end comes back as [`keystrokes_ended`].
fn deliver_keystrokes(parts: &mut Parts<'_>, id: InteractionId, interaction: &Interaction) {
    if interaction.state != InteractionState::DecisionRecorded {
        // A sequence is on its way, or the delivery ended.
        return;
    }
    let request = match crate::keystrokes::request_for(interaction) {
        Ok(request) => request,
        Err(reason) => return failed(parts, id, reason),
    };
    // The harness shows one dialog at a time. While the run has another open request of the same
    // kind, the dialog on the screen can be the one of the other request: no key goes in, and the
    // person answers in the harness.
    let several = interaction.run.is_some_and(|run| {
        parts
            .state
            .interactions
            .open_of_run(run)
            .iter()
            .any(|other| other.id != id && other.kind == interaction.kind)
    });
    if several {
        return failed(parts, id, "several_requests_open");
    }
    let target = interaction
        .run
        .and_then(|run| parts.state.runs.by_id.get(&run))
        .and_then(|run| {
            let pane = run.pane?;
            let loaded = runs::loaded_manifests(parts);
            let screen = runs::manifest_of(&loaded, run)?.screen.as_ref()?;
            Some((pane, screen.manifest.clone()))
        });
    let (Some((pane, manifest)), Some(service)) = (target, parts.state.keystrokes.clone()) else {
        return failed(parts, id, "keystrokes_unavailable");
    };
    sent(parts, id);
    service.start(id, pane.ulid(), manifest, request);
}

/// The sequence is about to send the key that can commit the answer. The state knows it before
/// the program does, so that the hook that follows confirms the delivery.
pub(crate) fn keystrokes_committed(parts: &mut Parts<'_>, id: InteractionId) {
    let key = parts
        .state
        .interactions
        .get(id)
        .and_then(|i| i.delivery.key.clone());
    if let Some(key) = key {
        acked(parts, id, &key, true);
    }
}

/// The sequence of keys ended. A failure is before the last key: nothing went in. A dialog that
/// stays after the last key leaves the answer unknown. A dialog that went waits for the hook or the
/// screen to confirm it.
pub(crate) fn keystrokes_ended(
    parts: &mut Parts<'_>,
    id: InteractionId,
    end: Result<nh_agents::keystroke::Outcome, nh_agents::keystroke::Failure>,
) {
    match end {
        Ok(nh_agents::keystroke::Outcome::DialogGone) => {}
        Ok(nh_agents::keystroke::Outcome::DialogStays) => keystrokes_unconfirmed(parts, id),
        Err(failure) => failed(parts, id, failure.reason()),
    }
}

/// No hook and no screen confirmed the answer in time: it stays unknown.
pub(crate) fn keystrokes_unconfirmed(parts: &mut Parts<'_>, id: InteractionId) {
    let at = Timestamp(now_ms());
    transition(parts, id, |interaction| {
        let step = interaction.unconfirmed(at);
        ((), step)
    });
}

/// Whether the interaction exists and belongs to the pane: the pane of the token of a shim.
pub(crate) fn belongs_to(parts: &Parts<'_>, id: InteractionId, pane: PaneId) -> bool {
    parts
        .state
        .interactions
        .get(id)
        .is_some_and(|interaction| interaction.pane == Some(pane))
}

/// `adapter.gate_resume`: the reply for a shim that holds the hook again. The shim must be of the
/// pane of the interaction: for another pane the interaction does not exist (`None`).
pub(crate) fn resume(
    parts: &mut Parts<'_>,
    id: InteractionId,
    pane: PaneId,
    lease: Lease,
) -> Option<GateReply> {
    if !belongs_to(parts, id, pane) {
        return None;
    }
    let interaction = parts.state.interactions.get(id)?;
    if interaction.is_final() {
        return Some(GateReply::NoDecision(final_reason(interaction.state)));
    }
    if interaction.mode != GateMode::Gate {
        return Some(GateReply::NoDecision("observe"));
    }
    resumed(parts, id, lease);
    Some(GateReply::Wait { interaction: id })
}

fn final_reason(state: InteractionState) -> &'static str {
    match state {
        InteractionState::Delivered => "delivered",
        InteractionState::Failed => "failed",
        InteractionState::ResolvedElsewhere => "resolved_elsewhere",
        InteractionState::Dismissed => "dismissed",
        InteractionState::Cancelled => "cancelled",
        _ => "ended",
    }
}

/// The decision cannot be delivered.
pub(crate) fn failed(parts: &mut Parts<'_>, id: InteractionId, reason: &str) {
    let at = Timestamp(now_ms());
    transition(parts, id, |interaction| {
        let step = interaction.failed(reason, at);
        ((), step)
    });
}

/// A shim holds the hook again after its connection ended (`adapter.gate_resume`).
fn resumed(parts: &mut Parts<'_>, id: InteractionId, lease: Lease) {
    let at = Timestamp(now_ms());
    transition(parts, id, |interaction| {
        let step = interaction.resumed(lease, at);
        ((), step)
    });
}

/// Takes the interaction, makes one change, and stores and publishes the result. Gives the value
/// of the change, or `None` when the interaction does not exist.
pub(crate) fn transition<R>(
    parts: &mut Parts<'_>,
    id: InteractionId,
    change: impl FnOnce(&mut Interaction) -> (R, Step),
) -> Option<R> {
    let before = parts.state.interactions.get(id)?.clone();
    let mut interaction = before.clone();
    let (result, step) = change(&mut interaction);
    if interaction == before && step.is_empty() {
        return Some(result);
    }
    let stored = parts.commit(|tx| rows::write(tx, &interaction, &step.events));
    if let Err(error) = stored {
        // The processes do not wait for the disk: the change stays in memory.
        tracing::warn!(interaction = %id, %error, "an interaction is not stored");
    }
    finish(parts, &before, interaction, step);
    Some(result)
}

/// Puts a changed interaction in the table and the projections, tells the arbiter that it ended,
/// and runs the effects.
pub(crate) fn finish(
    parts: &mut Parts<'_>,
    before: &Interaction,
    interaction: Interaction,
    step: Step,
) {
    let run = interaction.run;
    let closing = closes_for_arbiter(before, &interaction);
    let id = interaction.id;
    crate::agents::drift::interaction_changed(parts, before, &interaction);
    in_state(parts.state, parts.projections, interaction, &step);
    if let Some(run) = run {
        if closing {
            close_for_arbiter(parts, run, id);
        }
        runs::publish(parts, run);
    }
}

/// An interaction that ended by an answer or by the harness is closed for the arbiter. One that
/// the arbiter cancelled is closed there already.
fn closes_for_arbiter(before: &Interaction, after: &Interaction) -> bool {
    after.is_final() && !before.is_final() && after.state != InteractionState::Cancelled
}

fn close_for_arbiter(parts: &mut Parts<'_>, run: RunId, id: InteractionId) {
    runs::input(
        parts,
        run,
        Input::new(
            ArbiterSource::Structured,
            Timestamp(now_ms()),
            Observation::Closed { id },
        ),
    );
}

/// The screen no longer shows the dialog of a provisional interaction.
pub(crate) fn arbiter_closed_by_screen(
    parts: &mut Parts<'_>,
    run: RunId,
    id: InteractionId,
    at: Timestamp,
) {
    runs::input(
        parts,
        run,
        Input::new(ArbiterSource::Screen, at, Observation::Closed { id }),
    );
    runs::publish(parts, run);
}

/// The arbiter learns that an interaction ended, from a command.
pub(crate) fn arbiter_closed(parts: &mut Parts<'_>, run: RunId, id: InteractionId) {
    close_for_arbiter(parts, run, id);
    runs::publish(parts, run);
}

/// The part of a change that needs no store: the table, the projections, `attention` and the
/// effects. A command runs it in its mutation, and the other changes run it after their commit.
pub(crate) fn in_state(
    state: &mut crate::actor::State,
    hub: &crate::projection::ProjectionHub,
    interaction: Interaction,
    step: &Step,
) {
    let id = interaction.id;
    let run = interaction.run;
    let snapshot = interaction.clone();
    let new = state.interactions.get(id).is_none();
    let evicted = state.interactions.put(interaction);
    if let Some(interaction) = state.interactions.get(id) {
        project::publish(hub, &state.interactions, interaction, &evicted);
    }
    if let Some(run) = run {
        runs::publish_attention(state, hub, run);
    }
    crate::native::changed(state, id, new);
    if let Some(gates) = state.gates.clone() {
        for effect in &step.effects {
            gates.perform(&snapshot, effect);
        }
    }
}

/// A notice of the core that belongs to no run: it tells the person something, and "Dismiss"
/// closes it. It has no channel, so no client answers it.
pub(crate) fn open_notice(parts: &mut Parts<'_>, title: &str, text: &str) -> Option<InteractionId> {
    let at = Timestamp(now_ms());
    let spec = OpenSpec {
        kind: InteractionKind::Notice,
        run: None,
        pane: None,
        native_ref: None,
        title: title.to_owned(),
        risk: nh_proto::api::common::Risk::Low,
        payload: InteractionPayload::Notice(nh_proto::api::model::NoticePayload {
            text: text.to_owned(),
            resets_at: None,
        }),
        mode: GateMode::Observe,
        answerable: false,
        channels: Vec::new(),
        source: Source::Core,
        confidence: 1.0,
        deadline: None,
        facts: None,
        by_policy: false,
        keystroke: false,
    };
    let id = new_interaction_id().ok()?;
    let stored = parts.commit(|tx| {
        let number = tx.next_handle("interaction")?;
        let (interaction, step) = Interaction::open(id, format!("i{number}"), spec, at);
        rows::write(tx, &interaction, &step.events)?;
        Ok(interaction)
    });
    let interaction = match stored {
        Ok(interaction) => interaction,
        Err(error) => {
            tracing::warn!(%error, "a notice is not stored");
            return None;
        }
    };
    let evicted = parts.state.interactions.put(interaction);
    if let Some(interaction) = parts.state.interactions.get(id) {
        project::publish(
            parts.projections,
            &parts.state.interactions,
            interaction,
            &evicted,
        );
    }
    crate::native::changed(parts.state, id, true);
    Some(id)
}

/// The core closes an interaction that it opened itself.
pub(crate) fn dismiss_by_core(parts: &mut Parts<'_>, id: InteractionId) {
    transition(parts, id, |interaction| {
        let step = interaction.dismiss(Timestamp(now_ms())).unwrap_or_default();
        ((), step)
    });
}
