//! The methods of the session as a whole: `session.stop_everything` and
//! `session.shutdown`.
//!
//! `session.stop_everything` stops every program of every pane, as "Quit and stop everything" of
//! the app does. The core keeps running, and its panes show that their programs ended.
//!
//! `session.shutdown` asks the host to end the core: the host checkpoints each pane, ends the
//! leases and exits, and the panes keep running, as after a quit of the app. Only a host that can
//! end without a person registers for it ([`crate::CoreHandle::accept_shutdown`]): `nh daemon`
//! does. The app does not, because it quits from its menu, and there the method answers
//! `unsupported`.

use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use nh_proto::api::{
    calls::workspace::SessionStopped,
    common::Empty,
    methods::{Call, Method},
};
use serde_json::{Value, to_value};

use crate::{
    ErrorCode,
    error::RpcError,
    pane::lock,
    runtime::{CoreHandle, Inner},
};

/// How long a program gets to end after `Term` and `Hup` before it gets `Kill`. It is the grace of
/// "Quit and stop everything" in the app.
pub const STOP_GRACE: Duration = Duration::from_secs(5);

/// How long the agents get to end by themselves first.
pub const AGENT_GRACE: Duration = Duration::from_secs(2);

/// What a host does when a client asks it to end.
pub(crate) type ShutdownHook = Arc<dyn Fn() + Send + Sync>;

/// What a host does on `debug.drop_links`: it closes every link of `link.sock`.
pub(crate) type DropLinksHook = Arc<dyn Fn() + Send + Sync>;

/// The hook of the host for `debug.drop_links`, or nothing.
#[derive(Default)]
pub(crate) struct DropLinksSlot(Mutex<Option<DropLinksHook>>);

impl DropLinksSlot {
    pub fn set(&self, hook: DropLinksHook) {
        *lock(&self.0) = Some(hook);
    }

    fn get(&self) -> Option<DropLinksHook> {
        lock(&self.0).clone()
    }
}

/// The hook of the host for `session.shutdown`, or nothing.
#[derive(Default)]
pub(crate) struct ShutdownSlot(Mutex<Option<ShutdownHook>>);

impl ShutdownSlot {
    pub fn set(&self, hook: ShutdownHook) {
        *lock(&self.0) = Some(hook);
    }

    fn get(&self) -> Option<ShutdownHook> {
        lock(&self.0).clone()
    }
}

/// Whether this module runs the method.
pub(crate) fn handles(method: &str) -> bool {
    matches!(
        Method::from_name(method),
        Some(Method::SessionStopEverything | Method::SessionShutdown | Method::DebugDropLinks)
    )
}

/// Runs a call that [`handles`] accepted.
pub(crate) async fn run(inner: &Arc<Inner>, call: Call) -> Result<Value, RpcError> {
    match call {
        Call::SessionStopEverything(p) => {
            let core = CoreHandle::from_inner(Arc::clone(inner));
            let report = if p.force {
                core.stop_everything_with(Duration::ZERO, None).await
            } else {
                core.stop_everything_with(STOP_GRACE, Some(AGENT_GRACE))
                    .await
            };
            tracing::info!(?report, force = p.force, "a client stopped everything");
            let count = |n: usize| u32::try_from(n).unwrap_or(u32::MAX);
            json(&SessionStopped {
                ended: count(report.ended),
                killed: count(report.killed),
                left: count(report.left),
            })
        }
        Call::SessionShutdown(_) => {
            let Some(hook) = inner.shutdown.get() else {
                return Err(RpcError::new(
                    ErrorCode::Unsupported,
                    "this host ends from its menu: quit the app",
                ));
            };
            tracing::info!("a client asked the host to end the core");
            // The host does the work on its own thread after the reply: it checkpoints each pane,
            // ends the leases and exits.
            hook();
            json(&Empty {})
        }
        // For the tests of the reconnect: the clients see a link that goes down.
        Call::DebugDropLinks(_) => {
            let Some(hook) = inner.drop_links.get() else {
                return Err(RpcError::new(
                    ErrorCode::Unsupported,
                    "this host serves no link.sock, so it has no link to drop",
                ));
            };
            hook();
            json(&Empty {})
        }
        other => Err(RpcError::internal(format!(
            "{} is not a method of the session",
            other.method().name()
        ))),
    }
}

fn json<T: serde::Serialize>(value: &T) -> Result<Value, RpcError> {
    to_value(value).map_err(|e| RpcError::internal(e.to_string()))
}
