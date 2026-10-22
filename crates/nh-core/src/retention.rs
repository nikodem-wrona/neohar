//! The retention of the store: the event log and the scrollback archive keep only what the
//! configuration allows.
//!
//! When the core starts and then once an hour, one pass runs on the thread of the state actor:
//!
//! - **Events.** An event older than `events.sync_retention` goes. An event of an audit kind
//!   ([`nh_store::is_audit_kind`]) stays until it is older than `events.audit_retention`. The
//!   store remembers the highest deleted number, so a client whose cursor is below it gets
//!   `truncated` and takes a new snapshot.
//! - **Scrollback.** When `scrollback.max_bytes_per_pane` is above 0, the oldest segments of a pane
//!   go until its archive fits. The newest segment of a pane stays, because its writer appends to
//!   it.
//!
//! Each delete is one transaction of the store. The task reads the configuration once a minute,
//! so a reload counts: a change of these settings starts a new pass within a minute.

use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use nh_store::{Retention, Store, StoreError, Swept};
use tokio::time::{MissedTickBehavior, interval};

use crate::{
    actor::ActorHandle,
    config::{Config, ConfigStore},
    error::CoreError,
    persist::now_ms,
};

/// The time between two passes.
pub(crate) const PERIOD: Duration = Duration::from_secs(3600);
/// How often the task reads the settings.
const LOOK: Duration = Duration::from_secs(60);

/// The settings that a pass uses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Limits {
    pub retention: Retention,
    /// The most bytes of archive for one pane. 0 means no limit.
    pub max_bytes_per_pane: u64,
}

impl Limits {
    /// The limits of a configuration.
    pub fn of(config: &Config) -> Self {
        Self {
            retention: Retention {
                sync: config.events.sync_retention.0,
                audit: config.events.audit_retention.0,
            },
            max_bytes_per_pane: config.scrollback.max_bytes_per_pane,
        }
    }
}

/// What a pass deleted.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Pass {
    pub events: Swept,
    /// The scrollback segments of all panes.
    pub segments: usize,
}

/// One pass over the store at the time `now_ms`. It runs where the store is: on the actor thread
/// in the core.
pub(crate) fn pass(store: &mut Store, limits: &Limits, now_ms: i64) -> Result<Pass, StoreError> {
    let events = store.retention_sweep(now_ms, &limits.retention)?;
    let mut segments = 0;
    if limits.max_bytes_per_pane > 0 {
        for pane in store.scrollback_panes()? {
            segments += store.trim_scrollback(pane, limits.max_bytes_per_pane, now_ms)?;
        }
    }
    Ok(Pass { events, segments })
}

/// Whether a pass is due at `now`: the first one, one after a change of the limits, and one a
/// [`PERIOD`] after the last.
fn due(last: Option<&(Limits, Instant)>, limits: &Limits, now: Instant) -> bool {
    last.is_none_or(|(used, at)| used != limits || now.saturating_duration_since(*at) >= PERIOD)
}

/// Runs the passes until the core ends.
pub(crate) async fn run(actor: ActorHandle, config: Arc<ConfigStore>) {
    let mut look = interval(LOOK);
    look.set_missed_tick_behavior(MissedTickBehavior::Delay);
    // The limits of the last pass, and when it ended.
    let mut last: Option<(Limits, Instant)> = None;
    loop {
        // The first tick comes at once, so the first pass runs at the start.
        look.tick().await;
        let limits = Limits::of(&config.config());
        if !due(last.as_ref(), &limits, Instant::now()) {
            continue;
        }
        let now = now_ms();
        match actor.store(move |store| pass(store, &limits, now)).await {
            Ok(done) => log(&done),
            Err(CoreError::Store(error)) => {
                tracing::warn!(%error, "the retention pass failed; the next one is in an hour");
            }
            // The actor ended with the core.
            Err(_) => return,
        }
        last = Some((limits, Instant::now()));
    }
}

fn log(done: &Pass) {
    if *done == Pass::default() {
        tracing::debug!("the retention pass deleted nothing");
    } else {
        tracing::info!(
            events = done.events.sync + done.events.audit,
            audit_events = done.events.audit,
            scrollback_segments = done.segments,
            "the retention pass deleted old events and scrollback"
        );
    }
}

#[cfg(test)]
mod tests;
