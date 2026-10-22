//! The end of a turn that nobody saw.
//!
//! When the execution of a run goes from `working` to `idle` and stays `idle` for the settle time
//! ([`SETTLE`] in the product), and the pane of the run does not have the focus of the user at the
//! end of it, the run waits for the person: `attention` gives it the reason `turn_ended`, the badge
//! counts it, and the run gets one notification. The settle time absorbs a screen reading that
//! changes back and forth.
//!
//! The reason ends when the pane gets the focus, when an input reaches the pane, after
//! `agent.reply`, when the execution changes again, and when the run ends. The state lives in
//! memory only: a new core starts with no run in it, because the core cannot tell what the person
//! saw while it was down.

use std::{
    collections::HashSet,
    sync::{Mutex, PoisonError},
    time::Duration,
};

use nh_proto::api::{
    ids::{PaneId, RunId},
    model::Execution,
    time::Timestamp,
};

use super::runs::{self, Run};
use crate::actor::Parts;

/// How long the execution of a run stays `idle` after `working` before the turn counts as ended.
pub const SETTLE: Duration = Duration::from_secs(3);

/// The panes whose run waits after a turn. The input path of the panes shares the set with the
/// actor: an input to one of these panes tells the actor, and an input to any other pane costs one
/// look in the set.
#[derive(Debug, Default)]
pub struct TurnWaits(Mutex<HashSet<PaneId>>);

impl TurnWaits {
    /// Whether the run of the pane waits after a turn.
    pub fn contains(&self, pane: PaneId) -> bool {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .contains(&pane)
    }

    fn set(&self, pane: PaneId, waits: bool) {
        let mut set = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        if waits {
            set.insert(pane);
        } else {
            set.remove(&pane);
        }
    }
}

/// What a run keeps about its turns.
#[derive(Clone, Debug, Default)]
pub(crate) struct Turn {
    /// Grows with each change of the execution, so that the settle of an older change does
    /// nothing.
    seq: u64,
    /// The turn ended at this time, and nobody saw it.
    pub ended: Option<Timestamp>,
}

/// What a change of the execution asks for, after the run is recorded.
#[derive(Debug, Default)]
#[must_use]
pub(crate) struct Note {
    /// The settle to arm: the number of the change and the time of the turn end.
    arm: Option<(u64, Timestamp)>,
    /// The run waited, and the change ends it.
    ended: bool,
}

/// The execution of the run changed. `from_working` is true when it was `working` before the
/// change, or in the middle of the signals that made it.
pub(crate) fn note(run: &mut Run, from_working: bool, at: Timestamp) -> Note {
    run.turn.seq += 1;
    let ended = run.turn.ended.take().is_some();
    let arm = (from_working && run.live() && run.arbiter.execution().value == Execution::Idle)
        .then_some((run.turn.seq, at));
    Note { arm, ended }
}

/// Acts on a [`Note`]: the surfaces hear of an end, and the settle goes on the clock.
pub(crate) fn follow(parts: &mut Parts<'_>, id: RunId, note: Note) {
    if note.ended {
        changed(parts, id);
    }
    let Some((seq, at)) = note.arm else {
        return;
    };
    let delay = parts.state.native.turn_settle();
    let armed = parts
        .state
        .native
        .after(delay, move |mut parts| settle(&mut parts, id, seq, at));
    // A state without a clock (a unit test of the actor) settles at once.
    if !armed {
        settle(parts, id, seq, at);
    }
}

/// The settle time of a turn end is over. The run waits when nothing changed since, and its pane
/// does not have the focus of the user.
pub(crate) fn settle(parts: &mut Parts<'_>, id: RunId, seq: u64, at: Timestamp) {
    let Some(run) = parts.state.runs.by_id.get_mut(&id) else {
        return;
    };
    if run.turn.seq != seq || !run.live() || run.arbiter.execution().value != Execution::Idle {
        return;
    }
    let Some(pane) = run.pane else {
        return;
    };
    if parts.state.focus.is_focused(pane) {
        return;
    }
    run.turn.ended = Some(at);
    changed(parts, id);
}

/// The pane got the focus of the user, or an input reached it: its run does not wait any more.
pub(crate) fn seen(parts: &mut Parts<'_>, pane: PaneId) {
    let Some(id) = parts.state.runs.of_pane(pane).map(|run| run.id) else {
        parts.state.turn_waits.set(pane, false);
        return;
    };
    let Some(run) = parts.state.runs.by_id.get_mut(&id) else {
        return;
    };
    if run.turn.ended.take().is_some() {
        changed(parts, id);
    } else {
        parts.state.turn_waits.set(pane, false);
    }
}

/// The reason of the run started or ended: the set of the input path, `attention`, the badge and
/// the notification follow.
fn changed(parts: &mut Parts<'_>, id: RunId) {
    let Some(run) = parts.state.runs.by_id.get(&id) else {
        return;
    };
    let waits = run.turn.ended.is_some();
    if let Some(pane) = run.pane {
        parts.state.turn_waits.set(pane, waits);
    }
    runs::publish(parts, id);
    crate::native::run_changed(parts.state, id);
}

#[cfg(test)]
mod tests;
