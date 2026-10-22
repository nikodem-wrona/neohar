//! A "remote" machine for the fake ssh: its own directories, a session
//! name for its daemon, and the helpers that open a link to it. Each test makes its own machine
//! and stops its daemon when the machine drops.

#![allow(dead_code)]

use std::{
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::Command,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

use nh_core::RecordingHost;
use nh_remote::{
    ClientHello, ClientOptions, LinkClient,
    ssh::{SshError, SshSpec, connect_ssh},
};
use ulid::Ulid;

pub const NH: &str = env!("CARGO_BIN_EXE_nh");
pub const WAIT: Duration = Duration::from_secs(30);

static COUNTER: AtomicU64 = AtomicU64::new(1);

pub fn fake_ssh() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/support/fake-ssh.sh")
}

/// A machine for the fake ssh: its own directories, and a session name for its daemon.
pub struct Remote {
    pub dir: PathBuf,
    pub session: String,
    /// More sessions of the same machine, from `session_for`. Dropping the machine stops them.
    others: std::sync::Mutex<Vec<String>>,
}

impl Remote {
    pub fn new() -> Self {
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        // Short: the runtime directory holds Unix sockets.
        let dir = PathBuf::from(format!("/tmp/nhs-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        for sub in ["home", "config", "data", "run", "local"] {
            std::fs::create_dir_all(dir.join(sub)).expect("a directory");
        }
        std::fs::set_permissions(dir.join("run"), std::fs::Permissions::from_mode(0o700))
            .expect("mode");
        Self {
            dir,
            session: format!("s{}x{n}", std::process::id()),
            others: std::sync::Mutex::new(Vec::new()),
        }
    }

    /// The variables that make the command of the fake ssh a command of this machine.
    pub fn env(&self) -> Vec<(String, String)> {
        let path = |sub: &str| self.dir.join(sub).display().to_string();
        vec![
            ("REMOTE_NEOHAR_CONFIG_DIR".into(), path("config")),
            ("REMOTE_NEOHAR_DATA_DIR".into(), path("data")),
            ("REMOTE_NEOHAR_RUNTIME_DIR".into(), path("run")),
            ("REMOTE_HOME".into(), path("home")),
            ("REMOTE_SHELL".into(), "/bin/sh".into()),
            ("REMOTE_PATH".into(), "/usr/bin:/bin:/usr/sbin:/sbin".into()),
        ]
    }

    /// The spec of a link: the fake ssh, `nh` of this build as the remote `nh`.
    pub fn spec(&self) -> SshSpec {
        let mut spec = SshSpec::new("studio", &self.session, NH, self.dir.join("local"));
        spec.program = fake_ssh();
        spec.env = self
            .env()
            .into_iter()
            .map(|(k, v)| (k.into(), v.into()))
            .collect();
        spec
    }

    /// The processes `nh bridge` of this machine.
    pub fn bridges(&self) -> usize {
        self.bridges_of(&self.session)
    }

    /// The processes `nh bridge` of one session of this machine.
    pub fn bridges_of(&self, session: &str) -> usize {
        let out = Command::new("pgrep")
            .args(["-f", &format!("bridge --session {session} --proto")])
            .output()
            .expect("pgrep runs");
        String::from_utf8_lossy(&out.stdout).lines().count()
    }

    /// Another session of this machine: its name is `<session><suffix>`, and dropping the machine
    /// stops its daemon too.
    pub fn session_for(&self, suffix: &str) -> String {
        let name = format!("{}{suffix}", self.session);
        self.others
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(name.clone());
        name
    }

    fn stop_session(&self, session: &str) {
        let mut command = Command::new(NH);
        command.args(["daemon", "stop", "--session", session]);
        for (key, value) in self.env() {
            if let Some(name) = key.strip_prefix("REMOTE_") {
                command.env(name, value);
            }
        }
        let _ = command.output();
    }

    pub fn stop_daemon(&self) {
        self.stop_session(&self.session);
    }
}

impl Drop for Remote {
    /// Stops the daemons of the machine, kills the holders of their panes and removes the
    /// directory. `nh daemon stop` ends a daemon as a quit does: its panes keep running.
    fn drop(&mut self) {
        self.stop_daemon();
        let others = std::mem::take(
            &mut *self
                .others
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
        );
        for session in others {
            self.stop_session(&session);
        }
        crate::core_support::reap::kill_under(&self.dir);
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

pub fn hello() -> ClientHello {
    ClientHello {
        device: Ulid::from_parts(1, 77),
        name: "laptop".to_owned(),
        app_version: env!("CARGO_PKG_VERSION").to_owned(),
    }
}

pub fn host() -> Arc<RecordingHost> {
    Arc::new(RecordingHost::new(PathBuf::from("/bin/nh"), true))
}

pub async fn connect(spec: &SshSpec) -> Result<LinkClient, SshError> {
    connect_ssh(spec, hello(), host(), ClientOptions::default()).await
}

pub async fn eventually(what: &str, mut done: impl FnMut() -> bool) {
    let deadline = Instant::now() + WAIT;
    while !done() {
        assert!(Instant::now() < deadline, "waited for {what}");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

pub fn write_program(path: &Path, text: &str) {
    std::fs::write(path, text).expect("a program");
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).expect("mode");
}

/// A `claude` that reports the login state in `$HOME/claude-login` (`in` or anything else), and a
/// `codex` that is logged out. The `PATH` of the login shell of the daemon finds them first.
pub const FAKE_CLAUDE: &str = r#"#!/bin/sh
case "$1 $2" in
  "--version "*) echo '2.1.292 (Claude Code)' ;;
  "auth status")
    if [ "$(cat "$HOME/claude-login" 2>/dev/null)" = in ]; then
      echo '{"loggedIn": true, "authMethod": "claude.ai"}'
    else
      echo '{"loggedIn": false, "authMethod": "none"}'; exit 1
    fi ;;
  *) exit 2 ;;
esac
"#;

pub const FAKE_CODEX: &str = r#"#!/bin/sh
case "$1 $2" in
  "--version "*) echo 'codex-cli 0.162.1' ;;
  "login status") echo 'Not logged in' >&2; exit 1 ;;
  *) exit 2 ;;
esac
"#;

impl Remote {
    /// Puts the two fake harnesses on the `PATH` of the login shell of the machine, as
    /// `~/.local/bin` is on the `PATH` of a real machine. The state of `claude` is
    /// `set_claude_login`.
    pub fn with_harnesses(self) -> Self {
        let bin = self.dir.join("home/.local/bin");
        std::fs::create_dir_all(&bin).expect("a bin");
        write_program(&bin.join("claude"), FAKE_CLAUDE);
        write_program(&bin.join("codex"), FAKE_CODEX);
        std::fs::write(
            self.dir.join("home/.profile"),
            format!(
                "export PATH='{}:/usr/bin:/bin:/usr/sbin:/sbin'\n",
                bin.display()
            ),
        )
        .expect("a profile");
        self
    }

    /// Changes what `claude auth status` says on the machine.
    pub fn set_claude_login(&self, logged_in: bool) {
        std::fs::write(
            self.dir.join("home/claude-login"),
            if logged_in { "in" } else { "out" },
        )
        .expect("a state file");
    }
}
