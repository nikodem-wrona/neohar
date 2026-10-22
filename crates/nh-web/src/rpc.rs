//! `/rpc` of a phone: the JSON-RPC loop of `nh_core::ws_rpc` with the origin of the phone.
//!
//! A phone has full access and one window on the core. The listener finds the
//! phone window when the socket opens, and each call has it in its origin, so a key of the phone
//! moves the geometry lease to the phone. The core serves every projection but `link`,
//! which the host of a window serves: the listener serves it here, with the state
//! `local`.

use std::sync::{
    Mutex, PoisonError,
    atomic::{AtomicU64, Ordering},
};

use nh_core::{
    CoreHandle, ErrorCode, MessageSink, Origin, RpcError, phones::phone_may_follow, ws_rpc,
};
use nh_proto::api::{
    ids::WindowId,
    model::{LinkValue, MachineState},
    time::Timestamp,
};
use serde_json::{Value, json};
use tokio::sync::{broadcast::error::RecvError, watch};
use ulid::Ulid;

/// The name of the projection that the host of a window serves.
const LINK: &str = "link";

/// The version of `link` on this listener: the machine of the core is local and never changes.
const LINK_VERSION: u64 = 1;

/// The numbers of the subscriptions of `link`.
static LINK_IDS: AtomicU64 = AtomicU64::new(1);

/// The calls of one connection of a phone.
pub(crate) struct PhoneBackend {
    pub core: CoreHandle,
    pub device: Ulid,
    /// The number of the connection: `phone.visible` holds for it.
    pub connection: u64,
    /// The phone window of the device on this core.
    pub window: WindowId,
    /// The subscriptions of `link` of this connection.
    pub links: Mutex<Vec<String>>,
}

impl PhoneBackend {
    fn origin(&self) -> Origin {
        Origin::phone_on(self.device, self.connection).from_window(self.window)
    }

    /// `projection.versions`: the core answers for its projections, and the listener for `link`.
    async fn versions(&self, params: Value) -> Result<Value, RpcError> {
        let names: Vec<Value> = params
            .get("names")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let link = names.iter().any(|name| name == LINK);
        let core: Vec<Value> = names.into_iter().filter(|name| name != LINK).collect();
        let mut reply = self
            .core
            .call_as(
                self.origin(),
                "projection.versions",
                json!({ "names": core }),
            )
            .await?;
        if link && let Some(versions) = reply.get_mut("versions").and_then(Value::as_object_mut) {
            versions.insert(LINK.to_owned(), json!(LINK_VERSION));
        }
        Ok(reply)
    }
}

/// The value of `link` for a window of the core of the listener.
fn local_link() -> Value {
    let value = LinkValue {
        machine: "local".to_owned(),
        state: MachineState::Local,
        rtt_ms: None,
        changed_at: Timestamp(0),
        reason: None,
        text: "local".to_owned(),
    };
    json!({
        "type": "snapshot",
        "version": LINK_VERSION,
        "data": { "items": { "_": value }, "order": ["_"] },
    })
}

#[async_trait::async_trait]
impl ws_rpc::RpcBackend for PhoneBackend {
    async fn call(&self, method: &str, params: Value) -> Result<Value, RpcError> {
        if method == "projection.versions" {
            return self.versions(params).await;
        }
        self.core.call_as(self.origin(), method, params).await
    }

    async fn subscribe(
        &self,
        name: &str,
        since: Option<u64>,
        sink: Box<dyn MessageSink>,
    ) -> Result<String, RpcError> {
        if name == LINK {
            // One snapshot, and no patch: the machine of the core does not change.
            let id = format!("link#{}", LINK_IDS.fetch_add(1, Ordering::Relaxed));
            if since != Some(LINK_VERSION) {
                sink.send(local_link());
            }
            self.links
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push(id.clone());
            return Ok(id);
        }
        if !phone_may_follow(name, self.window) {
            return Err(RpcError::new(
                ErrorCode::PermissionDenied,
                format!("a phone may not follow {name}"),
            ));
        }
        self.core.subscribe_tracked(name, since, sink)
    }

    async fn unsubscribe(&self, subscription: &str) -> bool {
        let mut links = self.links.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(index) = links.iter().position(|id| id == subscription) {
            links.remove(index);
            return true;
        }
        drop(links);
        self.core.unsubscribe_tracked(subscription)
    }
}

/// Completes when the listener stops or `phone.remove` removes the device.
pub(crate) async fn ended(
    core: CoreHandle,
    device: Ulid,
    token: String,
    mut removed: tokio::sync::broadcast::Receiver<Ulid>,
    mut stop: watch::Receiver<bool>,
) {
    loop {
        tokio::select! {
            changed = stop.changed() => {
                if changed.is_err() || *stop.borrow() {
                    return;
                }
            }
            next = removed.recv() => match next {
                Ok(id) if id == device => return,
                Ok(_) => {}
                // Missed removals: the device may be one of them.
                Err(RecvError::Lagged(_)) => {
                    if core.phone_of_token(&token).await.is_none() {
                        return;
                    }
                }
                Err(RecvError::Closed) => std::future::pending::<()>().await,
            },
        }
    }
}
