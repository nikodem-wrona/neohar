//! The host of the core of a remote machine.
//!
//! The link client of a machine gives the requests of that core to this host, after it has
//! checked them against the requests that a link carries and the limits of `HostGuard`. The host
//! does what the app does for its own core, with the label of the machine:
//!
//! - A notification gets the label in its title, and a click or a button goes back to this
//!   machine. The paired phones get a push of it from the core of this app.
//! - The badge is one part of the sum, and the tray has a section for this machine.
//! - A request for a window names a window of this machine, so it finds the native window with
//!   the label `<machine>/<id>`. Opening and closing a window need nothing: the app follows the
//!   `windows` projection of the machine.
//! - The clipboard is the one of this Mac.

use std::{path::PathBuf, sync::Arc};

use nh_core::{
    CoreHandle, Host,
    host::{HostNotification, NotificationCapabilities, TrayModel, WindowSpec},
};
use nh_proto::api::ids::{NotificationId, WindowId};

use crate::{host::TauriHost, machines::label_of};

pub struct MachineHost {
    machine: String,
    app: Arc<TauriHost>,
    /// The core of this app: it sends the push of a notice to the paired phones.
    core: Option<CoreHandle>,
}

impl std::fmt::Debug for MachineHost {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MachineHost")
            .field("machine", &self.machine)
            .finish_non_exhaustive()
    }
}

impl MachineHost {
    pub fn new(machine: &str, app: Arc<TauriHost>) -> Self {
        Self {
            machine: machine.to_owned(),
            app,
            core: None,
        }
    }

    /// The notices of the machine also go to the paired phones as a push.
    pub fn pushing_to(mut self, core: CoreHandle) -> Self {
        self.core = Some(core);
        self
    }
}

impl Host for MachineHost {
    fn notify(&self, notification: HostNotification) -> NotificationId {
        if let Some(core) = &self.core {
            core.push_machine_notice(&self.machine, notification.clone());
        }
        self.app.notify_for(&self.machine, notification)
    }

    fn update_notification(&self, id: NotificationId, notification: HostNotification) {
        self.app
            .update_notification_for(&self.machine, id, notification);
    }

    fn withdraw_notification(&self, id: NotificationId) {
        self.app.withdraw_notification(id);
    }

    fn notification_capabilities(&self) -> NotificationCapabilities {
        self.app.notification_capabilities()
    }

    fn set_badge(&self, count: u32) {
        self.app.set_badge_for(&self.machine, count);
    }

    fn set_tray(&self, tray: TrayModel) {
        self.app.set_tray_for(&self.machine, tray);
    }

    // The app follows the `windows` projection of the machine, so these two have nothing to do.
    fn open_window(&self, _spec: WindowSpec) {}

    fn close_window(&self, _id: WindowId) {}

    fn focus_window(&self, id: WindowId) {
        self.app
            .focus_label(label_of(&self.machine, &id.to_string()));
    }

    /// Never asked: the path belongs to the machine of the core.
    fn sidecar_path(&self) -> PathBuf {
        self.app.sidecar_path()
    }

    fn clipboard_write(&self, text: String) {
        self.app.clipboard_write(text);
    }

    /// Never sent over a link.
    fn open_url(&self, _: String) {}

    fn open_settings(&self) {
        self.app.open_settings_of(&self.machine);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_window_request_names_a_window_of_the_machine_that_sent_it() {
        let id = "01ARZ3NDEKTSV4RRFFQ69G5FAV";
        assert_eq!(label_of("studio", id), format!("studio/{id}"));
        // The same id on `local` is another window.
        assert_eq!(label_of("local", id), id);
    }
}
