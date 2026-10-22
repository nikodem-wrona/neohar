//! The machines of the app.
//!
//! A window shows one machine. The label of a native window is the id of its row for `local`, and
//! `<machine>/<window id>` for a remote machine. The three commands of the webview choose the
//! [`Backend`] of the machine of their window, so the page and `TauriTransport` do not change.
//!
//! - [`Machines`] is the state of the app: the backend of `local`, and the link of each remote
//!   machine that is up (from the [`MachineManager`]). A remote [`Linked`] lives as long as its
//!   link, so the blobs of a machine stay in its own cache of 64 MiB.
//! - [`follow`] reads the projection `machines` of the local core. For each machine that is up it
//!   follows the `windows` of that core and opens a native window for each row of this device
//!   ([`crate::windows::follow`]). It also feeds the projection `link`.
//! - The projection `link` is served here and not by a core: one key `_` for each
//!   window, with the machine, the state, the round-trip time and the text of the status bar.
//!   [`LinkFeed`] keeps one value for each machine and sends it to the windows of that machine.

use std::{
    collections::{BTreeMap, HashMap},
    sync::{Arc, Mutex, PoisonError},
};

use nh_core::{ErrorCode, MessageSink, RpcError};
use nh_proto::api::{
    ids::WindowId,
    model::{MachineState, MachineValue},
};
use nh_remote::{LinkFeed, SettingsFeed, Side, machines::MachineManager};
use serde_json::Value;
use tauri::AppHandle;
use tokio::{sync::mpsc, task::AbortHandle};

use crate::backend::{Backend, Linked};

/// The label of the machine that hosts this app's core.
pub const LOCAL: &str = "local";

/// The label of the settings window of `local`.
pub const SETTINGS: &str = "settings";

/// The machine and the window of a native window, by its label. `None` for a window that is no
/// row of a layout, for example the settings window.
pub fn machine_of(label: &str) -> Option<(&str, WindowId)> {
    let (machine, id) = match label.split_once('/') {
        Some((machine, id)) => {
            if !nh_core::config::valid_machine_label(machine) {
                return None;
            }
            (machine, id)
        }
        None => (LOCAL, label),
    };
    id.parse::<WindowId>().ok().map(|id| (machine, id))
}

/// The machine of a settings window: `settings` is the window of `local`, and `settings/<machine>`
/// shows the settings of that machine.
pub fn settings_machine(label: &str) -> Option<&str> {
    if label == SETTINGS {
        return Some(LOCAL);
    }
    label
        .strip_prefix("settings/")
        .filter(|machine| *machine != LOCAL && nh_core::config::valid_machine_label(machine))
}

/// The label of the settings window of a machine.
pub fn settings_label(machine: &str) -> String {
    if machine == LOCAL {
        SETTINGS.to_owned()
    } else {
        format!("{SETTINGS}/{machine}")
    }
}

/// The label of the native window of a row of `machine`.
pub fn label_of(machine: &str, id: &str) -> String {
    if machine == LOCAL {
        id.to_owned()
    } else {
        format!("{machine}/{id}")
    }
}

/// The window of a label that belongs to this app's own core.
pub fn local_window(label: &str) -> Option<WindowId> {
    machine_of(label)
        .filter(|(machine, _)| *machine == LOCAL)
        .map(|(_, id)| id)
}

/// The error of a call to a machine whose link is not up.
pub fn link_down(machine: &str) -> RpcError {
    RpcError::new(
        ErrorCode::LinkDown,
        format!("the link to {machine} is not up: connect it from the Machines menu"),
    )
}

/// The machines of the app. Cloning the `Arc` is cheap.
pub struct Machines {
    local: Backend,
    manager: Option<MachineManager>,
    /// The `Linked` of each remote machine, with the number of the link it was made for.
    linked: Mutex<HashMap<String, (u64, Linked)>>,
    feeds: Mutex<HashMap<String, Arc<LinkFeed>>>,
    /// The merged `settings` of each remote machine.
    settings: Mutex<HashMap<String, SettingsHub>>,
}

/// The merged `settings` of a machine, and the tasks that feed it from the two cores.
struct SettingsHub {
    feed: Arc<SettingsFeed>,
    /// The number of the link the tasks were made for.
    generation: u64,
    tasks: Vec<AbortHandle>,
}

impl SettingsHub {
    /// Whether the tasks still run, for the link they were made for.
    fn alive(&self, generation: u64) -> bool {
        self.generation == generation && self.tasks.iter().all(|task| !task.is_finished())
    }
}

/// The state of the app, as Tauri holds it.
pub fn of(app: &AppHandle) -> Option<Arc<Machines>> {
    use tauri::Manager;
    app.try_state::<Arc<Machines>>()
        .map(|state| Arc::clone(state.inner()))
}

impl Machines {
    pub fn new(local: Backend, manager: Option<MachineManager>) -> Self {
        Self {
            local,
            manager,
            linked: Mutex::new(HashMap::new()),
            feeds: Mutex::new(HashMap::new()),
            settings: Mutex::new(HashMap::new()),
        }
    }

    /// The backend of the core in this process, or of the daemon on this Mac.
    pub fn local(&self) -> &Backend {
        &self.local
    }

    /// The manager of the links, in local mode.
    pub fn manager(&self) -> Option<&MachineManager> {
        self.manager.as_ref()
    }

    /// The backend of a machine. A remote machine whose link is down has none: the call fails at
    /// once with `link_down`.
    pub fn backend(&self, machine: &str) -> Result<Backend, RpcError> {
        if machine == LOCAL {
            return Ok(self.local.clone());
        }
        let manager = self.manager.as_ref().ok_or_else(|| link_down(machine))?;
        let (client, generation) = manager
            .client_of(machine)
            .ok_or_else(|| link_down(machine))?;
        let device = self.local.device().ok_or_else(|| link_down(machine))?;
        let mut linked = lock(&self.linked);
        match linked.get(machine) {
            Some((known, backend)) if *known == generation => Ok(Backend::Link(backend.clone())),
            _ => {
                let backend = Linked::persistent(client, device);
                linked.insert(machine.to_owned(), (generation, backend.clone()));
                Ok(Backend::Link(backend))
            }
        }
    }

    /// The backend of the machine of a native window. A settings window belongs to its machine,
    /// and any other window that is no row of a layout belongs to `local`.
    pub fn backend_of_label(&self, label: &str) -> Result<Backend, RpcError> {
        self.backend(self.machine_of_label(label))
    }

    /// The machine of a native window, `local` when the label is no row of a layout.
    pub fn machine_of_label<'a>(&self, label: &'a str) -> &'a str {
        machine_of(label)
            .map(|(machine, _)| machine)
            .unwrap_or_else(|| settings_machine(label).unwrap_or(LOCAL))
    }

    /// The merged `settings` of a remote machine. The first call starts the two
    /// subscriptions that feed it. A call after the link was made again, or after a feeding task
    /// ended, starts them again and keeps the feed, so its versions only grow.
    pub async fn settings_feed(
        self: &Arc<Self>,
        machine: &str,
    ) -> Result<Arc<SettingsFeed>, RpcError> {
        let generation = self
            .manager
            .as_ref()
            .and_then(|manager| manager.client_of(machine))
            .map(|(_, generation)| generation)
            .ok_or_else(|| link_down(machine))?;
        let remote = self.backend(machine)?;
        let mut hubs = lock(&self.settings);
        let feed = match hubs.get(machine) {
            Some(hub) if hub.alive(generation) => return Ok(Arc::clone(&hub.feed)),
            Some(hub) => {
                for task in &hub.tasks {
                    task.abort();
                }
                Arc::clone(&hub.feed)
            }
            None => Arc::new(SettingsFeed::new(machine)),
        };
        let tasks = vec![
            feed_settings(&feed, Side::Local, self.local.clone()),
            feed_settings(&feed, Side::Machine, remote),
        ];
        hubs.insert(
            machine.to_owned(),
            SettingsHub {
                feed: Arc::clone(&feed),
                generation,
                tasks,
            },
        );
        Ok(feed)
    }

    /// The version of the merged `settings` of a machine, `None` when no window asked for it yet.
    pub fn settings_version(&self, machine: &str) -> Option<u64> {
        lock(&self.settings)
            .get(machine)
            .map(|hub| hub.feed.version())
    }

    /// Ends a subscription to the merged `settings`, whichever machine it is for.
    pub fn unsubscribe_settings(&self, subscription: &str) -> bool {
        lock(&self.settings)
            .values()
            .any(|hub| hub.feed.unsubscribe(subscription))
    }

    /// The feed of the projection `link` of a machine.
    pub fn feed(&self, machine: &str) -> Arc<LinkFeed> {
        Arc::clone(
            lock(&self.feeds)
                .entry(machine.to_owned())
                .or_insert_with(|| Arc::new(LinkFeed::new(machine))),
        )
    }

    /// Ends a subscription to `link`, whichever machine it is for.
    pub fn unsubscribe_link(&self, subscription: &str) -> bool {
        lock(&self.feeds)
            .values()
            .any(|feed| feed.unsubscribe(subscription))
    }

    /// The versions of the projections that the host serves: `link` of a machine.
    pub fn link_version(&self, machine: &str) -> u64 {
        self.feed(machine).version()
    }
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Sends the messages of a projection to a channel.
pub(crate) struct Feed(pub(crate) mpsc::UnboundedSender<Value>);

impl MessageSink for Feed {
    fn send(&self, message: Value) -> bool {
        self.0.send(message).is_ok()
    }
}

/// The keys of an `order` array.
fn keys(order: &Value) -> Vec<String> {
    order
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|key| key.as_str().map(str::to_owned))
        .collect()
}

/// Applies a message of the projection `machines` to the rows and their order. Returns false for
/// `resync`.
fn apply(
    rows: &mut BTreeMap<String, MachineValue>,
    order: &mut Vec<String>,
    message: &Value,
) -> bool {
    match message["type"].as_str() {
        Some("snapshot") => {
            rows.clear();
            for (key, value) in message["data"]["items"].as_object().into_iter().flatten() {
                if let Ok(row) = serde_json::from_value(value.clone()) {
                    rows.insert(key.clone(), row);
                }
            }
            *order = keys(&message["data"]["order"]);
            true
        }
        Some("patch") => {
            for op in message["ops"].as_array().into_iter().flatten() {
                let key = op["key"].as_str().unwrap_or_default().to_owned();
                match op["op"].as_str() {
                    Some("set") => {
                        if let Ok(row) = serde_json::from_value(op["value"].clone()) {
                            rows.insert(key, row);
                        }
                    }
                    Some("remove") => {
                        rows.remove(&key);
                    }
                    Some("order") => *order = keys(&op["keys"]),
                    _ => {}
                }
            }
            true
        }
        _ => false,
    }
}

/// Starts the windows of this app: those of `local`, and those of each remote machine while its
/// link is up. Call it once, after the state is managed.
pub fn start(app: &AppHandle, machines: &Arc<Machines>) -> Result<(), RpcError> {
    crate::windows::follow(app, machines, LOCAL)?;
    if machines.manager().is_some() {
        follow(app, machines)?;
    }
    Ok(())
}

/// Feeds the merged `settings` from one core. A `resync` makes it subscribe again. The task ends
/// when the subscription cannot start or when the core ends it.
fn feed_settings(feed: &Arc<SettingsFeed>, side: Side, backend: Backend) -> AbortHandle {
    let feed = Arc::clone(feed);
    tauri::async_runtime::spawn(async move {
        let (tx, mut rx) = mpsc::unbounded_channel();
        loop {
            if let Err(error) = backend
                .subscribe("settings", None, Box::new(Feed(tx.clone())))
                .await
            {
                tracing::warn!(?side, %error, "the settings of a machine are not followed");
                return;
            }
            loop {
                let Some(message) = rx.recv().await else {
                    return;
                };
                if !feed.apply(side, &message) {
                    break;
                }
            }
        }
    })
    .inner()
    .abort_handle()
}

/// Follows the projection `machines` of the local core. It feeds `link` and follows the windows
/// of each machine that is up. When a link goes down its windows stay open.
fn follow(app: &AppHandle, machines: &Arc<Machines>) -> Result<(), RpcError> {
    let (tx, mut rx) = mpsc::unbounded_channel();
    tauri::async_runtime::block_on(machines.local().subscribe(
        "machines",
        None,
        Box::new(Feed(tx.clone())),
    ))?;
    let (app, machines) = (app.clone(), Arc::clone(machines));
    tauri::async_runtime::spawn(async move {
        let mut rows = BTreeMap::<String, MachineValue>::new();
        let mut order = Vec::<String>::new();
        let mut following: HashMap<String, (u64, AbortHandle)> = HashMap::new();
        while let Some(message) = rx.recv().await {
            if !apply(&mut rows, &mut order, &message) {
                let again = machines
                    .local()
                    .subscribe("machines", None, Box::new(Feed(tx.clone())))
                    .await;
                if again.is_err() {
                    tracing::error!("cannot follow the machines again");
                }
                continue;
            }
            show_machines(&app, &rows, &order);
            for (label, row) in &rows {
                if label == LOCAL {
                    continue;
                }
                machines
                    .feed(label)
                    .update(row.state, row.latency_ms, row.reason.clone());
                let up = row.state == MachineState::Connected;
                let link = machines.manager().and_then(|m| m.client_of(label));
                match (up, link) {
                    (true, Some((_, generation))) => {
                        if following.get(label).map(|(known, _)| *known) != Some(generation) {
                            if let Some((_, old)) = following.remove(label) {
                                old.abort();
                            }
                            let task = follow_windows(&app, &machines, label);
                            following.insert(label.clone(), (generation, task));
                        }
                    }
                    _ => {
                        if let Some((_, old)) = following.remove(label) {
                            old.abort();
                        }
                    }
                }
            }
            // A machine that left `config.toml`: its windows stay, and nothing follows them.
            let gone: Vec<String> = following
                .keys()
                .filter(|label| !rows.contains_key(*label))
                .cloned()
                .collect();
            for label in gone {
                if let Some((_, old)) = following.remove(&label) {
                    old.abort();
                }
                machines.feed(&label).update(
                    MachineState::Disconnected,
                    None,
                    Some("removed from config.toml".to_owned()),
                );
            }
        }
    });
    Ok(())
}

/// What the host shows of the machines: the order of the tray, the Machines menu, and no badge or
/// tray section for a machine whose link is not up.
fn show_machines(app: &AppHandle, rows: &BTreeMap<String, MachineValue>, order: &[String]) {
    use tauri::Manager;

    if let Some(host) = app.try_state::<Arc<crate::host::TauriHost>>() {
        host.set_machine_order(order.to_vec());
        for (label, row) in rows {
            if label != LOCAL && row.state != MachineState::Connected {
                host.clear_machine(label);
            }
        }
    }
    let ordered: Vec<MachineValue> = order
        .iter()
        .filter_map(|label| rows.get(label).cloned())
        .collect();
    let shown = app.clone();
    let _ = app.run_on_main_thread(move || crate::menu::update_machines(&shown, &ordered));
}

/// Follows the windows of one machine in a task of its own.
fn follow_windows(app: &AppHandle, machines: &Arc<Machines>, label: &str) -> AbortHandle {
    let (app, machines, label) = (app.clone(), Arc::clone(machines), label.to_owned());
    tauri::async_runtime::spawn(async move {
        if let Err(error) = crate::windows::follow_machine(&app, &machines, &label).await {
            tracing::warn!(machine = label, %error, "the windows of a machine are not followed");
        }
    })
    .inner()
    .abort_handle()
}

#[cfg(test)]
mod tests {

    use serde_json::json;

    use super::*;

    const ID: &str = "01ARZ3NDEKTSV4RRFFQ69G5FAV";

    #[test]
    fn the_label_of_a_window_names_its_machine_and_its_row() {
        assert_eq!(machine_of(ID).map(|(m, _)| m), Some("local"));
        let label = format!("studio/{ID}");
        let (machine, id) = machine_of(&label).expect("a remote window");
        assert_eq!((machine, id.to_string()), ("studio", ID.to_owned()));
        // The settings window, a label, a bad machine and a bad id are no windows of a layout.
        for label in [
            "settings",
            "settings/studio",
            "main",
            "",
            "studio/main",
            &format!("Studio/{ID}"),
            &format!("local/x/{ID}"),
            &format!("{}/{ID}", "a".repeat(33)),
        ] {
            assert!(machine_of(label).is_none(), "{label:?}");
        }
        assert_eq!(settings_machine("settings"), Some("local"));
        assert_eq!(settings_machine("settings/studio"), Some("studio"));
        assert_eq!(settings_machine("settings/local"), None);
        assert_eq!(settings_machine("settings/Studio"), None);
        assert_eq!(settings_machine(ID), None);
        assert_eq!(settings_label("local"), "settings");
        assert_eq!(settings_label("studio"), "settings/studio");
        assert_eq!(label_of("local", ID), ID);
        assert_eq!(label_of("studio", ID), format!("studio/{ID}"));
        assert!(local_window(ID).is_some());
        assert!(local_window(&format!("studio/{ID}")).is_none());
        // The label round-trips.
        let label = label_of("build-1", ID);
        assert_eq!(machine_of(&label).map(|(m, _)| m), Some("build-1"));
    }

    #[test]
    fn the_rows_of_the_machines_follow_a_snapshot_and_its_patches() {
        let mut rows = BTreeMap::new();
        let mut order = Vec::new();
        assert!(apply(
            &mut rows,
            &mut order,
            &json!({"type": "snapshot", "version": 1, "data": {
                "items": {
                    "local": {"label": "local", "state": "local"},
                    "studio": {"label": "studio", "state": "connected", "latency_ms": 12.5}
                },
                "order": ["local", "studio"]}})
        ));
        assert_eq!(rows["studio"].state, MachineState::Connected);
        assert_eq!(order, ["local", "studio"]);
        assert!(apply(
            &mut rows,
            &mut order,
            &json!({"type": "patch", "version": 2, "ops": [
                {"op": "set", "key": "studio", "value":
                    {"label": "studio", "state": "disconnected", "reason": "you disconnected"}},
                {"op": "remove", "key": "local"},
                {"op": "order", "keys": ["studio"]}]})
        ));
        assert_eq!(rows.len(), 1);
        assert_eq!(order, ["studio"]);
        assert_eq!(rows["studio"].reason.as_deref(), Some("you disconnected"));
        assert!(!apply(&mut rows, &mut order, &json!({"type": "resync"})));
    }
}
