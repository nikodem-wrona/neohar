//! `nh bridge --session <s> --proto link/1`: the end of a link on the remote machine.
//!
//! ssh starts the bridge, and the bridge copies bytes between its stdio and `link.sock` of the
//! session. It has no state and does not parse the protocol, and it does not depend on
//! `nh-remote`.
//!
//! 1. It connects to `$RUNTIME/neohar/<s>/link.sock`.
//! 2. If no core answers, it starts `nh daemon --detach --session <s>`, waits up to 10 s for
//!    `READY`, and connects again. If two bridges start a daemon at the same moment, one daemon
//!    wins the lock of the session and the other ends with `ApiSocketInUse`. The bridge of the loser
//!    tries to connect until the 10 s are over, so it reaches the daemon that won.
//! 3. It copies bytes in both directions until either side closes. After the end of stdin it
//!    passes on what the core still sends, for at most 5 s.
//! 4. If it does not know the protocol of `--proto`, or `--proto` is missing, it writes one line
//!    `NH-BRIDGE <version> <protos>` to stdout and exits with status 3. The app then offers to
//!    install or update `nh`.

use std::{
    io::{self, BufRead, BufReader, Read, Write},
    net::Shutdown,
    os::unix::net::UnixStream,
    path::Path,
    process::{Command, ExitCode, Stdio},
    sync::mpsc,
    time::{Duration, Instant},
};

use nh_proto::paths::{Env, PathsError, validate_session_name};

/// The protocols that this bridge can carry. It copies bytes, so the list says which clients this
/// version of `nh` serves.
pub const PROTOS: &[&str] = &["link/1"];

/// How long the bridge waits for a daemon that it started.
const START_WAIT: Duration = Duration::from_secs(10);

/// How long the bridge passes on bytes of the core after the end of its stdin.
const DRAIN: Duration = Duration::from_secs(5);

/// The exit status when the bridge does not know the protocol.
const UNKNOWN_PROTO: u8 = 3;

pub fn main(args: &[String]) -> ExitCode {
    let mut session = std::env::var("NEOHAR_SESSION").unwrap_or_else(|_| "default".to_owned());
    let mut proto: Option<String> = None;
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--session" => match args.next() {
                Some(name) => session.clone_from(name),
                None => return fail("--session needs a name"),
            },
            "--proto" => match args.next() {
                Some(name) => proto = Some(name.clone()),
                None => return fail("--proto needs a name"),
            },
            other => return fail(&format!("unknown argument {other:?}")),
        }
    }
    if !proto.as_deref().is_some_and(|p| PROTOS.contains(&p)) {
        let mut out = io::stdout().lock();
        let _ = writeln!(out, "{}", version_line());
        let _ = out.flush();
        return ExitCode::from(UNKNOWN_PROTO);
    }
    if let Err(error) = validate_session_name(&session) {
        return fail(&error.to_string());
    }
    let env = Env::from_process();
    // On a host without `XDG_RUNTIME_DIR` the root is in `/tmp`, where another user can make it
    // first and serve a fake core in it. A missing root means that no core ran here: the daemon
    // makes the root and checks it.
    if let Err(error) = env.verify_runtime_root()
        && !matches!(&error, PathsError::Io { source, .. } if source.kind() == io::ErrorKind::NotFound)
    {
        return fail(&error.to_string());
    }
    let path = match env.link_socket(&session) {
        Ok(path) => path,
        Err(error) => return fail(&error.to_string()),
    };
    let stream = match connect_or_start(&path, &session) {
        Ok(stream) => stream,
        Err(message) => return fail(&message),
    };
    copy(&stream);
    // The thread that reads stdin may still wait in a read. The process leaves without it.
    ExitCode::SUCCESS
}

/// `NH-BRIDGE <version> <protos>`: the protocols are separated by commas.
pub fn version_line() -> String {
    format!(
        "NH-BRIDGE {} {}",
        env!("CARGO_PKG_VERSION"),
        PROTOS.join(",")
    )
}

/// Whether the error of a connect means that no core serves the socket.
fn nobody(error: &io::Error) -> bool {
    matches!(
        error.kind(),
        io::ErrorKind::NotFound | io::ErrorKind::ConnectionRefused
    )
}

/// Connects to `link.sock`. When no core answers, it starts a daemon and tries again until
/// [`START_WAIT`] is over. Each connect checks the directory of the socket and the user of the
/// core ([`crate::peer::connect`]).
fn connect_or_start(path: &Path, session: &str) -> Result<UnixStream, String> {
    match crate::peer::connect(path) {
        Ok(stream) => return Ok(stream),
        Err(error) if nobody(&error) => {}
        Err(error) => return Err(format!("{}: {error}", path.display())),
    }
    let deadline = Instant::now() + START_WAIT;
    let started = start_daemon(session, deadline);
    loop {
        match crate::peer::connect(path) {
            Ok(stream) => {
                // Another bridge started a daemon at the same moment, and its daemon won.
                if let Err(reason) = &started {
                    eprintln!(
                        "nh bridge: the daemon of this bridge did not start ({reason}); the bridge uses the core that runs"
                    );
                }
                return Ok(stream);
            }
            Err(error) if nobody(&error) && Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(25));
            }
            Err(error) => {
                let why = match &started {
                    Ok(()) => String::new(),
                    Err(reason) => format!(" ({reason})"),
                };
                return Err(format!(
                    "no core serves {} after a start{why}: {error}",
                    path.display()
                ));
            }
        }
    }
}

/// Starts `nh daemon --detach --session <s>` and waits for its line until `deadline`. `Err` has
/// the reason when the daemon did not start, for example because another one won the race.
fn start_daemon(session: &str, deadline: Instant) -> Result<(), String> {
    let nh = std::env::current_exe().map_err(|e| format!("cannot find this executable: {e}"))?;
    let mut child = Command::new(nh)
        .args(["daemon", "--detach", "--session", session])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|e| format!("cannot start the daemon: {e}"))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "the daemon has no stdout".to_owned())?;
    let (tx, rx) = mpsc::channel();
    let _ = std::thread::Builder::new()
        .name("nh-bridge-ready".to_owned())
        .spawn(move || {
            let mut line = String::new();
            let _ = BufReader::new(stdout).read_line(&mut line);
            let _ = tx.send(line);
        });
    let line = rx
        .recv_timeout(deadline.saturating_duration_since(Instant::now()))
        .map_err(|_| "the daemon did not say READY in time".to_owned())?;
    // The first process of the daemon exits after its line.
    let _ = child.wait();
    let line = line.trim_end();
    if line.starts_with("READY ") {
        Ok(())
    } else if let Some(reason) = line.strip_prefix("ERROR ") {
        Err(reason.to_owned())
    } else {
        Err(format!("the daemon said {line:?}"))
    }
}

/// Copies stdin to the socket and the socket to stdout until either side closes. The bytes are
/// not parsed.
///
/// The end of stdin closes the write side of the socket, so the core sees the client leave, and
/// the bridge passes on what the core still sends until the core closes or [`DRAIN`] is over. The
/// end of the socket ends the bridge at once.
fn copy(stream: &UnixStream) {
    if let Ok(mut to_socket) = stream.try_clone() {
        let _ = std::thread::Builder::new()
            .name("nh-bridge-up".to_owned())
            .spawn(move || {
                let _ = io::copy(&mut io::stdin().lock(), &mut to_socket);
                let _ = to_socket.shutdown(Shutdown::Write);
                std::thread::sleep(DRAIN);
                let _ = to_socket.shutdown(Shutdown::Both);
            });
    }
    let mut from_socket = stream;
    let mut out = io::stdout().lock();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        match from_socket.read(&mut buffer) {
            Ok(0) | Err(_) => break,
            Ok(n) => {
                if out.write_all(&buffer[..n]).is_err() || out.flush().is_err() {
                    break;
                }
            }
        }
    }
}

fn fail(message: &str) -> ExitCode {
    eprintln!("nh bridge: {message}");
    ExitCode::from(1)
}
