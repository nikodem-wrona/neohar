//! The task that reads the transcripts.
//!
//! Once a second it asks the actor which runs have a transcript. For each it reads the new
//! lines in a blocking task and gives the result back to the actor. The poll costs one `stat` for
//! a file that did not change.

use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    time::Duration,
};

use nh_agents::transcript::Billing;
use nh_proto::api::{common::Harness, ids::RunId, projections::Projection, time::Timestamp};
use nh_store::TranscriptState;
use tokio::time::{MissedTickBehavior, interval};

use super::{apply::apply, prices, tail::Tail};
use crate::{
    actor::{ActorHandle, Parts},
    config::Billing as ConfigBilling,
    persist::now_ms,
};

/// The time between two polls.
#[cfg(not(test))]
const POLL: Duration = Duration::from_secs(1);
#[cfg(test)]
const POLL: Duration = Duration::from_millis(40);

/// What the service needs to know about a run.
#[derive(Debug)]
pub(super) struct Target {
    pub run: RunId,
    pub harness: Harness,
    pub path: PathBuf,
    pub ended: bool,
    pub billing: Billing,
    pub wants_timeline: bool,
    /// The saved state, for a run that the service has not seen yet.
    pub saved: Option<TranscriptState>,
}

fn billing(config: ConfigBilling) -> Billing {
    match config {
        ConfigBilling::Api => Billing::Api,
        ConfigBilling::Subscription => Billing::Subscription,
    }
}

/// The runs that have a transcript. Runs of `known` have a tail already, so they need no state.
pub(super) fn targets(parts: &mut Parts<'_>, known: &HashSet<RunId>) -> Vec<Target> {
    let config = parts.state.config.config();
    let mut found: Vec<Target> = parts
        .state
        .runs
        .by_id
        .values()
        .filter_map(|run| {
            Some(Target {
                run: run.id,
                harness: run.harness,
                path: PathBuf::from(run.transcript_path.as_ref()?),
                ended: !run.live(),
                billing: billing(config.agents.billing_for(run.harness)),
                wants_timeline: parts
                    .projections
                    .subscriber_count(Projection::Timeline(run.id))
                    > 0,
                saved: None,
            })
        })
        .collect();
    for target in found.iter_mut().filter(|t| !known.contains(&t.run)) {
        let ulid = target.run.ulid();
        target.saved = parts.commit(|tx| tx.transcript_state(ulid)).ok().flatten();
    }
    found
}

fn new_tail(target: &Target, ledger: Option<nh_agents::transcript::Ledger>) -> Tail {
    let (offset, saved_ledger) = match &target.saved {
        Some(saved) => (saved.offset, saved.ledger_json.as_deref()),
        None => (0, None),
    };
    // A file that is new for the run starts at its beginning, and keeps the count.
    let (offset, ledger) = match ledger {
        Some(ledger) => (0, ledger),
        None => (offset, Tail::ledger_from(saved_ledger)),
    };
    let mut tail = Tail::new(
        target.harness,
        target.path.clone(),
        offset,
        ledger,
        prices(),
    );
    tail.set_billing(target.billing);
    tail
}

/// Reads the transcripts until the core ends.
pub(crate) async fn run(actor: ActorHandle) {
    let mut tails: HashMap<RunId, Tail> = HashMap::new();
    let mut tick = interval(POLL);
    tick.set_missed_tick_behavior(MissedTickBehavior::Delay);
    loop {
        tick.tick().await;
        let known: HashSet<RunId> = tails.keys().copied().collect();
        let Ok(found) = actor
            .update(move |mut parts| targets(&mut parts, &known))
            .await
        else {
            return;
        };
        tails.retain(|id, _| found.iter().any(|target| target.run == *id));
        for target in found {
            let mut tail = match tails.remove(&target.run) {
                Some(tail) if tail.path == target.path => tail,
                // The run moved to another file (a resumed session): the count goes on.
                Some(old) => new_tail(&target, Some(old.into_ledger())),
                None => new_tail(&target, None),
            };
            if tail.finished {
                tails.insert(target.run, tail);
                continue;
            }
            tail.set_billing(target.billing);
            let wants = target.wants_timeline;
            let polled = tokio::task::spawn_blocking(move || {
                let batch = tail.poll(wants, Timestamp(now_ms()));
                (tail, batch)
            })
            .await;
            let Ok((mut tail, batch)) = polled else {
                continue;
            };
            // A run that ended and has nothing more to read is done.
            if target.ended && batch.is_none() {
                tail.finished = true;
            }
            let run = target.run;
            tails.insert(run, tail);
            if let Some(batch) = batch {
                let _ = actor
                    .update(move |mut parts| apply(&mut parts, run, batch))
                    .await;
            }
        }
    }
}
