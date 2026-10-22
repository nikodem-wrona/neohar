//! A test client and a guard for real holders.
//!
//! `TestHolder::spawn` starts `nh hold` the way the core does: stdin on `/dev/null`, stdout on a
//! ready pipe and descriptor 3 on a config pipe. It uses a shell for the redirections, so that the
//! tests need no `unsafe` code. Dropping a `TestHolder` kills the child and then the holder.

#![allow(dead_code)]

use std::{
    io::{BufRead, BufReader, Write},
    os::unix::net::UnixStream,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

use nh_proto::{
    framing::{self, FramingError},
    holder::{
        HoldConfig, HoldMode, HolderEvent, HolderRequest, SignalKind, SignalTarget, StreamId,
    },
    version::HOLDER_PROTO,
};
use rustix::process::{Pid, Signal, WaitOptions, kill_process, waitpid};
use ulid::Ulid;

pub const TIMEOUT: Duration = Duration::from_secs(10);

static COUNTER: AtomicU64 = AtomicU64::new(1);

/// A running holder and what a test needs to talk to it.
pub struct TestHolder {
    pub dir: PathBuf,
    pub socket: PathBuf,
    pub config: HoldConfig,
    pub holder_pid: i32,
    /// The pid of the spawner script, when the test asked for one.
    pub spawner_pid: Option<i32>,
    next_fence: AtomicU64,
}

/// The default config of a test holder for `argv`: 80×24, a 1 MiB ring, and a minimal environment.
pub fn config(argv: &[&str]) -> HoldConfig {
    HoldConfig {
        proto: HOLDER_PROTO,
        pane: [0; 16],
        secret: [0x5a; 32],
        mode: HoldMode::Pty {
            cols: 80,
            rows: 24,
            px_width: 0,
            px_height: 0,
        },
        argv: argv.iter().map(|s| (*s).to_owned()).collect(),
        env: vec![
            ("PATH".into(), "/usr/bin:/bin".into()),
            ("TERM".into(), "xterm".into()),
        ],
        cwd: "/".into(),
        socket_path: String::new(),
        log_path: String::new(),
        ring_bytes: 1 << 20,
        checkpoint_idle_ms: 2_000,
        checkpoint_max_interval_ms: 30_000,
        query_fresh_ms: 5_000,
    }
}

fn nh() -> &'static str {
    env!("CARGO_BIN_EXE_nh")
}

impl TestHolder {
    /// Starts a holder that runs `argv` with the default config.
    pub fn spawn(argv: &[&str]) -> Self {
        Self::spawn_with(config(argv), None)
    }

    /// Starts a holder with `config`. With `spawner`, the holder starts from that shell script.
    /// The script writes `HOLD` where the `nh hold` command goes, and it stays alive until the test
    /// kills it.
    pub fn spawn_with(mut config: HoldConfig, spawner: Option<&str>) -> Self {
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        // The directory name is short: a Unix socket path holds about 100 bytes.
        let dir = PathBuf::from(format!("/tmp/nhh-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("the test directory is created");
        let ulid = Ulid::from_parts(1_700_000_000_000 + n, u128::from(n));
        config.pane = ulid.to_bytes();
        let socket = dir.join("holders").join(format!("{ulid}.sock"));
        config.socket_path = socket.display().to_string();
        config.log_path = dir.join("holder.log").display().to_string();

        // The core's setup: stdin on `/dev/null`, descriptor 3 on the config pipe. A shell makes
        // the redirections, so that the tests need no `unsafe` code. The holder must not start as a
        // background command: a shell then replaces its stdin with `/dev/null` before the
        // redirections run.
        let hold = r#""$0" hold --pane "$1" --config-fd 3 3<&0 </dev/null"#;
        let script = spawner.map_or_else(|| format!("exec {hold}"), |s| s.replace("HOLD", hold));
        let mut child = Command::new("sh")
            .args(["-c", &script, nh(), &ulid.to_string()])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("sh starts");
        let mut stdin = child.stdin.take().expect("stdin is piped");
        framing::write_msg(&mut stdin, &config).expect("the config is written");
        drop(stdin);
        let stdout = child.stdout.take().expect("stdout is piped");
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let mut line = String::new();
            let _ = BufReader::new(stdout).read_line(&mut line);
            let _ = tx.send(line);
        });
        let line = rx
            .recv_timeout(TIMEOUT)
            .expect("the holder reports within the timeout");
        let holder_pid = line
            .trim()
            .strip_prefix("READY ")
            .unwrap_or_else(|| panic!("the holder did not report READY: {line:?}"))
            .parse()
            .expect("READY has a pid");
        let spawner_pid = if spawner.is_none() {
            // The first process leaves after the holder detached.
            assert!(child.wait().expect("sh ends").success());
            None
        } else {
            // The test owns the spawner. It kills it and does not wait for it.
            let pid = i32::try_from(child.id()).expect("a pid fits in i32");
            std::mem::forget(child);
            Some(pid)
        };
        Self {
            dir,
            socket,
            config,
            holder_pid,
            spawner_pid,
            next_fence: AtomicU64::new(1),
        }
    }

    /// Connects without saying `Hello`.
    pub fn connect(&self) -> Client {
        Client::connect(&self.socket)
    }

    /// Connects and says `Hello` with the next fencing token. Panics unless the holder accepts.
    pub fn client(&self) -> Client {
        let mut client = self.connect();
        let token = self.next_fence.fetch_add(1, Ordering::Relaxed);
        client.send(&hello(&self.config.secret, token));
        match client.recv() {
            HolderEvent::HelloOk { fencing_token, .. } => assert_eq!(fencing_token, token),
            other => panic!("expected HelloOk, got {other:?}"),
        }
        client
    }

    /// A client that already asked for the journal from `from`.
    pub fn attach(&self, from: u64) -> Client {
        let mut client = self.client();
        client.send(&HolderRequest::Attach { from_offset: from });
        client
    }
}

impl Drop for TestHolder {
    fn drop(&mut self) {
        // Stop the child first, so that it does not outlive the holder. The token is high enough
        // to take the lease from any other client.
        if let Ok(stream) = UnixStream::connect(&self.socket) {
            let mut client = Client {
                stream,
                pending: Vec::new(),
            };
            client.send(&hello(&self.config.secret, u64::MAX));
            client.send(&HolderRequest::Signal {
                target: SignalTarget::Child,
                signal: SignalKind::Kill,
            });
            std::thread::sleep(Duration::from_millis(50));
        }
        if let Some(pid) = Pid::from_raw(self.holder_pid) {
            let _ = kill_process(pid, Signal::KILL);
        }
        // The spawner is a child of the test. Kill it only if it is still there to collect: a pid
        // that the test already collected could belong to another process by now.
        if let Some(pid) = self.spawner_pid.and_then(Pid::from_raw)
            && matches!(waitpid(Some(pid), WaitOptions::NOHANG), Ok(None))
        {
            let _ = kill_process(pid, Signal::KILL);
            let _ = waitpid(Some(pid), WaitOptions::empty());
        }
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

pub fn hello(secret: &[u8; 32], fencing_token: u64) -> HolderRequest {
    HolderRequest::Hello {
        proto: HOLDER_PROTO,
        secret: *secret,
        fencing_token,
        client: "test".into(),
    }
}

/// A blocking client with a read timeout.
pub struct Client {
    stream: UnixStream,
    /// Events that `output_until` read but did not use.
    pending: Vec<HolderEvent>,
}

impl Client {
    pub fn connect(socket: &Path) -> Self {
        let stream = UnixStream::connect(socket).expect("the socket accepts a connection");
        stream
            .set_read_timeout(Some(TIMEOUT))
            .expect("a read timeout is set");
        Self {
            stream,
            pending: Vec::new(),
        }
    }

    pub fn send(&mut self, request: &HolderRequest) {
        framing::write_msg(&mut self.stream, request).expect("the request is written");
        self.stream.flush().expect("the stream flushes");
    }

    /// The next event. Panics on a timeout or when the holder closed the connection.
    pub fn recv(&mut self) -> HolderEvent {
        self.try_recv().unwrap_or_else(|e| panic!("no event: {e}"))
    }

    /// The next event, or the error when the stream ends or times out.
    pub fn try_recv(&mut self) -> Result<HolderEvent, FramingError> {
        if self.pending.is_empty() {
            framing::read_msg(&mut self.stream)
        } else {
            Ok(self.pending.remove(0))
        }
    }

    /// Reads events until `done` is true for the output so far. Returns the output and the offset
    /// after its last byte. Events that are not output go to `others`.
    pub fn output_until(
        &mut self,
        done: impl Fn(&[u8]) -> bool,
        others: &mut Vec<HolderEvent>,
    ) -> (Vec<u8>, u64) {
        let (mut out, mut end) = (Vec::new(), 0);
        let deadline = Instant::now() + TIMEOUT;
        while !done(&out) {
            assert!(
                Instant::now() < deadline,
                "timed out; output so far: {:?}",
                String::from_utf8_lossy(&out)
            );
            match self.recv() {
                HolderEvent::Output {
                    offset,
                    stream: StreamId::Pty,
                    bytes,
                } => {
                    end = offset + bytes.len() as u64;
                    out.extend_from_slice(&bytes);
                }
                other => others.push(other),
            }
        }
        (out, end)
    }

    /// Output until it contains `needle`.
    pub fn output_containing(&mut self, needle: &str) -> (Vec<u8>, u64) {
        let n = needle.as_bytes().to_vec();
        self.output_until(move |o| o.windows(n.len()).any(|w| w == n), &mut Vec::new())
    }

    /// Waits for an event that `pick` returns a value for, and skips the others.
    pub fn wait_for<T>(&mut self, pick: impl Fn(&HolderEvent) -> Option<T>) -> T {
        let deadline = Instant::now() + TIMEOUT;
        loop {
            assert!(Instant::now() < deadline, "timed out waiting for an event");
            let event = self.recv();
            if let Some(found) = pick(&event) {
                return found;
            }
        }
    }

    /// The next event, or `None` when none arrives within `within`.
    pub fn recv_within(&mut self, within: Duration) -> Option<HolderEvent> {
        if !self.pending.is_empty() {
            return Some(self.pending.remove(0));
        }
        self.stream
            .set_read_timeout(Some(within))
            .expect("a read timeout is set");
        let result = framing::read_msg(&mut self.stream);
        self.stream
            .set_read_timeout(Some(TIMEOUT))
            .expect("a read timeout is set");
        match result {
            Ok(event) => Some(event),
            Err(FramingError::Io(e))
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) =>
            {
                None
            }
            Err(e) => panic!("the connection failed: {e}"),
        }
    }

    /// The events that arrive within `within`.
    pub fn events_within(&mut self, within: Duration) -> Vec<HolderEvent> {
        let deadline = Instant::now() + within;
        let mut out = Vec::new();
        while let Some(left) = deadline.checked_duration_since(Instant::now()) {
            match self.recv_within(left.max(Duration::from_millis(1))) {
                Some(event) => out.push(event),
                None => break,
            }
        }
        out
    }

    /// Whether the holder closes the connection within `within`.
    pub fn closed_within(&mut self, within: Duration) -> bool {
        // macOS refuses to set a timeout on a socket that the peer has already closed.
        if self.stream.set_read_timeout(Some(within)).is_err() {
            return true;
        }
        let closed = matches!(
            self.try_recv(),
            Err(FramingError::Eof | FramingError::Truncated)
        );
        let _ = self.stream.set_read_timeout(Some(TIMEOUT));
        closed
    }
}

/// `count` of the non-overlapping occurrences of `needle` in `haystack`.
pub fn count(haystack: &[u8], needle: &str) -> usize {
    String::from_utf8_lossy(haystack).matches(needle).count()
}
