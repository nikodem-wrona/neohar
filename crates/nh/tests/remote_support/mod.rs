//! A second machine that ssh reaches (`NEOHAR_REMOTE_TARGET`) as a test target.
//!
//! A test that uses this module runs only with `NEOHAR_REMOTE=1` and `NEOHAR_REMOTE_TARGET=<ssh
//! target>`; else `RemoteTarget::from_env` prints why and returns `None`, and the test returns.
//! Everything of the test lives under `~/.neohar-dev/` on the remote machine, and the wrapper
//! `nh-dev` of `scripts/remote-dev.sh` sets the config, data and runtime directories there. The
//! remote home is not the local home, so every remote path in a command is written with `~`, for
//! the remote shell. A `RemoteSession` stops what its test started when it is dropped.

#![allow(dead_code)]

use std::{
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
    time::{Duration, Instant},
};

/// The wrapper that sets the three directories (`scripts/remote-dev.sh` writes it).
pub const NH_DEV: &str = "~/.neohar-dev/bin/nh-dev";

/// The ssh options of every command: a key that needs a question is a failure, not a prompt.
pub const SSH_OPTIONS: [&str; 4] = ["-o", "BatchMode=yes", "-o", "ConnectTimeout=10"];

#[derive(Clone, Debug)]
pub struct RemoteTarget {
    pub host: String,
}

impl RemoteTarget {
    /// The remote target, or `None` (with a message) when the test is not asked to run.
    pub fn from_env() -> Option<Self> {
        if std::env::var("NEOHAR_REMOTE").as_deref() != Ok("1") {
            eprintln!(
                "skipped: set NEOHAR_REMOTE=1 and NEOHAR_REMOTE_TARGET=<ssh target> to run the \
                 tests of the remote machine (mise run test:remote)"
            );
            return None;
        }
        match std::env::var("NEOHAR_REMOTE_TARGET") {
            Ok(host) if !host.is_empty() => Some(Self { host }),
            _ => panic!("NEOHAR_REMOTE=1 needs NEOHAR_REMOTE_TARGET=<ssh target>"),
        }
    }

    /// A command that runs `command` on the remote machine, with its stdio not set.
    pub fn command(&self, command: &str) -> Command {
        let mut ssh = Command::new("ssh");
        ssh.args(SSH_OPTIONS).arg("-T").arg(&self.host).arg(command);
        ssh
    }

    /// Runs `command` in the remote shell and waits for it.
    pub fn run(&self, command: &str) -> Output {
        self.command(command)
            .stdin(Stdio::null())
            .output()
            .expect("ssh starts")
    }

    /// Like `run`, but the command must succeed. Returns its stdout.
    pub fn run_ok(&self, command: &str) -> String {
        let out = self.run(command);
        assert!(
            out.status.success(),
            "`{command}` on {} failed ({}): {}",
            self.host,
            out.status,
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).into_owned()
    }

    /// Copies `nh` and the wrapper to `~/.neohar-dev/bin` (`scripts/remote-dev.sh`). It copies
    /// nothing when the remote file is the same.
    pub fn deploy(&self, nh: &Path) {
        let script = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../scripts/remote-dev.sh");
        let out = Command::new("bash")
            .arg(script)
            .arg("--bin")
            .arg(nh)
            .env("NEOHAR_REMOTE_TARGET", &self.host)
            .output()
            .expect("remote-dev.sh starts");
        assert!(
            out.status.success(),
            "remote-dev.sh failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    /// Every process of the test bed on the remote machine: `(pid, command)`. The test bed runs
    /// `~/.neohar-dev/bin/nh`, so a command starts with that path. The `ps` line of the remote
    /// shell itself does not.
    pub fn processes(&self) -> Vec<(u32, String)> {
        let listing = self.run_ok("ps -o pid=,command= -u $(id -u) -ww");
        listing
            .lines()
            .filter_map(|line| {
                let line = line.trim_start();
                let (pid, command) = line.split_once(' ')?;
                command
                    .contains("/.neohar-dev/bin/nh ")
                    .then(|| command.starts_with('/'))
                    .filter(|starts| *starts)?;
                Some((pid.parse().ok()?, command.trim().to_owned()))
            })
            .collect()
    }

    /// The pids of the holders (`nh hold`) of the test bed.
    pub fn holders(&self) -> Vec<u32> {
        self.processes()
            .into_iter()
            .filter(|(_, command)| command.contains("/nh hold"))
            .map(|(pid, _)| pid)
            .collect()
    }

    /// Waits until no process of the test bed runs, or fails with the list.
    pub fn assert_clean(&self, what: &str) {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let left = self.processes();
            if left.is_empty() {
                return;
            }
            assert!(
                Instant::now() < deadline,
                "{what}: processes of the test bed run on {}: {left:?}",
                self.host
            );
            std::thread::sleep(Duration::from_millis(200));
        }
    }
}

/// A daemon session of the test bed. Dropping it stops the daemon, and then kills each process of
/// the test bed that is left (a holder, for example), so a failed test leaves nothing behind.
pub struct RemoteSession {
    pub target: RemoteTarget,
    pub name: String,
}

impl RemoteSession {
    pub fn new(target: &RemoteTarget, name: &str) -> Self {
        Self {
            target: target.clone(),
            name: name.to_owned(),
        }
    }

    /// The remote command of the bridge for this session.
    pub fn bridge_command(&self) -> String {
        format!("{NH_DEV} bridge --session {} --proto link/1", self.name)
    }

    /// `nh-dev daemon stop --session <name>`. Returns the output; it fails when no daemon runs.
    pub fn stop(&self) -> Output {
        self.target
            .run(&format!("{NH_DEV} daemon stop --session {}", self.name))
    }
}

impl Drop for RemoteSession {
    fn drop(&mut self) {
        let _ = self.stop();
        for (pid, command) in self.target.processes() {
            if command.contains(&format!("--session {}", self.name)) || command.contains("/nh hold")
            {
                let _ = self.target.run(&format!("kill -9 {pid}"));
            }
        }
    }
}
