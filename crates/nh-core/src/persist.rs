//! The store as the rest of the core uses it.
//!
//! The store belongs to the state actor (`actor/`). The code that is not a command and still needs
//! the database (the pane tasks) runs a closure on the thread of the
//! actor. It waits for its turn, like a command, so there is one writer and one order.

use std::{
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use nh_store::{Store, StoreError};

use crate::{actor::ActorHandle, error::CoreError};

/// The database of the session.
pub(crate) struct Persist {
    actor: ActorHandle,
}

impl Persist {
    /// The store behind the actor of a core.
    pub fn on(actor: ActorHandle) -> Arc<Self> {
        Arc::new(Self { actor })
    }

    /// Runs `f` on the database. The task waits without holding a thread.
    pub async fn run<T: Send + 'static>(
        self: &Arc<Self>,
        f: impl FnOnce(&mut Store) -> Result<T, StoreError> + Send + 'static,
    ) -> Result<T, CoreError> {
        self.actor.store(f).await
    }
}

/// The wall clock in unix milliseconds.
pub(crate) fn now_ms() -> i64 {
    i64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_millis()),
    )
    .unwrap_or(i64::MAX)
}
