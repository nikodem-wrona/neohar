//! The notification centre of the system.
//!
//! The core decides what a notification says and when it goes. This module posts it, takes it
//! back and reports a click. On a Mac it uses the notification centre of the system, which has
//! buttons, so an approval can be answered from the banner. Other platforms have no notification
//! code yet, and the core falls back to the tray, the badge and the cards.

use std::sync::Arc;

use nh_core::host::HostNotification;
use nh_proto::api::ids::NotificationId;

#[cfg(target_os = "macos")]
mod mac;
#[cfg(not(target_os = "macos"))]
mod other;

#[cfg(target_os = "macos")]
use mac as platform;
#[cfg(not(target_os = "macos"))]
use other as platform;

/// What the person did with a notification.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Click {
    /// The body of the banner: show the card.
    Open,
    /// The button Allow.
    Allow,
    /// The button Deny.
    Deny,
}

/// A click, with the identifier of the notification that it was on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Response {
    pub click: Click,
    pub identifier: String,
}

/// What the system lets the app do with notifications.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Permission {
    Granted,
    /// The person said no, in the system settings or in the first prompt.
    Denied,
    /// This run has no notification centre, for example a binary that is not in an app bundle.
    Unavailable,
}

/// The identifier of a notification for the system. It is the id that the host gave to the core,
/// so a notification that the core updates is the same notification of the system.
pub fn identifier(id: NotificationId) -> String {
    id.to_string()
}

pub struct Notifier {
    inner: platform::Platform,
}

impl Notifier {
    /// Starts the notification centre. `on_response` hears a click, and `on_permission` hears
    /// the state of the permission, once the system has answered. Call it on the main thread.
    pub fn start(
        on_response: Arc<dyn Fn(Response) + Send + Sync>,
        on_permission: Arc<dyn Fn(Permission) + Send + Sync>,
    ) -> Self {
        Self {
            inner: platform::Platform::start(on_response, on_permission),
        }
    }

    /// Whether the banners have buttons.
    pub fn actions(&self) -> bool {
        self.inner.actions()
    }

    /// Shows a notification, or replaces the one with the same identifier.
    pub fn post(&self, identifier: &str, notification: &HostNotification) {
        self.inner.post(identifier, notification);
    }

    /// Takes a notification back, from the screen and from the list of the centre.
    pub fn withdraw(&self, identifier: &str) {
        self.inner.withdraw(identifier);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_identifier_is_the_id_of_the_notification() {
        let id = NotificationId::new(ulid::Ulid::from(1));
        assert_eq!(identifier(id), id.to_string());
    }
}
