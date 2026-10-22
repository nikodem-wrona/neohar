//! The interactions in memory.
//!
//! The store is the truth, and this is the shape that the handlers read. A change goes to the
//! store and the events in one transaction, and then to this table (the pattern of the runs).

use std::collections::{HashMap, VecDeque};

use nh_proto::api::{
    common::Risk,
    ids::{InteractionId, PaneId, Ref, RefValue, RunId, Target},
    model::{GateMode, InteractionState},
    time::Timestamp,
};
use nh_store::{Store, StoreError};

use super::{model::Interaction, rows};

/// How many interactions that ended stay in the table, and in the projection, so that a card can
/// show how its interaction ended.
pub(crate) const KEEP_FINAL: usize = 50;

#[derive(Clone, Default)]
pub(crate) struct Interactions {
    by_id: HashMap<InteractionId, Interaction>,
    /// The ids of the interactions that ended, the oldest first.
    finals: VecDeque<InteractionId>,
}

/// How urgent an interaction is: the riskier first.
fn risk_rank(risk: Risk) -> u8 {
    match risk {
        Risk::High => 0,
        Risk::Unknown => 1,
        Risk::Medium => 2,
        Risk::Low => 3,
    }
}

impl Interactions {
    pub fn new() -> Self {
        Self::default()
    }

    /// The interactions that the last core left open. An interaction of a run that is gone ends
    /// now, one whose deadline passed fails, and the store learns it in the same transaction.
    pub fn load(
        store: &mut Store,
        now: Timestamp,
        run_live: &dyn Fn(RunId) -> bool,
    ) -> Result<Self, StoreError> {
        let mut table = Self::new();
        let mut changed = Vec::new();
        for (row, record) in store.open_interactions()? {
            let Some(mut interaction) = rows::from_rows(&row, record.as_ref()) else {
                // A row that this version cannot read is history. It must not stay open.
                tracing::warn!(interaction = %row.id, "an interaction of the table is not readable and ends now");
                let mut unreadable = row.clone();
                unreadable.state = "cancelled".to_owned();
                unreadable.resolved_at = Some(now.0);
                store.tx(now.0, |tx| tx.put_interaction(&unreadable))?;
                continue;
            };
            let mut events = Vec::new();
            if !interaction.run.is_some_and(run_live) {
                events.extend(interaction.cancel("process_exited", now).events);
            } else if interaction.deadline.is_some_and(|d| d <= now) {
                events.extend(interaction.deadline_passed(now).events);
            }
            if !events.is_empty() {
                changed.push((interaction.clone(), events));
            }
            table.put(interaction);
        }
        if !changed.is_empty() {
            store.tx(now.0, |tx| {
                changed
                    .iter()
                    .try_for_each(|(interaction, events)| rows::write(tx, interaction, events))
            })?;
        }
        Ok(table)
    }

    pub fn get(&self, id: InteractionId) -> Option<&Interaction> {
        self.by_id.get(&id)
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.by_id.len()
    }

    /// Adds an interaction or replaces it. Gives the ids that left the table because too many
    /// interactions had ended.
    pub fn put(&mut self, interaction: Interaction) -> Vec<InteractionId> {
        let id = interaction.id;
        let ended = interaction.is_final();
        self.by_id.insert(id, interaction);
        let mut evicted = Vec::new();
        if ended && !self.finals.contains(&id) {
            self.finals.push_back(id);
            while self.finals.len() > KEEP_FINAL {
                if let Some(old) = self.finals.pop_front() {
                    self.by_id.remove(&old);
                    evicted.push(old);
                }
            }
        }
        evicted
    }

    /// The interaction that a reference names: its id, or its handle (`i42`).
    pub fn resolve(&self, reference: &Ref<InteractionId>) -> Option<InteractionId> {
        match &reference.0 {
            RefValue::Id(id) => self.by_id.contains_key(id).then_some(*id),
            RefValue::Handle(handle) => {
                if handle.machine.is_some() {
                    return None;
                }
                let Target::Interaction(number) = handle.target else {
                    return None;
                };
                let wanted = format!("i{number}");
                self.by_id
                    .values()
                    .find(|interaction| interaction.handle == wanted)
                    .map(|interaction| interaction.id)
            }
            RefValue::Name(_) => None,
        }
    }

    /// The interaction that a run opened for a native ref and that has not ended.
    pub fn open_by_native(&self, run: RunId, native_ref: &str) -> Option<InteractionId> {
        self.by_id
            .values()
            .find(|i| {
                i.run == Some(run) && i.native_ref.as_deref() == Some(native_ref) && !i.is_final()
            })
            .map(|i| i.id)
    }

    /// The interactions of a run that have not ended, in the order of opening.
    pub fn open_of_run(&self, run: RunId) -> Vec<&Interaction> {
        let mut found: Vec<&Interaction> = self
            .by_id
            .values()
            .filter(|i| i.run == Some(run) && !i.is_final())
            .collect();
        found.sort_by_key(|i| (i.opened_at, i.id));
        found
    }

    /// The interactions that have not ended, the more urgent first: the riskier, then the
    /// one that waited longer.
    pub fn open(&self) -> Vec<&Interaction> {
        let mut found: Vec<&Interaction> = self.by_id.values().filter(|i| !i.is_final()).collect();
        found.sort_by_key(|i| (risk_rank(i.risk), i.opened_at, i.id));
        found
    }

    /// The interactions of a pane that wait for an answer with the hook held (or lost for a moment).
    /// When the pane gets the focus, the core releases these.
    pub fn held_for(&self, pane: PaneId) -> Vec<InteractionId> {
        let mut found: Vec<&Interaction> = self
            .by_id
            .values()
            .filter(|i| {
                i.pane == Some(pane)
                    && i.state == InteractionState::Open
                    && i.mode == GateMode::Gate
            })
            .collect();
        found.sort_by_key(|i| (i.opened_at, i.id));
        found.into_iter().map(|i| i.id).collect()
    }

    /// The interactions that ended, the newest first.
    pub fn ended(&self) -> Vec<&Interaction> {
        self.finals
            .iter()
            .rev()
            .filter_map(|id| self.by_id.get(id))
            .collect()
    }

    /// The most urgent open interaction that a client can answer now. With `after`, the next one
    /// after that interaction, and the first one again at the end of the order.
    pub fn next(&self, after: Option<InteractionId>) -> Option<&Interaction> {
        let answerable: Vec<&Interaction> = self
            .open()
            .into_iter()
            .filter(|i| i.answerable && i.state == InteractionState::Open)
            .collect();
        let position = after.and_then(|id| answerable.iter().position(|i| i.id == id));
        match position {
            Some(at) => answerable
                .get(at + 1)
                .or_else(|| answerable.first())
                .copied(),
            None => answerable.first().copied(),
        }
    }
}

#[cfg(test)]
mod tests;
