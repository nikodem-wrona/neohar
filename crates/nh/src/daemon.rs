//! `nh daemon`: the core without windows, a role of the shipped sidecar.
//!
//! ```text
//! nh daemon --detach [--session s]     the product form
//! nh daemon stop [--session s]         `session.shutdown`, then wait until the daemon ended
//! nh daemon [--session s] [--dev-ws] [--phone-port p]   the foreground form: the seam of the tests
//! ```
//!
//! **Detached.** The daemon forks twice and calls `setsid`, as a holder does ([`detach`]). It
//! writes its log to `<data>/logs/core.log`. When the core serves `api.sock` and
//! `link.sock`, it writes `READY <pid>` to its ready pipe: the stdout of the caller. A daemon that
//! cannot start writes `ERROR <reason>`, for example when another core serves the session
//! (`ApiSocketInUse`). It runs until `nh daemon stop` or `session.shutdown`. Then it
//! checkpoints each pane, ends the leases and exits, and the panes keep running. A client that
//! disconnects never stops it. While agents work, it keeps the machine awake
//! (`daemon.prevent_sleep`).
//!
//! **Foreground (test seam).** Without `--detach` the daemon stays the child of its caller. It
//! prints one line of JSON that tells where the sockets are, and logs to stderr:
//!
//! ```text
//! {"pid":123,"session":"default","api_socket":"/…/api.sock","link_socket":"/…/link.sock","ws_port":50123,"token":"…"}
//! ```
//!
//! With `--dev-ws` (a build with the feature `dev-ws`) it opens the dev WebSocket on a
//! free port of `127.0.0.1`. With `--phone-port <port>` (the same builds) the listener of the phone
//! serves that port (0 is a free one) with the origin `http://127.0.0.1:<port>`, and the line has
//! `phone_port`. Without it, the listener follows `[phone]`, as in a detached daemon. When its standard input closes, it stops every program of its panes
//! (the children get `Term` and `Hup`, then `Kill`) and exits, so that a test leaves nothing
//! behind. `session.shutdown` ends it as it ends a detached daemon. `tests/ui` and the tests of the
//! transport use this form.
//!
//! A debug build refuses to run unless all three directory variables (`NEOHAR_CONFIG_DIR`,
//! `NEOHAR_DATA_DIR`, `NEOHAR_RUNTIME_DIR`) are set.

mod detach;

use std::{
    fs::OpenOptions,
    io::{Read, Write},
    path::Path,
    process::ExitCode,
    sync::mpsc,
    time::{Duration, Instant},
};

use nh_core::{Core, CoreConfig, Tuning, logging::LogTo};
use nh_proto::paths::{Env, validate_session_name};
use serde_json::{Value, json};

use crate::cli::client::{CallError, Client};

/// How long the children get to end when the foreground daemon stops.
const GRACE: Duration = Duration::from_secs(2);

/// How long the quit may take: a checkpoint of each pane, then the end of each lease.
const QUIT_WAIT: Duration = Duration::from_secs(5);

/// How long `nh daemon stop` waits for the daemon to end.
const STOP_WAIT: Duration = Duration::from_secs(20);

/// The variables that move the three roots of the directories.
const ROOTS: [&str; 3] = ["NEOHAR_CONFIG_DIR", "NEOHAR_DATA_DIR", "NEOHAR_RUNTIME_DIR"];

/// The session when neither `--session` nor `NEOHAR_SESSION` names one, as for the CLI.
const DEFAULT_SESSION: &str = "default";

struct Args {
    session: String,
    detach: bool,
    dev_ws: bool,
    /// The dev WebSocket serves from a link client of this daemon.
    through_link: bool,
    /// The port of the listener of the phone in a development run.
    phone_port: Option<u16>,
}

/// Why the daemon ends.
enum End {
    /// The standard input of the foreground daemon closed: stop every program.
    InputClosed,
    /// `session.shutdown` or `nh daemon stop`: the panes keep running.
    Shutdown,
}

pub fn main(args: &[String]) -> ExitCode {
    if args.first().map(String::as_str) == Some("stop") {
        return stop(&args[1..]);
    }
    let args = match parse(args) {
        Ok(args) => args,
        Err(message) => return fail(&message),
    };
    if let Err(message) = check_roots() {
        return fail(&message);
    }
    if args.detach {
        detached(&args)
    } else {
        foreground(&args)
    }
}

fn parse(args: &[String]) -> Result<Args, String> {
    let mut parsed = Args {
        session: std::env::var("NEOHAR_SESSION").unwrap_or_else(|_| DEFAULT_SESSION.to_owned()),
        detach: false,
        dev_ws: false,
        through_link: false,
        phone_port: None,
    };
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--detach" => parsed.detach = true,
            "--dev-ws" => parsed.dev_ws = true,
            "--dev-ws-through-link" => {
                parsed.dev_ws = true;
                parsed.through_link = true;
            }
            // The port is always a free one, so the value is read and ignored.
            "--port" => {
                args.next();
            }
            "--session" => match args.next() {
                Some(name) => parsed.session.clone_from(name),
                None => return Err("--session needs a name".to_owned()),
            },
            "--phone-port" => match args.next().map(|port| port.parse::<u16>()) {
                Some(Ok(port)) => parsed.phone_port = Some(port),
                _ => return Err("--phone-port needs a port".to_owned()),
            },
            other => return Err(format!("unknown argument {other:?}")),
        }
    }
    validate_session_name(&parsed.session).map_err(|e| e.to_string())?;
    if parsed.detach && parsed.dev_ws {
        return Err("--dev-ws is for the foreground form, without --detach".to_owned());
    }
    if parsed.dev_ws && !cfg!(feature = "dev-ws") {
        return Err("this build has no dev WebSocket (build nh with --features dev-ws)".to_owned());
    }
    if parsed.phone_port.is_some() && (parsed.detach || !cfg!(feature = "dev-ws")) {
        return Err(
            "--phone-port is for the foreground form of a build with --features dev-ws".to_owned(),
        );
    }
    Ok(parsed)
}

/// A debug build needs the three directory overrides.
fn check_roots() -> Result<(), String> {
    if cfg!(debug_assertions) {
        let missing: Vec<_> = ROOTS
            .iter()
            .filter(|name| std::env::var_os(name).is_none_or(|v| v.is_empty()))
            .collect();
        if !missing.is_empty() {
            return Err(format!("a debug build needs {missing:?}"));
        }
    }
    Ok(())
}

/// The configuration of the core of the daemon.
fn core_config(session: &str, paths: &Env) -> Result<CoreConfig, String> {
    let nh_path =
        std::env::current_exe().map_err(|e| format!("cannot find this executable: {e}"))?;
    Ok(CoreConfig {
        session: session.to_owned(),
        paths: paths.clone(),
        nh_path: nh_path.clone(),
        // A process that ssh starts has a short environment.
        base_env: nh_core::login_env::capture(),
        tuning: Tuning::default(),
        // The daemon has no shell of its own: its host requests go to the link clients only,
        // and `nh_path` is the sidecar of the machine.
        host: None,
        pane_tokens: Vec::new(),
    })
}

/// The product form: detach, start, `READY`, run until a shutdown.
fn detached(args: &Args) -> ExitCode {
    let mut ready = match detach::detach() {
        Ok(ready) => ready,
        Err(error) => return fail(&format!("cannot detach: {error}")),
    };
    // Only the daemon is here. Its stdio still names the pipes and the terminal of the caller.
    let paths = Env::from_process();
    if let Err(error) = quiet_stdio(&paths, &args.session) {
        ready.error(&error);
        return ExitCode::from(2);
    }
    nh_core::logging::init(&paths, &args.session, LogTo::File);
    let files = nh_web::raise_fd_limit();
    tracing::debug!(?files, "the soft limit of open files");
    let config = match core_config(&args.session, &paths) {
        Ok(config) => config,
        Err(error) => {
            ready.error(&error);
            return ExitCode::from(2);
        }
    };
    let core = match Core::start(config) {
        Ok(core) => core,
        Err(error) => {
            // `ApiSocketInUse` when another core serves the session: two bridges started a
            // daemon at the same moment, and the other one won.
            tracing::warn!(%error, "the core does not start");
            ready.error(&format!("the core does not start: {error}"));
            return ExitCode::from(2);
        }
    };
    let handle = core.handle();
    let link = match nh_remote::serve(handle.clone(), &paths, &args.session) {
        Ok(link) => link,
        Err(error) => {
            tracing::warn!(%error, "link.sock is not served");
            ready.error(&format!("link.sock is not served: {error}"));
            core.shutdown(QUIT_WAIT);
            return ExitCode::from(2);
        }
    };
    let (tx, rx) = mpsc::channel();
    handle.accept_shutdown(move || {
        let _ = tx.send(End::Shutdown);
    });
    {
        let dropper = link.dropper();
        handle.accept_drop_links(move || dropper.drop_links("dropped by debug.drop_links"));
    }
    handle.prevent_sleep();
    // The listener of the phone follows `[phone]`.
    let phone = nh_web::PhoneService::start(handle.clone());
    tracing::info!(
        pid = std::process::id(),
        session = %args.session,
        "the daemon serves api.sock and link.sock"
    );
    ready.ready(std::process::id());
    // The daemon runs until a client asks it to end. A core that is gone drops the sender.
    let _ = rx.recv();
    drop(phone);
    end(core, link, &End::Shutdown);
    ExitCode::SUCCESS
}

/// Points stdin and stdout at `/dev/null` and stderr at `<data>/logs/daemon.err`, so that the
/// daemon holds no pipe and no terminal of its caller, and a panic message is kept. The working
/// directory becomes `/`.
fn quiet_stdio(paths: &Env, session: &str) -> Result<(), String> {
    let logs = paths
        .data_dir(session)
        .map_err(|e| e.to_string())?
        .join("logs");
    {
        use std::os::unix::fs::DirBuilderExt;
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(&logs)
            .map_err(|e| format!("{}: {e}", logs.display()))?;
    }
    let null = OpenOptions::new()
        .read(true)
        .write(true)
        .open("/dev/null")
        .map_err(|e| format!("/dev/null: {e}"))?;
    let err_path = logs.join("daemon.err");
    let err = {
        use std::os::unix::fs::OpenOptionsExt;
        OpenOptions::new()
            .create(true)
            .append(true)
            .mode(0o600)
            .open(&err_path)
            .map_err(|e| format!("{}: {e}", err_path.display()))?
    };
    rustix::stdio::dup2_stdin(&null)
        .and_then(|()| rustix::stdio::dup2_stdout(&null))
        .and_then(|()| rustix::stdio::dup2_stderr(&err))
        .map_err(|e| format!("stdio: {e}"))?;
    let _ = std::env::set_current_dir("/");
    Ok(())
}

/// The test seam: the JSON line, then run until stdin closes or a shutdown.
fn foreground(args: &Args) -> ExitCode {
    let paths = Env::from_process();
    nh_core::logging::init(&paths, &args.session, LogTo::Stderr);
    let files = nh_web::raise_fd_limit();
    tracing::debug!(?files, "the soft limit of open files");
    let config = match core_config(&args.session, &paths) {
        Ok(config) => config,
        Err(error) => return fail(&error),
    };
    let core = match Core::start(config) {
        Ok(core) => core,
        Err(error) => return fail(&format!("the core does not start: {error}")),
    };
    let handle = core.handle();
    let link = match nh_remote::serve(handle.clone(), &paths, &args.session) {
        Ok(link) => link,
        Err(error) => {
            core.shutdown(QUIT_WAIT);
            return fail(&format!("link.sock is not served: {error}"));
        }
    };
    let mut line = json!({
        "pid": std::process::id(),
        "session": args.session,
        "api_socket": paths
            .api_socket(&args.session)
            .map(|p| p.display().to_string())
            .unwrap_or_default(),
        "link_socket": link.path().display().to_string(),
    });
    // The client of the dev WebSocket through the link. It lives until the daemon stops.
    #[cfg(feature = "dev-ws")]
    let mut link_client = None;
    #[cfg(feature = "dev-ws")]
    if args.dev_ws {
        let served = if args.through_link {
            dev_ws_through_link(&handle, link.path()).map(|(info, client)| {
                link_client = Some(client);
                info
            })
        } else {
            handle.dev_ws_serve().map_err(|e| e.to_string())
        };
        match served {
            Ok(info) => {
                line["ws_port"] = json!(info.port);
                line["token"] = json!(info.token);
            }
            Err(error) => {
                drop(link);
                core.shutdown(QUIT_WAIT);
                return fail(&format!("the dev WebSocket does not start: {error}"));
            }
        }
    }
    // The listener of the phone: a fixed port of a development run, else `[phone]`.
    let phone = match args.phone_port {
        Some(port) => match nh_web::PhoneService::start_dev(handle.clone(), port) {
            Ok((phone, port)) => {
                line["phone_port"] = json!(port);
                phone
            }
            Err(error) => {
                drop(link);
                core.shutdown(QUIT_WAIT);
                return fail(&format!("the phone listener does not start: {error}"));
            }
        },
        None => nh_web::PhoneService::start(handle.clone()),
    };
    let (tx, rx) = mpsc::channel();
    let input = tx.clone();
    handle.accept_shutdown(move || {
        let _ = tx.send(End::Shutdown);
    });
    {
        let dropper = link.dropper();
        handle.accept_drop_links(move || dropper.drop_links("dropped by debug.drop_links"));
    }
    println!("{line}");
    let _ = std::io::stdout().flush();
    // The standard input closes when the test ends.
    let _ = std::thread::Builder::new()
        .name("nh-daemon-stdin".to_owned())
        .spawn(move || {
            let mut sink = [0u8; 256];
            let mut stdin = std::io::stdin();
            while matches!(stdin.read(&mut sink), Ok(n) if n > 0) {}
            let _ = input.send(End::InputClosed);
        });
    let why = rx.recv().unwrap_or(End::InputClosed);
    drop(phone);
    #[cfg(feature = "dev-ws")]
    drop(link_client);
    end(core, link, &why);
    ExitCode::SUCCESS
}

/// Ends the core. A shutdown checkpoints each pane and ends the leases, and the panes keep running.
/// The end of the input of the foreground daemon stops every program first.
fn end(core: Core, link: nh_remote::LinkServer, why: &End) {
    let handle = core.handle();
    match why {
        End::InputClosed => {
            handle.stop_everything_blocking(GRACE, GRACE + Duration::from_secs(10));
        }
        End::Shutdown => {
            let report = handle.quit_blocking(QUIT_WAIT);
            tracing::info!(?report, "the daemon ended the leases");
        }
    }
    // The links end before the core, so that no client sees a core that does not answer.
    drop(link);
    core.shutdown(QUIT_WAIT);
}

/// `nh daemon stop [--session s]`: asks the core of the session to end, and waits until its
/// process is gone. The panes keep running.
fn stop(args: &[String]) -> ExitCode {
    let mut session =
        std::env::var("NEOHAR_SESSION").unwrap_or_else(|_| DEFAULT_SESSION.to_owned());
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--session" => match args.next() {
                Some(name) => session.clone_from(name),
                None => return fail("--session needs a name"),
            },
            other => return fail(&format!("unknown argument {other:?}")),
        }
    }
    let socket = match Env::from_process().api_socket(&session) {
        Ok(socket) => socket,
        Err(error) => return fail(&error.to_string()),
    };
    match stop_at(&socket) {
        Ok(pid) => {
            println!("{}", json!({"stopped": pid, "session": session}));
            ExitCode::SUCCESS
        }
        Err(message) => {
            eprintln!("nh daemon stop: {message}");
            ExitCode::from(1)
        }
    }
}

/// Calls `session.shutdown` on `socket` and waits until the process of the core ended.
fn stop_at(socket: &Path) -> Result<u32, String> {
    let mut client =
        Client::connect(socket, Duration::from_secs(10)).map_err(|error| match error {
            CallError::NotRunning(_) => format!("no core serves {}", socket.display()),
            other => describe(&other),
        })?;
    let info = client
        .call("session.info", json!({}))
        .map_err(|e| describe(&e))?;
    let pid = info
        .get("pid")
        .and_then(Value::as_u64)
        .and_then(|pid| u32::try_from(pid).ok())
        .ok_or_else(|| "session.info has no pid".to_owned())?;
    match client.call("session.shutdown", json!({})) {
        Ok(_) => {}
        Err(CallError::Rpc(failure)) => return Err(failure.message),
        // The core may close the connection while it ends.
        Err(CallError::Io(_) | CallError::Timeout(_) | CallError::Protocol(_)) => {}
        Err(other) => return Err(describe(&other)),
    }
    drop(client);
    let deadline = Instant::now() + STOP_WAIT;
    while alive(pid) {
        if Instant::now() > deadline {
            return Err(format!("the daemon {pid} did not end within {STOP_WAIT:?}"));
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    Ok(pid)
}

fn alive(pid: u32) -> bool {
    i32::try_from(pid)
        .ok()
        .and_then(rustix::process::Pid::from_raw)
        .is_some_and(|pid| rustix::process::test_kill_process(pid).is_ok())
}

fn describe(error: &CallError) -> String {
    match error {
        CallError::NotRunning(path) => format!("no core serves {}", path.display()),
        CallError::Timeout(after) => format!("the core did not answer within {after:?}"),
        CallError::Io(error) => error.to_string(),
        CallError::Protocol(message) => message.clone(),
        CallError::Rpc(failure) => failure.message.clone(),
    }
}

/// Connects a link to `link.sock` and serves the dev WebSocket from it. The link comes back when
/// it ends, and the page gets the projection `link`, so the shell tests can cut the
/// link with `debug.drop_links` and see the banner.
#[cfg(feature = "dev-ws")]
fn dev_ws_through_link(
    core: &nh_core::CoreHandle,
    socket: &Path,
) -> Result<(nh_core::dev_ws::DevWsInfo, nh_remote::PersistentLink), String> {
    let runtime = core.runtime();
    let hello = nh_remote::ClientHello {
        device: nh_remote::new_ulid(),
        name: "dev-ws".to_owned(),
        app_version: env!("CARGO_PKG_VERSION").to_owned(),
    };
    let connector = std::sync::Arc::new(nh_remote::UnixConnector {
        path: socket.to_owned(),
        hello,
        host: std::sync::Arc::new(nh_core::NoopHost::default()),
        options: nh_remote::ClientOptions::default(),
    });
    let link = runtime
        .block_on(nh_remote::PersistentLink::connect(connector))
        .map_err(|e| match e {
            nh_remote::ConnectError::Retry(m) | nh_remote::ConnectError::Fatal(m) => m,
        })?;
    let backend = nh_remote::dev_ws::DevLink::persistent(link.clone(), &runtime);
    let info = nh_core::dev_ws::serve(std::sync::Arc::new(backend), &runtime)
        .map_err(|e| e.to_string())?;
    eprintln!(
        "nh daemon: the dev WebSocket serves through {} (link/1)",
        socket.display()
    );
    Ok((info, link))
}

fn fail(message: &str) -> ExitCode {
    eprintln!("nh daemon: {message}");
    ExitCode::from(2)
}
