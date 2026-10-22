//! Port forwards of a link to a loopback address, with the fake ssh.
//! The fake answers `-O forward` and `-O cancel` and logs the call, so the test sees the
//! arguments that the real `ssh` would get.

mod core_support;
mod ssh_support;

use std::{net::TcpListener, path::PathBuf, sync::Arc};

use core_support::Harness;
use nh_core::ErrorCode;
use nh_remote::machines::{MachineManager, ManagerConfig};
use serde_json::{Value, json};
use ssh_support::{NH, Remote, eventually, host};
use ulid::Ulid;

fn manager(remote: &Remote, core: &Harness, extra: &[(&str, String)]) -> MachineManager {
    let mut env: Vec<(std::ffi::OsString, std::ffi::OsString)> = remote
        .env()
        .into_iter()
        .map(|(k, v)| (k.into(), v.into()))
        .collect();
    env.push(("FAKE_SSH_LOG".into(), log_of(remote).into()));
    env.extend(extra.iter().map(|(k, v)| ((*k).into(), v.into())));
    MachineManager::new(
        ManagerConfig {
            runtime_dir: remote.dir.join("local"),
            nh: PathBuf::from(NH),
            api_socket: core.api_socket(),
            session: "test".to_owned(),
            device: Ulid::from_parts(1, 99),
            device_name: "laptop".to_owned(),
            app_version: env!("CARGO_PKG_VERSION").to_owned(),
            host_for: Arc::new(|_| host() as Arc<dyn nh_core::Host>),
            options: nh_remote::ClientOptions::default(),
            ssh_program: Some(ssh_support::fake_ssh()),
            ssh_env: env,
        },
        tokio::runtime::Handle::current(),
    )
}

fn log_of(remote: &Remote) -> PathBuf {
    remote.dir.join("ssh.log")
}

/// The `-L` values of the calls with `-O <op>`.
fn forwards_logged(remote: &Remote, op: &str) -> Vec<String> {
    let text = std::fs::read_to_string(log_of(remote)).unwrap_or_default();
    let mut found = Vec::new();
    let mut call: Vec<&str> = Vec::new();
    for line in text.lines() {
        if line == "--" {
            if let Some(at) = call.iter().position(|arg| *arg == "-O")
                && call.get(at + 1) == Some(&op)
                && let Some(l) = call.iter().position(|arg| *arg == "-L")
            {
                found.push(call[l + 1].to_owned());
            }
            call.clear();
        } else if !line.starts_with("SSH_ASKPASS") && !line.starts_with("NEOHAR_") {
            call.push(line);
        }
    }
    found
}

async fn setup(extra: &[(&str, String)]) -> (Remote, Harness, MachineManager, String) {
    let remote = Remote::new();
    let core = Harness::start();
    let manager = manager(&remote, &core, extra);
    core.handle.set_machine_links(Arc::new(manager.clone()));
    let session = remote.session_for("a");
    core.call(
        "config.set",
        json!({
            "key": "machines.alpha",
            "value": {"ssh": "alpha", "session": session, "connect": "manual", "remote_nh": NH}
        }),
    )
    .await;
    (remote, core, manager, session)
}

async fn forwards_of_row(core: &Harness) -> Vec<Value> {
    let listed = core.call("machine.list", json!({})).await;
    listed["machines"]
        .as_array()
        .and_then(|rows| rows.iter().find(|r| r["label"] == "alpha"))
        .and_then(|row| row["forwards"].as_array().cloned())
        .unwrap_or_default()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_forward_is_added_through_the_master_listed_in_the_row_and_ends_with_the_link() {
    let (remote, core, manager, _session) = setup(&[]).await;

    // No link, no forward.
    let error = manager
        .add_forward("alpha", 8765)
        .await
        .expect_err("the link is down");
    assert_eq!(error.code, ErrorCode::LinkDown);

    core.call("machine.connect", json!({"machine": "alpha"}))
        .await;
    let added = manager.add_forward("alpha", 8765).await.expect("a forward");
    assert_eq!(added.remote_port, 8765);
    assert_eq!(
        forwards_logged(&remote, "forward"),
        [format!("{}:localhost:8765", added.local_port)]
    );
    let rows = forwards_of_row(&core).await;
    assert_eq!(
        rows,
        [json!({"remote_port": 8765, "local_port": added.local_port})]
    );

    // The same port again is the same forward, and ssh is not called again.
    let again = manager.add_forward("alpha", 8765).await.expect("a forward");
    assert_eq!(again, added);
    assert_eq!(forwards_logged(&remote, "forward").len(), 1);

    // A port that is busy on this Mac gets another local port.
    let held = TcpListener::bind("127.0.0.1:0").expect("a port");
    let busy = held.local_addr().expect("an address").port();
    let other = manager.add_forward("alpha", busy).await.expect("a forward");
    assert_eq!(other.remote_port, busy);
    assert_ne!(other.local_port, busy);
    assert_eq!(forwards_of_row(&core).await.len(), 2);

    // The link ends, and so do the forwards: cancelled in the master, gone from the row.
    core.call("machine.disconnect", json!({"machine": "alpha"}))
        .await;
    eventually("both forwards to be cancelled", || {
        forwards_logged(&remote, "cancel").len() == 2
    })
    .await;
    let mut cancelled = forwards_logged(&remote, "cancel");
    cancelled.sort();
    let mut wanted = vec![
        format!("{}:localhost:8765", added.local_port),
        format!("{}:localhost:{busy}", other.local_port),
    ];
    wanted.sort();
    assert_eq!(cancelled, wanted);
    assert!(forwards_of_row(&core).await.is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_forward_that_ssh_refuses_is_an_error_and_not_in_the_row() {
    let (remote, core, manager, _session) = setup(&[(
        "FAKE_SSH_FORWARD_FAIL",
        "mux_client_forward: forwarding request failed".to_owned(),
    )])
    .await;
    core.call("machine.connect", json!({"machine": "alpha"}))
        .await;
    let error = manager
        .add_forward("alpha", 8765)
        .await
        .expect_err("ssh refuses");
    assert_eq!(error.code, ErrorCode::ForwardFailed);
    assert!(
        error.message.contains("forwarding request failed"),
        "{error}"
    );
    assert!(forwards_of_row(&core).await.is_empty());
    assert!(forwards_logged(&remote, "forward").len() == 1);
}
