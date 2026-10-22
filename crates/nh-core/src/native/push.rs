//! When a phone gets a push.
//!
//! The rule: a run that comes into `attention` gets one push for each paired
//! phone with a subscription, when no window of a desktop client has the focus. The facts are
//! those of the focus after its debounce ([`crate::focus::Focus::desk_focused`]). These rules come
//! with it:
//!
//! - A row that came while a window had the focus gets no push when the window loses it. A lock of
//!   the screen, or a display that sleeps, is the exception: each open row that had no push gets
//!   one ([`away`]).
//! - A new reason of a row is a new chance for a push, for example a question after an error. A
//!   second push of the same run within the coalesce window does not go.
//! - A row that ends before its push gets none: the push goes from a later step of the actor,
//!   which looks at the row again.
//! - A phone whose app is open and visible gets none.
//! - The text is the harness and the work of the run, and the reason in a few words
//!   ([`PushMessage::notice`]). No text of the agent goes into a push.
//!
//! The rule runs on the actor after each change of `attention`, as a diff, as the badge does. The
//! sender is async and lives in `nh-web`: the actor hands it the jobs, and the results come back as
//! the events `phone.pushed` and `phone.push_failed`.
//!
//! A notice of another machine comes from the host, which gets it over the link
//! ([`machine_notice`]). It follows the same rule: no push while a desktop window has the focus or
//! the app of the phone is visible, and one push of each notice in the coalesce window.

use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
    time::Duration,
};

use nh_proto::api::{
    events::{EventPayload, PhonePushFailed, PhonePushed},
    ids::{DeviceId, RunId},
    model::AttentionReason,
};
use ulid::Ulid;

use crate::{
    actor::{Parts, State},
    agents::project::attention_value,
    host::HostNotification,
    persist::now_ms,
    phones::{PushJob, PushMessage, PushOutcome, PushSlot, PushUrgency, drop_gone, reason_urgency},
};

/// What the rule knows of one row of `attention`.
#[derive(Clone, Debug, Default)]
struct Row {
    /// The reasons that the rule looked at already: pushed, seen in the app, or coalesced.
    handled: HashSet<AttentionReason>,
    /// The row had a push.
    pushed: bool,
}

/// The state of the rule.
#[derive(Clone, Default)]
pub(crate) struct Pushes {
    rows: HashMap<RunId, Row>,
    /// When the last push of each run went, in unix milliseconds: the coalesce window.
    last: HashMap<RunId, i64>,
    /// When the last push of each notice of another machine went, by its tag.
    machine_last: HashMap<String, i64>,
    /// When the push service took the last push, in unix milliseconds: for `phone.keep_awake`.
    delivered: Option<i64>,
    /// The sender of `nh-web`. A state without one decides and sends nothing.
    slot: Option<Arc<PushSlot>>,
}

impl Pushes {
    pub(crate) fn with_slot(slot: Arc<PushSlot>) -> Self {
        Self {
            slot: Some(slot),
            ..Self::default()
        }
    }

    /// How long ago the push service took the last push. `None` before the first one.
    pub(crate) fn since_delivered(&self) -> Option<Duration> {
        let at = self.delivered?;
        Some(Duration::from_millis(
            u64::try_from(now_ms() - at).unwrap_or(0),
        ))
    }
}

/// The order in which the reasons of a row choose the text of its push: what waits for an answer
/// first.
fn rank(reason: AttentionReason) -> u8 {
    match reason {
        AttentionReason::Approval => 0,
        AttentionReason::Question => 1,
        AttentionReason::PlanReview => 2,
        AttentionReason::Error => 3,
        AttentionReason::RateLimited => 4,
        AttentionReason::TurnEnded => 5,
    }
}

/// The reasons of each run in `attention` now.
fn rows_now(state: &State) -> HashMap<RunId, Vec<AttentionReason>> {
    state
        .runs
        .by_id
        .values()
        .filter_map(|run| attention_value(run, &state.interactions))
        .map(|value| (value.run, value.reasons))
        .collect()
}

/// The rows of the last core get no push: a core that starts takes them as handled. Call it
/// before the first publication of the runs.
pub(crate) fn adopt(state: &mut State) {
    let rows = rows_now(state);
    let pushes = &mut state.native.push;
    pushes.rows = rows
        .into_iter()
        .map(|(run, reasons)| {
            let row = Row {
                handled: reasons.into_iter().collect(),
                pushed: true,
            };
            (run, row)
        })
        .collect();
}

/// `attention` may have changed: a new row or a new reason of a row gets a push when no desktop
/// window has the focus. A row that ended is forgotten.
pub(crate) fn follow(state: &mut State) {
    if state.native.push.slot.is_none() {
        return;
    }
    let now = rows_now(state);
    let focused = state.focus.desk_focused();
    let window = coalesce_ms(state);
    let at = now_ms();
    let mut due: Vec<(RunId, AttentionReason)> = Vec::new();
    let pushes = &mut state.native.push;
    pushes.rows.retain(|run, _| now.contains_key(run));
    pushes.last.retain(|_, sent| at - *sent < window);
    for (run, reasons) in now {
        let row = pushes.rows.entry(run).or_default();
        let Some(top) = reasons
            .iter()
            .copied()
            .filter(|reason| !row.handled.contains(reason))
            .min_by_key(|reason| rank(*reason))
        else {
            continue;
        };
        row.handled.extend(reasons);
        if focused || pushes.last.contains_key(&run) {
            continue;
        }
        row.pushed = true;
        pushes.last.insert(run, at);
        due.push((run, top));
    }
    for (run, reason) in due {
        schedule(state, run, reason);
    }
}

/// The screen of the host locked, or its display slept, and no desktop window has the focus: each
/// open row that had no push gets one.
pub(crate) fn away(state: &mut State) {
    if state.native.push.slot.is_none() {
        return;
    }
    let now = rows_now(state);
    let window = coalesce_ms(state);
    let at = now_ms();
    let mut due: Vec<(RunId, AttentionReason)> = Vec::new();
    let pushes = &mut state.native.push;
    for (run, reasons) in now {
        let row = pushes.rows.entry(run).or_default();
        let recent = pushes
            .last
            .get(&run)
            .is_some_and(|sent| at - *sent < window);
        let Some(top) = reasons.iter().copied().min_by_key(|reason| rank(*reason)) else {
            continue;
        };
        row.handled.extend(reasons);
        if row.pushed || recent {
            continue;
        }
        row.pushed = true;
        pushes.last.insert(run, at);
        due.push((run, top));
    }
    for (run, reason) in due {
        schedule(state, run, reason);
    }
}

fn coalesce_ms(state: &State) -> i64 {
    i64::try_from(state.native.window().as_millis()).unwrap_or(i64::MAX)
}

/// The push goes from a later step of the actor, so that a row that ends in the same step gets
/// none. A state without a clock sends nothing.
fn schedule(state: &State, run: RunId, reason: AttentionReason) {
    state.native.after(Duration::ZERO, move |mut parts| {
        send(&mut parts, run, reason)
    });
}

/// Sends the push of a row to each phone that gets one, when the row still has the reason.
fn send(parts: &mut Parts<'_>, run: RunId, reason: AttentionReason) {
    let state = &*parts.state;
    let Some(out) = state.native.push.slot.as_ref().and_then(|slot| slot.get()) else {
        return;
    };
    let rows = rows_now(state);
    if !rows
        .get(&run)
        .is_some_and(|reasons| reasons.contains(&reason))
    {
        tracing::debug!(%run, ?reason, "the row ended before its push");
        return;
    }
    let Some(found) = state.runs.by_id.get(&run) else {
        return;
    };
    let badge = u32::try_from(rows.len()).unwrap_or(u32::MAX);
    let message = PushMessage::notice(
        super::harness_name(found.harness),
        Some(&super::work(state, found)),
        reason,
        run.ulid(),
        badge,
    );
    for (device, subscription) in state.phones.push_targets() {
        let endpoint = subscription.endpoint.clone();
        let job = PushJob {
            device,
            subscription,
            message: message.clone(),
            urgency: reason_urgency(reason),
            topic: Some(run.to_string()),
        };
        let out = Arc::clone(&out);
        state.native.run(
            async move { out.send(job).await },
            move |mut parts, outcome| {
                sent(&mut parts, device, Some((run, reason)), &endpoint, outcome);
            },
        );
    }
}

/// A notice of another machine: each phone with a subscription gets a push, when no
/// desktop window has the focus and the app of the phone is not visible. Only a notice of an
/// interaction or of a pane goes, because the title of another notice can be the text of a
/// program. A second push of one notice within the coalesce window does not go.
pub(crate) fn machine_notice(parts: &mut Parts<'_>, machine: &str, notice: &HostNotification) {
    let state = &mut *parts.state;
    let Some(out) = state.native.push.slot.as_ref().and_then(|slot| slot.get()) else {
        return;
    };
    let about = match (notice.interaction, notice.pane) {
        (Some(interaction), _) => interaction.to_string(),
        (None, Some(pane)) => pane.to_string(),
        (None, None) => return,
    };
    if state.focus.desk_focused() {
        return;
    }
    let window = coalesce_ms(state);
    let at = now_ms();
    let tag = format!("{machine}/{about}");
    let pushes = &mut state.native.push;
    pushes.machine_last.retain(|_, sent| at - *sent < window);
    if pushes.machine_last.contains_key(&tag) {
        return;
    }
    pushes.machine_last.insert(tag.clone(), at);
    let interaction = notice.interaction.is_some();
    let message = PushMessage::machine(machine, &notice.title, interaction, &tag);
    let urgency = if interaction {
        PushUrgency::High
    } else {
        PushUrgency::Normal
    };
    for (device, subscription) in state.phones.push_targets() {
        let endpoint = subscription.endpoint.clone();
        let job = PushJob {
            device,
            subscription,
            message: message.clone(),
            urgency,
            topic: None,
        };
        let out = Arc::clone(&out);
        state.native.run(
            async move { out.send(job).await },
            move |mut parts, outcome| sent(&mut parts, device, None, &endpoint, outcome),
        );
    }
}

/// The push service answered a push: the event, and the end of a subscription that it no longer
/// knows. A push of a run has its run and reason. A notice of another machine has none, and its
/// delivery has no event.
fn sent(
    parts: &mut Parts<'_>,
    device: Ulid,
    of_run: Option<(RunId, AttentionReason)>,
    endpoint: &str,
    outcome: PushOutcome,
) {
    let event = match &outcome {
        PushOutcome::Delivered => {
            parts.state.native.push.delivered = Some(now_ms());
            let Some((run, reason)) = of_run else {
                tracing::info!(%device, "the push of a notice of a machine went to the phone");
                return;
            };
            tracing::info!(%device, %run, ?reason, "a push went to the phone");
            EventPayload::PhonePushed(PhonePushed {
                device: DeviceId(device),
                run,
                reason,
            })
        }
        PushOutcome::Gone { status } => {
            tracing::info!(%device, status, "the push service no longer knows the phone");
            if let Err(error) = drop_gone(parts, device, endpoint) {
                tracing::warn!(%device, %error, "the subscription of the phone stays");
            }
            EventPayload::PhonePushFailed(PhonePushFailed {
                device: DeviceId(device),
                status: Some(*status),
            })
        }
        PushOutcome::Failed { status, reason } => {
            tracing::warn!(%device, ?status, reason, "a push did not go");
            EventPayload::PhonePushFailed(PhonePushFailed {
                device: DeviceId(device),
                status: *status,
            })
        }
    };
    if let Err(error) = parts.commit(|tx| tx.append(Some(&device.to_string()), &event)) {
        tracing::warn!(%device, %error, "the push is not in the log");
    }
}

#[cfg(test)]
mod tests;
