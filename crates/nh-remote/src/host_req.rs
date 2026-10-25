//! Host requests over a link.
//!
//! The core sends a [`HostRequest`] for each call of the [`Host`] trait that its router gives to a
//! link client. The enum has one variant for each host request that goes over a link.
//! `sidecar_path` and `open_url` are never sent, so they have no variant. No variant names a path
//! of the client.
//!
//! The client does not trust a remote core with its machine. [`HostGuard`] checks each request
//! against the [`HostLimits`] before it calls the host of the client, and refuses the rest.

use std::{
    collections::{HashMap, HashSet, VecDeque},
    sync::{
        Arc, Mutex, PoisonError,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use nh_core::{
    Host,
    host::{HostNotification, NotificationAction, TrayEntry, TrayModel, WindowSpec},
};
use nh_proto::api::{
    common::{Bounds, Urgency},
    ids::{InteractionId, NotificationId, PaneId, TabId, WindowId},
};
use serde::{Deserialize, Serialize};

use crate::msg::HostOutcome;

/// A request of the [`Host`] trait, from a core to a link client.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum HostRequest {
    /// `id` is the id that the core gave the notification. The client keeps its own id for it.
    Notify {
        id: String,
        notification: Notification,
    },
    UpdateNotification {
        id: String,
        notification: Notification,
    },
    WithdrawNotification {
        id: String,
    },
    /// The core asks once, after `Welcome`, and keeps the answer.
    NotificationCapabilities,
    SetBadge {
        count: u32,
    },
    SetTray {
        tray: Tray,
    },
    OpenWindow {
        window: Window,
    },
    CloseWindow {
        window: String,
    },
    FocusWindow {
        window: String,
    },
    ClipboardWrite {
        text: String,
    },
    OpenSettings,
    /// The notifications of the open interactions, for a client that attaches: the id
    /// of the core and the notification. The client takes it once, as the first notification
    /// request of the link, and counts it as one notification of its limit.
    NotifyOpen {
        notifications: Vec<(String, Notification)>,
    },
}

/// The urgency of a notification.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Level {
    Low,
    Normal,
    High,
}

/// An action of a notification: allow or deny, and no other.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Action {
    Allow,
    Deny,
}

/// A notification: text only.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Notification {
    pub title: String,
    pub body: String,
    pub urgency: Level,
    pub interaction: Option<String>,
    pub actions: Vec<Action>,
    /// The pane that a notification without an interaction is about: a run whose turn ended.
    pub pane: Option<String>,
}

/// A line of the menu of the tray.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrayLine {
    pub label: String,
    pub interaction: Option<String>,
    pub approval: bool,
    /// The core offers Allow. A core of an earlier version sends no field, and then the tray
    /// offers only Deny and Open.
    #[serde(default)]
    pub allow: bool,
    /// The rule of "Allow always", when the core offers it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub always: Option<String>,
}

/// The menu of the tray.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tray {
    pub entries: Vec<TrayLine>,
    pub badge: u32,
}

/// The bounds of a window, in logical pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

/// A window that the core asks the client to open.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Window {
    pub id: String,
    pub bounds: Option<Rect>,
    pub panes: Vec<String>,
    pub tab: Option<String>,
}

impl From<&HostNotification> for Notification {
    fn from(n: &HostNotification) -> Self {
        Self {
            title: n.title.clone(),
            body: n.body.clone(),
            urgency: match n.urgency {
                Urgency::Low => Level::Low,
                Urgency::Normal => Level::Normal,
                Urgency::High => Level::High,
            },
            interaction: n.interaction.map(|i| i.to_string()),
            pane: n.pane.map(|p| p.to_string()),
            actions: n
                .actions
                .iter()
                .map(|a| match a {
                    NotificationAction::Allow => Action::Allow,
                    NotificationAction::Deny => Action::Deny,
                })
                .collect(),
        }
    }
}

impl From<&TrayModel> for Tray {
    fn from(tray: &TrayModel) -> Self {
        Self {
            entries: tray
                .entries
                .iter()
                .map(|e| TrayLine {
                    label: e.label.clone(),
                    interaction: e.interaction.map(|i| i.to_string()),
                    approval: e.approval,
                    allow: e.allow,
                    always: e.always.clone(),
                })
                .collect(),
            badge: tray.badge,
        }
    }
}

impl From<&WindowSpec> for Window {
    fn from(spec: &WindowSpec) -> Self {
        Self {
            id: spec.id.to_string(),
            bounds: spec.bounds.map(|b| Rect {
                x: b.x,
                y: b.y,
                width: b.width,
                height: b.height,
            }),
            panes: spec.panes.iter().map(ToString::to_string).collect(),
            tab: spec.tab.map(|t| t.to_string()),
        }
    }
}

/// The limits that a client puts on the requests of a remote core.
#[derive(Clone, Copy, Debug)]
pub struct HostLimits {
    /// The most characters of the title of a notification.
    pub title_chars: usize,
    /// The most characters of the body of a notification.
    pub body_chars: usize,
    /// The most notifications that one machine posts in [`HostLimits::notify_window`]. An update
    /// counts too, because the host posts the notification again.
    pub notifications: usize,
    pub notify_window: Duration,
    /// The most notifications of open interactions that one `NotifyOpen` shows.
    pub open_notifications: usize,
    /// The most bytes of text of one clipboard write.
    pub clipboard_bytes: usize,
}

impl Default for HostLimits {
    fn default() -> Self {
        Self {
            title_chars: 200,
            body_chars: 1_000,
            notifications: 10,
            notify_window: Duration::from_secs(10),
            open_notifications: 64,
            clipboard_bytes: 1 << 20,
        }
    }
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The longest id of a notification of the core.
const MAX_ID: usize = 64;

fn refused(why: impl Into<String>) -> HostOutcome {
    HostOutcome::Refused(why.into())
}

/// The side of the client: it checks each request of the core and calls the host of the client.
pub(crate) struct HostGuard {
    host: Arc<dyn Host>,
    limits: HostLimits,
    /// The windows that this client owns. The client host keeps the set.
    owned: Mutex<HashSet<WindowId>>,
    /// When the last notifications came.
    recent: Mutex<VecDeque<Instant>>,
    /// The notifications of interactions that the host of the client shows.
    shown: Mutex<Shown>,
    /// A `Notify` or a `NotifyOpen` came: a `NotifyOpen` is no longer taken.
    notified: AtomicBool,
}

/// The most notifications of the core that the client keeps ids for.
const MAX_SHOWN: usize = 1_024;

/// The id of the host of the client for each notification of an interaction that the core sent.
/// The core updates and withdraws only those, so a note without an interaction is not
/// kept. When the map is full, the oldest entry goes.
#[derive(Default)]
struct Shown {
    ids: HashMap<String, (NotificationId, u64)>,
    next: u64,
}

impl Shown {
    fn insert(&mut self, id: String, local: NotificationId) {
        if self.ids.len() >= MAX_SHOWN
            && !self.ids.contains_key(&id)
            && let Some(oldest) = self
                .ids
                .iter()
                .min_by_key(|(_, (_, at))| *at)
                .map(|(id, _)| id.clone())
        {
            self.ids.remove(&oldest);
        }
        self.next += 1;
        self.ids.insert(id, (local, self.next));
    }

    fn get(&self, id: &str) -> Option<NotificationId> {
        self.ids.get(id).map(|(local, _)| *local)
    }

    fn remove(&mut self, id: &str) -> Option<NotificationId> {
        self.ids.remove(id).map(|(local, _)| local)
    }
}

impl HostGuard {
    pub fn new(host: Arc<dyn Host>, limits: HostLimits) -> Self {
        Self {
            host,
            limits,
            owned: Mutex::default(),
            recent: Mutex::default(),
            shown: Mutex::default(),
            notified: AtomicBool::new(false),
        }
    }

    pub fn set_owned(&self, windows: impl IntoIterator<Item = WindowId>) {
        *lock(&self.owned) = windows.into_iter().collect();
    }

    pub fn own(&self, window: WindowId, owned: bool) {
        let mut set = lock(&self.owned);
        if owned {
            set.insert(window);
        } else {
            set.remove(&window);
        }
    }

    pub fn host(&self) -> &Arc<dyn Host> {
        &self.host
    }

    /// Answers one request of the core. `request` is its `postcard` body.
    pub fn handle(&self, request: &[u8]) -> HostOutcome {
        let Ok(request) = postcard::from_bytes::<HostRequest>(request) else {
            return refused("this client does not take this request");
        };
        let outcome = self.run(request);
        if let HostOutcome::Refused(why) = &outcome {
            tracing::warn!(why, "a host request of a remote core was refused");
        }
        outcome
    }

    fn run(&self, request: HostRequest) -> HostOutcome {
        match request {
            HostRequest::Notify { id, notification } => {
                let notification = match self.notification(notification) {
                    Ok(n) => n,
                    Err(why) => return refused(why),
                };
                if id.len() > MAX_ID {
                    return refused("the id of the notification is too long");
                }
                if !self.take_notification_slot() {
                    return refused(self.over_the_limit());
                }
                self.notified.store(true, Ordering::SeqCst);
                // The core can change or withdraw a notification of an interaction or of a pane.
                let tracked = notification.interaction.is_some() || notification.pane.is_some();
                let local = self.host.notify(notification);
                if tracked {
                    lock(&self.shown).insert(id, local);
                }
                HostOutcome::Done
            }
            HostRequest::UpdateNotification { id, notification } => {
                let notification = match self.notification(notification) {
                    Ok(n) => n,
                    Err(why) => return refused(why),
                };
                let Some(local) = lock(&self.shown).get(&id) else {
                    return refused("no such notification");
                };
                // The host posts the notification again, with its sound: an update takes a slot
                // as a new notification does.
                if !self.take_notification_slot() {
                    return refused(self.over_the_limit());
                }
                self.host.update_notification(local, notification);
                HostOutcome::Done
            }
            HostRequest::NotifyOpen { notifications } => self.notify_open(notifications),
            HostRequest::WithdrawNotification { id } => {
                let Some(local) = lock(&self.shown).remove(&id) else {
                    return refused("no such notification");
                };
                self.host.withdraw_notification(local);
                HostOutcome::Done
            }
            HostRequest::NotificationCapabilities => HostOutcome::Capabilities {
                actions: self.host.notification_capabilities().actions,
            },
            HostRequest::SetBadge { count } => {
                self.host.set_badge(count);
                HostOutcome::Done
            }
            HostRequest::SetTray { tray } => match tray_model(tray) {
                Ok(tray) => {
                    self.host.set_tray(tray);
                    HostOutcome::Done
                }
                Err(why) => refused(why),
            },
            HostRequest::OpenWindow { window } => match self.window_spec(window) {
                Ok(spec) => {
                    self.host.open_window(spec);
                    HostOutcome::Done
                }
                Err(why) => refused(why),
            },
            HostRequest::CloseWindow { window } => match self.owned_window(&window) {
                Ok(id) => {
                    self.host.close_window(id);
                    HostOutcome::Done
                }
                Err(why) => refused(why),
            },
            HostRequest::FocusWindow { window } => match self.owned_window(&window) {
                Ok(id) => {
                    self.host.focus_window(id);
                    HostOutcome::Done
                }
                Err(why) => refused(why),
            },
            HostRequest::ClipboardWrite { text } => {
                if text.len() > self.limits.clipboard_bytes {
                    return refused(format!(
                        "a clipboard write has at most {} bytes",
                        self.limits.clipboard_bytes
                    ));
                }
                self.host.clipboard_write(text);
                HostOutcome::Done
            }
            HostRequest::OpenSettings => {
                self.host.open_settings();
                HostOutcome::Done
            }
        }
    }

    /// The notifications of the open interactions at the attach: one request, which counts as one
    /// notification. Only the first notification request of a link can be one, so a core cannot
    /// use it to get around the limit. It shows at most [`HostLimits::open_notifications`], each
    /// one of an interaction and within the limits of the text.
    fn notify_open(&self, notifications: Vec<(String, Notification)>) -> HostOutcome {
        if self.notified.swap(true, Ordering::SeqCst) {
            return refused("the notifications of the open interactions come once, first");
        }
        if !self.take_notification_slot() {
            return refused(self.over_the_limit());
        }
        let mut skipped = notifications
            .len()
            .saturating_sub(self.limits.open_notifications);
        for (id, notification) in notifications
            .into_iter()
            .take(self.limits.open_notifications)
        {
            let notification = match self.notification(notification) {
                Ok(n) if (n.interaction.is_some() || n.pane.is_some()) && id.len() <= MAX_ID => n,
                _ => {
                    skipped += 1;
                    continue;
                }
            };
            let local = self.host.notify(notification);
            lock(&self.shown).insert(id, local);
        }
        if skipped > 0 {
            tracing::warn!(
                skipped,
                "a remote core sent notifications of open interactions that the client does not show"
            );
        }
        HostOutcome::Done
    }

    fn over_the_limit(&self) -> String {
        format!(
            "more than {} notifications in {} s",
            self.limits.notifications,
            self.limits.notify_window.as_secs()
        )
    }

    /// Takes one of the notifications of the window of time, if one is free.
    fn take_notification_slot(&self) -> bool {
        let now = Instant::now();
        let mut recent = lock(&self.recent);
        while recent
            .front()
            .is_some_and(|at| now.duration_since(*at) >= self.limits.notify_window)
        {
            recent.pop_front();
        }
        if recent.len() >= self.limits.notifications {
            return false;
        }
        recent.push_back(now);
        true
    }

    fn notification(&self, n: Notification) -> Result<HostNotification, String> {
        if n.title.chars().count() > self.limits.title_chars {
            return Err(format!(
                "a title has at most {} characters",
                self.limits.title_chars
            ));
        }
        if n.body.chars().count() > self.limits.body_chars {
            return Err(format!(
                "a body has at most {} characters",
                self.limits.body_chars
            ));
        }
        let interaction = n
            .interaction
            .map(|i| i.parse::<InteractionId>())
            .transpose()
            .map_err(|e| e.to_string())?;
        let pane = n
            .pane
            .map(|p| p.parse::<PaneId>())
            .transpose()
            .map_err(|e| e.to_string())?;
        Ok(HostNotification {
            title: n.title,
            body: n.body,
            urgency: match n.urgency {
                Level::Low => Urgency::Low,
                Level::Normal => Urgency::Normal,
                Level::High => Urgency::High,
            },
            interaction,
            pane,
            actions: n
                .actions
                .into_iter()
                .map(|a| match a {
                    Action::Allow => NotificationAction::Allow,
                    Action::Deny => NotificationAction::Deny,
                })
                .collect(),
        })
    }

    fn owned_window(&self, window: &str) -> Result<WindowId, String> {
        let id = window.parse::<WindowId>().map_err(|e| e.to_string())?;
        if lock(&self.owned).contains(&id) {
            Ok(id)
        } else {
            Err(format!("this client does not own the window {id}"))
        }
    }

    fn window_spec(&self, window: Window) -> Result<WindowSpec, String> {
        let id = self.owned_window(&window.id)?;
        let panes = window
            .panes
            .iter()
            .map(|p| p.parse::<PaneId>())
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        let tab = window
            .tab
            .map(|t| t.parse::<TabId>())
            .transpose()
            .map_err(|e| e.to_string())?;
        Ok(WindowSpec {
            id,
            bounds: window.bounds.map(|b| Bounds {
                x: b.x,
                y: b.y,
                width: b.width,
                height: b.height,
            }),
            panes,
            tab,
        })
    }
}

fn tray_model(tray: Tray) -> Result<TrayModel, String> {
    let entries = tray
        .entries
        .into_iter()
        .map(|e| {
            Ok(TrayEntry {
                label: e.label,
                interaction: e
                    .interaction
                    .map(|i| i.parse::<InteractionId>())
                    .transpose()
                    .map_err(|e| e.to_string())?,
                approval: e.approval,
                allow: e.allow,
                always: e.always,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(TrayModel {
        entries,
        badge: tray.badge,
    })
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use nh_core::{RecordingHost, host::HostCall};
    use ulid::Ulid;

    use super::*;

    fn guard() -> (HostGuard, Arc<RecordingHost>) {
        let host = Arc::new(RecordingHost::new(PathBuf::from("/bin/nh"), true));
        (HostGuard::new(host.clone(), HostLimits::default()), host)
    }

    fn bytes(request: &HostRequest) -> Vec<u8> {
        postcard::to_allocvec(request).expect("encodes")
    }

    fn note(title: &str, body: &str) -> Notification {
        Notification {
            title: title.to_owned(),
            body: body.to_owned(),
            urgency: Level::High,
            interaction: None,
            actions: vec![Action::Allow, Action::Deny],
            pane: None,
        }
    }

    /// A notification of an interaction: an approval.
    fn approval(n: u128) -> Notification {
        Notification {
            interaction: Some(InteractionId::new(Ulid::from(n)).to_string()),
            ..note("Claude Code asks", "Run `pnpm test`")
        }
    }

    fn notify(id: &str, notification: Notification) -> Vec<u8> {
        bytes(&HostRequest::Notify {
            id: id.to_owned(),
            notification,
        })
    }

    fn withdraw(id: &str) -> Vec<u8> {
        bytes(&HostRequest::WithdrawNotification { id: id.to_owned() })
    }

    fn notified(host: &RecordingHost) -> usize {
        host.calls()
            .iter()
            .filter(|c| matches!(c, HostCall::Notify(..)))
            .count()
    }

    #[test]
    fn a_notification_within_the_limits_reaches_the_host() {
        let (guard, host) = guard();
        let title = "t".repeat(200);
        let body = "é".repeat(1_000);
        assert_eq!(
            guard.handle(&bytes(&HostRequest::Notify {
                id: "n1".into(),
                notification: Notification {
                    interaction: approval(1).interaction,
                    ..note(&title, &body)
                },
            })),
            HostOutcome::Done
        );
        assert!(matches!(&host.calls()[..], [HostCall::Notify(_, n)] if n.title == title));
        assert_eq!(
            guard.handle(&bytes(&HostRequest::WithdrawNotification {
                id: "n1".into()
            })),
            HostOutcome::Done
        );
        // The id is gone now.
        assert!(matches!(
            guard.handle(&bytes(&HostRequest::WithdrawNotification {
                id: "n1".into()
            })),
            HostOutcome::Refused(_)
        ));
    }

    #[test]
    fn text_over_the_limits_is_refused() {
        let (guard, host) = guard();
        for notification in [note(&"t".repeat(201), "b"), note("t", &"b".repeat(1_001))] {
            assert!(matches!(
                guard.handle(&bytes(&HostRequest::Notify {
                    id: "n".into(),
                    notification,
                })),
                HostOutcome::Refused(_)
            ));
        }
        assert!(matches!(
            guard.handle(&bytes(&HostRequest::ClipboardWrite {
                text: "x".repeat((1 << 20) + 1),
            })),
            HostOutcome::Refused(_)
        ));
        assert!(host.calls().is_empty());
    }

    #[test]
    fn at_most_ten_notifications_in_ten_seconds() {
        let (guard, host) = guard();
        let outcomes: Vec<HostOutcome> = (0..12)
            .map(|n| {
                guard.handle(&bytes(&HostRequest::Notify {
                    id: format!("n{n}"),
                    notification: note("t", "b"),
                }))
            })
            .collect();
        assert!(outcomes[..10].iter().all(|o| *o == HostOutcome::Done));
        assert!(
            outcomes[10..]
                .iter()
                .all(|o| matches!(o, HostOutcome::Refused(_)))
        );
        assert_eq!(host.calls().len(), 10);
    }

    #[test]
    fn an_update_takes_a_slot_of_the_limit() {
        let (guard, host) = guard();
        assert_eq!(guard.handle(&notify("n1", approval(1))), HostOutcome::Done);
        let update = bytes(&HostRequest::UpdateNotification {
            id: "n1".into(),
            notification: approval(1),
        });
        let outcomes: Vec<HostOutcome> = (0..10).map(|_| guard.handle(&update)).collect();
        // The notification and 9 updates are the 10 posts of the window.
        assert!(outcomes[..9].iter().all(|o| *o == HostOutcome::Done));
        assert!(matches!(outcomes[9], HostOutcome::Refused(_)));
        assert!(matches!(
            guard.handle(&notify("n2", approval(2))),
            HostOutcome::Refused(_)
        ));
        let updates = host
            .calls()
            .iter()
            .filter(|c| matches!(c, HostCall::UpdateNotification(..)))
            .count();
        assert_eq!(updates, 9);
    }

    #[test]
    fn a_note_without_an_interaction_is_not_kept_and_the_oldest_entry_goes_first() {
        let host = Arc::new(RecordingHost::new(PathBuf::from("/bin/nh"), true));
        let limits = HostLimits {
            notifications: usize::MAX,
            ..HostLimits::default()
        };
        let guard = HostGuard::new(host.clone(), limits);
        // The router never withdraws a note, so the client keeps no id for it.
        assert_eq!(
            guard.handle(&notify("note", note("build", "done"))),
            HostOutcome::Done
        );
        assert!(matches!(
            guard.handle(&withdraw("note")),
            HostOutcome::Refused(_)
        ));
        // Many notes do not push out the id of an open approval.
        assert_eq!(guard.handle(&notify("a0", approval(0))), HostOutcome::Done);
        for n in 0..2 * MAX_SHOWN {
            guard.handle(&notify(&format!("note{n}"), note("build", "done")));
        }
        // A full map loses its oldest entry, and only that one.
        for n in 1..=MAX_SHOWN {
            guard.handle(&notify(&format!("a{n}"), approval(n as u128)));
        }
        assert!(matches!(
            guard.handle(&withdraw("a0")),
            HostOutcome::Refused(_)
        ));
        assert_eq!(guard.handle(&withdraw("a1")), HostOutcome::Done);
        assert_eq!(
            guard.handle(&withdraw(&format!("a{MAX_SHOWN}"))),
            HostOutcome::Done
        );
    }

    #[test]
    fn the_open_notifications_count_once_and_come_only_first() {
        let (first, host) = guard();
        let open = |range: std::ops::Range<u128>| {
            bytes(&HostRequest::NotifyOpen {
                notifications: range.map(|n| (format!("o{n}"), approval(n))).collect(),
            })
        };
        assert_eq!(first.handle(&open(0..12)), HostOutcome::Done);
        assert_eq!(notified(&host), 12);
        assert_eq!(first.handle(&withdraw("o11")), HostOutcome::Done);
        // The request took one slot: 9 new approvals still fit, and the 10th does not.
        let outcomes: Vec<HostOutcome> = (20..30)
            .map(|n| first.handle(&notify(&format!("n{n}"), approval(n))))
            .collect();
        assert!(outcomes[..9].iter().all(|o| *o == HostOutcome::Done));
        assert!(matches!(outcomes[9], HostOutcome::Refused(_)));
        // A second one is refused: it would get around the limit.
        assert!(matches!(
            first.handle(&open(40..42)),
            HostOutcome::Refused(_)
        ));
        assert_eq!(notified(&host), 21);

        // After a notification, the request is refused too.
        let (late, host) = guard_with_one_note();
        assert!(matches!(late.handle(&open(0..2)), HostOutcome::Refused(_)));
        assert_eq!(notified(&host), 1);

        // At most 64, and only notifications of interactions.
        let (many, host) = guard();
        let mut notifications: Vec<(String, Notification)> =
            (0..100).map(|n| (format!("o{n}"), approval(n))).collect();
        notifications[0].1.interaction = None;
        assert_eq!(
            many.handle(&bytes(&HostRequest::NotifyOpen { notifications })),
            HostOutcome::Done
        );
        assert_eq!(notified(&host), 63);
    }

    fn guard_with_one_note() -> (HostGuard, Arc<RecordingHost>) {
        let (guard, host) = guard();
        assert_eq!(
            guard.handle(&notify("n", note("build", "done"))),
            HostOutcome::Done
        );
        (guard, host)
    }

    #[test]
    fn a_window_request_needs_a_window_that_this_client_owns() {
        let (guard, host) = guard();
        let mine = WindowId::new(Ulid::from(7u128));
        let other = WindowId::new(Ulid::from(8u128));
        guard.set_owned([mine]);
        assert_eq!(
            guard.handle(&bytes(&HostRequest::FocusWindow {
                window: mine.to_string()
            })),
            HostOutcome::Done
        );
        assert!(matches!(
            guard.handle(&bytes(&HostRequest::CloseWindow {
                window: other.to_string()
            })),
            HostOutcome::Refused(_)
        ));
        assert!(matches!(
            guard.handle(&bytes(&HostRequest::OpenWindow {
                window: Window {
                    id: other.to_string(),
                    bounds: None,
                    panes: Vec::new(),
                    tab: None,
                },
            })),
            HostOutcome::Refused(_)
        ));
        assert_eq!(host.calls(), [HostCall::FocusWindow(mine)]);
    }

    #[test]
    fn a_request_that_the_client_does_not_know_is_refused() {
        let (guard, host) = guard();
        // The kind 200 is no variant: for example an `open_url` of a hostile core.
        let mut unknown = vec![200u8];
        unknown.extend(postcard::to_allocvec("https://example.com").expect("encodes"));
        assert!(matches!(guard.handle(&unknown), HostOutcome::Refused(_)));
        assert!(host.calls().is_empty());
    }

    #[test]
    fn the_capabilities_are_the_ones_of_the_host() {
        let (guard, _) = guard();
        assert_eq!(
            guard.handle(&bytes(&HostRequest::NotificationCapabilities)),
            HostOutcome::Capabilities { actions: true }
        );
    }
}
