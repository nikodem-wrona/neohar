//! The `health` projection: what the user needs to know about the core itself.
//!
//! Each part of the core reports its own facts, and [`Health`] puts them together in the one row
//! of the projection:
//!
//! - **storage**: the state actor reports when the database becomes degraded and when it works
//!   again;
//! - **renderer**: the window that reports its metrics says which renderer paints it;
//! - **holder protocols**: the protocol versions that the holders of this session speak;
//! - **unvalidated harnesses**: harness versions that no recording has covered;
//! - **warnings**: notes for the doctor, each with an id. A part of the core sets its warnings
//!   by group (`config`, `harness`) and replaces them as a set, so a warning that is gone is gone.
//!
//! A report that changes nothing makes no patch.

use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{Arc, Mutex, MutexGuard, PoisonError},
};

use nh_proto::api::{
    common::RendererKind,
    model::{DoctorWarning, HealthValue, StorageHealth, UnvalidatedHarness},
    projections::Projection,
};

use crate::projection::ProjectionHub;

struct Inner {
    storage: StorageHealth,
    renderer: Option<RendererKind>,
    holder_protocols: BTreeSet<u16>,
    unvalidated: Vec<UnvalidatedHarness>,
    /// By id, so the order is stable.
    warnings: BTreeMap<String, DoctorWarning>,
    /// What the projection shows.
    published: HealthValue,
}

impl Inner {
    fn value(&self) -> HealthValue {
        HealthValue {
            storage: self.storage.clone(),
            renderer: self.renderer,
            holder_protocols: self.holder_protocols.iter().copied().collect(),
            unvalidated_harnesses: self.unvalidated.clone(),
            warnings: self.warnings.values().cloned().collect(),
        }
    }
}

/// The health of the core. Cloning the `Arc` is how the parts share it.
pub struct Health {
    hub: Arc<ProjectionHub>,
    inner: Mutex<Inner>,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

impl Health {
    /// Makes the health, with nothing wrong, and publishes it.
    pub fn new(hub: Arc<ProjectionHub>) -> Arc<Self> {
        let health = Self {
            hub,
            inner: Mutex::new(Inner {
                storage: StorageHealth::Ok,
                renderer: None,
                holder_protocols: BTreeSet::new(),
                unvalidated: Vec::new(),
                warnings: BTreeMap::new(),
                published: HealthValue {
                    storage: StorageHealth::Ok,
                    renderer: None,
                    holder_protocols: Vec::new(),
                    unvalidated_harnesses: Vec::new(),
                    warnings: Vec::new(),
                },
            }),
        };
        {
            let inner = lock(&health.inner);
            health.hub.set_singleton(
                Projection::Health,
                serde_json::to_value(&inner.published).unwrap_or_default(),
            );
        }
        Arc::new(health)
    }

    /// What the projection shows now.
    pub fn value(&self) -> HealthValue {
        lock(&self.inner).value()
    }

    fn change(&self, change: impl FnOnce(&mut Inner)) {
        let mut inner = lock(&self.inner);
        change(&mut inner);
        let value = inner.value();
        if value != inner.published {
            self.hub.set_singleton(
                Projection::Health,
                serde_json::to_value(&value).unwrap_or_default(),
            );
            inner.published = value;
        }
    }

    /// The database works, or it is degraded and why.
    pub fn set_storage(&self, storage: StorageHealth) {
        self.change(|inner| inner.storage = storage);
    }

    /// The renderer of the window that reported last.
    pub fn set_renderer(&self, renderer: Option<RendererKind>) {
        self.change(|inner| inner.renderer = renderer);
    }

    /// A holder spoke this protocol version. The list is the versions seen since the core started.
    pub fn holder_protocol(&self, version: u16) {
        self.change(|inner| {
            inner.holder_protocols.insert(version);
        });
    }

    pub fn set_unvalidated_harnesses(&self, harnesses: Vec<UnvalidatedHarness>) {
        self.change(|inner| inner.unvalidated = harnesses);
    }

    /// Sets one warning. A warning with the same id is replaced.
    pub fn warn(&self, id: &str, text: impl Into<String>, hint: Option<String>) {
        let warning = DoctorWarning {
            id: id.to_owned(),
            text: text.into(),
            hint,
        };
        self.change(|inner| {
            inner.warnings.insert(id.to_owned(), warning);
        });
    }

    /// Removes one warning.
    pub fn clear(&self, id: &str) {
        self.change(|inner| {
            inner.warnings.remove(id);
        });
    }

    /// Replaces all the warnings of a group with the given texts. The ids are `group:0`,
    /// `group:1`, and so on. An empty list removes the group.
    pub fn set_warnings(
        &self,
        group: &str,
        warnings: impl IntoIterator<Item = String>,
        hint: Option<&str>,
    ) {
        let prefix = format!("{group}:");
        let fresh: Vec<DoctorWarning> = warnings
            .into_iter()
            .enumerate()
            .map(|(n, text)| DoctorWarning {
                id: format!("{prefix}{n}"),
                text,
                hint: hint.map(str::to_owned),
            })
            .collect();
        self.change(|inner| {
            inner.warnings.retain(|id, _| !id.starts_with(&prefix));
            for warning in fresh {
                inner.warnings.insert(warning.id.clone(), warning);
            }
        });
    }
}

#[cfg(test)]
mod tests;
