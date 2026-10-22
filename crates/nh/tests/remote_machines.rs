//! A second machine as a machine of `config.toml`: the manager connects it over
//! the real ssh, and its row shows Claude Code and Codex as installed. Run with
//! `NEOHAR_REMOTE=1 NEOHAR_REMOTE_TARGET=<ssh target> mise run test:remote`; else the test skips.

mod core_support;
mod remote_support;

use std::{
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};

use core_support::Harness;
use nh_core::RecordingHost;
use nh_remote::machines::{MachineManager, ManagerConfig};
use remote_support::{NH_DEV, RemoteSession, RemoteTarget};
use serde_json::{Value, json};
use ulid::Ulid;

const NH: &str = env!("CARGO_BIN_EXE_nh");
const LONG: Duration = Duration::from_secs(90);

async fn row(core: &Harness, label: &str, ready: impl Fn(&Value) -> bool) -> Value {
    let deadline = Instant::now() + LONG;
    loop {
        let listed = core.call("machine.list", json!({})).await;
        if let Some(found) = listed["machines"]
            .as_array()
            .and_then(|rows| rows.iter().find(|r| r["label"] == label))
            .filter(|found| ready(found))
        {
            return found.clone();
        }
        assert!(
            Instant::now() < deadline,
            "waited for the row of {label}: {listed}"
        );
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_second_mac_connects_and_its_row_shows_the_harnesses() {
    let Some(target) = RemoteTarget::from_env() else {
        return;
    };
    target.deploy(std::path::Path::new(NH));
    target.assert_clean("before the test");
    let session = RemoteSession::new(&target, "t4");
    let runtime = PathBuf::from(format!("/tmp/nhm-{}", std::process::id()));

    let core = Harness::start();
    let manager = MachineManager::new(
        ManagerConfig {
            runtime_dir: runtime.clone(),
            nh: PathBuf::from(NH),
            api_socket: core.api_socket(),
            session: "test".to_owned(),
            device: Ulid::from_parts(1, 0x7e59),
            device_name: "remote-test".to_owned(),
            app_version: env!("CARGO_PKG_VERSION").to_owned(),
            host_for: Arc::new(|_| {
                Arc::new(RecordingHost::new(PathBuf::from("/bin/nh"), true))
                    as Arc<dyn nh_core::Host>
            }),
            options: nh_remote::ClientOptions::default(),
            ssh_program: None,
            ssh_env: Vec::new(),
        },
        tokio::runtime::Handle::current(),
    );
    core.handle.set_machine_links(Arc::new(manager.clone()));
    core.call(
        "config.set",
        json!({"key": "machines.studio", "value": {
            "ssh": target.host, "session": "t4", "remote_nh": NH_DEV}}),
    )
    .await;
    core.call("machine.connect", json!({"machine": "studio"}))
        .await;
    let studio = row(&core, "studio", |r| {
        r["harnesses"].as_array().is_some_and(|h| h.len() == 2)
    })
    .await;
    eprintln!("the row of the remote machine: {studio:#}");
    assert_eq!(studio["state"], "connected");
    for harness in studio["harnesses"].as_array().expect("harnesses") {
        assert_eq!(harness["installed"], true, "{harness}");
    }
    core.call("machine.disconnect", json!({"machine": "studio"}))
        .await;
    row(&core, "studio", |r| r["state"] == "disconnected").await;
    manager.shutdown();
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

/// What a plain HTTP request to `127.0.0.1:<port>` on this Mac answers: the first line, or `None`
/// when nothing listens.
fn http_status_line(port: u16) -> Option<String> {
    use std::io::{Read, Write};

    let mut stream = std::net::TcpStream::connect_timeout(
        &std::net::SocketAddr::from(([127, 0, 0, 1], port)),
        Duration::from_secs(3),
    )
    .ok()?;
    stream.set_read_timeout(Some(Duration::from_secs(5))).ok()?;
    stream
        .write_all(b"GET / HTTP/1.0\r\nHost: localhost\r\n\r\n")
        .ok()?;
    let mut text = String::new();
    let mut buf = [0_u8; 512];
    while let Ok(n) = stream.read(&mut buf) {
        if n == 0 {
            break;
        }
        text.push_str(&String::from_utf8_lossy(&buf[..n]));
        if text.contains('\n') {
            break;
        }
    }
    text.lines().next().map(str::to_owned)
}

/// A server on a port of the remote machine is reached through a forward, and the
/// forward ends with the link.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_forward_reaches_a_server_on_the_second_mac_and_ends_with_the_link() {
    let Some(target) = RemoteTarget::from_env() else {
        return;
    };
    target.deploy(std::path::Path::new(NH));
    target.assert_clean("before the test");
    let session = RemoteSession::new(&target, "t5");
    let runtime = PathBuf::from(format!("/tmp/nhf-{}", std::process::id()));
    let pid_file = format!("/tmp/nh-forward-test-{}.pid", std::process::id());
    const PORT: u16 = 8765;

    let core = Harness::start();
    let manager = MachineManager::new(
        ManagerConfig {
            runtime_dir: runtime.clone(),
            nh: PathBuf::from(NH),
            api_socket: core.api_socket(),
            session: "test".to_owned(),
            device: Ulid::from_parts(1, 0x7e5a),
            device_name: "remote-test".to_owned(),
            app_version: env!("CARGO_PKG_VERSION").to_owned(),
            host_for: Arc::new(|_| {
                Arc::new(RecordingHost::new(PathBuf::from("/bin/nh"), true))
                    as Arc<dyn nh_core::Host>
            }),
            options: nh_remote::ClientOptions::default(),
            ssh_program: None,
            ssh_env: Vec::new(),
        },
        tokio::runtime::Handle::current(),
    );
    core.handle.set_machine_links(Arc::new(manager.clone()));
    core.call(
        "config.set",
        json!({"key": "machines.studio", "value": {
            "ssh": target.host, "session": "t5", "remote_nh": NH_DEV}}),
    )
    .await;

    // A server on the remote machine, on its loopback address only.
    target.run_ok(&format!(
        "nohup python3 -m http.server {PORT} --bind 127.0.0.1 >/dev/null 2>&1 & echo $! > {pid_file}"
    ));
    tokio::time::sleep(Duration::from_secs(1)).await;

    core.call("machine.connect", json!({"machine": "studio"}))
        .await;
    let forward = manager
        .add_forward("studio", PORT)
        .await
        .expect("a forward");
    let status = http_status_line(forward.local_port);
    let listed = row(&core, "studio", |r| {
        r["forwards"].as_array().is_some_and(|f| f.len() == 1)
    })
    .await;
    eprintln!("the forward: {forward:?}, the answer: {status:?}, the row: {listed}");

    core.call("machine.disconnect", json!({"machine": "studio"}))
        .await;
    row(&core, "studio", |r| r["state"] == "disconnected").await;
    tokio::time::sleep(Duration::from_secs(1)).await;
    let after = http_status_line(forward.local_port);

    // Clean up first, so a failed assertion leaves nothing on the remote machine.
    let _ = target.run(&format!("kill $(cat {pid_file}); rm -f {pid_file}"));
    manager.shutdown();
    let stopped = session.stop();
    let _ = std::process::Command::new("/usr/bin/ssh")
        .args(["-O", "exit", "-o"])
        .arg(format!("ControlPath={}/ssh/%C", runtime.display()))
        .arg(&target.host)
        .output();
    let _ = std::fs::remove_dir_all(&runtime);
    assert!(stopped.status.success());
    assert!(
        status.as_deref().is_some_and(|line| line.contains("200")),
        "the forward answered {status:?}"
    );
    assert_eq!(after, None, "the forward ended with the link");
    target.assert_clean("after daemon stop");
}
