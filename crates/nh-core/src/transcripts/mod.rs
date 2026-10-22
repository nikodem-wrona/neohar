//! The transcripts of the runs: usage, cost, limits and the timeline.
//!
//! The harness writes a transcript file for each session. One service reads the new lines of the
//! file of every run once a second. It counts the tokens and the cost into the table `usage`, saves
//! the place in the file and the state of the count, ends the approvals that a tool result
//! answered, and publishes the `limits` projection. A window that follows `timeline:<run>` makes
//! the service keep the latest 500 items of that run. `timeline.page` reads older items from the
//! file. The file is the source: neohar stores counts and pointers, and no copy of the text.

use std::{collections::HashMap, sync::OnceLock};

use nh_agents::transcript::Prices;
use nh_proto::api::{common::Harness, model::RateLimitWindow, time::Timestamp};

mod apply;
pub(crate) mod calls;
mod service;
mod tail;

pub(crate) use apply::{publish_limits, started};
pub(crate) use service::run;

#[cfg(test)]
mod tests;

/// The price table that ships with neohar. It is parsed once.
pub(crate) fn prices() -> &'static Prices {
    static PRICES: OnceLock<Prices> = OnceLock::new();
    PRICES.get_or_init(Prices::bundled)
}

/// The UTC day of a moment, as `2026-10-10`. The table `usage` has one row for each day.
pub(crate) fn day(at: Timestamp) -> String {
    at.to_rfc3339().chars().take(10).collect()
}

/// What the core knows of the limits of the harnesses.
#[derive(Clone, Debug, Default)]
pub(crate) struct Limits {
    /// The windows that a transcript reported last (Codex).
    pub windows: HashMap<Harness, Vec<RateLimitWindow>>,
    /// The usage of the day for each harness, with the day it is for.
    pub today: HashMap<Harness, (String, nh_proto::api::model::Usage)>,
}
