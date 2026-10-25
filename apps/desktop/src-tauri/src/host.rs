//! The host of the core in the app: what the core asks of the app that carries it.
//!
//! The core never touches a window, the clipboard or the system. It asks a [`Host`]. This one
//! writes to the clipboard, opens addresses with the system, shows notifications with buttons,
//! sets the badge on the dock and fills the menu of the tray. The windows are not here: the app
//! opens a window for each row of the `windows` projection ([`crate::windows`]), so a window that
//! the CLI opens appears the same way as one from the menu.
//!
//! The core decides what a notification says and when it goes (`nh_core::native`). The host
//! draws what it gets and reports a click ([`crate::reach`]).

use std::{
    collections::{BTreeMap, HashMap},
    path::PathBuf,
    sync::{
        Mutex, OnceLock, PoisonError,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};

use nh_core::{
    Host,
    host::{
        HostNotification, NotificationAction, NotificationCapabilities, NotificationPermission,
        OpenRefused, PathOpen, SecretAnswered, SecretQuestion, TrayModel, WindowSpec,
    },
};
use nh_proto::api::ids::{InteractionId, NotificationId, PaneId, WindowId};
use tauri::{AppHandle, Manager};
use tauri_plugin_clipboard_manager::ClipboardExt;
use tauri_plugin_opener::OpenerExt;

use crate::{
    machines::LOCAL,
    notifier::{self, Click, Notifier, Permission, Response},
    reach::{self, Choice},
};

/// The host of the core in the app. It gets the handle of the app when the app is built, which is
/// after the core starts, so the handle comes late.
#[derive(Debug)]
pub struct TauriHost {
    app: OnceLock<AppHandle>,
    sidecar: PathBuf,
    /// The text to write if the app is not up yet. It is written when the handle comes.
    waiting: Mutex<Vec<String>>,
    notifier: OnceLock<Notifier>,
    /// The interaction that each notification on the screen is about, the buttons that it has,
    /// and the machine whose core posted it. A click names the notification. A notification
    /// keeps its interaction: an update to another one loses the buttons, so a click on the
    /// banner that the person read never answers another request.
    shown: Mutex<HashMap<String, Shown>>,
    /// The badge that each machine asked for. The dock shows the sum.
    badges: Mutex<BTreeMap<String, u32>>,
    /// The tray model of each machine. The tray has one section for each.
    trays: Mutex<BTreeMap<String, TrayModel>>,
    /// The machines in the order of `config.toml`, `local` first: the order of the tray.
    order: Mutex<Vec<String>>,
    /// The system refused notifications.
    denied: AtomicBool,
}

impl std::fmt::Debug for Notifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Notifier")
    }
}

/// What the host remembers of a notification on the screen.
#[derive(Clone, Debug)]
struct Shown {
    interaction: Option<InteractionId>,
    /// The pane of a notification that names no interaction: a run whose turn ended.
    pane: Option<PaneId>,
    /// The machine of the core that posted it (`local` for the core of this Mac).
    machine: String,
    /// The buttons that the notification has. A click on another one answers nothing.
    actions: Vec<NotificationAction>,
}

/// What the host keeps of a notification that the core posts or updates, and the buttons that it
/// shows. An update that names another interaction than the one on the screen shows no buttons:
/// the person may read the old text when they click.
fn remember(
    known: Option<&Shown>,
    machine: &str,
    notification: &HostNotification,
) -> (Shown, Vec<NotificationAction>) {
    let same = known.is_none_or(|known| known.interaction == notification.interaction);
    let actions = if same {
        notification.actions.clone()
    } else {
        Vec::new()
    };
    let shown = Shown {
        interaction: notification.interaction,
        pane: notification.pane,
        machine: machine.to_owned(),
        actions: actions.clone(),
    };
    (shown, actions)
}

/// The decision of a click on a button, or `None` when the notification did not offer that
/// button when it was posted.
fn choice_of(click: Click, offered: &[NotificationAction]) -> Option<Choice> {
    match click {
        Click::Allow if offered.contains(&NotificationAction::Allow) => Some(Choice::Allow),
        Click::Deny if offered.contains(&NotificationAction::Deny) => Some(Choice::Deny),
        _ => None,
    }
}

/// The title of a notification of a machine: the label comes first, so that the person sees which
/// machine asks. `local` adds nothing.
pub fn title_for(machine: &str, title: &str) -> String {
    if machine == LOCAL {
        title.to_owned()
    } else {
        format!("{machine}: {title}")
    }
}

/// The badge of the dock: the sum of the badges of all machines.
pub fn total_badge(badges: &BTreeMap<String, u32>) -> u32 {
    badges.values().fold(0u32, |sum, n| sum.saturating_add(*n))
}

/// The sections of the tray: the machines that have a model, in `order`, then any other by label.
pub fn tray_sections(
    trays: &BTreeMap<String, TrayModel>,
    order: &[String],
) -> Vec<(String, TrayModel)> {
    let mut out: Vec<(String, TrayModel)> = order
        .iter()
        .filter_map(|label| trays.get(label).map(|model| (label.clone(), model.clone())))
        .collect();
    for (label, model) in trays {
        if !order.contains(label) {
            out.push((label.clone(), model.clone()));
        }
    }
    out
}

impl TauriHost {
    pub fn new(sidecar: PathBuf) -> Self {
        Self {
            app: OnceLock::new(),
            sidecar,
            waiting: Mutex::new(Vec::new()),
            notifier: OnceLock::new(),
            shown: Mutex::new(HashMap::new()),
            badges: Mutex::new(BTreeMap::new()),
            trays: Mutex::new(BTreeMap::new()),
            order: Mutex::new(Vec::new()),
            denied: AtomicBool::new(false),
        }
    }

    /// Gives the host the handle of the app, and starts the notification centre. Call it on the
    /// main thread, after the core handle is managed.
    pub fn attach(&self, app: AppHandle) {
        if self.app.set(app.clone()).is_err() {
            return;
        }
        let waiting =
            std::mem::take(&mut *self.waiting.lock().unwrap_or_else(PoisonError::into_inner));
        for text in waiting {
            self.clipboard_write(text);
        }
        let on_response = {
            let app = app.clone();
            std::sync::Arc::new(move |response: Response| respond(&app, response))
        };
        let on_permission = {
            let app = app.clone();
            std::sync::Arc::new(move |permission: Permission| {
                tracing::info!(
                    ?permission,
                    "the system answered the request for notifications"
                );
                let known = match permission {
                    Permission::Granted => NotificationPermission::Granted,
                    Permission::Denied => NotificationPermission::Denied,
                    Permission::Unavailable => NotificationPermission::Unavailable,
                };
                if let Some(backend) = crate::backend::of(&app) {
                    backend.notification_permission(known);
                }
                if let Some(host) = app.try_state::<std::sync::Arc<TauriHost>>() {
                    host.denied
                        .store(permission != Permission::Granted, Ordering::SeqCst);
                }
            })
        };
        let _ = self
            .notifier
            .set(Notifier::start(on_response, on_permission));
    }

    /// Shows the settings window of a machine.
    pub fn open_settings_of(&self, machine: &str) {
        let machine = machine.to_owned();
        self.run_on_main(move |app| crate::windows::open_settings_of(app, &machine));
    }

    fn run_on_main(&self, job: impl FnOnce(&AppHandle) + Send + 'static) {
        let Some(app) = self.app.get() else {
            return;
        };
        let handle = app.clone();
        if let Err(error) = app.run_on_main_thread(move || job(&handle)) {
            tracing::warn!(%error, "a job for the main thread did not start");
        }
    }
}

/// A click on a notification of the system: the body shows the card, the buttons answer.
fn respond(app: &AppHandle, response: Response) {
    let shown = app
        .try_state::<std::sync::Arc<TauriHost>>()
        .and_then(|host| {
            host.shown
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .get(&response.identifier)
                .cloned()
        });
    // After a restart of the app the host does not know the notification. Then a click shows the
    // card that waits longest, and a button does nothing: it must not answer an unknown request.
    let machine = shown
        .as_ref()
        .map_or_else(|| LOCAL.to_owned(), |known| known.machine.clone());
    let pane = shown.as_ref().and_then(|known| known.pane);
    let offered = shown
        .as_ref()
        .map(|known| known.actions.clone())
        .unwrap_or_default();
    let interaction = shown
        .and_then(|known| known.interaction)
        .map(|id| id.to_string());
    match response.click {
        Click::Open => match (interaction, pane) {
            (Some(interaction), _) => reach::reveal(app, &machine, Some(interaction)),
            // A run whose turn ended: the window shows its pane, in any tab.
            (None, Some(pane)) => reach::show_pane(app, &machine, pane),
            (None, None) => reach::reveal_next(app),
        },
        Click::Allow | Click::Deny => {
            // A button that the notification did not offer shows the card instead.
            let (Some(interaction), Some(choice)) =
                (interaction, choice_of(response.click, &offered))
            else {
                reach::reveal_next(app);
                return;
            };
            reach::answer(app, &machine, interaction, choice, "notification");
        }
    }
}

/// A new id for a notification. The time and a counter make it unique in this run.
fn next_id() -> NotificationId {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(0));
    let count = COUNTER.fetch_add(1, Ordering::Relaxed);
    NotificationId::new(ulid::Ulid::from_parts(now, u128::from(count)))
}

impl TauriHost {
    /// The interaction and the machine that a notification on the screen is about.
    #[cfg(test)]
    fn shown_of(&self, id: NotificationId) -> Option<(Option<InteractionId>, String)> {
        self.shown
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(&notifier::identifier(id))
            .map(|known| (known.interaction, known.machine.clone()))
    }

    /// Brings the native window with this label to the front.
    pub fn focus_label(&self, label: String) {
        self.run_on_main(move |app| {
            if let Some(window) = app.get_webview_window(&label) {
                let _ = window.unminimize();
                let _ = window.show();
                let _ = window.set_focus();
            }
        });
    }

    /// Shows a notification of the core of `machine`. The title carries the label of a remote
    /// machine, and a click goes back to that machine.
    pub fn notify_for(&self, machine: &str, mut notification: HostNotification) -> NotificationId {
        let id = next_id();
        let identifier = notifier::identifier(id);
        notification.title = title_for(machine, &notification.title);
        let (shown, actions) = remember(None, machine, &notification);
        notification.actions = actions;
        self.shown
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(identifier.clone(), shown);
        if let Some(notifier) = self.notifier.get() {
            notifier.post(&identifier, &notification);
        }
        id
    }

    pub fn update_notification_for(
        &self,
        machine: &str,
        id: NotificationId,
        mut notification: HostNotification,
    ) {
        let identifier = notifier::identifier(id);
        notification.title = title_for(machine, &notification.title);
        {
            let mut known = self.shown.lock().unwrap_or_else(PoisonError::into_inner);
            let (shown, actions) = remember(known.get(&identifier), machine, &notification);
            notification.actions = actions;
            known.insert(identifier.clone(), shown);
        }
        if let Some(notifier) = self.notifier.get() {
            notifier.post(&identifier, &notification);
        }
    }

    /// The badge of one machine changed. The dock shows the sum of all.
    pub fn set_badge_for(&self, machine: &str, count: u32) {
        let total = {
            let mut badges = self.badges.lock().unwrap_or_else(PoisonError::into_inner);
            badges.insert(machine.to_owned(), count);
            total_badge(&badges)
        };
        self.run_on_main(move |app| crate::badge::set(app, total));
    }

    /// The tray model of one machine changed. The tray has a section for each machine.
    pub fn set_tray_for(&self, machine: &str, model: TrayModel) {
        self.trays
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(machine.to_owned(), model);
        self.show_tray();
    }

    /// A machine is gone (its link is down): its notifications, its badge and its tray section go.
    /// The core sends the notifications of the open interactions again when the link
    /// is back, so the person does not see them twice.
    pub fn clear_machine(&self, machine: &str) {
        let gone: Vec<String> = {
            let mut shown = self.shown.lock().unwrap_or_else(PoisonError::into_inner);
            let names: Vec<String> = shown
                .iter()
                .filter(|(_, known)| known.machine == machine)
                .map(|(name, _)| name.clone())
                .collect();
            for name in &names {
                shown.remove(name);
            }
            names
        };
        if let Some(notifier) = self.notifier.get() {
            for name in &gone {
                notifier.withdraw(name);
            }
        }
        let had_badge = self
            .badges
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(machine)
            .is_some();
        let had_tray = self
            .trays
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(machine)
            .is_some();
        if had_badge {
            let total = total_badge(&self.badges.lock().unwrap_or_else(PoisonError::into_inner));
            self.run_on_main(move |app| crate::badge::set(app, total));
        }
        if had_tray {
            self.show_tray();
        }
    }

    /// Tells the host the order of the machines (`local`, then `config.toml`).
    pub fn set_machine_order(&self, order: Vec<String>) {
        let changed = {
            let mut known = self.order.lock().unwrap_or_else(PoisonError::into_inner);
            if *known == order {
                false
            } else {
                *known = order;
                true
            }
        };
        if changed {
            self.show_tray();
        }
    }

    fn show_tray(&self) {
        let sections = tray_sections(
            &self.trays.lock().unwrap_or_else(PoisonError::into_inner),
            &self.order.lock().unwrap_or_else(PoisonError::into_inner),
        );
        self.run_on_main(move |app| crate::tray::show(app, &sections));
    }
}

impl Host for TauriHost {
    fn notify(&self, notification: HostNotification) -> NotificationId {
        self.notify_for(LOCAL, notification)
    }

    fn update_notification(&self, id: NotificationId, notification: HostNotification) {
        self.update_notification_for(LOCAL, id, notification);
    }

    fn withdraw_notification(&self, id: NotificationId) {
        let identifier = notifier::identifier(id);
        self.shown
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(&identifier);
        if let Some(notifier) = self.notifier.get() {
            notifier.withdraw(&identifier);
        }
    }

    fn notification_capabilities(&self) -> NotificationCapabilities {
        NotificationCapabilities {
            actions: self.notifier.get().is_some_and(Notifier::actions)
                && !self.denied.load(Ordering::SeqCst),
        }
    }

    fn set_badge(&self, count: u32) {
        self.set_badge_for(LOCAL, count);
    }

    fn set_tray(&self, tray: TrayModel) {
        self.set_tray_for(LOCAL, tray);
    }

    // The app follows the `windows` projection, so these two calls have nothing to do.
    fn open_window(&self, _spec: WindowSpec) {}

    fn close_window(&self, _id: WindowId) {}

    fn focus_window(&self, id: WindowId) {
        self.focus_label(id.to_string());
    }

    fn sidecar_path(&self) -> PathBuf {
        self.sidecar.clone()
    }

    fn clipboard_write(&self, text: String) {
        let Some(app) = self.app.get() else {
            self.waiting
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push(text);
            return;
        };
        if let Err(error) = app.clipboard().write_text(text) {
            tracing::warn!(%error, "the clipboard did not take the text");
        }
    }

    fn open_settings(&self) {
        self.run_on_main(crate::windows::open_settings);
    }

    fn open_settings_at(&self, section: &str) {
        let section = section.to_owned();
        self.run_on_main(move |app| crate::windows::open_settings_at(app, &section));
    }

    fn open_url(&self, url: String) {
        let Some(app) = self.app.get() else {
            return;
        };
        if let Err(error) = app.opener().open_url(&url, None::<&str>) {
            tracing::warn!(%error, "the system did not open an address");
        }
    }

    /// `fs.open`: the opener of the system starts the app of `ui.editor`, or the default
    /// app of the path. A reveal selects the path in the Finder, and AppKit runs on the main
    /// thread.
    fn open_path(&self, open: PathOpen) -> Result<(), OpenRefused> {
        let Some(app) = self.app.get() else {
            return Err(OpenRefused::Unsupported);
        };
        if open.reveal {
            self.run_on_main(move |app| {
                if let Err(error) = app.opener().reveal_item_in_dir(&open.path) {
                    tracing::warn!(%error, "the Finder did not show a path");
                }
            });
            return Ok(());
        }
        app.opener()
            .open_path(open.path.to_string_lossy(), open.with)
            .map_err(|error| OpenRefused::Failed(error.to_string()))
    }

    /// A question of ssh: a sheet on the main thread. Only macOS has one; on another
    /// system `done` drops and the core answers `unsupported`.
    #[cfg(target_os = "macos")]
    fn ask_secret(&self, question: SecretQuestion, done: SecretAnswered) {
        self.run_on_main(move |_| done(crate::askpass::ask(&question)));
    }

    #[cfg(not(target_os = "macos"))]
    fn ask_secret(&self, _: SecretQuestion, done: SecretAnswered) {
        drop(done);
    }
}

#[cfg(test)]
mod tests {
    use nh_core::host::TrayEntry;
    use nh_proto::api::common::Urgency;
    use std::sync::Arc;

    use super::*;
    use crate::machine_host::MachineHost;

    fn notification(title: &str, interaction: Option<InteractionId>) -> HostNotification {
        HostNotification {
            title: title.to_owned(),
            body: String::new(),
            urgency: Urgency::Normal,
            interaction,
            pane: None,
            actions: Vec::new(),
        }
    }

    #[test]
    fn the_title_of_a_remote_notification_has_the_label_and_a_local_one_has_none() {
        assert_eq!(title_for("local", "Claude asks"), "Claude asks");
        assert_eq!(title_for("studio", "Claude asks"), "studio: Claude asks");
    }

    #[test]
    fn a_notification_remembers_its_machine_so_a_click_answers_there() {
        let host = Arc::new(TauriHost::new(PathBuf::from("/bin/nh")));
        let a = MachineHost::new("studio", Arc::clone(&host));
        let b = MachineHost::new("build", Arc::clone(&host));
        let (one, two) = (
            InteractionId::new(ulid::Ulid::from_parts(1, 1)),
            InteractionId::new(ulid::Ulid::from_parts(1, 2)),
        );
        let on_a = a.notify(notification("one", Some(one)));
        let on_b = b.notify(notification("two", Some(two)));
        let here = host.notify(notification("three", None));
        assert_eq!(host.shown_of(on_a), Some((Some(one), "studio".to_owned())));
        assert_eq!(host.shown_of(on_b), Some((Some(two), "build".to_owned())));
        assert_eq!(host.shown_of(here), Some((None, "local".to_owned())));
        // The core updates a notification: the machine stays the one of that core.
        a.update_notification(on_a, notification("one again", Some(two)));
        assert_eq!(host.shown_of(on_a), Some((Some(two), "studio".to_owned())));
        b.withdraw_notification(on_b);
        assert_eq!(host.shown_of(on_b), None);
    }

    #[test]
    fn a_button_answers_only_the_request_that_the_notification_showed_with_it() {
        let (one, two) = (
            InteractionId::new(ulid::Ulid::from_parts(1, 1)),
            InteractionId::new(ulid::Ulid::from_parts(1, 2)),
        );
        let mut first = notification("one", Some(one));
        first.actions = vec![NotificationAction::Allow, NotificationAction::Deny];
        let (posted, buttons) = remember(None, LOCAL, &first);
        assert_eq!(buttons, first.actions);
        assert_eq!(
            choice_of(Click::Allow, &posted.actions),
            Some(Choice::Allow)
        );
        // An update of the text of the same request keeps the buttons.
        let (again, buttons) = remember(Some(&posted), LOCAL, &first);
        assert_eq!(buttons, first.actions);
        assert_eq!(again.interaction, Some(one));
        // An update that names another request loses them: a click shows the card.
        let mut other = notification("two", Some(two));
        other.actions = first.actions.clone();
        let (moved, buttons) = remember(Some(&posted), LOCAL, &other);
        assert!(buttons.is_empty());
        assert_eq!(choice_of(Click::Allow, &moved.actions), None);
        assert_eq!(choice_of(Click::Deny, &moved.actions), None);
        // A notification with Deny only does not take Allow.
        let deny_only = [NotificationAction::Deny];
        assert_eq!(choice_of(Click::Allow, &deny_only), None);
        assert_eq!(choice_of(Click::Deny, &deny_only), Some(Choice::Deny));
        assert_eq!(choice_of(Click::Open, &deny_only), None);
    }

    #[test]
    fn the_badge_is_the_sum_of_the_badges_of_all_machines() {
        let mut badges = BTreeMap::new();
        assert_eq!(total_badge(&badges), 0);
        badges.insert("local".to_owned(), 2);
        badges.insert("studio".to_owned(), 3);
        badges.insert("build".to_owned(), 0);
        assert_eq!(total_badge(&badges), 5);
        badges.insert("lab".to_owned(), u32::MAX);
        assert_eq!(total_badge(&badges), u32::MAX);
    }

    #[test]
    fn the_tray_has_a_section_for_each_machine_in_the_order_of_the_machines() {
        let model = |badge| TrayModel {
            entries: vec![TrayEntry {
                interaction: None,
                label: format!("entry {badge}"),
                approval: false,
                allow: false,
                always: None,
            }],
            badge,
        };
        let mut trays = BTreeMap::new();
        trays.insert("studio".to_owned(), model(1));
        trays.insert("local".to_owned(), model(2));
        trays.insert("zed".to_owned(), model(3));
        let order = vec!["local".to_owned(), "studio".to_owned(), "build".to_owned()];
        let labels: Vec<String> = tray_sections(&trays, &order)
            .into_iter()
            .map(|(label, _)| label)
            .collect();
        // `build` has no model. `zed` is not in the order and comes last.
        assert_eq!(labels, ["local", "studio", "zed"]);
    }
}
