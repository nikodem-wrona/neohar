//! The windows of the app.
//!
//! The core owns the list of windows: the `windows` projection has a row for each one. The app
//! follows that list. A row that appears opens a native window whose label is the id of the row,
//! and a row that goes closes the window. So a window that the CLI opens appears like one from the
//! menu, and a window comes back after a restart because the layout has it.
//!
//! The native window tells the core what only the host knows: focus, minimize, show and hide
//! (`window.focus_report`), its bounds (`window.set_bounds`, after 500 ms without a change), and a
//! click on the close button (`window.close`; the core closes the window by removing the row).

use std::{
    collections::HashMap,
    sync::{Arc, Mutex, PoisonError},
    time::Duration,
};

use nh_proto::api::{common::Bounds, ids::WindowId};
use serde_json::{Value, json};
use tauri::{
    AppHandle, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder, Window, WindowEvent,
};
use tokio::sync::mpsc;

use crate::{
    backend::Backend,
    commands::layout_window,
    machines::{self, Feed, LOCAL, Machines},
    policy::{Navigation, navigation},
};

/// How long the bounds of a window stay still before the host tells the core.
const BOUNDS_DEBOUNCE: Duration = Duration::from_millis(500);

/// The size of a window that has no bounds yet.
const DEFAULT_SIZE: (f64, f64) = (1400.0, 900.0);

/// The size of the settings window.
const SETTINGS_SIZE: (f64, f64) = (780.0, 580.0);

/// Where the three buttons of a window go on macOS, in logical pixels from the top left corner.
/// The command line of the page is the title bar: the panel starts 8 px from the
/// top and is 52 px high, so the buttons have their middle at 34 px. AppKit makes the box of the
/// buttons as high as a button plus `y`, so `y` is about the middle of the buttons.
#[cfg(target_os = "macos")]
const TRAFFIC_LIGHTS: (f64, f64) = (20.0, 34.0);

/// What the host knows of a window, and the last report that the page sent.
struct Native {
    focused: bool,
    visible: bool,
    minimized: bool,
    /// The last `window.focus_report` of the page.
    page: Option<Value>,
    /// Counts the changes of the bounds, so that only the last one is sent.
    bounds_changes: u64,
}

impl Default for Native {
    fn default() -> Self {
        Self {
            focused: false,
            visible: true,
            minimized: false,
            page: None,
            bounds_changes: 0,
        }
    }
}

/// The windows of the app, as the host tracks them.
#[derive(Clone, Default)]
pub struct Windows {
    native: Arc<Mutex<HashMap<String, Native>>>,
}

impl Windows {
    fn with<T>(&self, label: &str, f: impl FnOnce(&mut Native) -> T) -> T {
        let mut all = self.native.lock().unwrap_or_else(PoisonError::into_inner);
        f(all.entry(label.to_owned()).or_default())
    }

    /// The page reports focus and visibility from the DOM, and the host knows the native state of
    /// the window. The core gets one answer that joins them: the window has focus when both say
    /// so, and it is visible when both say so and it is not minimized. The refresh rate and the
    /// active pane are the page's.
    pub fn merge_page_report(&self, window: &Window, params: Value) -> Value {
        let label = window.label();
        let Some(id) = layout_window(label) else {
            return params;
        };
        let flag = |name: &str, default: bool| {
            params.get(name).and_then(Value::as_bool).unwrap_or(default)
        };
        self.with(label, |native| {
            native.page = Some(params.clone());
            let mut merged = params.clone();
            if let Some(map) = merged.as_object_mut() {
                map.insert("window".into(), json!(id.to_string()));
                map.insert(
                    "focused".into(),
                    json!(flag("focused", false) && native.focused),
                );
                map.insert(
                    "visible".into(),
                    json!(flag("visible", true) && native.visible),
                );
                map.insert(
                    "minimized".into(),
                    json!(flag("minimized", false) || native.minimized),
                );
            }
            merged
        })
    }

    /// Tells the core about a change that only the host sees.
    fn report_native(&self, window: &Window) {
        let label = window.label();
        let Some(id) = layout_window(label) else {
            return;
        };
        let params = self.with(label, |native| {
            let mut params = json!({
                "window": id.to_string(),
                "focused": native.focused,
                "visible": native.visible,
                "minimized": native.minimized,
            });
            // The rate and the active pane come from the page.
            if let (Some(map), Some(page)) = (params.as_object_mut(), &native.page) {
                for name in ["refresh_hz", "active_pane"] {
                    if let Some(value) = page.get(name) {
                        map.insert(name.to_owned(), value.clone());
                    }
                }
                let page_flag = |name: &str, default: bool| {
                    page.get(name).and_then(Value::as_bool).unwrap_or(default)
                };
                map.insert(
                    "focused".into(),
                    json!(page_flag("focused", true) && native.focused),
                );
                map.insert(
                    "visible".into(),
                    json!(page_flag("visible", true) && native.visible),
                );
            }
            params
        });
        let Some(backend) = backend_of(window.app_handle(), label) else {
            return;
        };
        tauri::async_runtime::spawn(async move {
            if let Err(error) = backend.call(Some(id), "window.focus_report", params).await {
                tracing::debug!(%error, "the core refused a focus report");
            }
        });
    }

    /// The bounds of the window changed. After half a second without another change, the core
    /// gets the new bounds.
    fn bounds_changed(&self, window: &Window) {
        let label = window.label().to_owned();
        let Some(id) = layout_window(&label) else {
            return;
        };
        let generation = self.with(&label, |native| {
            native.bounds_changes += 1;
            native.bounds_changes
        });
        let this = self.clone();
        let window = window.clone();
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(BOUNDS_DEBOUNCE).await;
            if this.with(&label, |native| native.bounds_changes) != generation {
                return;
            }
            let Some(bounds) = bounds_of(&window) else {
                return;
            };
            let Some(backend) = backend_of(window.app_handle(), &label) else {
                return;
            };
            let params = json!({ "window": id.to_string(), "bounds": bounds });
            if let Err(error) = backend.call(Some(id), "window.set_bounds", params).await {
                tracing::debug!(%error, "the core refused the bounds of a window");
            }
        });
    }

    /// Reacts to an event of a native window.
    pub fn on_event(&self, window: &Window, event: &WindowEvent) {
        match event {
            WindowEvent::Focused(focused) => {
                self.with(window.label(), |native| native.focused = *focused);
                self.report_native(window);
            }
            WindowEvent::Resized(_) => {
                let minimized = window.is_minimized().unwrap_or(false);
                let changed = self.with(window.label(), |native| {
                    std::mem::replace(&mut native.minimized, minimized) != minimized
                });
                if changed {
                    self.report_native(window);
                }
                if !minimized {
                    self.bounds_changed(window);
                }
            }
            WindowEvent::Moved(_) => self.bounds_changed(window),
            WindowEvent::CloseRequested { api, .. } => {
                // The button asks the core to close the window. The core removes the row, and the
                // app then closes the native window.
                if let Some(id) = layout_window(window.label())
                    && let Some(backend) = backend_of(window.app_handle(), window.label())
                {
                    api.prevent_close();
                    tauri::async_runtime::spawn(async move {
                        let params = json!({ "window": id.to_string() });
                        if let Err(error) = backend.call(Some(id), "window.close", params).await {
                            tracing::warn!(%error, "the core did not close the window");
                        }
                    });
                }
            }
            WindowEvent::Destroyed => {
                self.native
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .remove(window.label());
            }
            _ => {}
        }
    }
}

/// The backend of the machine of a native window. A window of a machine whose link is down has
/// none, and what the host would tell that core waits for the link.
fn backend_of(app: &AppHandle, label: &str) -> Option<Backend> {
    machines::of(app)?.backend_of_label(label).ok()
}

/// The bounds of a window in logical pixels.
fn bounds_of(window: &Window) -> Option<Bounds> {
    let scale = window.scale_factor().ok()?;
    let position = window.outer_position().ok()?.to_logical::<i32>(scale);
    let size = window.inner_size().ok()?.to_logical::<u32>(scale);
    Some(Bounds {
        x: position.x,
        y: position.y,
        width: size.width,
        height: size.height,
    })
}

/// The gesture token of this run of the app: two random ULIDs. Each webview gets it in a global of
/// its page (`__NEOHAR_GESTURE__`) before the page loads. The page adds it to the navigation of a
/// click on a link, and the host opens a link only with it ([`navigation`]). A page from a
/// hostile core is HTML with no script, so it cannot read the token.
fn gesture() -> &'static str {
    static TOKEN: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    TOKEN.get_or_init(|| format!("{}{}", nh_remote::new_ulid(), nh_remote::new_ulid()))
}

/// The script that gives a page the gesture token. A ULID has only letters and digits, so the
/// token needs no escape.
fn gesture_script() -> String {
    format!(
        "Object.defineProperty(window, '__NEOHAR_GESTURE__', {{ value: '{}' }});",
        gesture()
    )
}

/// The rule for the addresses that a webview may open: the app, and the web pages that the
/// system browser opens after a click on a link. An address to a loopback host in a window of a
/// remote machine is a service of that machine, so the host adds a forward first and opens the
/// address with the port of the forward.
fn navigation_guard(
    handle: AppHandle,
    machine: String,
) -> impl Fn(&tauri::Url) -> bool + Send + 'static {
    move |url| match navigation(url, gesture()) {
        Navigation::Allow => true,
        Navigation::OpenOutside(target) => {
            use nh_core::Host as _;
            if machine != machines::LOCAL
                && let Some(loopback) = nh_remote::forward::Loopback::parse(&target)
            {
                open_forwarded(&handle, &machine, &loopback);
            } else if let Some(host) = handle.try_state::<Arc<crate::host::TauriHost>>() {
                host.open_url(target);
            }
            false
        }
        Navigation::Deny => {
            tracing::warn!(%url, "a webview tried to leave the app");
            false
        }
    }
}

/// Adds the forward for an address of a service of a remote machine, then opens the address with
/// the port on this Mac. It runs off the thread of the webview, because `ssh -O forward` takes a
/// moment. When the forward fails, nothing opens: the address on this Mac would be another
/// service.
fn open_forwarded(app: &AppHandle, machine: &str, loopback: &nh_remote::forward::Loopback) {
    use nh_core::Host as _;
    let (app, machine, loopback) = (app.clone(), machine.to_owned(), loopback.clone());
    tauri::async_runtime::spawn(async move {
        let Some(manager) = machines::of(&app).and_then(|m| m.manager().cloned()) else {
            return;
        };
        match manager.add_forward(&machine, loopback.port).await {
            Ok(forward) => {
                if let Some(host) = app.try_state::<Arc<crate::host::TauriHost>>() {
                    host.open_url(loopback.with_port(forward.local_port));
                }
            }
            Err(error) => {
                tracing::warn!(machine, port = loopback.port, %error, "cannot forward a port");
            }
        }
    });
}

/// Shows the settings window of this Mac, or brings it to the front when it is open. It
/// is a window of the app and no row of the `windows` projection, so the core does not know it.
pub fn open_settings(app: &AppHandle) {
    open_settings_of(app, crate::machines::LOCAL);
}

/// Shows the settings window of a machine. A remote machine has a window of its own, with the
/// label of the machine in its title. It shows the settings of the machine, with the sections of
/// the client from this Mac.
pub fn open_settings_of(app: &AppHandle, machine: &str) {
    open_settings_window(app, machine, None);
}

/// Shows the settings window of this Mac at a section, for example `phone`. The page
/// takes the section from the fragment of its address. An open window gets the new fragment.
pub fn open_settings_at(app: &AppHandle, section: &str) {
    // The name of a section is a key of `config.toml`: small letters and `_`.
    if section.is_empty() || !section.bytes().all(|b| b.is_ascii_lowercase() || b == b'_') {
        tracing::warn!(section, "not a section of the settings");
        open_settings(app);
        return;
    }
    open_settings_window(app, crate::machines::LOCAL, Some(section));
}

fn open_settings_window(app: &AppHandle, machine: &str, section: Option<&str>) {
    let label = crate::machines::settings_label(machine);
    if let Some(window) = app.get_webview_window(&label) {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
        if let Some(section) = section {
            let _ = window.eval(format!("window.location.hash = '{section}';"));
        }
        return;
    }
    let fragment = section.map_or_else(String::new, |section| format!("#{section}"));
    let (url, title) = if machine == crate::machines::LOCAL {
        (
            format!("index.html?view=settings{fragment}"),
            "neohar settings".to_owned(),
        )
    } else {
        (
            format!("index.html?view=settings&machine={machine}{fragment}"),
            format!("neohar settings · {machine}"),
        )
    };
    let built = WebviewWindowBuilder::new(app, &label, WebviewUrl::App(url.into()))
        .title(title)
        .inner_size(SETTINGS_SIZE.0, SETTINGS_SIZE.1)
        .initialization_script(gesture_script())
        .on_navigation(navigation_guard(app.clone(), machine.to_owned()))
        .build();
    match built {
        Ok(window) => {
            tracing::info!(machine, "opened the settings window");
            drop(window);
        }
        Err(error) => tracing::error!(%error, machine, "cannot open the settings window"),
    }
}

/// Whether a new native window takes the focus. A window of this Mac does. A remote core adds rows
/// when it wants, and such a window must not take the keyboard from the window where the person
/// types. Thus a window of a remote machine takes the focus only when the person asked this app
/// for a window of it a short time ago (`asked`, which takes that request).
fn takes_focus(local: bool, asked: &dyn Fn() -> bool) -> bool {
    local || asked()
}

/// Opens the native window of a row of the `windows` projection. `label` is the label of the
/// window and `id` the id of the row. `asked` says whether the person asked for a window of the
/// machine a short time ago.
fn open_native(
    app: &AppHandle,
    label: &str,
    id: &str,
    bounds: Option<Bounds>,
    asked: &dyn Fn() -> bool,
) {
    if app.get_webview_window(label).is_some() {
        return;
    }
    let url = WebviewUrl::App(format!("index.html?window={id}").into());
    let machine = machines::machine_of(label).map_or(machines::LOCAL, |(m, _)| m);
    let mut builder = WebviewWindowBuilder::new(app, label, url)
        .title("neohar")
        .focused(takes_focus(machine == machines::LOCAL, asked))
        .initialization_script(gesture_script())
        .on_navigation(navigation_guard(app.clone(), machine.to_owned()));
    // On macOS the window has no title bar of its own. The page moves the window by its command
    // line (`data-tauri-drag-region`), and leaves room at its left for the three buttons. The
    // settings window keeps its title bar.
    #[cfg(target_os = "macos")]
    {
        builder = builder
            .title_bar_style(tauri::TitleBarStyle::Overlay)
            .hidden_title(true)
            .traffic_light_position(tauri::LogicalPosition::new(
                TRAFFIC_LIGHTS.0,
                TRAFFIC_LIGHTS.1,
            ));
    }
    builder = match bounds {
        Some(b) => builder
            .inner_size(f64::from(b.width), f64::from(b.height))
            .position(f64::from(b.x), f64::from(b.y)),
        None => builder.inner_size(DEFAULT_SIZE.0, DEFAULT_SIZE.1),
    };
    match builder.build() {
        Ok(window) => {
            tracing::info!(window = label, "opened a window");
            #[cfg(feature = "selftest")]
            crate::selftest::start(window);
            #[cfg(not(feature = "selftest"))]
            drop(window);
        }
        Err(error) => tracing::error!(%error, window = label, "cannot open a window"),
    }
}

/// Closes the native window of a row that went.
fn close_native(app: &AppHandle, id: &str) {
    if let Some(window) = app.get_webview_window(id) {
        // `destroy` does not ask the page, and it does not send `CloseRequested` again.
        match window.destroy() {
            Ok(()) => tracing::info!(window = id, "closed a window"),
            Err(error) => tracing::warn!(%error, window = id, "cannot close a window"),
        }
    }
}

/// What the host knows of a row of the `windows` projection: its bounds and its client.
#[derive(Clone, Debug, PartialEq)]
struct Row {
    bounds: Option<Bounds>,
    /// The device id of the client that owns the row.
    client: Option<String>,
}

/// The rows of the `windows` projection that the host knows, by id.
type Rows = HashMap<String, Row>;

fn row_from(row: &Value) -> Row {
    Row {
        bounds: row
            .get("bounds")
            .and_then(|b| serde_json::from_value(b.clone()).ok()),
        client: row.get("client").and_then(Value::as_str).map(str::to_owned),
    }
}

/// Whether this app shows the row: a row of its own device. A row without a client
/// has no owner yet; the core of this Mac gives such rows to the app at its start, so the app
/// shows them too. A remote core does not, so there (`local` false) a row without a
/// client is not shown.
fn is_mine(row: &Row, device: Option<&str>, local: bool) -> bool {
    match (&row.client, device) {
        (None, _) => local,
        (Some(client), Some(device)) => client == device,
        (Some(_), None) => false,
    }
}

/// Applies a message of the projection to the rows. Returns false for `resync`.
fn apply(rows: &mut Rows, message: &Value) -> bool {
    match message["type"].as_str() {
        Some("snapshot") => {
            rows.clear();
            if let Some(items) = message["data"]["items"].as_object() {
                for (id, row) in items {
                    rows.insert(id.clone(), row_from(row));
                }
            }
            true
        }
        Some("patch") => {
            for op in message["ops"].as_array().into_iter().flatten() {
                let key = op["key"].as_str().unwrap_or_default().to_owned();
                match op["op"].as_str() {
                    Some("set") => {
                        rows.insert(key, row_from(&op["value"]));
                    }
                    Some("remove") => {
                        rows.remove(&key);
                    }
                    _ => {}
                }
            }
            true
        }
        _ => false,
    }
}

/// Follows the `windows` projection of the core of this Mac: opens a native window for each row
/// of this device and closes the ones that went. When the first snapshot has no row of
/// this device and the app has no window of any other kind, the app asks the core for one. This is
/// the window that a first launch shows, and it is the case when the user closed the last window
/// before the last quit.
///
/// In client mode the app also tells its link client which windows it owns, so that the window
/// requests of the core for them pass its guard.
pub fn follow(
    app: &AppHandle,
    machines: &Arc<Machines>,
    machine: &str,
) -> Result<(), nh_core::RpcError> {
    let backend = machines.backend(machine)?;
    let (tx, rx) = mpsc::unbounded_channel();
    tauri::async_runtime::block_on(backend.subscribe("windows", None, Box::new(Feed(tx.clone()))))?;
    let (app, machine) = (app.clone(), machine.to_owned());
    tauri::async_runtime::spawn(async move { run(app, machine, backend, tx, rx).await });
    Ok(())
}

/// Follows the `windows` projection of a remote machine while its link is up. The windows stay
/// open when the link ends: the caller only stops this task. When the machine has no
/// row of this device, the app asks it for a window.
pub async fn follow_machine(
    app: &AppHandle,
    machines: &Arc<Machines>,
    machine: &str,
) -> Result<(), nh_core::RpcError> {
    let backend = machines.backend(machine)?;
    let (tx, rx) = mpsc::unbounded_channel();
    backend
        .subscribe("windows", None, Box::new(Feed(tx.clone())))
        .await?;
    run(app.clone(), machine.to_owned(), backend, tx, rx).await;
    Ok(())
}

/// The loop of both: rows in, native windows out.
async fn run(
    app: AppHandle,
    machine: String,
    backend: Backend,
    tx: mpsc::UnboundedSender<Value>,
    mut rx: mpsc::UnboundedReceiver<Value>,
) {
    let local = machine == LOCAL;
    let device = backend.device().map(|d| d.to_string());
    let mut rows = Rows::new();
    let mut first = true;
    // The rows over the limit of windows, at the last message.
    let mut dropped = 0;
    while let Some(message) = rx.recv().await {
        if !apply(&mut rows, &message) {
            // The core no longer has the patches. Subscribe again for a snapshot.
            if backend
                .subscribe("windows", None, Box::new(Feed(tx.clone())))
                .await
                .is_err()
            {
                tracing::error!(machine, "cannot follow the windows again");
            }
            continue;
        }
        if let (Some(client), Some(device)) = (backend.link(), device.as_deref()) {
            client.set_owned_windows(crate::backend::owned_windows(
                rows.iter().map(|(id, row)| (id, row.client.as_deref())),
                device,
            ));
        }
        // A core of this Mac gives the rows without an owner to the app. A remote core
        // does not: a row without an owner there is for the tools of that machine.
        let (rows_shown, over) = shown_rows(&rows, device.as_deref(), local);
        if over != dropped {
            dropped = over;
            if over > 0 {
                tracing::warn!(
                    machine,
                    over,
                    "a machine has more windows than the app opens for one machine"
                );
            }
        }
        let wanted: Vec<(String, String, Option<Bounds>)> = rows_shown
            .into_iter()
            .map(|(id, bounds)| (machines::label_of(&machine, &id), id, bounds))
            .collect();
        let (shown, of) = (app.clone(), machine.clone());
        // The link knows whether the person asked for a window of the machine just now.
        let link = backend.link().cloned();
        let _ = app.run_on_main_thread(move || {
            let asked = || link.as_ref().is_some_and(|link| link.take_window_request());
            for (label, id, bounds) in &wanted {
                open_native(&shown, label, id, *bounds, &asked);
            }
            // Only the windows of this machine close: those of another machine are not in `wanted`.
            for label in shown.webview_windows().into_keys() {
                if machines::machine_of(&label).is_some_and(|(m, _)| m == of)
                    && !wanted.iter().any(|(wanted, _, _)| *wanted == label)
                {
                    close_native(&shown, &label);
                }
            }
        });
        if first {
            first = false;
            let alone = !local || app.webview_windows().is_empty();
            if wanted_none(&rows, device.as_deref(), local) && alone {
                open_first_window(&backend).await;
            }
        }
    }
}

/// The most native windows of one remote machine. Its core can add rows without end, and each
/// row would be a webview of this Mac.
const MAX_REMOTE_WINDOWS: usize = 16;

/// The rows that the app opens as native windows, by id in order, and the number of rows over the
/// limit. A row must be one of this device, and its id must be a window id in its usual form:
/// else its label would name no window of a machine, and its calls would go to the core of this
/// Mac. A remote machine gets at most [`MAX_REMOTE_WINDOWS`]: the oldest rows, by their ULIDs.
fn shown_rows(
    rows: &Rows,
    device: Option<&str>,
    local: bool,
) -> (Vec<(String, Option<Bounds>)>, usize) {
    let mut shown: Vec<(String, Option<Bounds>)> = rows
        .iter()
        .filter(|(id, row)| {
            is_mine(row, device, local)
                && id
                    .parse::<WindowId>()
                    .is_ok_and(|window| window.to_string() == **id)
        })
        .map(|(id, row)| (id.clone(), row.bounds))
        .collect();
    shown.sort_by(|a, b| a.0.cmp(&b.0));
    let over = if local {
        0
    } else {
        shown.len().saturating_sub(MAX_REMOTE_WINDOWS)
    };
    shown.truncate(shown.len() - over);
    (shown, over)
}

/// Whether no row of the projection is a row of this device.
fn wanted_none(rows: &Rows, device: Option<&str>, local: bool) -> bool {
    !rows.values().any(|row| is_mine(row, device, local))
}

/// Asks the core for a window of this device. The `windows` projection then names it. A client
/// owns the new window at once, because a request of the core for it can come before the row.
pub async fn open_first_window(backend: &Backend) {
    match backend.rpc("window.open", json!({})).await {
        Ok(opened) => {
            let id = opened["window"]
                .as_str()
                .and_then(|w| w.parse::<WindowId>().ok());
            if let (Some(client), Some(id)) = (backend.link(), id) {
                client.own_window(id, true);
            }
        }
        Err(error) => tracing::error!(%error, "the core did not open a window"),
    }
}

/// Brings the app to the front: a window gets focus, and a minimized one is shown. When no window
/// is open, the core opens one.
pub fn focus_any_window(app: &AppHandle) {
    let windows = app.webview_windows();
    let target = windows
        .values()
        .find(|w| w.is_focused().unwrap_or(false))
        .or_else(|| windows.values().next());
    match target {
        Some(window) => {
            let _ = window.unminimize();
            let _ = window.show();
            let _ = window.set_focus();
        }
        None => {
            if let Some(backend) = machines::of(app).map(|m| m.local().clone()) {
                tauri::async_runtime::spawn(async move { open_first_window(&backend).await });
            }
        }
    }
}

/// Whether a native window of `machine` is open.
pub fn has_window_of(app: &AppHandle, machine: &str) -> bool {
    app.webview_windows()
        .keys()
        .any(|label| machines::machine_of(label).is_some_and(|(m, _)| m == machine))
}

/// The machine and the id of the window that has focus, if it shows a window of a layout.
pub fn focused_window(app: &AppHandle) -> Option<(String, WindowId)> {
    app.webview_windows()
        .into_values()
        .filter(|w: &WebviewWindow| w.is_focused().unwrap_or(false))
        .find_map(|w| machines::machine_of(w.label()).map(|(m, id)| (m.to_owned(), id)))
}

/// The window to use for something that names a machine: the window in focus when it shows that
/// machine, else the first window of that machine.
pub fn window_of(app: &AppHandle, machine: &str) -> Option<WindowId> {
    focused_window(app)
        .filter(|(focused, _)| focused == machine)
        .map(|(_, id)| id)
        .or_else(|| {
            app.webview_windows().into_keys().find_map(|label| {
                machines::machine_of(&label)
                    .filter(|(m, _)| *m == machine)
                    .map(|(_, id)| id)
            })
        })
}

/// A window of the layout that is open, if any.
pub fn first_layout_window(app: &AppHandle) -> Option<WindowId> {
    app.webview_windows()
        .into_keys()
        .find_map(|label| machines::local_window(&label))
}

/// The id of the window that has focus, if the focused window shows a window of the layout.
pub fn focused_layout_window(app: &AppHandle) -> Option<WindowId> {
    app.webview_windows()
        .into_values()
        .filter(|w: &WebviewWindow| w.is_focused().unwrap_or(false))
        .find_map(|w| machines::local_window(w.label()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bounds(x: i32, y: i32, width: u32, height: u32) -> Option<Bounds> {
        Some(Bounds {
            x,
            y,
            width,
            height,
        })
    }

    #[test]
    fn a_snapshot_and_its_patches_give_the_rows_of_the_windows() {
        let mut rows = Rows::new();
        let id = "01ARZ3NDEKTSV4RRFFQ69G5FAV";
        assert!(apply(
            &mut rows,
            &json!({ "type": "snapshot", "version": 1, "data": {
                "items": { id: { "bounds": { "x": 10, "y": 20, "width": 800, "height": 600 },
                                 "client": "01ARZ3NDEKTSV4RRFFQ69G5FAX" },
                           "01ARZ3NDEKTSV4RRFFQ69G5FAW": {} },
                "order": [id] } })
        ));
        assert_eq!(rows[id].bounds, bounds(10, 20, 800, 600));
        assert_eq!(
            rows[id].client.as_deref(),
            Some("01ARZ3NDEKTSV4RRFFQ69G5FAX")
        );
        assert_eq!(rows["01ARZ3NDEKTSV4RRFFQ69G5FAW"].bounds, None);

        assert!(apply(
            &mut rows,
            &json!({ "type": "patch", "version": 2, "ops": [
                { "op": "set", "key": id, "value": { "bounds": { "x": 0, "y": 0, "width": 1, "height": 2 } } },
                { "op": "remove", "key": "01ARZ3NDEKTSV4RRFFQ69G5FAW" },
                { "op": "order", "keys": [id] } ] })
        ));
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[id].bounds, bounds(0, 0, 1, 2));

        // `resync` is not a message that changes the rows.
        assert!(!apply(&mut rows, &json!({ "type": "resync" })));
        assert_eq!(rows.len(), 1);
    }

    #[test]
    fn a_remote_window_takes_the_focus_only_when_the_person_asked_for_it() {
        let asked = std::cell::Cell::new(0);
        let request = |answer: bool| {
            let asked = &asked;
            move || {
                asked.set(asked.get() + 1);
                answer
            }
        };
        // A window of this Mac takes the focus, and takes no request of a link.
        assert!(takes_focus(true, &request(false)));
        assert_eq!(asked.get(), 0);
        // A remote window takes it after "New window" or the palette on this Mac.
        assert!(takes_focus(false, &request(true)));
        // A window that the remote core opens by itself does not.
        assert!(!takes_focus(false, &request(false)));
        assert_eq!(asked.get(), 2);
    }

    #[test]
    fn a_remote_machine_gets_at_most_sixteen_windows_with_window_ids() {
        let me = "01ARZ3NDEKTSV4RRFFQ69G5FAX";
        let row = || Row {
            bounds: None,
            client: Some(me.to_owned()),
        };
        let id = |n: u128| WindowId::new(ulid::Ulid::from_parts(1, n)).to_string();
        let mut rows = Rows::new();
        for n in 0..40 {
            rows.insert(id(n), row());
        }
        // An id that is no window id would give a label that names no machine.
        for bad in ["x", "../settings", "settings", &id(1).to_ascii_lowercase()] {
            rows.insert(bad.to_owned(), row());
        }
        let (shown, over) = shown_rows(&rows, Some(me), false);
        assert_eq!((shown.len(), over), (16, 24));
        let ids: Vec<String> = shown.into_iter().map(|(id, _)| id).collect();
        assert_eq!(ids, (0..16).map(id).collect::<Vec<_>>(), "the oldest rows");
        // The core of this Mac has no limit, but the ids must be window ids there too.
        let (shown, over) = shown_rows(&rows, Some(me), true);
        assert_eq!((shown.len(), over), (40, 0));
    }

    #[test]
    fn the_gesture_token_is_random_text_that_a_script_can_hold() {
        let token = gesture();
        assert_eq!(token.len(), 52);
        assert!(token.bytes().all(|b| b.is_ascii_alphanumeric()), "{token}");
        assert_eq!(gesture(), token, "one token for the run");
        assert!(gesture_script().contains(&format!("value: '{token}'")));
        // The random parts of the two ULIDs differ.
        assert_ne!(&token[10..26], &token[36..52]);
    }

    #[test]
    fn the_app_shows_only_the_rows_of_its_own_device() {
        let row = |client: Option<&str>| Row {
            bounds: None,
            client: client.map(str::to_owned),
        };
        let me = Some("01ARZ3NDEKTSV4RRFFQ69G5FAX");
        assert!(is_mine(&row(me), me, true));
        assert!(!is_mine(&row(Some("01ARZ3NDEKTSV4RRFFQ69G5FAY")), me, true));
        // A row without an owner waits for the first client, and the app takes it.
        assert!(is_mine(&row(None), me, true));
        // A remote core has no unowned rows for this app.
        assert!(!is_mine(&row(None), me, false));
        let mut rows = Rows::new();
        rows.insert("a".to_owned(), row(Some("01ARZ3NDEKTSV4RRFFQ69G5FAY")));
        assert!(
            wanted_none(&rows, me, true),
            "a window of another client is not one of ours"
        );
        rows.insert("b".to_owned(), row(me));
        assert!(!wanted_none(&rows, me, true));
    }
}
