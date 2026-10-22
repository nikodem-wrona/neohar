//! `nh daemon` as a product role and `nh bridge`.
//!
//! Each test has its own directory under `/tmp` with the config, data and runtime roots, a home,
//! and fake `claude` and `codex` programs, so no daemon touches the real session or the real
//! harnesses. The guard of a test stops each daemon that it started and kills each holder that is
//! left in the directory of the test.

use std::{
    io::{BufRead, BufReader, Read, Write},
    os::unix::{fs::DirBuilderExt, net::UnixListener},
    path::{Path, PathBuf},
    process::{Child, Command, Output, Stdio},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};

use nh_proto::paths::{Env, Platform};
use nh_store::{PaneLiveness, Store};
use rustix::process::{Pid, Signal, kill_process, test_kill_process};
use serde_json::{Value, json};

#[path = "core_support/reap.rs"]
mod reap;

const NH: &str = env!("CARGO_BIN_EXE_nh");
const TIMEOUT: Duration = Duration::from_secs(30);

static COUNTER: AtomicU64 = AtomicU64::new(1);

/// The directories of one test, and the daemons that it started.
struct Machine {
    dir: PathBuf,
    paths: Env,
    session: String,
    daemons: Vec<i32>,
}

impl Machine {
    fn new(tag: &str) -> Self {
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        // Short: the runtime directory holds Unix sockets.
        let dir = PathBuf::from(format!("/tmp/nhd-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let home = dir.join("home");
        let bin = dir.join("bin");
        std::fs::create_dir_all(&home).expect("a home");
        std::fs::create_dir_all(&bin).expect("a bin");
        for (name, script) in [("claude", FAKE_CLAUDE), ("codex", FAKE_CODEX)] {
            let path = bin.join(name);
            std::fs::write(&path, script).expect("a program");
            std::fs::set_permissions(&path, std::os::unix::fs::PermissionsExt::from_mode(0o755))
                .expect("it runs");
        }
        // The login shell of the daemon reads this file last, so the fakes come first
        // on the PATH of its panes and of the harness probes.
        std::fs::write(
            home.join(".profile"),
            format!(
                "export PATH='{}:/usr/bin:/bin:/usr/sbin:/sbin'\n",
                bin.display()
            ),
        )
        .expect("a profile");
        let paths = Env {
            platform: Platform::current(),
            uid: rustix::process::getuid().as_raw(),
            home: Some(home),
            config_override: Some(dir.join("config")),
            data_override: Some(dir.join("data")),
            runtime_override: Some(dir.join("run")),
            xdg_config_home: None,
            xdg_data_home: None,
            xdg_runtime_dir: None,
        };
        Self {
            session: format!("{tag}-{n}"),
            dir,
            paths,
            daemons: Vec::new(),
        }
    }

    /// `nh <args>` in the environment of this machine.
    fn nh(&self, args: &[&str]) -> Command {
        let mut command = Command::new(NH);
        command
            .args(args)
            .env_clear()
            .env("HOME", self.dir.join("home"))
            .env(
                "PATH",
                format!("{}:/usr/bin:/bin", self.dir.join("bin").display()),
            )
            .env("SHELL", "/bin/sh")
            .env("LANG", "en_US.UTF-8")
            .env("NEOHAR_CONFIG_DIR", self.dir.join("config"))
            .env("NEOHAR_DATA_DIR", self.dir.join("data"))
            .env("NEOHAR_RUNTIME_DIR", self.dir.join("run"))
            .env("NEOHAR_SESSION", &self.session)
            .stdin(Stdio::null());
        command
    }

    fn run(&self, args: &[&str]) -> Output {
        self.nh(args).output().expect("nh runs")
    }

    /// `nh daemon --detach`: its first process exits after the line. Returns the pid of `READY`.
    fn start(&mut self) -> i32 {
        let out = self.run(&["daemon", "--detach", "--session", &self.session.clone()]);
        let line = String::from_utf8_lossy(&out.stdout).to_string();
        assert!(
            out.status.success(),
            "the daemon did not start: {line} {out:?}"
        );
        let pid: i32 = line
            .trim()
            .strip_prefix("READY ")
            .unwrap_or_else(|| panic!("no READY in {line:?}"))
            .parse()
            .expect("a pid");
        self.daemons.push(pid);
        pid
    }

    /// `nh daemon stop`, which must end the daemon.
    fn stop(&mut self, pid: i32) {
        let out = self.run(&["daemon", "stop", "--session", &self.session.clone()]);
        assert!(out.status.success(), "stop failed: {out:?}");
        let said: Value = serde_json::from_slice(&out.stdout).expect("a JSON line");
        assert_eq!(said["stopped"], json!(pid), "{said}");
        assert!(!alive(pid), "the daemon {pid} still runs after the stop");
        self.daemons.retain(|d| *d != pid);
    }

    /// Calls a method through the CLI and returns its result.
    fn call(&self, method: &str, params: Value) -> Value {
        let out = self.run(&["api", "call", method, &params.to_string()]);
        assert!(out.status.success(), "{method} failed: {out:?}");
        serde_json::from_slice(&out.stdout).expect("JSON")
    }

    fn data_dir(&self) -> PathBuf {
        self.paths
            .data_dir(&self.session)
            .expect("a data directory")
    }

    fn link_socket(&self) -> PathBuf {
        self.paths.link_socket(&self.session).expect("a path")
    }

    fn api_socket(&self) -> PathBuf {
        self.paths.api_socket(&self.session).expect("a path")
    }

    /// The pids of the holders that the database knows, by liveness.
    fn holders(&self, liveness: PaneLiveness) -> Vec<i32> {
        let Ok(db) = Store::open(&self.data_dir().join("state.db")) else {
            return Vec::new();
        };
        db.panes(liveness)
            .unwrap_or_default()
            .into_iter()
            .map(|pane| pane.holder_pid)
            .filter(|pid| *pid > 0)
            .collect()
    }
}

impl Drop for Machine {
    fn drop(&mut self) {
        for pid in self.daemons.drain(..) {
            if let Some(pid) = Pid::from_raw(pid) {
                let _ = kill_process(pid, Signal::KILL);
            }
        }
        // The daemons are gone, so no holder starts after this point. The search also finds a
        // holder that a daemon started and did not store before its end.
        reap::kill_under(&self.dir);
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

const FAKE_CLAUDE: &str = r#"#!/bin/sh
case "$1 $2" in
  "--version "*) echo '2.1.292 (Claude Code)' ;;
  "auth status") echo '{"loggedIn": true, "authMethod": "claude.ai"}' ;;
  *) exit 2 ;;
esac
"#;

const FAKE_CODEX: &str = r#"#!/bin/sh
case "$1 $2" in
  "--version "*) echo 'codex-cli 0.162.1' ;;
  "login status") echo 'Logged in using ChatGPT' >&2 ;;
  *) exit 2 ;;
esac
"#;

fn alive(pid: i32) -> bool {
    Pid::from_raw(pid).is_some_and(|pid| test_kill_process(pid).is_ok())
}

fn wait_until(what: &str, mut done: impl FnMut() -> bool) {
    let deadline = Instant::now() + TIMEOUT;
    while !done() {
        assert!(Instant::now() < deadline, "timed out: {what}");
        std::thread::sleep(Duration::from_millis(25));
    }
}

/// The parent of a process, from `ps`.
fn parent_of(pid: i32) -> Option<i32> {
    let out = Command::new("ps")
        .args(["-o", "ppid=", "-p", &pid.to_string()])
        .output()
        .ok()?;
    String::from_utf8_lossy(&out.stdout).trim().parse().ok()
}

/// How many holders of this user run now (`nh hold` on the command line).
fn holder_count() -> usize {
    let uid = rustix::process::getuid().as_raw().to_string();
    let out = Command::new("ps")
        .args(["-o", "command=", "-u", &uid, "-ww"])
        .output()
        .expect("ps runs");
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter(|line| line.contains("nh hold"))
        .count()
}

/// The daemons of a session that run now, by the command line.
fn daemons_of(session: &str) -> Vec<i32> {
    let uid = rustix::process::getuid().as_raw().to_string();
    let out = Command::new("ps")
        .args(["-o", "pid=,command=", "-u", &uid, "-ww"])
        .output()
        .expect("ps runs");
    let needle = format!("daemon --detach --session {session}");
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter(|line| line.contains(&needle) && line.contains(NH))
        .filter_map(|line| line.split_whitespace().next()?.parse().ok())
        .collect()
}

#[test]
fn a_detached_daemon_says_ready_and_stop_ends_it_while_its_panes_keep_running() {
    let before = holder_count();
    let mut machine = Machine::new("det");
    let pid = machine.start();
    // Detached: the caller has gone, the daemon is not its child and serves both sockets.
    assert!(alive(pid), "the daemon runs after READY");
    assert_eq!(parent_of(pid), Some(1), "the daemon left its caller");
    assert!(machine.api_socket().exists() && machine.link_socket().exists());
    let info = machine.call("session.info", json!({}));
    assert_eq!(info["pid"], json!(pid));
    // The log of the daemon is the log of a core.
    let logs = machine.data_dir().join("logs");
    wait_until("core.log has the start of the daemon", || {
        std::fs::read_dir(&logs).is_ok_and(|entries| {
            entries.flatten().any(|e| {
                e.file_name().to_string_lossy().starts_with("core")
                    && std::fs::read_to_string(e.path())
                        .is_ok_and(|text| text.contains("the daemon serves api.sock"))
            })
        })
    });

    // A pane with a program.
    machine.call("workspace.create", json!({"name": "w"}));
    wait_until("the pane has a holder", || {
        !machine.holders(PaneLiveness::Running).is_empty()
    });
    let holders = machine.holders(PaneLiveness::Running);

    // The stop ends the daemon, and the panes keep running ("Quit").
    machine.stop(pid);
    assert!(!machine.api_socket().exists(), "api.sock is gone");
    assert!(!machine.link_socket().exists(), "link.sock is gone");
    assert!(
        holders.iter().all(|holder| alive(*holder)),
        "the holders keep running"
    );

    // A new daemon takes the panes back, and `session stop-everything` ends their programs.
    let again = machine.start();
    assert_ne!(again, pid);
    let out = machine.run(&["session", "stop-everything", "--json"]);
    assert!(out.status.success(), "{out:?}");
    let report: Value = serde_json::from_slice(&out.stdout).expect("JSON");
    assert_eq!(report["left"], json!(0), "{report}");
    assert_eq!(
        report["ended"].as_u64().unwrap_or(0) + report["killed"].as_u64().unwrap_or(0),
        holders.len() as u64,
        "{report}"
    );
    wait_until("the holders leave", || {
        holders.iter().all(|holder| !alive(*holder))
    });
    machine.stop(again);
    drop(machine);
    wait_until("no holder of the test is left", || holder_count() <= before);
}

/// The `caffeinate` children that follow `pid`.
fn caffeinates_of(pid: i32) -> usize {
    let uid = rustix::process::getuid().as_raw().to_string();
    let out = Command::new("ps")
        .args(["-o", "command=", "-u", &uid, "-ww"])
        .output()
        .expect("ps runs");
    let needle = format!("caffeinate -i -w {pid}");
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter(|line| line.contains(&needle))
        .count()
}

#[cfg(target_os = "macos")]
#[test]
fn a_daemon_keeps_the_machine_awake_when_the_setting_says_always() {
    let mut machine = Machine::new("awake");
    let config = machine.dir.join("config");
    std::fs::create_dir_all(&config).expect("a config directory");
    std::fs::write(
        config.join("config.toml"),
        "[daemon]\nprevent_sleep = \"always\"\n",
    )
    .expect("config.toml");
    let pid = machine.start();
    wait_until("caffeinate runs for the daemon", || {
        caffeinates_of(pid) == 1
    });
    machine.stop(pid);
    wait_until("caffeinate ends with the daemon", || {
        caffeinates_of(pid) == 0
    });
}

#[test]
fn a_second_daemon_of_a_session_ends_with_api_socket_in_use() {
    let mut machine = Machine::new("two");
    let first = machine.start();
    let out = machine.run(&["daemon", "--detach", "--session", &machine.session.clone()]);
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    let line = String::from_utf8_lossy(&out.stdout);
    assert!(
        line.starts_with("ERROR ") && line.contains("another core serves"),
        "{line}"
    );
    assert_eq!(
        daemons_of(&machine.session),
        vec![first],
        "only the first one runs"
    );
    machine.stop(first);
}

#[test]
fn stop_without_a_daemon_says_so() {
    let machine = Machine::new("none");
    let out = machine.run(&["daemon", "stop"]);
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    assert!(String::from_utf8_lossy(&out.stderr).contains("no core serves"));
}

#[test]
fn two_bridges_that_start_at_once_share_one_daemon() {
    let mut machine = Machine::new("race");
    let session = machine.session.clone();
    let spawn = |machine: &Machine| -> Child {
        machine
            .nh(&["bridge", "--session", &session, "--proto", "link/1"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("a bridge")
    };
    let mut bridges = [spawn(&machine), spawn(&machine)];
    wait_until("a daemon serves the session", || {
        daemons_of(&session).len() == 1 && machine.link_socket().exists()
    });
    let winner = daemons_of(&session)[0];
    machine.daemons.push(winner);
    // The bridges connect to the daemon that won, and they keep the link while their input is
    // open. A bridge that found no core would have ended.
    std::thread::sleep(Duration::from_millis(500));
    for bridge in &mut bridges {
        assert!(
            matches!(bridge.try_wait(), Ok(None)),
            "a bridge ended: {:?}",
            bridge.try_wait()
        );
    }
    assert_eq!(daemons_of(&session), vec![winner], "one daemon, not two");
    // The end of the input ends each bridge, and the daemon keeps running.
    let mut said = String::new();
    for mut bridge in bridges {
        drop(bridge.stdin.take());
        let out = bridge.wait_with_output().expect("the bridge ends");
        assert!(out.status.success(), "{out:?}");
        said.push_str(&String::from_utf8_lossy(&out.stderr));
    }
    // A daemon that lost the race ended with `ApiSocketInUse`, and its bridge said so.
    assert!(
        said.matches("did not start").count() <= 1,
        "at most one daemon lost: {said}"
    );
    if said.contains("did not start") {
        assert!(said.contains("another core serves"), "{said}");
    }
    assert!(alive(winner), "a client that leaves never stops the daemon");
    machine.stop(winner);
}

#[test]
fn a_bridge_that_does_not_know_the_protocol_tells_its_version() {
    let machine = Machine::new("ver");
    for args in [
        &["bridge", "--session", "x", "--proto", "link/9"][..],
        &["bridge", "--session", "x"][..],
    ] {
        let out = machine.run(args);
        assert_eq!(out.status.code(), Some(3), "{out:?}");
        assert_eq!(
            String::from_utf8_lossy(&out.stdout),
            format!("NH-BRIDGE {} link/1\n", env!("CARGO_PKG_VERSION"))
        );
    }
    assert!(
        !machine.api_socket().exists() && !machine.link_socket().exists(),
        "a bridge with an unknown protocol starts nothing"
    );
}

#[test]
fn a_bridge_copies_bytes_both_ways_until_a_side_closes() {
    let machine = Machine::new("copy");
    let socket = machine.link_socket();
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(socket.parent().expect("a directory"))
        .expect("the runtime directory");
    let listener = UnixListener::bind(&socket).expect("a test listener");
    let bytes: Vec<u8> = (0..=255u8).cycle().take(200_000).collect();
    let expected = bytes.clone();
    // The test listener reads what the client sends until the client closes, then answers with
    // the same bytes reversed and closes.
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("the bridge connects");
        let mut got = Vec::new();
        stream
            .read_to_end(&mut got)
            .expect("the bytes of the client");
        assert_eq!(got, expected, "the bridge changed the bytes");
        let answer: Vec<u8> = got.iter().rev().copied().collect();
        stream.write_all(&answer).expect("the answer");
    });
    let mut bridge = machine
        .nh(&["bridge", "--proto", "link/1"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("a bridge");
    let mut stdin = bridge.stdin.take().expect("stdin");
    let writer = std::thread::spawn(move || {
        stdin.write_all(&bytes).expect("the bytes go in");
        // The client closes its side.
    });
    writer.join().expect("the writer");
    let out = bridge.wait_with_output().expect("the bridge ends");
    server.join().expect("the server");
    assert!(out.status.success(), "{out:?}");
    // The end of the input closed the write side only, so the whole answer came back.
    let answer: Vec<u8> = (0..=255u8)
        .cycle()
        .take(200_000)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    assert!(out.stdout == answer, "the answer changed on the way");
}

#[test]
fn a_bridge_ends_when_the_core_closes_the_link() {
    let machine = Machine::new("down");
    let socket = machine.link_socket();
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(socket.parent().expect("a directory"))
        .expect("the runtime directory");
    let listener = UnixListener::bind(&socket).expect("a test listener");
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("the bridge connects");
        let mut line = String::new();
        BufReader::new(stream.try_clone().expect("a clone"))
            .read_line(&mut line)
            .expect("a line");
        stream
            .write_all(format!("echo {line}").as_bytes())
            .expect("the answer");
        // The core closes the link.
    });
    let mut bridge = machine
        .nh(&["bridge", "--proto", "link/1"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("a bridge");
    let mut stdin = bridge.stdin.take().expect("stdin");
    stdin.write_all(b"hello\n").expect("a line");
    server.join().expect("the server");
    // The input stays open: the end of the socket alone ends the bridge.
    let out = bridge.wait_with_output().expect("the bridge ends");
    drop(stdin);
    assert!(out.status.success(), "{out:?}");
    assert_eq!(String::from_utf8_lossy(&out.stdout), "echo hello\n");
    assert!(Path::new(&socket).exists());
}
