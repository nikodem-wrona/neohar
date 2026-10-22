//! The other machines of the host: a phone reaches them through the app that
//! hosts the core. The host gives the listener its [`PhoneMachines`]. `/rpc`, `/stream` and
//! `/blob` with `machine=<label>` go to that machine, and no label is the core of the listener.
//! `nh daemon` gives none, so its phone shows that machine only.
//!
//! When a socket opens, the listener asks the machine for the window of the phone there
//! (`phone.window`), as it does on its own core. The calls and the streams carry the device of the
//! phone, and the core of the machine puts it in their origin. The listener keeps the
//! rules of a phone: the projections that a phone may follow, a render stream of its own window
//! only, and only the images of the blob store. The other `phone.*` methods (`phone.visible`, the
//! push, the pairing) go to the core of the listener, where the phone is paired.

use std::sync::Arc;

use async_trait::async_trait;
use nh_core::{
    ClientInfo, CoreHandle, ErrorCode, FrameSink, MessageSink, Origin, RpcError,
    phones::phone_may_follow, ws_rpc,
};
use nh_proto::api::ids::WindowId;
use serde_json::Value;

/// The machines that a phone reaches through the host of the core.
pub trait PhoneMachines: Send + Sync + 'static {
    /// The machine with this label, while its link is up. The error is `not_found` for a label
    /// that is no machine, and `link_down` for a machine whose link is down.
    fn machine(&self, label: &str) -> Result<Arc<dyn PhoneMachine>, RpcError>;
}

/// One machine, as a phone reaches it. The host answers what it serves itself for a window of a
/// machine: the projection `link`, the merged `settings`, their versions,
/// and the `config.*` calls of the sections of this Mac.
#[async_trait]
pub trait PhoneMachine: Send + Sync + 'static {
    /// A call of the phone, from its window on the machine.
    async fn call(
        &self,
        phone: &ClientInfo,
        window: Option<WindowId>,
        method: &str,
        params: Value,
    ) -> Result<Value, RpcError>;
    async fn subscribe(
        &self,
        name: &str,
        since: Option<u64>,
        sink: Box<dyn MessageSink>,
    ) -> Result<String, RpcError>;
    async fn unsubscribe(&self, subscription: &str) -> bool;
    /// A stream of the phone. The core of the machine attaches the phone while one is open.
    async fn stream(
        &self,
        phone: &ClientInfo,
        kind: &str,
        params: Value,
        window: Option<WindowId>,
        sink: Arc<dyn FrameSink>,
    ) -> Result<(), RpcError>;
    /// Puts a file that the phone attaches into the blob store of the machine. Returns its hash.
    async fn put_blob(&self, name: &str, bytes: Vec<u8>) -> Result<String, RpcError>;
    /// A blob of the machine: its bytes and its type.
    async fn blob(&self, hash: &str) -> Result<Option<(Vec<u8>, String)>, RpcError>;
}

/// The window of the phone on a machine: the machine makes it when there is none.
pub(crate) async fn phone_window(
    machine: &dyn PhoneMachine,
    phone: &ClientInfo,
) -> Result<WindowId, RpcError> {
    let reply = machine
        .call(phone, None, "phone.window", serde_json::json!({}))
        .await?;
    reply
        .get("window")
        .and_then(|window| serde_json::from_value::<WindowId>(window.clone()).ok())
        .ok_or_else(|| RpcError::internal("the machine gave no phone window"))
}

/// The calls of one connection of a phone to a machine.
pub(crate) struct MachineBackend {
    /// The core of the listener, where the phone is paired.
    pub core: CoreHandle,
    pub machine: Arc<dyn PhoneMachine>,
    pub phone: ClientInfo,
    /// The number of the connection: `phone.visible` holds for it.
    pub connection: u64,
    /// The window of the phone on the machine.
    pub window: WindowId,
}

#[async_trait]
impl ws_rpc::RpcBackend for MachineBackend {
    async fn call(&self, method: &str, params: Value) -> Result<Value, RpcError> {
        if method.starts_with("phone.") && method != "phone.window" {
            let origin = Origin::phone_on(self.phone.device, self.connection);
            return self.core.call_as(origin, method, params).await;
        }
        self.machine
            .call(&self.phone, Some(self.window), method, params)
            .await
    }

    async fn subscribe(
        &self,
        name: &str,
        since: Option<u64>,
        sink: Box<dyn MessageSink>,
    ) -> Result<String, RpcError> {
        // The host serves `link` and the merged `settings`. The core of the machine serves the
        // others, with the limits of a phone.
        if !matches!(name, "link" | "settings") && !phone_may_follow(name, self.window) {
            return Err(RpcError::new(
                ErrorCode::PermissionDenied,
                format!("a phone may not follow {name}"),
            ));
        }
        self.machine.subscribe(name, since, sink).await
    }

    async fn unsubscribe(&self, subscription: &str) -> bool {
        self.machine.unsubscribe(subscription).await
    }
}
