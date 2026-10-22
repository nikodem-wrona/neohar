//! The host router of the core.
//!
//! A core has at most one host in its process (the app that carries it) and any number of link
//! clients. The router implements [`Host`] for the rest of the core, and it sends each request to
//! the hosts that these rules name:
//!
//! - Notifications go to every host. The core keeps one [`NotificationId`], and the router maps it
//!   to the id that each host gave. An update or a withdraw follows the map. A host whose
//!   notifications have no buttons gets the notification without its actions.
//! - The badge and the tray go to every host.
//! - A window request goes to the host of the client that owns the row of `windows`.
//! - The clipboard and the settings window go to the host of one device, which the caller names.
//! - The folder panel goes to the host of the window that asked. Only the host in the process
//!   shows one. A link client answers `unsupported`.
//! - `sidecar_path` and `open_url` never leave the machine: only the host in the process answers.
//!
//! A client that attaches gets the badge, the tray and the notifications of the open interactions
//! at once. The notifications go in one [`Host::notify_open`]. A host must not call the router
//! from inside a request: the router holds its lock while it sends a notification, a badge or a
//! tray, so that a client that attaches at the same time misses none of them.

use std::{
    collections::{BTreeMap, HashMap},
    path::PathBuf,
    sync::{Arc, Mutex, MutexGuard, PoisonError},
};

use nh_proto::api::ids::{NotificationId, WindowId};
use ulid::Ulid;

use crate::{
    host::{
        ClientInfo, FolderChosen, FolderPick, Host, HostNotification, InProcessHost,
        NotificationCapabilities, OpenRefused, PathOpen, SecretAnswered, SecretQuestion, TrayModel,
        WindowSpec,
    },
    ids::new_notification_id,
    layout::model::Layout,
};

/// A host that the router sends to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
enum Target {
    /// The host in the process of the core.
    Local,
    /// A link client, by the key of its attachment.
    Link(u64),
}

/// A link client that is attached.
struct Link {
    key: u64,
    info: ClientInfo,
    host: Arc<dyn Host>,
    /// The router asks once, when the client attaches.
    caps: NotificationCapabilities,
    /// A paired phone: it has no native window, so a window that a caller without a
    /// device opens never goes to it.
    phone: bool,
}

/// A notification of an interaction, or of a run that waits after its turn, that is on the screen
/// of the hosts.
struct Live {
    /// The last text, with all its actions.
    notification: HostNotification,
    /// The id that each host gave to it.
    ids: Vec<(Target, NotificationId)>,
}

#[derive(Default)]
struct Routes {
    /// The link clients, in the order in which they attached.
    links: Vec<Link>,
    next_key: u64,
    /// The notifications of interactions and of waiting runs that are not withdrawn. A
    /// notification without an interaction or a pane is a note: it is sent once and not kept.
    live: BTreeMap<NotificationId, Live>,
    badge: Option<u32>,
    tray: Option<TrayModel>,
    /// The device of the client that owns each window. `None` for a row with no owner yet.
    owners: HashMap<WindowId, Option<Ulid>>,
}

/// What [`HostRouter::detach`] did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Detached {
    pub info: ClientInfo,
    /// Another attachment of the same device is still there, for example a new link that came
    /// up before the old one ended.
    pub still_connected: bool,
}

/// The host router. The core has one, in `Inner.host` and in the state actor.
pub struct HostRouter {
    local: Option<InProcessHost>,
    /// The `nh` binary of this machine, for a core with no host in its process.
    sidecar: PathBuf,
    routes: Mutex<Routes>,
}

impl std::fmt::Debug for HostRouter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HostRouter")
            .field("local", &self.local.as_ref().map(|l| &l.info))
            .field(
                "links",
                &self.routes.try_lock().map(|routes| routes.links.len()).ok(),
            )
            .finish()
    }
}

/// The notification as one host can show it: without buttons when the host has none.
fn fit(notification: &HostNotification, caps: NotificationCapabilities) -> HostNotification {
    let mut fitted = notification.clone();
    if !caps.actions {
        fitted.actions.clear();
    }
    fitted
}

impl HostRouter {
    /// A router with the host in the process, if there is one, and the `nh` binary of the machine.
    /// `nh daemon` has no host in its process.
    pub fn new(local: Option<InProcessHost>, sidecar: PathBuf) -> Self {
        Self {
            local,
            sidecar,
            routes: Mutex::default(),
        }
    }

    fn lock(&self) -> MutexGuard<'_, Routes> {
        self.routes.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The device of the host in the process.
    pub fn local_device(&self) -> Option<Ulid> {
        self.local.as_ref().map(|local| local.info.device)
    }

    /// The device and the name of the host in the process.
    pub fn local_info(&self) -> Option<&ClientInfo> {
        self.local.as_ref().map(|local| &local.info)
    }

    /// Whether a host of the device is there: the host in the process, or an attached link.
    pub fn is_connected(&self, device: Ulid) -> bool {
        self.local_device() == Some(device)
            || self.lock().links.iter().any(|l| l.info.device == device)
    }

    /// The devices that are there: the host in the process first, then the links in the order in
    /// which they attached. A device is in the list once.
    pub fn connected(&self) -> Vec<ClientInfo> {
        let mut out: Vec<ClientInfo> = self.local_info().cloned().into_iter().collect();
        for link in &self.lock().links {
            if !out.iter().any(|c| c.device == link.info.device) {
                out.push(link.info.clone());
            }
        }
        out
    }

    /// The client that gets a window which a caller without a device opens (the CLI): the host in
    /// the process, else the link client that attached first. `None` when no client is there; the
    /// next client that attaches adopts the row.
    pub fn default_device(&self) -> Option<Ulid> {
        self.local_device().or_else(|| {
            self.lock()
                .links
                .iter()
                .find(|l| !l.phone)
                .map(|l| l.info.device)
        })
    }

    /// The owner of a window row. `None` when the layout has no such row. `Some(None)` is a row
    /// with no owner.
    pub(crate) fn owner_of(&self, window: WindowId) -> Option<Option<Ulid>> {
        self.lock().owners.get(&window).copied()
    }

    /// Follows the owners of the rows of `windows`. The state actor calls it after each change
    /// of the layout.
    pub(crate) fn set_owners(&self, layout: &Layout) {
        self.lock().owners = layout.windows.iter().map(|w| (w.id, w.client)).collect();
    }

    /// The hosts that a request for every client goes to, with what their notifications can do.
    fn everyone(&self, routes: &Routes) -> Vec<(Target, Arc<dyn Host>, NotificationCapabilities)> {
        let mut out = Vec::with_capacity(routes.links.len() + 1);
        if let Some(local) = &self.local {
            out.push((
                Target::Local,
                Arc::clone(&local.host),
                local.host.notification_capabilities(),
            ));
        }
        for link in &routes.links {
            out.push((Target::Link(link.key), Arc::clone(&link.host), link.caps));
        }
        out
    }

    fn target_host(
        &self,
        routes: &Routes,
        target: Target,
    ) -> Option<(Arc<dyn Host>, NotificationCapabilities)> {
        match target {
            Target::Local => self
                .local
                .as_ref()
                .map(|l| (Arc::clone(&l.host), l.host.notification_capabilities())),
            Target::Link(key) => routes
                .links
                .iter()
                .find(|l| l.key == key)
                .map(|l| (Arc::clone(&l.host), l.caps)),
        }
    }

    /// The host of a device. `None` names the host in the process, for a caller with no device
    /// (the CLI). The newest link of the device wins when it has two.
    fn host_of(&self, device: Option<Ulid>) -> Option<Arc<dyn Host>> {
        let local = self.local.as_ref();
        match device {
            None => local.map(|l| Arc::clone(&l.host)),
            Some(device) if local.is_some_and(|l| l.info.device == device) => {
                local.map(|l| Arc::clone(&l.host))
            }
            Some(device) => self
                .lock()
                .links
                .iter()
                .rev()
                .find(|l| l.info.device == device)
                .map(|l| Arc::clone(&l.host)),
        }
    }

    /// The host of the client that owns a window. A row with no owner belongs to the host in the
    /// process.
    fn host_of_window(&self, window: WindowId) -> Option<Arc<dyn Host>> {
        let owner = self.lock().owners.get(&window).copied().flatten();
        self.host_of(owner)
    }

    /// Attaches a link client: it gets the badge, the tray and the notifications of the open
    /// interactions now. Returns the key for [`HostRouter::detach`].
    pub(crate) fn attach(&self, info: ClientInfo, host: Arc<dyn Host>) -> u64 {
        self.attach_as(info, host, false)
    }

    /// Attaches a client: a link client, or a paired phone.
    pub(crate) fn attach_as(&self, info: ClientInfo, host: Arc<dyn Host>, phone: bool) -> u64 {
        let caps = host.notification_capabilities();
        let mut routes = self.lock();
        let key = routes.next_key;
        routes.next_key += 1;
        if let Some(badge) = routes.badge {
            host.set_badge(badge);
        }
        if let Some(tray) = &routes.tray {
            host.set_tray(tray.clone());
        }
        // One request for all of them: the client counts it once in its limit of notifications,
        // so that it shows every open interaction and the new ones after it.
        let open: Vec<HostNotification> = routes
            .live
            .values()
            .map(|live| fit(&live.notification, caps))
            .collect();
        if !open.is_empty() {
            let ids = host.notify_open(open);
            for (live, id) in routes.live.values_mut().zip(ids) {
                live.ids.push((Target::Link(key), id));
            }
        }
        routes.links.push(Link {
            key,
            info,
            host,
            caps,
            phone,
        });
        key
    }

    /// Detaches a link client. Nothing goes to it from now on.
    pub(crate) fn detach(&self, key: u64) -> Option<Detached> {
        let mut routes = self.lock();
        let index = routes.links.iter().position(|l| l.key == key)?;
        let link = routes.links.remove(index);
        for live in routes.live.values_mut() {
            live.ids.retain(|(target, _)| *target != Target::Link(key));
        }
        let still_connected = self.local_device() == Some(link.info.device)
            || routes
                .links
                .iter()
                .any(|l| l.info.device == link.info.device);
        Some(Detached {
            info: link.info,
            still_connected,
        })
    }

    /// Writes the clipboard of one client: for `pane.copy` the client that called, for
    /// an OSC 52 write the client that sent the last input to the pane. `None` is the host in the
    /// process. Returns false when that client is not there, or is a phone, and the text is
    /// dropped: a phone writes its clipboard only in a tap of the person.
    pub fn clipboard_write_for(&self, device: Option<Ulid>, text: String) -> bool {
        if self.is_phone(device) {
            return false;
        }
        match self.host_of(device) {
            Some(host) => {
                host.clipboard_write(text);
                true
            }
            None => false,
        }
    }

    /// Whether a device is a paired phone that is connected. A phone writes its own
    /// clipboard in a tap of the person, so its host cannot write it.
    pub fn is_phone(&self, device: Option<Ulid>) -> bool {
        device.is_some_and(|device| {
            self.lock()
                .links
                .iter()
                .any(|l| l.phone && l.info.device == device)
        })
    }

    /// Shows the settings window on the client that asked. `None` is the host in the
    /// process.
    pub fn open_settings_for(&self, device: Option<Ulid>) {
        if let Some(host) = self.host_of(device) {
            host.open_settings();
        }
    }

    /// Shows the settings window at a section on the client that asked.
    pub fn open_settings_at_for(&self, device: Option<Ulid>, section: &str) {
        if let Some(host) = self.host_of(device) {
            host.open_settings_at(section);
        }
    }

    /// Shows a note on one client only, for example the answer to a late click. The
    /// router does not keep it.
    pub fn notify_for(
        &self,
        device: Option<Ulid>,
        notification: HostNotification,
    ) -> NotificationId {
        let id = new_notification_id().unwrap_or_else(|_| NotificationId::new(Ulid::nil()));
        let target = match device {
            Some(device) if self.local_device() != Some(device) => {
                let routes = self.lock();
                routes
                    .links
                    .iter()
                    .rev()
                    .find(|l| l.info.device == device)
                    .map(|l| (Arc::clone(&l.host), l.caps))
            }
            _ => self
                .local
                .as_ref()
                .map(|l| (Arc::clone(&l.host), l.host.notification_capabilities())),
        };
        if let Some((host, caps)) = target {
            host.notify(fit(&notification, caps));
        }
        id
    }
}

impl Host for HostRouter {
    fn notify(&self, notification: HostNotification) -> NotificationId {
        let id = new_notification_id().unwrap_or_else(|_| NotificationId::new(Ulid::nil()));
        let mut routes = self.lock();
        let ids: Vec<(Target, NotificationId)> = self
            .everyone(&routes)
            .into_iter()
            .map(|(target, host, caps)| (target, host.notify(fit(&notification, caps))))
            .collect();
        // A notification of an interaction or of a run that waits after its turn stays
        // until the core withdraws it.
        if notification.interaction.is_some() || notification.pane.is_some() {
            routes.live.insert(id, Live { notification, ids });
        }
        id
    }

    fn update_notification(&self, id: NotificationId, notification: HostNotification) {
        let mut routes = self.lock();
        let Some(ids) = routes.live.get(&id).map(|live| live.ids.clone()) else {
            return;
        };
        for (target, target_id) in ids {
            if let Some((host, caps)) = self.target_host(&routes, target) {
                host.update_notification(target_id, fit(&notification, caps));
            }
        }
        if let Some(live) = routes.live.get_mut(&id) {
            live.notification = notification;
        }
    }

    fn withdraw_notification(&self, id: NotificationId) {
        let mut routes = self.lock();
        let Some(live) = routes.live.remove(&id) else {
            return;
        };
        for (target, target_id) in live.ids {
            if let Some((host, _)) = self.target_host(&routes, target) {
                host.withdraw_notification(target_id);
            }
        }
    }

    /// The core always writes a notification with its actions, and the router takes them away for
    /// each host whose notifications have no buttons. So a client that attaches later
    /// gets the buttons even when the clients before it had none.
    fn notification_capabilities(&self) -> NotificationCapabilities {
        NotificationCapabilities { actions: true }
    }

    fn set_badge(&self, count: u32) {
        let mut routes = self.lock();
        routes.badge = Some(count);
        for (_, host, _) in self.everyone(&routes) {
            host.set_badge(count);
        }
    }

    fn set_tray(&self, tray: TrayModel) {
        let mut routes = self.lock();
        routes.tray = Some(tray.clone());
        for (_, host, _) in self.everyone(&routes) {
            host.set_tray(tray.clone());
        }
    }

    fn open_window(&self, spec: WindowSpec) {
        if let Some(host) = self.host_of_window(spec.id) {
            host.open_window(spec);
        }
    }

    fn close_window(&self, id: WindowId) {
        if let Some(host) = self.host_of_window(id) {
            host.close_window(id);
        }
    }

    fn focus_window(&self, id: WindowId) {
        if let Some(host) = self.host_of_window(id) {
            host.focus_window(id);
        }
    }

    /// The `nh` binary of the machine of the core. It is never asked over a link.
    fn sidecar_path(&self) -> PathBuf {
        self.local
            .as_ref()
            .map_or_else(|| self.sidecar.clone(), |l| l.host.sidecar_path())
    }

    fn clipboard_write(&self, text: String) {
        self.clipboard_write_for(None, text);
    }

    /// Only the host in the process opens a URL: a remote core opens nothing on a client.
    fn open_url(&self, url: String) {
        if let Some(local) = &self.local {
            local.host.open_url(url);
        }
    }

    fn open_settings(&self) {
        self.open_settings_for(None);
    }

    fn open_settings_at(&self, section: &str) {
        self.open_settings_at_for(None, section);
    }

    /// Only the host in the process opens a path, and only for one of its windows: the path is a
    /// path of the machine of the core, and a link client is on another machine.
    fn open_path(&self, open: PathOpen) -> Result<(), OpenRefused> {
        let owner = self.lock().owners.get(&open.window).copied().flatten();
        match &self.local {
            Some(local) if owner.is_none_or(|device| device == local.info.device) => {
                local.host.open_path(open)
            }
            _ => Err(OpenRefused::Unsupported),
        }
    }

    /// The host of the window that asked. A link client drops `done`, and the core answers
    /// `unsupported`.
    fn pick_folder(&self, pick: FolderPick, done: FolderChosen) {
        if let Some(host) = self.host_of_window(pick.window) {
            host.pick_folder(pick, done);
        }
    }

    /// Only the host in the process asks the person: ssh runs on this Mac, and a link client has
    /// no question of ssh to answer for it. A core without that host drops `done`.
    fn ask_secret(&self, question: SecretQuestion, done: SecretAnswered) {
        if let Some(local) = &self.local {
            local.host.ask_secret(question, done);
        }
    }
}

#[cfg(test)]
mod tests;
