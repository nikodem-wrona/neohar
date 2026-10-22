//! A core with real holders for the tests of `nh-core`.
//!
//! `Harness::start` makes a directory under `/tmp` with the config, data and runtime roots inside
//! it, starts a core that runs the `nh` binary of this build, and stops every child on drop.
//! Holders outlive their parent by design, so the guard also kills each holder that does not leave
//! in time, and each holder that is left in the directory after the core stopped ([`reap`]).

#![allow(dead_code)]

pub mod client;
pub mod reap;

use std::{
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

use nh_core::{
    Core, CoreConfig, CoreHandle, FrameSink, MessageSink, Tuning,
    pane::{Pane, PaneSpec},
};
use nh_proto::{
    frame::Frame,
    input::{LogicalInput, LogicalText},
    paths::{Env, Platform},
};
use nh_store::{SnapshotRecord, Store};
use rustix::process::{Pid, Signal, kill_process, test_kill_process};
use serde_json::{Value, json};
use tokio::{sync::mpsc, time::timeout};
use ulid::Ulid;

pub const TIMEOUT: Duration = Duration::from_secs(15);

/// The client device of the host in the process of a test core.
pub const TEST_DEVICE: Ulid = Ulid(0x0100_0000_0000_0000_0000_0000_0000_0001);

static COUNTER: AtomicU64 = AtomicU64::new(1);
static INPUT_IDS: AtomicU64 = AtomicU64::new(1);

/// The token that the hook tests send. [`Harness::with_hook_pane`] gives it to a pane of the
/// layout, because a hook belongs to a pane that exists.
pub const TEST_PANE_TOKEN: &str = "pane-1";

/// A `claude` for the tests. It tells its version, and its login probe says that it is logged in.
/// Started otherwise, it keeps its arguments in
/// `$HOME/claude-args`, draws the input box of Claude Code, and keeps what it reads in
/// `$HOME/claude-input`, so that a test sees what the core started and typed. The process is
/// called `claude`, as the detection of the agents wants.
const FAKE_CLAUDE: &str = r#"#!/bin/bash
if [ "$1" = "--version" ]; then echo '2.1.292 (Claude Code)'; exit 0; fi
if [ "$1 $2" = "auth status" ]; then echo '{"loggedIn": true, "authMethod": "claude.ai"}'; exit 0; fi
if [ -z "$NH_FAKE_TUI" ]; then
  export NH_FAKE_TUI=1
  exec -a claude /bin/bash "$0" "$@"
fi
printf '%s\n' "$*" >> "$HOME/claude-args"
printf '\033[2J\033[H'
echo "──────────────────────────────────────────────"
echo "❯ "
echo "──────────────────────────────────────────────"
stty -echo -icanon 2>/dev/null
while IFS= read -rsn1 key; do
  if [ -z "$key" ]; then printf '\n' >> "$HOME/claude-input"; else printf '%s' "$key" >> "$HOME/claude-input"; fi
done
"#;

/// A `codex` for the tests, so that the core never runs the one of the machine. It tells its
/// version and that it is logged in, and it does nothing else.
const FAKE_CODEX: &str = r#"#!/bin/sh
case "$1 $2" in
  "--version "*) echo 'codex-cli 0.162.1' ;;
  "login status") echo 'Logged in using ChatGPT' >&2 ;;
  *) exit 2 ;;
esac
"#;

/// Writes a program of the test, with the execute bit. A program that was there is replaced, so
/// it has a new modification time and the core reads its version again.
pub fn write_program(path: &Path, script: &str) {
    let _ = std::fs::remove_file(path);
    std::fs::write(path, script).expect("the program is written");
    std::fs::set_permissions(path, std::os::unix::fs::PermissionsExt::from_mode(0o755))
        .expect("the program runs");
}

pub struct Harness {
    /// The core. It is `None` between a `crash` and a `restart`.
    pub core: Option<Core>,
    pub handle: CoreHandle,
    pub dir: PathBuf,
    pub paths: Env,
    config: CoreConfig,
}

impl Harness {
    pub fn start() -> Self {
        Self::start_with(env!("CARGO_BIN_EXE_nh").into())
    }

    /// Starts a core that runs `nh` as the sidecar.
    pub fn start_with(nh: PathBuf) -> Self {
        Self::start_full(nh, Tuning::default(), None)
    }

    /// Starts a core whose host is `host`, for the tests of the notifications and the clipboard.
    pub fn start_hosted(host: Arc<dyn nh_core::Host>) -> Self {
        Self::start_full(
            env!("CARGO_BIN_EXE_nh").into(),
            Tuning::default(),
            Some(host),
        )
    }

    /// Starts a core whose host is `host`, with other settings of the pane runtime.
    pub fn start_hosted_tuned(host: Arc<dyn nh_core::Host>, tuning: Tuning) -> Self {
        Self::start_full(env!("CARGO_BIN_EXE_nh").into(), tuning, Some(host))
    }

    /// Starts a core that makes the panes of the layout and starts no holders for them, for the
    /// tests of the layout alone.
    pub fn start_layout() -> Self {
        Self::start_tuned(Tuning {
            start_holders: false,
            ..Tuning::default()
        })
    }

    /// Starts a core with other settings of the pane runtime.
    pub fn start_tuned(tuning: Tuning) -> Self {
        Self::start_full(env!("CARGO_BIN_EXE_nh").into(), tuning, None)
    }

    /// Starts a core whose machine has no harness: no fake `claude` or `codex`, and a `PATH`
    /// with only the directories of the system.
    pub fn start_without_harnesses() -> Self {
        Self::start_machine(
            env!("CARGO_BIN_EXE_nh").into(),
            Tuning::default(),
            None,
            false,
        )
    }

    fn start_full(nh: PathBuf, tuning: Tuning, host: Option<Arc<dyn nh_core::Host>>) -> Self {
        Self::start_machine(nh, tuning, host, true)
    }

    fn start_machine(
        nh: PathBuf,
        tuning: Tuning,
        host: Option<Arc<dyn nh_core::Host>>,
        harnesses: bool,
    ) -> Self {
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        // The directory name is short: a Unix socket path holds about 100 bytes.
        let dir = PathBuf::from(format!("/tmp/nhc-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let home = dir.join("home");
        std::fs::create_dir_all(&home).expect("the test directory is created");
        // A `claude` that only tells its version. The version decides what a run of Claude Code
        // may do, and the recordings cover 2.1.292.
        let bin = dir.join("bin");
        std::fs::create_dir_all(&bin).expect("the test directory is created");
        if harnesses {
            for (name, script) in [("claude", FAKE_CLAUDE), ("codex", FAKE_CODEX)] {
                write_program(&bin.join(name), script);
            }
        }
        let system = if harnesses {
            "/usr/bin:/bin:/usr/sbin:/sbin:/opt/homebrew/bin"
        } else {
            "/usr/bin:/bin:/usr/sbin:/sbin"
        };
        let paths = Env {
            platform: Platform::current(),
            uid: rustix::process::getuid().as_raw(),
            home: Some(home.clone()),
            config_override: Some(dir.join("config")),
            data_override: Some(dir.join("data")),
            runtime_override: Some(dir.join("run")),
            xdg_config_home: None,
            xdg_data_home: None,
            xdg_runtime_dir: None,
        };
        let config = CoreConfig {
            session: "test".to_owned(),
            paths: paths.clone(),
            nh_path: nh,
            base_env: vec![
                ("PATH".into(), format!("{}:{system}", bin.display())),
                ("HOME".into(), home.display().to_string()),
                ("SHELL".into(), "/bin/sh".into()),
                ("LANG".into(), "en_US.UTF-8".into()),
            ],
            tuning,
            host: Some(nh_core::InProcessHost::new(
                nh_core::ClientInfo {
                    device: TEST_DEVICE,
                    name: "test".to_owned(),
                },
                host.unwrap_or_else(|| std::sync::Arc::new(nh_core::NoopHost::default())),
            )),
            pane_tokens: vec![(ulid::Ulid::from(1u128), TEST_PANE_TOKEN.to_owned())],
        };
        let core = Core::start(config.clone()).expect("the core starts");
        let handle = core.handle();
        Self {
            core: Some(core),
            handle,
            dir,
            paths,
            config,
        }
    }

    /// Ends the core without a quit: the runtime stops and its tasks are gone, as after a crash of
    /// the host. The panes and their holders keep running.
    pub async fn crash(&mut self) {
        if let Some(core) = self.core.take() {
            // A runtime cannot stop inside a task, so a thread for blocking work does it.
            tokio::task::spawn_blocking(move || core.shutdown(Duration::from_secs(5)))
                .await
                .expect("the core stops");
        }
    }

    /// Starts a new core over the same directories, as a relaunch of the host does. Call it after
    /// [`Harness::crash`]. `handle` is the handle of the new core.
    pub fn restart(&mut self) {
        assert!(self.core.is_none(), "crash the core before the restart");
        let core = Core::start(self.config.clone()).expect("the core restarts");
        self.handle = core.handle();
        self.core = Some(core);
    }

    /// Like `restart`, with other settings of the pane runtime.
    pub fn restart_tuned(&mut self, tuning: Tuning) {
        self.config.tuning = tuning;
        self.restart();
    }

    /// The path of `state.db`.
    pub fn db_path(&self) -> PathBuf {
        self.paths
            .data_dir("test")
            .expect("the data directory")
            .join("state.db")
    }

    /// A second connection to `state.db`, for the test to read.
    pub fn db(&self) -> Store {
        Store::open(&self.db_path()).expect("the database opens")
    }

    /// Waits until the database holds a snapshot of `pane` at `min_offset` or later.
    pub async fn wait_snapshot(&self, pane: Ulid, min_offset: u64) -> SnapshotRecord {
        self.wait_snapshot_within(pane, min_offset, TIMEOUT)
            .await
            .unwrap_or_else(|| panic!("waited for a snapshot of {pane} at {min_offset} or later"))
    }

    /// Like [`Harness::wait_snapshot`], with a limit. Returns `None` when it passes.
    pub async fn wait_snapshot_within(
        &self,
        pane: Ulid,
        min_offset: u64,
        within: Duration,
    ) -> Option<SnapshotRecord> {
        let db = self.db();
        let deadline = Instant::now() + within;
        loop {
            let snapshots = db.snapshots(pane).expect("the snapshots");
            if let Some(newest) = snapshots.into_iter().next()
                && newest.offset >= min_offset
            {
                return Some(newest);
            }
            if Instant::now() >= deadline {
                return None;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    /// Waits until the restart is done and returns the pane with this id.
    pub async fn restored(&self, pane: Ulid) -> Arc<Pane> {
        timeout(TIMEOUT, self.handle.wait_restored())
            .await
            .expect("the restart ends in time");
        self.handle.pane(pane).unwrap_or_else(|| {
            panic!(
                "the pane {pane} is not back: {:?}",
                self.handle.restore_report()
            )
        })
    }

    /// The socket of the API, the one that `NEOHAR_SOCKET` names in a pane.
    pub fn api_socket(&self) -> PathBuf {
        self.dir.join("run").join("test").join("api.sock")
    }

    /// Starts a pane that runs `argv`.
    pub async fn spawn(&self, argv: &[&str], cols: u16, rows: u16) -> Arc<Pane> {
        let spec = PaneSpec {
            argv: argv.iter().map(|s| (*s).to_owned()).collect(),
            cwd: None,
            env: Vec::new(),
            cols,
            rows,
        };
        self.handle.spawn_pane(spec).await.expect("the pane starts")
    }

    /// Makes a workspace with a pane in the layout and gives the pane the token of the hook tests.
    /// The pane has no holder: start the core with [`Harness::start_layout`]. A core that
    /// restarts keeps the token.
    pub async fn with_hook_pane(mut self) -> Self {
        let made = self
            .call("workspace.create", json!({ "name": "Hooks" }))
            .await;
        let pane: Ulid = made["pane"]
            .as_str()
            .and_then(|id| id.parse::<nh_proto::api::ids::PaneId>().ok())
            .map(nh_proto::api::ids::PaneId::ulid)
            .expect("a pane id");
        self.config.pane_tokens = vec![(pane, TEST_PANE_TOKEN.to_owned())];
        self.handle.pane_tokens().register(pane, TEST_PANE_TOKEN);
        self
    }

    /// The pane that [`Harness::with_hook_pane`] made, as the reference of a call.
    pub fn hook_pane(&self) -> String {
        nh_proto::api::ids::PaneId::new(self.config.pane_tokens[0].0).to_string()
    }

    /// Waits until the pane with this id has a holder, and returns it. `id` is the id of a pane of
    /// the layout, as a call returns it.
    pub async fn started(&self, id: &Value) -> Arc<Pane> {
        let id: Ulid = id.as_str().expect("an id").parse().expect("a ULID");
        let deadline = Instant::now() + TIMEOUT;
        loop {
            if let Some(pane) = self.handle.pane(id) {
                return pane;
            }
            assert!(
                Instant::now() < deadline,
                "the holder of {id} did not start"
            );
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    /// Opens a window that shows the selected tab.
    pub async fn open_window(&self) -> String {
        self.call("window.open", json!({})).await["window"]
            .as_str()
            .expect("a window id")
            .to_owned()
    }

    /// The window reports what the user looks at: `pane` is the active element, or `None` when
    /// the focus is in the chrome.
    pub async fn look_at(&self, window: &str, pane: Option<&str>, os_focused: bool) {
        self.call(
            "window.focus_report",
            json!({
                "window": window,
                "focused": os_focused,
                "visible": true,
                "active_pane": pane,
            }),
        )
        .await;
    }

    /// Makes a workspace whose pane runs `argv`. Start the core with [`Harness::start`], so that
    /// the pane has a holder. The core gives the pane a token, which the program finds in
    /// `NEOHAR_PANE_TOKEN`.
    pub async fn with_command_pane(self, argv: &[&str]) -> Self {
        self.call(
            "workspace.create",
            json!({ "name": "Keys", "command": { "argv": argv } }),
        )
        .await;
        self
    }

    /// Calls a method and returns its result.
    pub async fn call(&self, method: &str, params: Value) -> Value {
        self.handle
            .rpc(method, params)
            .await
            .unwrap_or_else(|e| panic!("{method} failed: {e}"))
    }

    /// Types text into a pane as an input method would.
    pub async fn text(&self, pane: &Pane, text: &str) {
        self.input(pane, &LogicalInput::Text(LogicalText { text: text.into() }))
            .await;
    }

    /// Presses a key that has no text, for example `Enter`.
    pub async fn key(&self, pane: &Pane, code: &str, key: &str) {
        let event = json!({ "type": "key", "code": code, "key": key });
        self.call(
            "pane.input",
            json!({ "pane": pane.id().to_string(), "input_id": next_input_id(), "event": event }),
        )
        .await;
    }

    pub async fn input(&self, pane: &Pane, input: &LogicalInput) {
        self.call(
            "pane.input",
            json!({
                "pane": pane.id().to_string(),
                "input_id": next_input_id(),
                "event": serde_json::to_value(input).expect("the input serializes"),
            }),
        )
        .await;
    }
}

pub fn next_input_id() -> u64 {
    INPUT_IDS.fetch_add(1, Ordering::Relaxed)
}

impl Drop for Harness {
    /// Stops every pane that the core started, also the ones that `debug.spawn_panes` made.
    /// Holders outlive their parent by design, so a holder that does not leave in time is killed,
    /// and the guard waits for each of them: a test must leave no process behind.
    ///
    /// Then the guard stops the core and kills each holder that is left in the directory. The core
    /// does not know all of them: a holder after a crash, a holder whose start the crash cut
    /// before the core stored it, and a holder of a closed pane that waits for the core to
    /// acknowledge the exit. Without the kill, such a holder runs for ever and makes the directory
    /// again when it binds its socket again.
    fn drop(&mut self) {
        if self.core.is_none() {
            // The core crashed. It acknowledges no exit, so its holders never leave on their own,
            // and a pane that was in its start never ends it. Kill the programs and the processes
            // that they started; the search below kills the holders.
            for pane in self.handle.panes() {
                pane.kill_all();
            }
        } else {
            self.handle.stop_panes();
            let live: Vec<i32> = self.handle.panes().iter().map(|p| p.holder_pid()).collect();
            let deadline = Instant::now() + Duration::from_secs(5);
            for raw in &live {
                let Some(pid) = Pid::from_raw(*raw) else {
                    continue;
                };
                // A stopped holder does not leave on its own, and a stopped process ignores
                // SIGTERM.
                let _ = kill_process(pid, Signal::CONT);
                while test_kill_process(pid).is_ok() && Instant::now() < deadline {
                    std::thread::sleep(Duration::from_millis(10));
                }
                if test_kill_process(pid).is_ok() {
                    let _ = kill_process(pid, Signal::KILL);
                }
            }
        }
        // A core that runs can start one more holder, for example for a pane that the layout or an
        // agent asked for, and it can write into the directory after the removal. Stop it first,
        // so that the search below finds every holder. The tasks of the runtime stop at once: a
        // holder whose start they cut has its socket when it said READY, or it exits when it
        // cannot say READY. The guard does not wait for the blocking work of the core (the probes
        // of the harnesses): under load that wait is about 1.5 s for each test.
        if let Some(core) = self.core.take() {
            // `Core::shutdown` blocks, and the drop can run in a task of the test, where a runtime
            // must not block. A thread of its own stops the core.
            let _ = std::thread::spawn(move || core.shutdown(Duration::ZERO)).join();
        }
        reap::kill_under(&self.dir);
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// Waits until the text of `pane` satisfies `pred`. Returns the text. Panics with the screen when
/// it does not happen in time.
pub async fn wait_for(pane: &Pane, what: &str, pred: impl Fn(&[String]) -> bool) -> Vec<String> {
    let mut damage = pane.subscribe_damage();
    let deadline = Instant::now() + TIMEOUT;
    loop {
        let text = pane.text();
        if pred(&text) {
            return text;
        }
        let Some(left) = deadline.checked_duration_since(Instant::now()) else {
            panic!("waited for {what}, the screen is:\n{}", text.join("\n"));
        };
        let _ = timeout(left, damage.changed()).await;
    }
}

/// Whether any row of the screen equals `line`.
pub fn has_row(text: &[String], line: &str) -> bool {
    text.iter().any(|row| row == line)
}

/// A render stream that keeps the frames.
pub struct Frames {
    pub rx: mpsc::UnboundedReceiver<Vec<u8>>,
}

struct CollectSink(mpsc::UnboundedSender<Vec<u8>>);

impl FrameSink for CollectSink {
    fn send(&self, frame: Vec<u8>) -> bool {
        self.0.send(frame).is_ok()
    }
}

impl Frames {
    /// Opens the render stream of `window`.
    pub fn open(handle: &CoreHandle, window: &str, hz: f64) -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        handle
            .stream(
                "render",
                json!({ "window": window, "viewport_hz": hz, "proto": nh_proto::version::FRAME_PROTO }),
                Arc::new(CollectSink(tx)),
            )
            .expect("the stream opens");
        Self { rx }
    }

    /// The next frame, or `None` when none comes within `within`.
    pub async fn next(&mut self, within: Duration) -> Option<Frame> {
        let bytes = timeout(within, self.rx.recv()).await.ok()??;
        Some(postcard::from_bytes(&bytes).expect("the bytes are a frame"))
    }
}

/// A projection subscription that keeps the messages.
pub struct Messages {
    pub rx: mpsc::UnboundedReceiver<Value>,
}

struct CollectMessages(mpsc::UnboundedSender<Value>);

impl MessageSink for CollectMessages {
    fn send(&self, message: Value) -> bool {
        self.0.send(message).is_ok()
    }
}

impl Messages {
    pub fn subscribe(handle: &CoreHandle, name: &str) -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        handle
            .subscribe(name, None, Box::new(CollectMessages(tx)))
            .expect("the projection exists");
        Self { rx }
    }

    /// The next message that `pred` accepts. Panics when none comes in time.
    pub async fn wait_for(&mut self, what: &str, pred: impl Fn(&Value) -> bool) -> Value {
        let deadline = Instant::now() + TIMEOUT;
        loop {
            let left = deadline
                .checked_duration_since(Instant::now())
                .unwrap_or_else(|| panic!("waited for {what}"));
            let message = timeout(left, self.rx.recv())
                .await
                .unwrap_or_else(|_| panic!("waited for {what}"))
                .unwrap_or_else(|| panic!("the subscription ended before {what}"));
            if pred(&message) {
                return message;
            }
        }
    }
}
