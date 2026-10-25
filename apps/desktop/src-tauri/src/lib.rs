//! The Tauri host of neohar: the windows, and the three commands that connect a webview
//! to the core.
//!
//! - [`commands`]: `rpc`, `subscribe` and `stream`, the only commands that a webview may call.
//! - [`backend`]: the core that the app speaks to. At launch the app tries `link.sock` of its
//!   session. When a core answers, for example `nh daemon`, the app is its client and starts no
//!   core. Else the app starts its own core.
//! - [`windows`]: a native window for each row of the `windows` projection, and what the host
//!   tells the core about it.
//! - [`host`]: what the core asks of the app (clipboard, addresses, notifications, badge, tray).
//! - [`notifier`], [`tray`], [`badge`], [`hotkey`] and [`reach`]: the native surfaces and what a
//!   click on them asks of the core.
//! - [`menu`] and the lifecycle below: quit, quit and stop everything, new window, close window.
//! - [`protocol`] and [`policy`]: the `nh://` scheme and where a webview may go.
//!
//! A core of this process runs on its own runtime, apart from the one of Tauri, and it starts
//! before the first window. A link client runs on the runtime of Tauri.

mod askpass;
mod backend;
mod badge;
mod commands;
mod config;
mod deeplink;
#[allow(
    unsafe_code,
    reason = "CoreText finds the files of the fonts through functions that objc2 declares unsafe"
)]
mod fonts;
mod host;
mod hotkey;
mod machine_host;
mod machines;
mod menu;
#[allow(
    unsafe_code,
    reason = "the delegate of the notification centre is declared with a macro of objc2 that expands to unsafe code"
)]
mod notifier;
mod phone_machines;
mod policy;
mod protocol;
mod reach;
#[allow(
    unsafe_code,
    reason = "objc2 declares the observers of the notification centres and the names of NSWorkspace unsafe"
)]
mod screen;
#[cfg(feature = "selftest")]
mod selftest;
mod tray;
mod windows;

use std::{
    sync::{
        Arc, Mutex, PoisonError,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use nh_core::Core;
use serde_json::json;
use tauri::{AppHandle, Manager, RunEvent};
use tauri_plugin_deep_link::DeepLinkExt;
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};

use crate::{
    backend::{Backend, Launch, Linked},
    commands::{rpc, stream, subscribe},
    host::TauriHost,
    windows::Windows,
};

/// How long a child gets to end when the user stops everything, before it is killed.
const STOP_GRACE: Duration = Duration::from_secs(5);

/// Whether the app is ending because the user asked, and how.
#[derive(Default)]
struct Lifecycle {
    /// A quit menu item asked for the end. Without it, the last window that closes does not end
    /// the app.
    quitting: AtomicBool,
    /// The user chose "Quit and stop everything": the children are gone, and the quit leaves no
    /// lease to end.
    stopped: AtomicBool,
}

/// Opens a window for the first launch, or after the user closed the last one and used the dock.
fn open_window(app: &AppHandle) {
    let Some(backend) = backend::of(app) else {
        return;
    };
    tauri::async_runtime::spawn(async move { windows::open_first_window(&backend).await });
}

/// "Quit and stop everything…": a confirmation, then every program gets `Term` and
/// `Hup`, the ones that stay get `Kill`, and the app ends. In client mode the core of the link
/// stops its programs with `session.stop_everything`. When it cannot, a dialog says
/// why and the app stays open.
fn quit_and_stop(app: &AppHandle) {
    let Some(backend) = backend::of(app) else {
        return;
    };
    let app = app.clone();
    app.dialog()
        .message(
            "Every program in every pane will be stopped. Agents that work now stop with them.",
        )
        .title("Quit and stop everything?")
        .kind(MessageDialogKind::Warning)
        .buttons(MessageDialogButtons::OkCancelCustom(
            "Stop everything".to_owned(),
            "Cancel".to_owned(),
        ))
        .show({
            let app = app.clone();
            move |confirmed| {
                if !confirmed {
                    return;
                }
                let local = matches!(backend, Backend::Local(_));
                if local {
                    let lifecycle = app.state::<Lifecycle>();
                    lifecycle.quitting.store(true, Ordering::SeqCst);
                    lifecycle.stopped.store(true, Ordering::SeqCst);
                }
                let app = app.clone();
                tauri::async_runtime::spawn(async move {
                    match backend.stop_everything(STOP_GRACE).await {
                        Ok(report) => {
                            tracing::info!(report, "everything stopped");
                            let lifecycle = app.state::<Lifecycle>();
                            lifecycle.quitting.store(true, Ordering::SeqCst);
                            lifecycle.stopped.store(true, Ordering::SeqCst);
                            app.exit(0);
                        }
                        Err(error) => {
                            tracing::warn!(%error, "the core of the link did not stop everything");
                            alert(
                                &app,
                                "Nothing was stopped",
                                format!(
                                    "The core that serves this session did not stop its \
                                     programs: {error}. neohar stays open, and the programs \
                                     keep running."
                                ),
                                |_| {},
                            );
                        }
                    }
                });
            }
        });
}

/// Asks the local core for `machine.connect`, `machine.disconnect` or `machine.open_window` of an
/// item of the Machines menu. A failure shows its reason: the person asked for it.
fn machine_call(app: &AppHandle, method: &'static str, machine: String) {
    let Some(backend) = backend::of(app) else {
        return;
    };
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let params = json!({ "machine": machine });
        if let Err(error) = backend.rpc(method, params).await {
            tracing::warn!(%error, method, machine, "a Machines menu item did not work");
            alert(
                &app,
                &format!("Cannot reach {machine}"),
                error.message,
                |_| {},
            );
        }
    });
}

/// Handles an item of the menu, and of the menu of the tray.
fn on_menu(app: &AppHandle, id: &str) {
    match id {
        menu::QUIT => {
            app.state::<Lifecycle>()
                .quitting
                .store(true, Ordering::SeqCst);
            app.exit(0);
        }
        menu::QUIT_AND_STOP => quit_and_stop(app),
        // A new window shows the machine of the window in focus.
        menu::NEW_WINDOW => match windows::focused_window(app) {
            Some((machine, _)) if machine != machines::LOCAL => {
                machine_call(app, "machine.open_window", machine);
            }
            _ => open_window(app),
        },
        menu::CLOSE_WINDOW => {
            if let (Some((machine, window)), Some(all)) =
                (windows::focused_window(app), machines::of(app))
                && let Ok(backend) = all.backend(&machine)
            {
                tauri::async_runtime::spawn(async move {
                    let params = json!({ "window": window.to_string() });
                    if let Err(error) = backend.call(Some(window), "window.close", params).await {
                        tracing::warn!(%error, "the core did not close the window");
                    }
                });
            }
        }
        menu::SETTINGS => windows::open_settings(app),
        other => match menu::machine_action(other) {
            Some((method, machine)) => machine_call(app, method, machine.to_owned()),
            None => tracing::debug!(item = other, "a menu item without an action"),
        },
    }
}

/// The link of a client of a daemon on this Mac ended while the app runs: the core died or was
/// stopped. The app says that the core is gone and quits. A link to a remote machine comes
/// back by itself and does not come here.
fn watch_link(app: &AppHandle, linked: &Linked, session: String) {
    let (app, client) = (app.clone(), Arc::clone(&linked.client));
    tauri::async_runtime::spawn(async move {
        let reason = client.closed().await;
        if app.state::<Lifecycle>().quitting.load(Ordering::SeqCst) {
            return;
        }
        tracing::error!(reason, "client mode: the link to the core ended");
        app.state::<Lifecycle>()
            .quitting
            .store(true, Ordering::SeqCst);
        let shown = app.clone();
        alert(
            &app,
            "The core of neohar is gone",
            format!(
                "The core that serves the session {session:?} is gone ({reason}). neohar quits \
                 now. The panes keep running when their holders run, and the next launch shows \
                 them."
            ),
            move |_| shown.exit(0),
        );
    });
}

/// Shows why the app does not start, then ends it with exit code 2. The event loop must run for
/// the dialog, so setup returns and the app has no window.
fn refuse(app: &AppHandle, message: &str) {
    eprintln!("neohar: {message}");
    tracing::error!(message, "the app does not start");
    app.state::<Lifecycle>()
        .quitting
        .store(true, Ordering::SeqCst);
    let shown = app.clone();
    alert(app, "neohar cannot start", message.to_owned(), move |_| {
        shown.exit(2);
    });
}

/// Shows an error. The dialog is a sheet of a window of the app when one is open: without a
/// parent, the system can put the sheet on a window that nobody sees and end it at once.
fn alert(app: &AppHandle, title: &str, message: String, then: impl FnOnce(bool) + Send + 'static) {
    let mut dialog = app
        .dialog()
        .message(message)
        .title(title)
        .kind(MessageDialogKind::Error);
    let parent = windows::focused_layout_window(app)
        .or_else(|| windows::first_layout_window(app))
        .and_then(|id| app.get_webview_window(&id.to_string()));
    if let Some(window) = parent {
        dialog = dialog.parent(&window);
    }
    dialog.show(then);
}

/// The manager of the links to the machines of `config.toml`, for a core of this process.
/// Each remote core gets a host that knows its label.
fn machine_manager(
    host: Arc<TauriHost>,
    core: &nh_core::CoreHandle,
    paths: &nh_proto::paths::Env,
    session: &str,
    nh_path: &std::path::Path,
    device: &nh_core::ClientInfo,
    app: &tauri::App,
) -> Result<nh_remote::machines::MachineManager, String> {
    let runtime_dir = paths.runtime_root().map_err(|error| error.to_string())?;
    let api_socket = paths
        .api_socket(session)
        .map_err(|error| error.to_string())?;
    let core = core.clone();
    Ok(nh_remote::machines::MachineManager::new(
        nh_remote::machines::ManagerConfig {
            runtime_dir,
            nh: nh_path.to_owned(),
            api_socket,
            session: session.to_owned(),
            device: device.device,
            device_name: device.name.clone(),
            app_version: app.package_info().version.to_string(),
            host_for: Arc::new(move |label| {
                Arc::new(
                    machine_host::MachineHost::new(label, Arc::clone(&host))
                        .pushing_to(core.clone()),
                ) as Arc<dyn nh_core::Host>
            }),
            options: nh_remote::ClientOptions::default(),
            ssh_program: None,
            ssh_env: Vec::new(),
        },
        tauri::async_runtime::handle().inner().clone(),
    ))
}

/// Connects to the core of the session, or starts one, and runs the app until a quit item ends
/// it.
///
/// # Panics
///
/// When the app cannot run. A core that cannot start shows a dialog, and the app then ends with exit
/// code 2. When another core holds the session, the app first tries its `link.sock` for
/// [`backend::BUSY_RETRY_FOR`] and is its client when it answers.
pub fn run() {
    let mut config = match config::core_config() {
        Ok(config) => config,
        Err(message) => {
            eprintln!("neohar: {message}");
            std::process::exit(2);
        }
    };
    config::init_logging(&config);
    let host = Arc::new(TauriHost::new(config.nh_path.clone()));
    let device = match config::device(&config) {
        Ok(device) => device,
        Err(message) => {
            eprintln!("neohar: {message}");
            std::process::exit(2);
        }
    };
    tracing::info!(device = %device.device, name = device.name, "the device of this app");
    // The core lives as long as the app. The exit handler stops it. It starts in `setup`, after
    // the plugin for a single instance has had its say: a second launch hands over to the
    // first one and ends before it connects to a core or starts one.
    let core: Arc<Mutex<Option<Core>>> = Arc::default();
    let started = Arc::clone(&core);
    // The server of `link.sock` lives as long as the core and ends before it.
    let link: Arc<Mutex<Option<nh_remote::LinkServer>>> = Arc::default();
    let linked = Arc::clone(&link);
    // The links to the remote machines of `config.toml`. Only a core of this process
    // has them; the exit handler ends them with `Bye` before the core ends.
    let machine_links: Arc<Mutex<Option<nh_remote::machines::MachineManager>>> = Arc::default();
    let manager_slot = Arc::clone(&machine_links);
    let setup_host = Arc::clone(&host);

    tauri::Builder::default()
        // A second launch focuses the running app, and its arguments may be links. The plugin must
        // come first.
        .plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
            windows::focus_any_window(app);
            deeplink::open_all(app, args.iter().map(String::as_str));
        }))
        .plugin(tauri_plugin_deep_link::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(hotkey::plugin())
        .plugin(tauri_plugin_opener::init())
        .manage(host)
        .manage(Windows::default())
        .manage(Lifecycle::default())
        .manage(tray::TrayState::default())
        .register_asynchronous_uri_scheme_protocol("nh", protocol::handle)
        .menu(menu::build)
        .on_menu_event(|app, event| on_menu(app, event.id().as_ref()))
        .on_window_event(|window, event| window.state::<Windows>().on_event(window, event))
        .invoke_handler(tauri::generate_handler![rpc, subscribe, stream])
        .setup(move |app| {
            config::link_sidecar(&config);
            let (paths, session) = (config.paths.clone(), config.session.clone());
            let nh_path = config.nh_path.clone();
            // A core of this session may run already, for example `nh daemon`.
            let hello = nh_remote::ClientHello {
                device: device.device,
                name: device.name.clone(),
                app_version: app.package_info().version.to_string(),
            };
            let connect_link = || {
                let (handle, host, hello) =
                    (app.handle().clone(), Arc::clone(&setup_host), hello.clone());
                let (paths, session) = (paths.clone(), session.clone());
                async move {
                    // It checks the runtime root first: the core of the app has not checked it yet.
                    backend::connect(&paths, &session, hello, move || {
                        // The core asks the host for its notification capabilities after
                        // `Welcome`, so the notification centre must be up.
                        host.attach(handle);
                        host as Arc<dyn nh_core::Host>
                    })
                    .await
                }
            };
            let device_id = device.device;
            let client_of = |client: nh_remote::LinkClient| {
                tracing::info!(
                    session,
                    core = client.welcome().core_version,
                    host = client.welcome().hostname,
                    "client mode: a core serves the session, so the app starts no core and is its \
                     client through link.sock"
                );
                let linked = Linked::new(client, device_id);
                watch_link(app.handle(), &linked, session.clone());
                Backend::Link(linked)
            };
            let outcome = tauri::async_runtime::block_on(connect_link());
            let backend = match backend::launch_of(outcome, &session) {
                Launch::Client(client) => client_of(client),
                Launch::Refuse(message) => {
                    refuse(app.handle(), &message);
                    return Ok(());
                }
                Launch::StartCore(why) => {
                    tracing::info!(
                        session,
                        why,
                        "local mode: no core answers on link.sock, so the app starts its core"
                    );
                    // Only a core of this process needs the login environment.
                    config.base_env = nh_core::login_env::capture();
                    config.host = Some(nh_core::InProcessHost::new(
                        device.clone(),
                        Arc::clone(&setup_host) as Arc<dyn nh_core::Host>,
                    ));
                    match Core::start(config) {
                        Ok(core) => {
                            let handle = core.handle();
                            // A client of this machine connects here: a bridge of ssh, or an app
                            // on this Mac. The app works without it.
                            match nh_remote::serve(handle.clone(), &paths, &session) {
                                Ok(server) => {
                                    *linked.lock().unwrap_or_else(PoisonError::into_inner) =
                                        Some(server);
                                }
                                Err(error) => tracing::warn!(%error, "link.sock does not start"),
                            }
                            match machine_manager(
                                Arc::clone(&setup_host),
                                &handle,
                                &paths,
                                &session,
                                &nh_path,
                                &device,
                                app,
                            ) {
                                Ok(manager) => {
                                    handle.set_machine_links(Arc::new(manager.clone()));
                                    *manager_slot.lock().unwrap_or_else(PoisonError::into_inner) =
                                        Some(manager);
                                }
                                Err(message) => {
                                    tracing::warn!(message, "the machines of config.toml are off");
                                }
                            }
                            *started.lock().unwrap_or_else(PoisonError::into_inner) = Some(core);
                            Backend::Local(handle)
                        }
                        // A core that starts holds the lock of the session before it serves
                        // link.sock, and a busy core can refuse a connection for a moment. The app
                        // never starts a second core: it waits for that one.
                        Err(nh_core::CoreError::ApiSocketInUse(path)) => {
                            tracing::info!(
                                session,
                                path = %path.display(),
                                "another core holds the session, so the app waits for its \
                                 link.sock"
                            );
                            let found = tauri::async_runtime::block_on(backend::wait_for_link(
                                connect_link,
                                &session,
                                backend::BUSY_RETRY_FOR,
                                backend::BUSY_RETRY_EVERY,
                            ));
                            match found {
                                Ok(client) => client_of(client),
                                Err(message) => {
                                    refuse(app.handle(), &message);
                                    return Ok(());
                                }
                            }
                        }
                        // A person who started the app from Finder sees why it stops.
                        Err(error) => {
                            refuse(app.handle(), &format!("the core does not start: {error}"));
                            return Ok(());
                        }
                    }
                }
            };
            // The listener of the phone follows `[phone]`, for the core in this process only. A
            // client of a daemon has none: the daemon serves the phone.
            if let Backend::Local(core) = &backend {
                let phone = nh_web::PhoneService::start(core.clone());
                // The phone gets the fonts of the person from CoreText, as the windows do.
                phone.set_font_source(std::sync::Arc::new(fonts::CoreTextFonts));
                app.manage(phone);
                // A paired phone with Notifications keeps the Mac awake while agents work, and a
                // lock of the screen counts as no focus for its push.
                core.keep_awake_for_phones();
                screen::follow(core.clone());
            }
            app.manage(backend.clone());
            if matches!(backend, Backend::Local(_)) {
                setup_host.attach(app.handle().clone());
            }
            // The machines of the app: this one, and the ones of `config.toml` that are up.
            // Their windows open here.
            let manager = manager_slot
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .clone();
            let machines = Arc::new(machines::Machines::new(backend.clone(), manager));
            app.manage(Arc::clone(&machines));
            // The phone reaches the other machines through this app.
            if let Some(phone) = app.try_state::<nh_web::PhoneService>() {
                phone.set_machines(Arc::new(phone_machines::PhoneMachines(Arc::clone(
                    &machines,
                ))));
            }
            machines::start(app.handle(), &machines)?;
            hotkey::follow(app.handle(), &backend)?;
            // Links that the system hands to the running app (macOS) or that start it.
            let links = app.handle().clone();
            app.deep_link().on_open_url(move |event| {
                deeplink::open_all(&links, event.urls().iter().map(tauri::Url::as_str));
            });
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("the app builds")
        .run(move |app, event| match event {
            RunEvent::ExitRequested { api, code, .. } => {
                // The last window that closes does not end the app: agents keep running and the
                // notifications keep coming. A quit item sets `quitting` first.
                if code.is_none() && !app.state::<Lifecycle>().quitting.load(Ordering::SeqCst) {
                    api.prevent_exit();
                }
            }
            #[cfg(target_os = "macos")]
            RunEvent::Reopen {
                has_visible_windows: false,
                ..
            } => open_window(app),
            RunEvent::Exit => {
                // The end of the link is no loss of the core.
                app.state::<Lifecycle>()
                    .quitting
                    .store(true, Ordering::SeqCst);
                match backend::of(app) {
                    Some(Backend::Local(handle)) => {
                        if std::env::var_os("NEOHAR_STOP_ON_EXIT").is_some() {
                            // For a run that must leave nothing behind, for example a test.
                            handle.stop_panes_blocking(Duration::from_secs(2));
                        } else if !app.state::<Lifecycle>().stopped.load(Ordering::SeqCst) {
                            // A quit leaves the panes running. The core stores a
                            // checkpoint of each pane and ends its lease, and the next launch
                            // brings the panes back.
                            let report = handle.quit_blocking(Duration::from_secs(5));
                            tracing::info!(?report, "the core ended the leases");
                        }
                    }
                    // A client ends its link with `Bye`. The core of the link and its panes keep
                    // running. `NEOHAR_STOP_ON_EXIT` is only for a core of this
                    // process.
                    Some(backend @ Backend::Link(_)) => backend.end_link_blocking(),
                    None => {}
                }
                if let Some(manager) = machine_links
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .take()
                {
                    manager.shutdown();
                }
                drop(link.lock().unwrap_or_else(PoisonError::into_inner).take());
                drop(core.lock().unwrap_or_else(PoisonError::into_inner).take());
            }
            _ => {}
        });
}
