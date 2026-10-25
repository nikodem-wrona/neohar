//! What the core needs from the app that hosts it.
//!
//! The core never touches a window, the tray, the notification centre or the clipboard itself. It
//! asks the [`Host`]. `neohar.app` implements the trait with Tauri. `nh daemon` and the tests use
//! [`NoopHost`] or [`RecordingHost`]. The UI does not call the host: the core drives it.

use std::{
    path::PathBuf,
    sync::{Mutex, PoisonError},
};

use nh_proto::api::{
    common::{Bounds, Urgency},
    ids::{InteractionId, NotificationId, PaneId, TabId, WindowId},
};

use crate::ids::new_notification_id;

/// A notification of the platform.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostNotification {
    pub title: String,
    pub body: String,
    pub urgency: Urgency,
    /// The interaction that the notification is about. A click opens it.
    pub interaction: Option<InteractionId>,
    /// The pane that the notification is about, when it is about no interaction: a run whose turn
    /// ended. A click shows the pane.
    pub pane: Option<PaneId>,
    /// The actions it offers, if the platform has them (see [`NotificationCapabilities`]).
    pub actions: Vec<NotificationAction>,
}

/// An action of a notification: allow or deny. The core offers Allow only for a request that
/// the text shows whole, at a low or medium risk.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NotificationAction {
    Allow,
    Deny,
}

/// What the platform can do with a notification.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NotificationCapabilities {
    /// The notification can have action buttons. Without it the core offers "open" only.
    pub actions: bool,
}

/// What the system lets the app do with notifications. The host reports it to the core, which
/// shows a line in the doctor when the notifications cannot reach the person.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NotificationPermission {
    Granted,
    /// The person said no, in the system settings or in the first prompt.
    Denied,
    /// This run has no notification centre: it does not run from an app bundle.
    Unavailable,
}

/// One line of the menu of the tray.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrayEntry {
    pub label: String,
    pub interaction: Option<InteractionId>,
    /// An approval has a submenu with Deny and Open. The other kinds have Open.
    pub approval: bool,
    /// The submenu has Allow: the label shows the whole request, at a low or medium risk.
    pub allow: bool,
    /// The submenu has "Allow always" with this rule, such as `pnpm test:*`.
    pub always: Option<String>,
}

/// The menu of the tray: the open interactions, and the count for the badge.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TrayModel {
    pub entries: Vec<TrayEntry>,
    pub badge: u32,
}

/// A window that the core wants the host to open.
#[derive(Clone, Debug, PartialEq)]
pub struct WindowSpec {
    pub id: WindowId,
    pub bounds: Option<Bounds>,
    pub panes: Vec<PaneId>,
    pub tab: Option<TabId>,
}

/// A path that the core asks the host to open, or to show in the file manager (`fs.open`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PathOpen {
    /// The window that asked.
    pub window: WindowId,
    /// An absolute path of the machine of the core.
    pub path: PathBuf,
    /// Show the path in the file manager, and do not open it.
    pub reveal: bool,
    /// The app of the setting `ui.editor`. `None` is the default app of the path.
    pub with: Option<String>,
}

/// Why a host did not open a path.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OpenRefused {
    /// This host cannot open a path of the machine of the core: it is a link client, or it has
    /// no file manager.
    Unsupported,
    /// The system did not open the path, with its message.
    Failed(String),
}

/// What kind of question ssh asks. The host shows a host key as a choice between
/// "Connect" and "Cancel", and the other questions with a field that hides what the person types.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SecretKind {
    /// "Are you sure you want to continue connecting": the answer is `yes` or no answer.
    HostKey,
    /// A password, a passphrase or a PIN.
    Secret,
}

impl SecretKind {
    /// The kind of a question from its text. ssh asks about a host key with "continue connecting".
    pub fn of_prompt(prompt: &str) -> Self {
        if prompt.to_ascii_lowercase().contains("continue connecting") {
            Self::HostKey
        } else {
            Self::Secret
        }
    }
}

/// A question of ssh that the host asks the person (`askpass.ask`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SecretQuestion {
    /// The text of ssh.
    pub prompt: String,
    /// The label of the machine that the link is for.
    pub machine: Option<String>,
    pub kind: SecretKind,
}

/// What the host calls when the person answers a question of ssh: the answer, or `None` when the
/// person cancelled. The answer goes to this function and nowhere else.
pub type SecretAnswered = Box<dyn FnOnce(Option<String>) + Send>;

/// The host. All methods are called from the threads of the core and must not block for long.
pub trait Host: Send + Sync + std::fmt::Debug + 'static {
    /// Shows a notification. Returns the id for [`Host::update_notification`] and
    /// [`Host::withdraw_notification`].
    fn notify(&self, notification: HostNotification) -> NotificationId;
    /// Shows the notifications of the open interactions to a client that attaches.
    /// Returns one id for each, in the same order. A link host sends them as one request, which
    /// its client counts as one notification of its limit.
    fn notify_open(&self, notifications: Vec<HostNotification>) -> Vec<NotificationId> {
        notifications.into_iter().map(|n| self.notify(n)).collect()
    }
    fn update_notification(&self, id: NotificationId, notification: HostNotification);
    fn withdraw_notification(&self, id: NotificationId);
    fn notification_capabilities(&self) -> NotificationCapabilities;
    /// The number on the icon of the app (the projection `attention`).
    fn set_badge(&self, count: u32);
    fn set_tray(&self, tray: TrayModel);
    fn open_window(&self, spec: WindowSpec);
    fn close_window(&self, id: WindowId);
    fn focus_window(&self, id: WindowId);
    /// The `nh` binary that starts holders and that hooks call: the sidecar of the app.
    fn sidecar_path(&self) -> PathBuf;
    fn clipboard_write(&self, text: String);
    /// Opens a URL in the browser of the system. The core has checked the scheme.
    fn open_url(&self, url: String);
    /// Shows the settings window, or brings it to the front.
    fn open_settings(&self);
    /// Shows the settings window at a section, for example `phone`. A host that cannot
    /// choose the section shows the window as [`Host::open_settings`] does.
    fn open_settings_at(&self, _section: &str) {
        self.open_settings();
    }
    /// Asks the person a question of ssh and returns at once. The host calls `done` with the
    /// answer. A host without a panel drops `done`, and the core answers `unsupported`.
    fn ask_secret(&self, _question: SecretQuestion, done: SecretAnswered) {
        drop(done);
    }
    /// Opens a path with an app, or shows it in the file manager. Only a host on the machine of
    /// the core can do it, so the default answer is [`OpenRefused::Unsupported`].
    fn open_path(&self, _open: PathOpen) -> Result<(), OpenRefused> {
        Err(OpenRefused::Unsupported)
    }
}

/// A client device that a link connects to the core: its device id (a ULID made at its
/// first launch) and its name (the host name).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClientInfo {
    pub device: ulid::Ulid,
    pub name: String,
}

/// The host in the process of the core: the app that carries it, with its client
/// device. `nh daemon` has none.
#[derive(Clone, Debug)]
pub struct InProcessHost {
    pub info: ClientInfo,
    pub host: std::sync::Arc<dyn Host>,
}

impl InProcessHost {
    pub fn new(info: ClientInfo, host: std::sync::Arc<dyn Host>) -> Self {
        Self { info, host }
    }

    /// A [`NoopHost`] in the process, with a fixed device. The tools that start a core use it, so
    /// that their windows belong to a client that is always there.
    pub fn noop() -> Self {
        Self::new(
            ClientInfo {
                device: ulid::Ulid(1),
                name: "headless".to_owned(),
            },
            std::sync::Arc::new(NoopHost::default()),
        )
    }
}

/// While it lives, the host router of the core sends the requests of the [`Host`] trait to one
/// link client, with the rules of the router. The server of the link drops it when the link ends.
#[must_use = "the client is detached when the attachment is dropped"]
pub struct ClientAttachment {
    pub(crate) detach: Option<Box<dyn FnOnce() + Send>>,
}

impl std::fmt::Debug for ClientAttachment {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ClientAttachment")
    }
}

impl Drop for ClientAttachment {
    fn drop(&mut self) {
        if let Some(detach) = self.detach.take() {
            detach();
        }
    }
}

/// A host that does nothing. The daemon and most tests use it.
#[derive(Clone, Debug, Default)]
pub struct NoopHost {
    sidecar: PathBuf,
}

impl NoopHost {
    pub fn new(sidecar: PathBuf) -> Self {
        Self { sidecar }
    }
}

impl Host for NoopHost {
    fn notify(&self, _: HostNotification) -> NotificationId {
        new_notification_id().unwrap_or_else(|_| NotificationId::new(ulid::Ulid::nil()))
    }

    fn update_notification(&self, _: NotificationId, _: HostNotification) {}

    fn withdraw_notification(&self, _: NotificationId) {}

    fn notification_capabilities(&self) -> NotificationCapabilities {
        NotificationCapabilities::default()
    }

    fn set_badge(&self, _: u32) {}

    fn set_tray(&self, _: TrayModel) {}

    fn open_window(&self, _: WindowSpec) {}

    fn close_window(&self, _: WindowId) {}

    fn focus_window(&self, _: WindowId) {}

    fn sidecar_path(&self) -> PathBuf {
        self.sidecar.clone()
    }

    fn clipboard_write(&self, _: String) {}

    fn open_url(&self, _: String) {}

    fn open_settings(&self) {}
}

/// What a [`RecordingHost`] was asked to do.
#[derive(Clone, Debug, PartialEq)]
pub enum HostCall {
    Notify(NotificationId, HostNotification),
    UpdateNotification(NotificationId, HostNotification),
    WithdrawNotification(NotificationId),
    SetBadge(u32),
    SetTray(TrayModel),
    OpenWindow(WindowSpec),
    CloseWindow(WindowId),
    FocusWindow(WindowId),
    ClipboardWrite(String),
    OpenUrl(String),
    OpenSettings,
    OpenSettingsAt(String),
    /// A question of ssh. The answer is not written down.
    AskSecret(SecretQuestion),
    OpenPath(PathOpen),
}

/// A host that writes down its calls, for tests of the code that drives a host.
#[derive(Debug, Default)]
pub struct RecordingHost {
    sidecar: PathBuf,
    actions: bool,
    calls: Mutex<Vec<HostCall>>,
    /// What the question panel gives. Without it the host has no panel.
    secret: Mutex<Option<Option<String>>>,
    /// The size of each call of [`Host::notify_open`].
    open_batches: Mutex<Vec<usize>>,
}

impl RecordingHost {
    /// A host whose notifications have action buttons when `actions` is true.
    pub fn new(sidecar: PathBuf, actions: bool) -> Self {
        Self {
            sidecar,
            actions,
            calls: Mutex::default(),
            secret: Mutex::default(),
            open_batches: Mutex::default(),
        }
    }

    /// The number of notifications in each call of [`Host::notify_open`], oldest first. Each
    /// notification of such a call is also a [`HostCall::Notify`].
    pub fn open_batches(&self) -> Vec<usize> {
        self.open_batches
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// Gives the host a question panel that answers `answer` at once: text, or `None` for a
    /// person who cancelled.
    pub fn answer_secret(&self, answer: Option<String>) {
        *self.secret.lock().unwrap_or_else(PoisonError::into_inner) = Some(answer);
    }

    /// The calls so far, oldest first.
    pub fn calls(&self) -> Vec<HostCall> {
        self.calls
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    fn record(&self, call: HostCall) {
        self.calls
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(call);
    }
}

impl Host for RecordingHost {
    fn notify(&self, notification: HostNotification) -> NotificationId {
        let id = new_notification_id().unwrap_or_else(|_| NotificationId::new(ulid::Ulid::nil()));
        self.record(HostCall::Notify(id, notification));
        id
    }

    fn notify_open(&self, notifications: Vec<HostNotification>) -> Vec<NotificationId> {
        self.open_batches
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(notifications.len());
        notifications.into_iter().map(|n| self.notify(n)).collect()
    }

    fn update_notification(&self, id: NotificationId, notification: HostNotification) {
        self.record(HostCall::UpdateNotification(id, notification));
    }

    fn withdraw_notification(&self, id: NotificationId) {
        self.record(HostCall::WithdrawNotification(id));
    }

    fn notification_capabilities(&self) -> NotificationCapabilities {
        NotificationCapabilities {
            actions: self.actions,
        }
    }

    fn set_badge(&self, count: u32) {
        self.record(HostCall::SetBadge(count));
    }

    fn set_tray(&self, tray: TrayModel) {
        self.record(HostCall::SetTray(tray));
    }

    fn open_window(&self, spec: WindowSpec) {
        self.record(HostCall::OpenWindow(spec));
    }

    fn close_window(&self, id: WindowId) {
        self.record(HostCall::CloseWindow(id));
    }

    fn focus_window(&self, id: WindowId) {
        self.record(HostCall::FocusWindow(id));
    }

    fn sidecar_path(&self) -> PathBuf {
        self.sidecar.clone()
    }

    fn clipboard_write(&self, text: String) {
        self.record(HostCall::ClipboardWrite(text));
    }

    fn open_url(&self, url: String) {
        self.record(HostCall::OpenUrl(url));
    }

    fn open_settings(&self) {
        self.record(HostCall::OpenSettings);
    }

    fn open_settings_at(&self, section: &str) {
        self.record(HostCall::OpenSettingsAt(section.to_owned()));
    }

    fn ask_secret(&self, question: SecretQuestion, done: SecretAnswered) {
        self.record(HostCall::AskSecret(question));
        let answer = self
            .secret
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        if let Some(answer) = answer {
            done(answer);
        }
    }

    /// Writes the request down, and opens it.
    fn open_path(&self, open: PathOpen) -> Result<(), OpenRefused> {
        self.record(HostCall::OpenPath(open));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn notification() -> HostNotification {
        HostNotification {
            title: "Claude needs you".to_owned(),
            body: "Allow `rm -rf build`?".to_owned(),
            urgency: Urgency::High,
            interaction: None,
            pane: None,
            actions: vec![NotificationAction::Allow, NotificationAction::Deny],
        }
    }

    #[test]
    fn the_noop_host_does_nothing_and_has_no_actions() {
        let host = NoopHost::new(PathBuf::from("/bin/nh"));
        let id = host.notify(notification());
        host.update_notification(id, notification());
        host.withdraw_notification(id);
        host.set_badge(3);
        assert!(!host.notification_capabilities().actions);
        assert_eq!(host.sidecar_path(), PathBuf::from("/bin/nh"));
    }

    #[test]
    fn the_recording_host_writes_down_what_it_was_asked() {
        let host = RecordingHost::new(PathBuf::from("/bin/nh"), true);
        let id = host.notify(notification());
        host.set_badge(2);
        host.clipboard_write("copied".to_owned());
        host.withdraw_notification(id);
        assert!(host.notification_capabilities().actions);
        assert_eq!(
            host.calls(),
            [
                HostCall::Notify(id, notification()),
                HostCall::SetBadge(2),
                HostCall::ClipboardWrite("copied".to_owned()),
                HostCall::WithdrawNotification(id),
            ]
        );
    }

    #[test]
    fn notification_ids_differ() {
        let host = NoopHost::default();
        assert_ne!(host.notify(notification()), host.notify(notification()));
    }
}
