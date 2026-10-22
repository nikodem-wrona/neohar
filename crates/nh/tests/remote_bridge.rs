//! A link without the app, over real ssh to a second machine:
//! `nh-dev bridge` starts a daemon there, a `LinkClient` on the stdio of `ssh` gets `Welcome`,
//! `session.info` works, and `nh-dev daemon stop` ends the daemon. A second test shows that the
//! login environment of the daemon finds `claude` and `codex`.
//!
//! Run with `NEOHAR_REMOTE=1 NEOHAR_REMOTE_TARGET=<ssh target> mise run test:remote`; else the
//! tests skip. They put everything under `~/.neohar-dev/` on the remote machine.

mod remote_support;

use std::{
    path::PathBuf,
    process::Stdio,
    sync::Arc,
    time::{Duration, Instant},
};

use nh_core::{MessageSink, RecordingHost};
use nh_remote::{ClientHello, ClientOptions, LinkClient, ssh::SshSpec};
use remote_support::{RemoteSession, RemoteTarget};
use serde_json::{Value, json};
use tokio::{process::Child, sync::mpsc, time::timeout};
use ulid::Ulid;

const NH: &str = env!("CARGO_BIN_EXE_nh");
const LONG: Duration = Duration::from_secs(60);

struct Messages(mpsc::UnboundedSender<Value>);

impl MessageSink for Messages {
    fn send(&self, message: Value) -> bool {
        self.0.send(message).is_ok()
    }
}

/// Starts `ssh -T <target> <bridge command>` and opens a link on its stdio.
async fn link_over_ssh(target: &RemoteTarget, session: &RemoteSession) -> (LinkClient, Child) {
    let mut ssh: tokio::process::Command = target.command(&session.bridge_command()).into();
    let mut child = ssh
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .kill_on_drop(true)
        .spawn()
        .expect("ssh starts");
    let read = child.stdout.take().expect("stdout");
    let write = child.stdin.take().expect("stdin");
    let host = Arc::new(RecordingHost::new(PathBuf::from("/bin/nh"), true));
    let client = timeout(
        LONG,
        LinkClient::connect(
            read,
            write,
            ClientHello {
                device: Ulid::from_parts(1, 0x7e57),
                name: "remote-test".to_owned(),
                app_version: "0.0.0".to_owned(),
            },
            host,
            ClientOptions::default(),
        ),
    )
    .await
    .expect("Welcome came in time")
    .expect("the link opens over ssh");
    (client, child)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_bridge_over_ssh_starts_a_daemon_and_a_link_works() {
    let Some(target) = RemoteTarget::from_env() else {
        return;
    };
    target.deploy(std::path::Path::new(NH));
    target.assert_clean("before the test");
    let session = RemoteSession::new(&target, "t1");

    let started = Instant::now();
    let (client, mut ssh) = link_over_ssh(&target, &session).await;
    eprintln!(
        "bridge start to Welcome, daemon not running: {:?}",
        started.elapsed()
    );
    let welcome = client.welcome();
    eprintln!("Welcome: {welcome:?}");

    let info = client
        .call(None, "session.info", json!({}))
        .await
        .expect("session.info works");
    assert_eq!(info["name"], "t1", "{info}");
    let pid = info["pid"].as_u64().expect("a pid");
    let daemons: Vec<_> = target
        .processes()
        .into_iter()
        .filter(|(p, c)| u64::from(*p) == pid && c.contains(" daemon "))
        .collect();
    assert_eq!(daemons.len(), 1, "the daemon runs as a test bed process");

    // A second link on the control of the same daemon: the bridge finds it, it starts no second.
    drop(client);
    let _ = timeout(LONG, ssh.wait()).await;
    let started = Instant::now();
    let (again, mut ssh2) = link_over_ssh(&target, &session).await;
    eprintln!(
        "bridge start to Welcome, daemon running: {:?}",
        started.elapsed()
    );
    let info2 = again
        .call(None, "session.info", json!({}))
        .await
        .expect("session.info works");
    assert_eq!(info2["pid"].as_u64(), Some(pid), "the same daemon");
    drop(again);
    let _ = timeout(LONG, ssh2.wait()).await;

    let stopped = session.stop();
    assert!(
        stopped.status.success(),
        "daemon stop: {}",
        String::from_utf8_lossy(&stopped.stderr)
    );
    target.assert_clean("after daemon stop");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_login_environment_of_the_daemon_finds_claude_and_codex() {
    let Some(target) = RemoteTarget::from_env() else {
        return;
    };
    target.deploy(std::path::Path::new(NH));
    target.assert_clean("before the test");
    let session = RemoteSession::new(&target, "t2");
    let (client, _ssh) = link_over_ssh(&target, &session).await;

    client
        .call(None, "harness.refresh", json!({}))
        .await
        .expect("harness.refresh works");
    // The probes of the login state run after the refresh answers: wait until none is unknown.
    let deadline = Instant::now() + LONG;
    let rows = loop {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let id = client
            .subscribe("harnesses", None, Box::new(Messages(tx)))
            .await
            .expect("harnesses");
        let snapshot = timeout(Duration::from_secs(10), rx.recv())
            .await
            .expect("a snapshot")
            .expect("open");
        client.unsubscribe(&id);
        let items = snapshot["data"]["items"].clone();
        let settled = ["claude", "codex"]
            .iter()
            .all(|h| items[*h]["login"].as_str().is_some_and(|l| l != "unknown"));
        if settled || Instant::now() > deadline {
            break items;
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    };
    eprintln!("harnesses on {}: {rows:#}", target.host);
    for id in ["claude", "codex"] {
        assert_eq!(
            rows[id]["installed"], true,
            "{id} is on the PATH of the login environment: {}",
            rows[id]
        );
    }
    drop(client);
    let stopped = session.stop();
    assert!(stopped.status.success());
    target.assert_clean("after daemon stop");
}

/// `connect_ssh` to the remote machine with the real `ssh`: the options of a link, a control
/// socket in a runtime directory of this Mac, and no question (the key works).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn connect_ssh_opens_a_link_to_the_second_mac() {
    let Some(target) = RemoteTarget::from_env() else {
        return;
    };
    target.deploy(std::path::Path::new(NH));
    target.assert_clean("before the test");
    let session = RemoteSession::new(&target, "t3");
    let runtime = PathBuf::from(format!("/tmp/nhr-{}", std::process::id()));
    let mut spec = SshSpec::new(&target.host, "t3", remote_support::NH_DEV, &runtime);
    // A user's ssh config may name another ssh; the test uses the system one.
    spec.program = PathBuf::from(nh_remote::ssh::SSH);
    let client = nh_remote::ssh::connect_ssh(
        &spec,
        ClientHello {
            device: Ulid::from_parts(1, 0x7e58),
            name: "remote-test".to_owned(),
            app_version: env!("CARGO_PKG_VERSION").to_owned(),
        },
        Arc::new(RecordingHost::new(PathBuf::from("/bin/nh"), true)),
        ClientOptions::default(),
    )
    .await
    .expect("the link opens over the system ssh");
    let info = client
        .call(None, "session.info", json!({}))
        .await
        .expect("session.info works");
    assert_eq!(info["name"], "t3", "{info}");
    eprintln!(
        "Welcome: {:?}; version check against the app: {:?}",
        client.welcome(),
        nh_remote::ssh::version_check(client.welcome(), env!("CARGO_PKG_VERSION"))
    );
    drop(client);
    let stopped = session.stop();
    assert!(stopped.status.success());
    // ControlPersist keeps the master of ssh for 10 minutes: end it, so the test leaves nothing.
    let _ = std::process::Command::new("/usr/bin/ssh")
        .args(["-O", "exit", "-o"])
        .arg(format!("ControlPath={}/ssh/%C", runtime.display()))
        .arg(&target.host)
        .output();
    target.assert_clean("after daemon stop");
    let _ = std::fs::remove_dir_all(&runtime);
}
