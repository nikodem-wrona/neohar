//! No notification centre yet: the core offers the tray, the badge and the cards.

use std::sync::Arc;

use nh_core::host::HostNotification;

use super::{Permission, Response};

pub struct Platform;

impl Platform {
    pub fn start(
        _on_response: Arc<dyn Fn(Response) + Send + Sync>,
        on_permission: Arc<dyn Fn(Permission) + Send + Sync>,
    ) -> Self {
        on_permission(Permission::Unavailable);
        Self
    }

    pub fn actions(&self) -> bool {
        false
    }

    pub fn post(&self, _identifier: &str, _notification: &HostNotification) {}

    pub fn withdraw(&self, _identifier: &str) {}
}
