//! Counts tokens and cost without counting one message twice.
//!
//! A message of Claude Code spans several lines, and each line repeats the usage of the message.
//! A response of Codex has a record for the usage, and the event `token_count` repeats it. The
//! ledger keeps the usage that it counted for the latest messages, and gives back only what is new.
//! It also uses the cost that the harness reports, when it does, over the price table.

use std::collections::{HashMap, VecDeque};

use nh_proto::api::time::Timestamp;
use serde::{Deserialize, Serialize};

use super::{Entry, Kind, TokenUsage, prices::Prices};

/// How many messages the ledger remembers. A message repeats within a few lines.
const REMEMBERED: usize = 64;

/// What to add to the usage of a run.
#[derive(Clone, Debug, PartialEq)]
pub struct UsageDelta {
    pub at: Timestamp,
    pub model: String,
    pub tokens: TokenUsage,
    /// The cost of the new tokens, or the correction of the cost that the harness reports.
    pub cost_usd: Option<f64>,
}

/// How a run is billed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Billing {
    /// Each token has a price.
    Api,
    /// A subscription: the tokens are counted and no dollars are shown.
    Subscription,
}

/// The state of a ledger, as the core saves it.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Saved {
    /// The latest messages and the usage that was counted for each, oldest first.
    #[serde(default)]
    pub seen: Vec<(String, TokenUsage)>,
    /// The model of the next calls of Codex.
    #[serde(default)]
    pub model: Option<String>,
    /// The cost that was counted for each model.
    #[serde(default)]
    pub costs: Vec<(String, f64)>,
}

/// The state of the count of one transcript.
#[derive(Clone, Debug, Default)]
pub struct Ledger {
    /// The usage counted for the latest messages, oldest first.
    seen: VecDeque<(String, TokenUsage)>,
    /// The model of the next calls, when the line of a call has none (Codex).
    model: Option<String>,
    /// The cost of each model that was added to the usage, from prices or from the harness.
    accounted: HashMap<String, f64>,
}

impl Ledger {
    /// What to save with the offset in the file, so that a restart goes on without counting a
    /// message twice.
    pub fn save(&self) -> Saved {
        let mut costs: Vec<_> = self
            .accounted
            .iter()
            .map(|(model, usd)| (model.clone(), *usd))
            .collect();
        costs.sort_by(|a, b| a.0.cmp(&b.0));
        Saved {
            seen: self.seen.iter().cloned().collect(),
            model: self.model.clone(),
            costs,
        }
    }

    /// A ledger that goes on from saved state.
    pub fn restore(saved: Saved) -> Self {
        Self {
            seen: saved.seen.into_iter().collect(),
            model: saved.model,
            accounted: saved.costs.into_iter().collect(),
        }
    }

    pub fn model(&self) -> Option<&str> {
        self.model.as_deref()
    }

    /// Counts one entry. The result is what to add to the usage of the run: nothing, or one delta
    /// for each model that has something new.
    pub fn apply(
        &mut self,
        entry: &Entry,
        prices: &Prices,
        billing: Billing,
        fallback_at: Timestamp,
    ) -> Vec<UsageDelta> {
        let at = entry.at.unwrap_or(fallback_at);
        match &entry.kind {
            Kind::Model { model } => {
                self.model = Some(model.clone());
                Vec::new()
            }
            Kind::Assistant {
                message,
                model,
                usage: Some(usage),
                ..
            } => {
                let Some(model) = model.clone().or_else(|| self.model.clone()) else {
                    return Vec::new();
                };
                let key = message.clone().unwrap_or_else(|| entry.id.clone());
                self.count(&key, usage, model, at, prices, billing)
                    .into_iter()
                    .collect()
            }
            Kind::Usage { key, model, usage } => {
                let Some(model) = model.clone().or_else(|| self.model.clone()) else {
                    return Vec::new();
                };
                self.count(key, usage, model, at, prices, billing)
                    .into_iter()
                    .collect()
            }
            Kind::Cost { models } if billing == Billing::Api => {
                // The cost of the harness is a total for the session. The part that the prices
                // did not cover is new. A total that is smaller than the count is not a refund.
                let mut deltas = Vec::new();
                for figure in models {
                    let accounted = self.accounted.entry(figure.model.clone()).or_insert(0.0);
                    if figure.usd > *accounted {
                        deltas.push(UsageDelta {
                            at,
                            model: figure.model.clone(),
                            tokens: TokenUsage::default(),
                            cost_usd: Some(figure.usd - *accounted),
                        });
                        *accounted = figure.usd;
                    }
                }
                deltas
            }
            _ => Vec::new(),
        }
    }

    fn count(
        &mut self,
        key: &str,
        usage: &TokenUsage,
        model: String,
        at: Timestamp,
        prices: &Prices,
        billing: Billing,
    ) -> Option<UsageDelta> {
        let new = match self.seen.iter_mut().find(|(seen, _)| seen == key) {
            Some((_, counted)) => {
                let new = usage.since(counted);
                *counted = counted.max(usage);
                new
            }
            None => {
                self.seen.push_back((key.to_owned(), *usage));
                if self.seen.len() > REMEMBERED {
                    self.seen.pop_front();
                }
                *usage
            }
        };
        if new.is_zero() {
            return None;
        }
        let cost_usd = match billing {
            Billing::Api => prices.cost(&model, &new),
            Billing::Subscription => None,
        };
        if let Some(cost) = cost_usd {
            *self.accounted.entry(model.clone()).or_insert(0.0) += cost;
        }
        Some(UsageDelta {
            at,
            model,
            tokens: new,
            cost_usd,
        })
    }
}
