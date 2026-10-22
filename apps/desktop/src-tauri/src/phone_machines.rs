//! The machines of the app for the listener of the phone.
//!
//! A phone reaches the other machines through this app: one home-screen app for all of them. The
//! listener gets [`PhoneMachines`], and each call, subscription and stream of a phone takes the
//! path of a window of that machine ([`crate::commands`]), with the device of the phone. The core
//! of the machine puts that device in the origin, so the window of the phone there belongs to the
//! phone, and no native window opens for it.

use std::sync::Arc;

use async_trait::async_trait;
use nh_core::{ClientInfo, ErrorCode, FrameSink, MessageSink, RpcError};
use nh_proto::api::ids::WindowId;
use nh_remote::Device;
use serde_json::{Value, json};

use crate::{
    backend::{Backend, image_type},
    commands,
    machines::{LOCAL, Machines},
};

/// The machines of the app, as the listener sees them.
pub struct PhoneMachines(pub Arc<Machines>);

impl nh_web::PhoneMachines for PhoneMachines {
    fn machine(&self, label: &str) -> Result<Arc<dyn nh_web::PhoneMachine>, RpcError> {
        if label == LOCAL || !nh_core::config::valid_machine_label(label) {
            return Err(RpcError::new(
                ErrorCode::NotFound,
                format!("no machine {label:?}"),
            ));
        }
        // A machine whose link is down fails now, with `link_down`.
        self.0.backend(label)?;
        Ok(Arc::new(PhoneMachine {
            machines: Arc::clone(&self.0),
            label: label.to_owned(),
        }))
    }
}

/// One machine of the app for a phone.
struct PhoneMachine {
    machines: Arc<Machines>,
    label: String,
}

/// The device of a phone on a link.
fn device_of(phone: &ClientInfo) -> Device {
    Device {
        id: phone.device.to_string(),
        name: phone.name.clone(),
    }
}

#[async_trait]
impl nh_web::PhoneMachine for PhoneMachine {
    async fn call(
        &self,
        phone: &ClientInfo,
        window: Option<WindowId>,
        method: &str,
        params: Value,
    ) -> Result<Value, RpcError> {
        let device = device_of(phone);
        commands::route_call(
            &self.machines,
            &self.label,
            window,
            Some(&device),
            method,
            params,
        )
        .await
    }

    async fn subscribe(
        &self,
        name: &str,
        since: Option<u64>,
        sink: Box<dyn MessageSink>,
    ) -> Result<String, RpcError> {
        commands::route_subscribe(&self.machines, &self.label, name, since, sink).await
    }

    async fn unsubscribe(&self, subscription: &str) -> bool {
        if self.machines.unsubscribe_link(subscription)
            || self.machines.unsubscribe_settings(subscription)
        {
            return true;
        }
        let Ok(backend) = self.machines.backend(&self.label) else {
            return false;
        };
        backend
            .rpc(
                "projection.unsubscribe",
                json!({ "subscription": subscription }),
            )
            .await
            .is_ok()
    }

    async fn stream(
        &self,
        phone: &ClientInfo,
        kind: &str,
        params: Value,
        window: Option<WindowId>,
        sink: Arc<dyn FrameSink>,
    ) -> Result<(), RpcError> {
        let device = device_of(phone);
        self.machines
            .backend(&self.label)?
            .stream_for(Some(&device), kind, params, window, sink)
            .await
    }

    async fn put_blob(&self, name: &str, bytes: Vec<u8>) -> Result<String, RpcError> {
        match self.machines.backend(&self.label)? {
            Backend::Link(linked) => linked.client.put_blob(name, bytes).await,
            Backend::Local(_) => Err(RpcError::internal("not a machine of a link")),
        }
    }

    /// Only an image, as for a window of the machine.
    async fn blob(&self, hash: &str) -> Result<Option<(Vec<u8>, String)>, RpcError> {
        let found = self.machines.backend(&self.label)?.blob(hash).await?;
        Ok(found.filter(|(_, mime)| image_type(mime).is_some()))
    }
}
