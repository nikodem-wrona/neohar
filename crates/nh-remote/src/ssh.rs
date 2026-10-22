//! The link over the system `ssh`.
//!
//! [`SshLink::start`] runs `ssh` with the options of [`SshSpec::args`] and the remote command
//! `<remote nh> bridge --session <session> --proto link/1`. [`connect_ssh`] opens a
//! [`LinkClient`] on the standard streams of that child, and ends the child when the client ends.
//!
//! - The target is given to `ssh` as the person wrote it. This module reads no ssh config.
//! - A question of ssh (a host key, a passphrase, a password) goes to `nh askpass`: the child has
//!   `SSH_ASKPASS` and `SSH_ASKPASS_REQUIRE=force`, and the answer comes from a sheet of the app.
//! - A bridge that does not know the protocol writes one line `NH-BRIDGE <version> <protos>` and
//!   exits with status 3. That line is not a `link/1` message, so the client sees an
//!   error. This module reads the first bytes through a tee and names the cause.
//! - The control master of ssh stays alive after the link ends (`ControlPersist=600`), because
//!   the CLI and the port forwards use it.

use std::{
    collections::VecDeque,
    ffi::OsString,
    io,
    os::unix::fs::{DirBuilderExt, PermissionsExt},
    path::{Path, PathBuf},
    pin::Pin,
    process::Stdio,
    sync::{Arc, Mutex, PoisonError},
    task::{Context, Poll},
    time::Duration,
};

use nh_core::Host;
use nh_proto::paths::validate_session_name;
use tokio::{
    io::{AsyncRead, AsyncReadExt, ReadBuf},
    process::{Child, ChildStderr, ChildStdin, ChildStdout, Command},
};

use crate::{
    client::{ClientHello, ClientOptions, LinkClient, LinkError},
    msg::Welcome,
};

/// The program that runs the link.
pub const SSH: &str = "/usr/bin/ssh";

/// How long ssh gets to open the link: a new connection, a question for the person, and the start
/// of a daemon. The core waits for no one else, so this is a limit for a person who walked away.
pub const CONNECT_WITHIN: Duration = Duration::from_secs(120);

/// The size of the part of the standard error of ssh that an error message can quote.
pub const STDERR_BYTES: usize = 16 << 10;

/// The lines of the standard error that an error message quotes.
const STDERR_LINES: usize = 8;

/// How long the start waits for ssh to end after a failed link, to learn its exit status.
const REAP_WITHIN: Duration = Duration::from_secs(2);

/// What the first bytes of a bridge that is too old look like.
const BRIDGE_MARK: &[u8] = b"NH-BRIDGE ";

/// The name of the program that ssh runs for a question. The file is in `<runtime>/ssh/`.
const ASKPASS_SCRIPT: &str = "askpass";

/// How the app answers a question of ssh: `nh askpass` calls `askpass.ask` on this socket.
#[derive(Clone, Debug)]
pub struct Askpass {
    /// The `nh` binary that runs `askpass`: the sidecar of the app.
    pub nh: PathBuf,
    /// The `api.sock` of the core that shows the sheet.
    pub socket: PathBuf,
    /// The session of that core.
    pub session: String,
    /// The label of the machine, for the sheet.
    pub machine: String,
}

/// What `ssh -O` does to a forward.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ForwardOp {
    Forward,
    Cancel,
}

/// How long `ssh -O forward` may take. The master is local, so it answers at once.
const FORWARD_WITHIN: Duration = Duration::from_secs(10);

/// What a link needs to start.
#[derive(Clone, Debug)]
pub struct SshSpec {
    /// The `ssh` value of the machine: anything that `ssh` accepts as a target.
    pub target: String,
    /// The session on the remote machine.
    pub session: String,
    /// The path of `nh` on the remote machine, as the remote shell reads it: `~/.local/bin/nh`.
    pub remote_nh: String,
    /// The runtime directory of this Mac. `<runtime>/ssh/` holds the control sockets.
    pub runtime_dir: PathBuf,
    /// The `ssh` program. Debug builds read `NEOHAR_SSH` for a fake (tests).
    pub program: PathBuf,
    /// `None` leaves the questions to ssh itself, which has no terminal and so fails.
    pub askpass: Option<Askpass>,
    /// More variables for the ssh child. A test gives its fake ssh the remote environment with
    /// them. The product gives none.
    pub env: Vec<(OsString, OsString)>,
    /// How long `connect_ssh` waits for `Welcome`.
    pub connect_within: Duration,
}

impl SshSpec {
    pub fn new(
        target: impl Into<String>,
        session: impl Into<String>,
        remote_nh: impl Into<String>,
        runtime_dir: impl Into<PathBuf>,
    ) -> Self {
        Self {
            target: target.into(),
            session: session.into(),
            remote_nh: remote_nh.into(),
            runtime_dir: runtime_dir.into(),
            program: ssh_program(),
            askpass: None,
            env: Vec::new(),
            connect_within: CONNECT_WITHIN,
        }
    }

    /// The directory of the control sockets: `<runtime>/ssh`.
    pub fn control_dir(&self) -> PathBuf {
        self.runtime_dir.join("ssh")
    }

    /// The arguments of `ssh`. The remote command is one argument after `--`.
    pub fn args(&self) -> Result<Vec<OsString>, SshError> {
        self.check()?;
        let mut args = self.base_args();
        args.push(
            format!(
                "{} bridge --session {} --proto link/1",
                self.remote_nh, self.session
            )
            .into(),
        );
        Ok(args)
    }

    /// The arguments of `ssh` for `nh --machine`: `<remote nh> <args…>` with each
    /// argument quoted for the shell of the remote machine. It uses the control socket of the
    /// link, so a link that is up carries the command.
    pub fn command_args(&self, remote_args: &[String]) -> Result<Vec<OsString>, SshError> {
        self.check()?;
        let mut command = self.remote_nh.clone();
        for arg in remote_args {
            command.push(' ');
            command.push_str(&shell_quote(arg));
        }
        let mut args = self.base_args();
        args.push(command.into());
        Ok(args)
    }

    /// The arguments of `ssh -O <op>` for the control socket of the link. The
    /// socket is the one of [`SshSpec::args`], so the command goes to the master of the link.
    /// `op` is `forward` or `cancel`, and the forward is `-L <local>:localhost:<remote>`.
    pub fn forward_args(
        &self,
        op: ForwardOp,
        local_port: u16,
        remote_port: u16,
    ) -> Result<Vec<OsString>, SshError> {
        self.check()?;
        let control = self.control_dir().join("%C");
        let mut args: Vec<OsString> = vec!["-T".into(), "-o".into()];
        args.push(format!("ControlPath={}", control.display()).into());
        args.push("-O".into());
        args.push(
            match op {
                ForwardOp::Forward => "forward",
                ForwardOp::Cancel => "cancel",
            }
            .into(),
        );
        args.push("-L".into());
        args.push(format!("{local_port}:localhost:{remote_port}").into());
        args.push(self.target.clone().into());
        Ok(args)
    }

    /// Runs `ssh -O <op>` and waits for it. A refusal of the master is an error with the last
    /// lines of what ssh wrote.
    pub async fn forward(
        &self,
        op: ForwardOp,
        local_port: u16,
        remote_port: u16,
    ) -> Result<(), SshError> {
        let args = self.forward_args(op, local_port, remote_port)?;
        let mut command = Command::new(&self.program);
        command
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        for (key, value) in &self.env {
            command.env(key, value);
        }
        let output = tokio::time::timeout(FORWARD_WITHIN, command.output())
            .await
            .map_err(|_| SshError::Ssh {
                status: None,
                stderr: "ssh did not answer the forward in time".to_owned(),
            })?
            .map_err(SshError::Spawn)?;
        if output.status.success() {
            return Ok(());
        }
        let text = String::from_utf8_lossy(&output.stderr);
        let tail: Vec<&str> = text.lines().rev().take(3).collect();
        Err(SshError::Ssh {
            status: output.status.code(),
            stderr: tail.into_iter().rev().collect::<Vec<_>>().join("\n"),
        })
    }

    /// Makes `<runtime>/ssh` with mode 0700, as the start of a link does.
    pub fn prepare_control_dir(&self) -> Result<(), SshError> {
        prepare_dir(&self.control_dir())
    }

    /// The options, the target and `--`.
    fn base_args(&self) -> Vec<OsString> {
        let control = self.control_dir().join("%C");
        let mut args: Vec<OsString> = vec!["-T".into()];
        for option in [
            "ControlMaster=auto".to_owned(),
            format!("ControlPath={}", control.display()),
            "ControlPersist=600".to_owned(),
            "ServerAliveInterval=15".to_owned(),
            "ServerAliveCountMax=3".to_owned(),
            "ObscureKeystrokeTiming=no".to_owned(),
        ] {
            args.push("-o".into());
            args.push(option.into());
        }
        args.push(self.target.clone().into());
        args.push("--".into());
        args
    }

    /// Refuses what could change the meaning of the command. The remote command goes through the
    /// shell of the remote machine, so its parts have a narrow alphabet.
    fn check(&self) -> Result<(), SshError> {
        let invalid = |what: &str| Err(SshError::Invalid(what.to_owned()));
        if self.target.is_empty() || self.target.starts_with('-') || self.target.contains('\0') {
            return invalid("the ssh target is empty or starts with a dash");
        }
        if validate_session_name(&self.session).is_err() {
            return invalid("the session name has characters other than a-z, 0-9 and dashes");
        }
        let word = |c: char| c.is_ascii_alphanumeric() || "_./~@+-".contains(c);
        if self.remote_nh.is_empty() || !self.remote_nh.chars().all(word) {
            return invalid("the path of nh on the remote machine has an unusual character");
        }
        Ok(())
    }
}

/// Quotes one argument for a POSIX shell, so that the shell gives the same text back. A word of
/// safe characters stays as it is. Any other text goes into single quotes, and a `'` in it
/// becomes `'\''`. The empty text is `''`. A newline is safe inside single quotes.
pub fn shell_quote(arg: &str) -> String {
    let safe = |c: char| c.is_ascii_alphanumeric() || "_@%+=:,./-".contains(c);
    if !arg.is_empty() && arg.chars().all(safe) {
        return arg.to_owned();
    }
    format!("'{}'", arg.replace('\'', r"'\''"))
}

/// The `ssh` program: the system one. A debug build takes `NEOHAR_SSH`, so that a test can run a
/// fake. A release build never reads it.
pub fn ssh_program() -> PathBuf {
    if cfg!(debug_assertions)
        && let Some(path) = std::env::var_os("NEOHAR_SSH").filter(|path| !path.is_empty())
    {
        return PathBuf::from(path);
    }
    PathBuf::from(SSH)
}

/// Why a link over ssh does not open.
#[derive(Debug, thiserror::Error)]
pub enum SshError {
    #[error("cannot run ssh: {0}")]
    Spawn(io::Error),
    #[error("cannot prepare {}: {source}", path.display())]
    Setup { path: PathBuf, source: io::Error },
    #[error("not used: {0}")]
    Invalid(String),
    /// The bridge on the remote machine does not know `link/1`. The app offers to install or
    /// update `nh`.
    #[error("the bridge on the remote machine is too old (version {version}, protocols {protos})")]
    BridgeTooOld { version: String, protos: String },
    /// ssh ended before `Welcome`. `status` is its exit status, and `stderr` the last lines of
    /// what it wrote.
    #[error("ssh ended{}: {stderr}", status.map(|s| format!(" with status {s}")).unwrap_or_default())]
    Ssh { status: Option<i32>, stderr: String },
    /// The core refused the `Hello`: `proto` or `api`.
    #[error("the core refused the link: {0}")]
    Refused(String),
    #[error("the link did not open: {0}")]
    Link(LinkError),
}

impl SshError {
    /// ssh ran the remote command, and the remote shell did not find `nh`. The status is 127. The
    /// app offers to install it.
    pub fn nh_is_missing(&self) -> bool {
        matches!(
            self,
            Self::Ssh {
                status: Some(127),
                ..
            }
        )
    }

    /// The core is of another major version of the API: the link does not work.
    pub fn is_version_mismatch(&self) -> bool {
        matches!(self, Self::Refused(reason) if reason == "api")
    }
}

/// The last bytes of the standard error of ssh. The ring keeps [`STDERR_BYTES`].
#[derive(Clone, Debug, Default)]
pub struct StderrRing(Arc<Mutex<VecDeque<u8>>>);

impl StderrRing {
    fn push(&self, bytes: &[u8]) {
        let mut ring = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        ring.extend(bytes);
        let over = ring.len().saturating_sub(STDERR_BYTES);
        ring.drain(..over);
    }

    /// The last lines of the ring, for an error message.
    pub fn tail(&self) -> String {
        let ring = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        let bytes: Vec<u8> = ring.iter().copied().collect();
        let text = String::from_utf8_lossy(&bytes);
        let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
        let from = lines.len().saturating_sub(STDERR_LINES);
        lines[from..].join("\n")
    }
}

/// A running `ssh`: its standard streams, the child and its standard error.
pub struct SshLink {
    pub stdout: ChildStdout,
    pub stdin: ChildStdin,
    pub child: Child,
    pub stderr: StderrRing,
}

impl SshLink {
    /// Starts `ssh` for `spec`. The child is killed when it drops.
    pub fn start(spec: &SshSpec) -> Result<Self, SshError> {
        let args = spec.args()?;
        prepare_dir(&spec.control_dir())?;
        let mut command = Command::new(&spec.program);
        command
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        if let Some(askpass) = &spec.askpass {
            let script = write_askpass(&spec.control_dir(), &askpass.nh)?;
            command
                .env("SSH_ASKPASS", script)
                .env("SSH_ASKPASS_REQUIRE", "force")
                .env("NEOHAR_SOCKET", &askpass.socket)
                .env("NEOHAR_SESSION", &askpass.session)
                .env("NEOHAR_MACHINE", &askpass.machine);
        }
        for (key, value) in &spec.env {
            command.env(key, value);
        }
        let mut child = command.spawn().map_err(SshError::Spawn)?;
        let (Some(stdout), Some(stdin), Some(stderr)) =
            (child.stdout.take(), child.stdin.take(), child.stderr.take())
        else {
            return Err(SshError::Spawn(io::Error::other("no standard streams")));
        };
        let ring = StderrRing::default();
        tokio::spawn(copy_stderr(stderr, ring.clone()));
        Ok(Self {
            stdout,
            stdin,
            child,
            stderr: ring,
        })
    }
}

async fn copy_stderr(mut stderr: ChildStderr, ring: StderrRing) {
    let mut buf = [0u8; 4096];
    while let Ok(n) = stderr.read(&mut buf).await {
        if n == 0 {
            break;
        }
        ring.push(&buf[..n]);
    }
}

/// Makes `<runtime>/ssh` with mode 0700. A directory that exists gets the mode too: a control
/// socket in a directory that others can enter is a way into the connection.
fn prepare_dir(dir: &Path) -> Result<(), SshError> {
    let setup = |source| SshError::Setup {
        path: dir.to_owned(),
        source,
    };
    if let Some(parent) = dir.parent() {
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(parent)
            .map_err(setup)?;
    }
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(dir)
        .map_err(setup)?;
    std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700)).map_err(setup)
}

/// Writes the program that ssh runs for a question: `exec '<nh>' askpass "$@"`. `SSH_ASKPASS` is a
/// path, so it cannot hold the argument `askpass`. The file is new for each start and has mode
/// 0700, in the directory of mode 0700.
fn write_askpass(dir: &Path, nh: &Path) -> Result<PathBuf, SshError> {
    let path = dir.join(ASKPASS_SCRIPT);
    let setup = |source| SshError::Setup {
        path: path.clone(),
        source,
    };
    let quoted = nh.to_string_lossy().replace('\'', r"'\''");
    let text = format!("#!/bin/sh\nexec '{quoted}' askpass \"$@\"\n");
    if std::fs::read_to_string(&path).is_ok_and(|old| old == text) {
        return Ok(path);
    }
    let temporary = dir.join(format!(".{ASKPASS_SCRIPT}.{}", std::process::id()));
    std::fs::write(&temporary, text).map_err(setup)?;
    std::fs::set_permissions(&temporary, std::fs::Permissions::from_mode(0o700)).map_err(setup)?;
    std::fs::rename(&temporary, &path).map_err(setup)?;
    Ok(path)
}

/// A reader that remembers the first bytes that it passes on.
struct Tee {
    inner: ChildStdout,
    head: Arc<Mutex<Vec<u8>>>,
}

/// How many bytes of the start the tee keeps. The line of an old bridge is short.
const HEAD_BYTES: usize = 256;

impl AsyncRead for Tee {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let before = buf.filled().len();
        let polled = Pin::new(&mut self.inner).poll_read(cx, buf);
        if matches!(polled, Poll::Ready(Ok(()))) {
            let mut head = self.head.lock().unwrap_or_else(PoisonError::into_inner);
            let room = HEAD_BYTES.saturating_sub(head.len());
            if room > 0 {
                let fresh = &buf.filled()[before..];
                head.extend_from_slice(&fresh[..fresh.len().min(room)]);
            }
        }
        polled
    }
}

/// Parses the line of a bridge that is too old: `NH-BRIDGE <version> <protos>`.
pub fn parse_bridge_line(head: &[u8]) -> Option<(String, String)> {
    let rest = head.strip_prefix(BRIDGE_MARK)?;
    let line = String::from_utf8_lossy(rest);
    let line = line.lines().next()?;
    let (version, protos) = line.split_once(' ').unwrap_or((line, ""));
    Some((version.trim().to_owned(), protos.trim().to_owned()))
}

/// Opens a link over ssh: starts the child, sends `Hello` and waits for `Welcome`. The child ends
/// when the client ends, and the client ends when the child does. Call it on a tokio runtime.
///
/// A question of ssh waits for the person, so the wait for `Welcome` is `spec.connect_within`
/// and not the 10 s of an ordinary link.
pub async fn connect_ssh(
    spec: &SshSpec,
    hello: ClientHello,
    host: Arc<dyn Host>,
    mut options: ClientOptions,
) -> Result<LinkClient, SshError> {
    let SshLink {
        stdout,
        stdin,
        mut child,
        stderr,
    } = SshLink::start(spec)?;
    let head = Arc::new(Mutex::new(Vec::new()));
    let read = Tee {
        inner: stdout,
        head: Arc::clone(&head),
    };
    options.timing.hello_within = options.timing.hello_within.max(spec.connect_within);
    match LinkClient::connect(read, stdin, hello, host, options).await {
        Ok(client) => {
            // The task holds the wire and not a clone of the client, so it does not keep the link
            // open: the link ends with the last clone of the client, and then ssh ends.
            let wire = client.wire();
            tokio::spawn(async move {
                tokio::select! {
                    _ = wire.wait_down() => {}
                    _ = child.wait() => {}
                }
                // The control master stays: it is a process of its own.
                let _ = child.start_kill();
                let _ = child.wait().await;
            });
            Ok(client)
        }
        Err(error) => {
            let _ = child.start_kill();
            let status = tokio::time::timeout(REAP_WITHIN, child.wait())
                .await
                .ok()
                .and_then(Result::ok)
                .and_then(|status| status.code());
            let head = head.lock().unwrap_or_else(PoisonError::into_inner).clone();
            Err(explain(error, &head, status, &stderr))
        }
    }
}

/// Names the cause of a link that did not open.
fn explain(error: LinkError, head: &[u8], status: Option<i32>, stderr: &StderrRing) -> SshError {
    if let Some((version, protos)) = parse_bridge_line(head) {
        return SshError::BridgeTooOld { version, protos };
    }
    match error {
        LinkError::Refused(reason) => SshError::Refused(reason),
        // The stream ended, or never started: ssh has the reason on its standard error.
        LinkError::Closed(_) | LinkError::Io(_) => SshError::Ssh {
            status,
            stderr: stderr.tail(),
        },
        other => SshError::Link(other),
    }
}

/// How the version of the core compares with the version of the app.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VersionCheck {
    /// The same version.
    Current,
    /// The core is older than the app: the link works, and the app offers an update.
    UpdateAvailable,
    /// The core is newer than the app within the major version: the link works, and the app asks
    /// the person to update the app.
    UpdateApp,
    /// The core is of a newer major version. The link works only if the core accepted `Hello`;
    /// the app says that the versions do not match.
    Mismatch,
    /// A version that is not three numbers. The link works and the app says nothing.
    Unknown,
}

/// Compares the version of the core in `Welcome` with the version of the app.
pub fn version_check(welcome: &Welcome, app_version: &str) -> VersionCheck {
    let (Some(core), Some(app)) = (
        parse_version(&welcome.core_version),
        parse_version(app_version),
    ) else {
        return VersionCheck::Unknown;
    };
    if core == app {
        VersionCheck::Current
    } else if core < app {
        VersionCheck::UpdateAvailable
    } else if core.0 > app.0 {
        VersionCheck::Mismatch
    } else {
        VersionCheck::UpdateApp
    }
}

/// `major.minor.patch`, and nothing after a `-` or a `+`.
fn parse_version(text: &str) -> Option<(u64, u64, u64)> {
    let core = text.split(['-', '+']).next()?;
    let mut parts = core.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    let patch = parts.next()?.parse().ok()?;
    parts.next().is_none().then_some((major, minor, patch))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec() -> SshSpec {
        SshSpec::new("studio", "default", "~/.local/bin/nh", "/tmp/neohar-501")
    }

    fn strings(args: Vec<OsString>) -> Vec<String> {
        args.into_iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn the_arguments_are_the_options_of_the_spec() {
        assert_eq!(
            strings(spec().args().unwrap()),
            [
                "-T",
                "-o",
                "ControlMaster=auto",
                "-o",
                "ControlPath=/tmp/neohar-501/ssh/%C",
                "-o",
                "ControlPersist=600",
                "-o",
                "ServerAliveInterval=15",
                "-o",
                "ServerAliveCountMax=3",
                "-o",
                "ObscureKeystrokeTiming=no",
                "studio",
                "--",
                "~/.local/bin/nh bridge --session default --proto link/1",
            ]
        );
    }

    #[test]
    fn a_command_has_the_quoted_arguments_after_the_path_of_nh() {
        let words = |list: &[&str]| list.iter().map(|w| (*w).to_owned()).collect::<Vec<_>>();
        let args = strings(
            spec()
                .command_args(&words(&["pane", "show", "w1:p1"]))
                .unwrap(),
        );
        assert_eq!(args[args.len() - 2], "--");
        assert_eq!(args[args.len() - 1], "~/.local/bin/nh pane show w1:p1");
        let args = strings(
            spec()
                .command_args(&words(&["pane", "send", "a b"]))
                .unwrap(),
        );
        assert_eq!(args[args.len() - 1], "~/.local/bin/nh pane send 'a b'");
        let mut bad = spec();
        bad.remote_nh = "nh; rm".to_owned();
        assert!(matches!(bad.command_args(&[]), Err(SshError::Invalid(_))));
    }

    #[test]
    fn the_quote_of_an_argument_is_read_back_by_a_shell() {
        assert_eq!(shell_quote("w1:p1"), "w1:p1");
        assert_eq!(shell_quote(""), "''");
        assert_eq!(shell_quote("a b"), "'a b'");
        assert_eq!(shell_quote("it's"), r"'it'\''s'");
        assert_eq!(shell_quote("$HOME"), "'$HOME'");
        let samples = [
            "plain",
            "",
            "a b  c",
            "it's",
            "'''",
            "say \"hi\"",
            "$HOME `id` $(id) ${x}",
            "line one\nline two\n",
            "tab\there",
            "back\\slash \\' \\\"",
            "*.rs ? [a-z] ~ ! # ; & | < > ( ) { }",
            "-n",
            "--flag=a b",
            "ünï cödé \u{1F600}",
        ];
        // Each sample alone, and all samples in one command line: the shell hands over the same
        // arguments. `printf '%s\0'` prints each one ended by NUL.
        for sample in samples {
            let out = std::process::Command::new("/bin/sh")
                .arg("-c")
                .arg(format!("printf '%s\\0' {}", shell_quote(sample)))
                .output()
                .unwrap();
            assert_eq!(
                String::from_utf8_lossy(&out.stdout),
                format!("{sample}\0"),
                "{sample:?}"
            );
        }
        let line: Vec<String> = samples.iter().map(|s| shell_quote(s)).collect();
        let out = std::process::Command::new("/bin/sh")
            .arg("-c")
            .arg(format!("printf '%s\\0' {}", line.join(" ")))
            .output()
            .unwrap();
        let text = String::from_utf8_lossy(&out.stdout).into_owned();
        let got: Vec<&str> = text.split_terminator('\0').collect();
        assert_eq!(got, samples);
    }

    #[test]
    fn a_target_that_looks_like_an_option_or_a_bad_command_part_is_refused() {
        for target in ["", "-oProxyCommand=x", "-J host"] {
            let mut bad = spec();
            bad.target = target.to_owned();
            assert!(
                matches!(bad.args(), Err(SshError::Invalid(_))),
                "{target:?}"
            );
        }
        for session in ["", "Default", "a b", "a;b", "a/b"] {
            let mut bad = spec();
            bad.session = session.to_owned();
            assert!(
                matches!(bad.args(), Err(SshError::Invalid(_))),
                "{session:?}"
            );
        }
        for nh in ["", "nh; rm -rf ~", "$(x)", "a b", "'nh'"] {
            let mut bad = spec();
            bad.remote_nh = nh.to_owned();
            assert!(matches!(bad.args(), Err(SshError::Invalid(_))), "{nh:?}");
        }
        // A target with a user, a host, a port-less name and a jump syntax is the person's.
        for target in ["me@studio", "studio.local", "host-1", "a@b.c.d"] {
            let mut good = spec();
            good.target = target.to_owned();
            assert!(good.args().is_ok(), "{target:?}");
        }
    }

    #[test]
    fn the_line_of_an_old_bridge_is_read() {
        assert_eq!(
            parse_bridge_line(b"NH-BRIDGE 0.0.0 link/0\n"),
            Some(("0.0.0".to_owned(), "link/0".to_owned()))
        );
        assert_eq!(
            parse_bridge_line(b"NH-BRIDGE 1.2.3 link/1 link/2\nmore"),
            Some(("1.2.3".to_owned(), "link/1 link/2".to_owned()))
        );
        assert_eq!(parse_bridge_line(b"\x30\0\0\0hello"), None);
        assert_eq!(parse_bridge_line(b"NH-BRIDG"), None);
    }

    #[test]
    fn the_version_rule_of_the_spec() {
        let welcome = |v: &str| Welcome {
            core_version: v.to_owned(),
            machine_uuid: String::new(),
            session_uuid: String::new(),
            hostname: String::new(),
            core_epoch: String::new(),
        };
        assert_eq!(
            version_check(&welcome("1.2.3"), "1.2.3"),
            VersionCheck::Current
        );
        assert_eq!(
            version_check(&welcome("1.2.2"), "1.2.3"),
            VersionCheck::UpdateAvailable
        );
        assert_eq!(
            version_check(&welcome("0.9.9"), "1.0.0"),
            VersionCheck::UpdateAvailable
        );
        assert_eq!(
            version_check(&welcome("1.3.0"), "1.2.3"),
            VersionCheck::UpdateApp
        );
        assert_eq!(
            version_check(&welcome("2.0.0"), "1.9.9"),
            VersionCheck::Mismatch
        );
        assert_eq!(
            version_check(&welcome("1.2.3-dev+abc"), "1.2.3"),
            VersionCheck::Current
        );
        assert_eq!(
            version_check(&welcome("dev"), "1.2.3"),
            VersionCheck::Unknown
        );
        assert_eq!(
            version_check(&welcome("1.2"), "1.2.3"),
            VersionCheck::Unknown
        );
    }

    #[test]
    fn the_ring_keeps_the_end_and_the_tail_the_last_lines() {
        let ring = StderrRing::default();
        ring.push(&vec![b'x'; STDERR_BYTES]);
        for n in 0..20 {
            ring.push(format!("line {n}\n").as_bytes());
        }
        let tail = ring.tail();
        assert!(tail.ends_with("line 19"), "{tail}");
        assert_eq!(tail.lines().count(), STDERR_LINES);
        let len = ring.0.lock().unwrap().len();
        assert_eq!(len, STDERR_BYTES);
    }

    #[test]
    fn the_askpass_program_names_nh_and_has_mode_0700() {
        let dir = std::env::temp_dir().join(format!("nh-ssh-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        prepare_dir(&dir.join("ssh")).unwrap();
        let mode = |p: &Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(&dir.join("ssh")), 0o700);
        let script = write_askpass(&dir.join("ssh"), Path::new("/Apps/it's/nh")).unwrap();
        assert_eq!(mode(&script), 0o700);
        let text = std::fs::read_to_string(&script).unwrap();
        assert!(
            text.contains(r#"exec '/Apps/it'\''s/nh' askpass "$@""#),
            "{text}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
