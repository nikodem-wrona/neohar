//! The holder side of a recording: the scratch directory, the environment of the program, the
//! holder process, the connection to it, and the guard that stops it.

use std::{
    fs::{self, DirBuilder, File},
    io::{BufRead, BufReader, Read, Write},
    os::unix::{fs::DirBuilderExt, net::UnixStream},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicU32, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use nh_proto::{
    framing::{self, MAX_MSG_BYTES},
    holder::{HoldConfig, HoldMode, HolderEvent, HolderRequest, SignalKind, SignalTarget},
    version::HOLDER_PROTO,
};
use rustix::process::{Pid, Signal, kill_process, kill_process_group, test_kill_process};
use ulid::Ulid;

use crate::{RecordError, signals};

/// How long a holder may take to report `READY` (the limit is 5 s; a loaded machine gets 10).
const READY_TIMEOUT: Duration = Duration::from_secs(10);

/// The size of the holder's ring. The tool reads while the program runs, so the ring only has to
/// hold what the tool has not read yet.
const RING_BYTES: u64 = 16 << 20;

/// The `PATH` of the program unless `--env PATH=...` says otherwise. It holds no user directory.
pub const DEFAULT_PATH: &str = "/usr/bin:/bin:/usr/sbin:/sbin";

/// The shell line that starts a holder like the core does: descriptor 3 is the config pipe and
/// stdin is `/dev/null`. `$0` is the `nh` binary and `$1` is the pane id.
const HOLD_SCRIPT: &str = r#"exec "$0" hold --pane "$1" --config-fd 3 3<&0 </dev/null"#;

static COUNTER: AtomicU32 = AtomicU32::new(0);

/// The directory tree of one recording. Dropping it removes the tree.
pub struct Scratch {
    pub root: PathBuf,
    pub home: PathBuf,
    pub tmp: PathBuf,
    /// The working directory of the program and of the setup script.
    pub work: PathBuf,
    pub config: PathBuf,
    pub data: PathBuf,
    pub run: PathBuf,
    /// Set by [`Scratch::keep_on_drop`]: the tree stays when this value goes out of scope.
    keep: bool,
}

impl Scratch {
    /// Makes a new tree under `/tmp`. The name is short: a Unix socket path holds about 100 bytes.
    pub fn create() -> Result<Self, RecordError> {
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        Self::create_at(PathBuf::from(format!(
            "/tmp/nhr-{}-{n}",
            std::process::id()
        )))
    }

    /// Makes a new tree at `root`, and removes what was there. The path of the working directory
    /// shows on the screen of some programs, so a recording that must repeat byte for byte takes
    /// a fixed root. The caller makes sure that no one else uses it.
    pub fn create_at(root: PathBuf) -> Result<Self, RecordError> {
        let _ = fs::remove_dir_all(&root);
        Self::open_at(root)
    }

    /// Takes the tree at `root` over from an earlier run that kept it. The files of the program
    /// (`home`, `work`, `tmp`, `config`, `data`) stay. What belonged to the earlier run (sockets,
    /// logs, the hook tee) goes, so that the new run starts clean. Directories that are missing
    /// are made.
    pub fn reuse_at(root: PathBuf) -> Result<Self, RecordError> {
        for old in ["run", "holders", "tee", "replies"] {
            let _ = fs::remove_dir_all(root.join(old));
        }
        for old in [
            "holder.log",
            "spawn.log",
            "llm-requests.jsonl",
            "sandbox.sb",
        ] {
            let _ = fs::remove_file(root.join(old));
        }
        Self::open_at(root)
    }

    /// Makes the directories of a tree, if they are not there.
    fn open_at(root: PathBuf) -> Result<Self, RecordError> {
        let scratch = Self {
            keep: false,
            home: root.join("home"),
            tmp: root.join("tmp"),
            work: root.join("work"),
            config: root.join("config"),
            data: root.join("data"),
            run: root.join("run"),
            root,
        };
        for dir in [
            &scratch.root,
            &scratch.home,
            &scratch.tmp,
            &scratch.work,
            &scratch.config,
            &scratch.data,
            &scratch.run,
        ] {
            DirBuilder::new()
                .recursive(true)
                .mode(0o700)
                .create(dir)
                .map_err(RecordError::io(format!("create {}", dir.display())))?;
        }
        Ok(scratch)
    }

    /// Makes the tree stay when this value is dropped, for a later run that takes it over with
    /// [`Scratch::reuse_at`].
    pub fn keep_on_drop(&mut self) {
        self.keep = true;
    }

    /// Replaces `{scratch}`, `{home}`, `{tmp}` and `{work}` with the paths of this tree.
    pub fn expand(&self, text: &str) -> String {
        text.replace("{scratch}", &self.root.display().to_string())
            .replace("{home}", &self.home.display().to_string())
            .replace("{tmp}", &self.tmp.display().to_string())
            .replace("{work}", &self.work.display().to_string())
    }

    /// The whole environment of the program. It is built from nothing: no variable of the user's
    /// environment gets in, so that no user name, path or secret reaches a recording. `extra`
    /// adds variables or replaces them; the values go through [`Scratch::expand`].
    pub fn child_env(&self, extra: &[(String, String)]) -> Vec<(String, String)> {
        let path = |p: &Path| p.display().to_string();
        let mut env = vec![
            ("PATH".to_owned(), DEFAULT_PATH.to_owned()),
            ("TERM".to_owned(), "xterm-256color".to_owned()),
            ("LANG".to_owned(), "en_US.UTF-8".to_owned()),
            ("HOME".to_owned(), path(&self.home)),
            ("TMPDIR".to_owned(), path(&self.tmp)),
            (
                "XDG_CONFIG_HOME".to_owned(),
                path(&self.home.join(".config")),
            ),
            (
                "XDG_DATA_HOME".to_owned(),
                path(&self.home.join(".local/share")),
            ),
            ("XDG_CACHE_HOME".to_owned(), path(&self.home.join(".cache"))),
            ("NEOHAR_CONFIG_DIR".to_owned(), path(&self.config)),
            ("NEOHAR_DATA_DIR".to_owned(), path(&self.data)),
            ("NEOHAR_RUNTIME_DIR".to_owned(), path(&self.run)),
        ];
        for (key, value) in extra {
            let value = self.expand(value);
            match env.iter_mut().find(|(k, _)| k == key) {
                Some(entry) => entry.1 = value,
                None => env.push((key.clone(), value)),
            }
        }
        env
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        if !self.keep {
            let _ = fs::remove_dir_all(&self.root);
        }
    }
}

/// Runs a setup script with `/bin/sh`, in the working directory, with the environment of the
/// program. It makes the files that the program needs.
pub fn run_setup(
    script: &Path,
    scratch: &Scratch,
    env: &[(String, String)],
) -> Result<(), RecordError> {
    let script = fs::canonicalize(script).map_err(RecordError::io(format!(
        "setup script {}",
        script.display()
    )))?;
    let output = Command::new("/bin/sh")
        .arg(&script)
        .current_dir(&scratch.work)
        .env_clear()
        .envs(env.iter().map(|(k, v)| (k, v)))
        .stdin(Stdio::null())
        .output()
        .map_err(RecordError::io("run the setup script"))?;
    if output.status.success() {
        return Ok(());
    }
    Err(RecordError::Holder(format!(
        "the setup script {} failed with {}:\n{}{}",
        script.display(),
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )))
}

/// What a holder starts with.
pub struct HolderSpec<'a> {
    /// The `nh` binary.
    pub nh: &'a Path,
    /// The program and its arguments.
    pub argv: &'a [String],
    /// The whole environment of the program.
    pub env: &'a [(String, String)],
    pub cols: u16,
    pub rows: u16,
}

/// A holder and its child, and the guard that stops them. Whatever way this value goes out of
/// scope, [`HolderProcess::stop`] runs, and then the scratch directory is removed.
pub struct HolderProcess {
    socket: PathBuf,
    secret: [u8; 32],
    holder_pid: Option<i32>,
    child_pid: Option<i32>,
    /// Set when the holder reported that the child has exited. Then the child's pid may be reused,
    /// so the guard does not signal it.
    pub child_exited: bool,
    /// The shell that starts the holder. It is gone once the holder reports `READY`.
    spawner: Option<Child>,
    stopped: bool,
    /// The last field: it is dropped after [`HolderProcess::stop`] ran.
    scratch: Scratch,
}

impl HolderProcess {
    /// A guard for the holder that [`HolderProcess::launch`] will start.
    pub fn new(scratch: Scratch) -> Result<Self, RecordError> {
        Ok(Self {
            socket: PathBuf::new(),
            secret: urandom()?,
            holder_pid: None,
            child_pid: None,
            child_exited: false,
            spawner: None,
            stopped: false,
            scratch,
        })
    }

    pub fn scratch(&self) -> &Scratch {
        &self.scratch
    }

    /// The pid of the program, once the connection said `HelloOk`.
    pub fn child_pid(&self) -> Option<i32> {
        self.child_pid
    }

    /// Starts the holder with the program and waits for `READY`.
    pub fn launch(&mut self, spec: &HolderSpec<'_>) -> Result<(), RecordError> {
        let random: [u8; 16] = urandom()?;
        let ulid = Ulid::from_parts(now_ms(), u128::from_le_bytes(random) & ((1u128 << 80) - 1));
        let root = &self.scratch.root;
        self.socket = root.join("holders").join(format!("{ulid}.sock"));
        let config = HoldConfig {
            proto: HOLDER_PROTO,
            pane: ulid.to_bytes(),
            secret: self.secret,
            mode: HoldMode::Pty {
                cols: spec.cols,
                rows: spec.rows,
                px_width: 0,
                px_height: 0,
            },
            argv: spec.argv.to_vec(),
            env: spec.env.to_vec(),
            cwd: self.scratch.work.display().to_string(),
            socket_path: self.socket.display().to_string(),
            log_path: root.join("holder.log").display().to_string(),
            ring_bytes: RING_BYTES,
            checkpoint_idle_ms: 2_000,
            checkpoint_max_interval_ms: 30_000,
            query_fresh_ms: 5_000,
        };
        let spawn_log = root.join("spawn.log");
        let spawn_err = File::create(&spawn_log).map_err(RecordError::io("create spawn.log"))?;
        let mut child = Command::new("/bin/sh")
            .args(["-c", HOLD_SCRIPT])
            .arg(spec.nh)
            .arg(ulid.to_string())
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .env("HOME", &self.scratch.home)
            .env("TMPDIR", &self.scratch.tmp)
            .env("NEOHAR_CONFIG_DIR", &self.scratch.config)
            .env("NEOHAR_DATA_DIR", &self.scratch.data)
            .env("NEOHAR_RUNTIME_DIR", &self.scratch.run)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(spawn_err)
            .spawn()
            .map_err(RecordError::io("start /bin/sh for the holder"))?;
        let stdin = child.stdin.take();
        let stdout = child.stdout.take();
        self.spawner = Some(child);
        let (Some(mut stdin), Some(stdout)) = (stdin, stdout) else {
            return Err(RecordError::Holder("the holder's pipes are missing".into()));
        };
        framing::write_msg(&mut stdin, &config)?;
        drop(stdin);

        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            let mut line = String::new();
            let _ = BufReader::new(stdout).read_line(&mut line);
            let _ = tx.send(line);
        });
        let failure = |what: &str| {
            let log = fs::read_to_string(&spawn_log).unwrap_or_default();
            RecordError::Holder(format!("{what}\n{log}"))
        };
        let line = rx
            .recv_timeout(READY_TIMEOUT)
            .map_err(|_| failure("the holder did not report READY in time"))?;
        let pid = line
            .trim()
            .strip_prefix("READY ")
            .and_then(|p| p.parse::<i32>().ok())
            .ok_or_else(|| failure(&format!("the holder did not report READY: {line:?}")))?;
        self.holder_pid = Some(pid);
        if let Some(mut spawner) = self.spawner.take() {
            // The first process leaves after the holder detached from it.
            let _ = spawner.wait();
        }
        Ok(())
    }

    /// Connects, says `Hello` and returns the connection after the holder accepted it.
    pub fn connect(&mut self) -> Result<Connection, RecordError> {
        let mut conn = Connection::connect(&self.socket)?;
        conn.send(&self.hello(1))?;
        match conn.recv(Duration::from_secs(5))? {
            Some(HolderEvent::HelloOk { child_pid, .. }) => {
                self.child_pid = Some(child_pid);
                Ok(conn)
            }
            other => Err(RecordError::Holder(format!(
                "the holder did not accept the connection: {other:?}"
            ))),
        }
    }

    fn hello(&self, fencing_token: u64) -> HolderRequest {
        HolderRequest::Hello {
            proto: HOLDER_PROTO,
            secret: self.secret,
            fencing_token,
            client: "nh-record".into(),
        }
    }

    /// Stops the child and the holder. It does not fail, it can run twice, and it leaves nothing
    /// behind that the tool started.
    pub fn stop(&mut self) {
        if std::mem::replace(&mut self.stopped, true) {
            return;
        }
        self.stop_through_socket();
        if !self.child_exited
            && let Some(pid) = self.child_pid.and_then(Pid::from_raw)
        {
            // The child is a session leader, so its pid is its process group.
            let _ = kill_process_group(pid, Signal::KILL);
            let _ = kill_process(pid, Signal::KILL);
        }
        if let Some(pid) = self.holder_pid.and_then(Pid::from_raw) {
            let _ = kill_process(pid, Signal::KILL);
            let deadline = Instant::now() + Duration::from_secs(2);
            while test_kill_process(pid).is_ok() && Instant::now() < deadline {
                thread::sleep(Duration::from_millis(10));
            }
        }
        if let Some(mut spawner) = self.spawner.take() {
            let _ = spawner.kill();
            let _ = spawner.wait();
        }
    }

    /// Asks the holder to kill the child, with a fencing token that no other client has. Then it
    /// tells the holder that it may exit. The holder also does this when the pid is not known.
    fn stop_through_socket(&self) {
        let Ok(mut conn) = Connection::connect(&self.socket) else {
            return;
        };
        let requests = [
            self.hello(u64::MAX),
            HolderRequest::Attach {
                from_offset: u64::MAX,
            },
            HolderRequest::Signal {
                target: SignalTarget::Child,
                signal: SignalKind::Kill,
            },
            HolderRequest::Signal {
                target: SignalTarget::Foreground,
                signal: SignalKind::Kill,
            },
        ];
        for request in &requests {
            if conn.send(request).is_err() {
                return;
            }
        }
        let deadline = Instant::now() + Duration::from_millis(500);
        while let Some(left) = deadline.checked_duration_since(Instant::now()) {
            match conn.recv(left) {
                Ok(Some(HolderEvent::ChildExited { .. })) => {
                    let _ = conn.send(&HolderRequest::ExitAck);
                    return;
                }
                Ok(Some(HolderEvent::Refused { .. })) | Ok(None) | Err(_) => return,
                Ok(Some(_)) => {}
            }
        }
    }
}

impl Drop for HolderProcess {
    fn drop(&mut self) {
        self.stop();
    }
}

/// A connection to a holder. It reads whole messages even when a read times out in the middle of
/// one.
pub struct Connection {
    stream: UnixStream,
    buf: Vec<u8>,
}

impl Connection {
    pub fn connect(socket: &Path) -> Result<Self, RecordError> {
        let stream = UnixStream::connect(socket)
            .map_err(RecordError::io(format!("connect to {}", socket.display())))?;
        stream
            .set_write_timeout(Some(Duration::from_secs(10)))
            .map_err(RecordError::io("set the write timeout"))?;
        Ok(Self {
            stream,
            buf: Vec::new(),
        })
    }

    pub fn send(&mut self, request: &HolderRequest) -> Result<(), RecordError> {
        framing::write_msg(&mut self.stream, request)?;
        self.stream
            .flush()
            .map_err(RecordError::io("write to the holder"))
    }

    /// The next event, or `None` when none arrives within `within`. It is an error when the
    /// holder closes the connection.
    pub fn recv(&mut self, within: Duration) -> Result<Option<HolderEvent>, RecordError> {
        let deadline = Instant::now() + within;
        loop {
            if let Some(event) = self.take_event()? {
                return Ok(Some(event));
            }
            if signals::interrupted() {
                return Err(RecordError::Interrupted);
            }
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return Ok(None);
            }
            // A zero timeout is an error on some systems, so the wait is at least 1 ms.
            self.stream
                .set_read_timeout(Some(left.max(Duration::from_millis(1))))
                .map_err(RecordError::io("set the read timeout"))?;
            let mut chunk = [0u8; 64 * 1024];
            match self.stream.read(&mut chunk) {
                Ok(0) => {
                    return Err(RecordError::Holder(
                        "the holder closed the connection".into(),
                    ));
                }
                Ok(n) => self.buf.extend_from_slice(&chunk[..n]),
                Err(e)
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::WouldBlock
                            | std::io::ErrorKind::TimedOut
                            | std::io::ErrorKind::Interrupted
                    ) => {}
                Err(e) => return Err(RecordError::io("read from the holder")(e)),
            }
        }
    }

    /// Decodes the first whole message in the buffer, if there is one.
    fn take_event(&mut self) -> Result<Option<HolderEvent>, RecordError> {
        let Some(header) = self.buf.first_chunk::<4>() else {
            return Ok(None);
        };
        let len = u32::from_le_bytes(*header) as usize;
        if len > MAX_MSG_BYTES {
            return Err(framing::FramingError::TooLarge { len: len as u64 }.into());
        }
        let total = 4 + len;
        if self.buf.len() < total {
            return Ok(None);
        }
        let event = framing::read_msg::<_, HolderEvent>(&mut &self.buf[..total])?;
        self.buf.drain(..total);
        Ok(Some(event))
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(0))
}

/// Random bytes from the system, for the holder's secret and the pane id.
fn urandom<const N: usize>() -> Result<[u8; N], RecordError> {
    let mut bytes = [0u8; N];
    File::open("/dev/urandom")
        .and_then(|mut f| f.read_exact(&mut bytes))
        .map_err(RecordError::io("read /dev/urandom"))?;
    Ok(bytes)
}

/// Finds the `nh` binary: the explicit path, else `NEOHAR_NH`, else the file next to this
/// executable. A test executable lives in `target/debug/deps/`, so the directory above counts as
/// "next to" it too.
pub fn locate_nh(explicit: Option<&Path>) -> Result<PathBuf, RecordError> {
    let is_file = |p: &Path| fs::metadata(p).is_ok_and(|m| m.is_file());
    if let Some(path) = explicit {
        return if is_file(path) {
            Ok(path.to_owned())
        } else {
            Err(RecordError::Holder(format!(
                "--nh {} is not a file",
                path.display()
            )))
        };
    }
    if let Some(path) = std::env::var_os("NEOHAR_NH").map(PathBuf::from) {
        return if is_file(&path) {
            Ok(path)
        } else {
            Err(RecordError::Holder(format!(
                "NEOHAR_NH={} is not a file",
                path.display()
            )))
        };
    }
    let mut tried = Vec::new();
    if let Some(dir) = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(Path::to_owned))
    {
        let mut dirs = vec![dir.clone()];
        if dir.file_name().is_some_and(|name| name == "deps")
            && let Some(up) = dir.parent()
        {
            dirs.push(up.to_owned());
        }
        for dir in dirs {
            let candidate = dir.join("nh");
            if is_file(&candidate) {
                return Ok(candidate);
            }
            tried.push(candidate.display().to_string());
        }
    }
    Err(RecordError::Holder(format!(
        "the nh binary was not found (looked at {}). Build it with \
         `mise exec -- cargo build -p nh`, or pass --nh <path>, or set NEOHAR_NH",
        tried.join(", ")
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_environment_has_only_what_the_tool_sets() {
        let scratch = Scratch::create().expect("scratch");
        let env = scratch.child_env(&[
            ("PS1".into(), "$ ".into()),
            ("PATH".into(), "{work}/bin:/usr/bin".into()),
        ]);
        let get = |key: &str| env.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str());
        assert_eq!(get("TERM"), Some("xterm-256color"));
        assert_eq!(get("LANG"), Some("en_US.UTF-8"));
        assert_eq!(get("PS1"), Some("$ "));
        let work = scratch.work.display().to_string();
        assert_eq!(get("PATH"), Some(format!("{work}/bin:/usr/bin").as_str()));
        assert_eq!(
            get("HOME"),
            Some(scratch.home.display().to_string().as_str())
        );
        // Nothing comes from the environment of the caller.
        for key in ["USER", "LOGNAME", "SHELL", "SSH_AUTH_SOCK", "PWD"] {
            assert_eq!(get(key), None, "{key}");
        }
        // Every path is inside the scratch directory, except the ones that the caller gave.
        for (key, value) in &env {
            if key != "PATH" && value.starts_with('/') {
                assert!(
                    value.starts_with(&scratch.root.display().to_string()),
                    "{key}={value}"
                );
            }
        }
    }

    #[test]
    fn placeholders_name_the_scratch_directories() {
        let scratch = Scratch::create().expect("scratch");
        assert_eq!(
            scratch.expand("{home}:{work}:{tmp}:{scratch}"),
            format!(
                "{}:{}:{}:{}",
                scratch.home.display(),
                scratch.work.display(),
                scratch.tmp.display(),
                scratch.root.display()
            )
        );
        assert_eq!(scratch.expand("{other}"), "{other}");
    }

    #[test]
    fn a_kept_tree_is_taken_over_without_the_files_of_the_old_run() {
        let mut scratch = Scratch::create().expect("scratch");
        let root = scratch.root.clone();
        fs::write(scratch.work.join("note.txt"), "x").expect("a file");
        fs::write(scratch.run.join("old.sock"), "x").expect("a file");
        fs::write(root.join("holder.log"), "x").expect("a file");
        scratch.keep_on_drop();
        drop(scratch);
        assert!(root.is_dir(), "the tree stays");
        let again = Scratch::reuse_at(root.clone()).expect("reused");
        assert!(
            again.work.join("note.txt").is_file(),
            "the program's files stay"
        );
        assert!(!again.run.join("old.sock").exists(), "the sockets go");
        assert!(!root.join("holder.log").exists(), "the logs go");
        drop(again);
        assert!(!root.exists(), "the second run did not keep the tree");
    }

    #[test]
    fn dropping_a_scratch_removes_the_tree() {
        let scratch = Scratch::create().expect("scratch");
        let root = scratch.root.clone();
        assert!(root.is_dir());
        drop(scratch);
        assert!(!root.exists());
    }

    #[test]
    fn a_socket_path_stays_short() {
        // A Unix socket path holds about 100 bytes. The pid has up to 7 digits on Linux, and the
        // counter can reach a few digits. Count 10 more bytes than this process has.
        let scratch = Scratch::create().expect("scratch");
        let ulid = Ulid::from_parts(1_700_000_000_000, 7);
        let socket = scratch.root.join("holders").join(format!("{ulid}.sock"));
        let length = socket.display().to_string().len() + 10;
        assert!(length < 100, "{length} bytes: {}", socket.display());
    }
}
