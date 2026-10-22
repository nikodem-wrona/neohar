//! `[machines]`, the machine manager, the projection `machines` and `machine.*`, with the
//! fake ssh. A local core hosts the manager, as the app does in local mode. Each "remote" machine
//! is a daemon that `nh bridge` starts with directories of its own.

mod core_support;
mod ssh_support;

use std::{path::PathBuf, sync::Arc, time::Duration};

use core_support::{Harness, Messages};
use nh_core::ErrorCode;
use nh_remote::{
    Link,
    machines::{MachineManager, ManagerConfig},
};
use serde_json::{Value, json};
use ssh_support::{NH, Remote, WAIT, eventually, host};
use ulid::Ulid;

fn manager(remote: &Remote, core: &Harness) -> MachineManager {
    manager_with_hosts(remote, core, |_| host() as Arc<dyn nh_core::Host>)
}

fn manager_with_hosts(
    remote: &Remote,
    core: &Harness,
    host_for: impl Fn(&str) -> Arc<dyn nh_core::Host> + Send + Sync + 'static,
) -> MachineManager {
    MachineManager::new(
        ManagerConfig {
            runtime_dir: remote.dir.join("local"),
            nh: PathBuf::from(NH),
            api_socket: core.api_socket(),
            session: "test".to_owned(),
            device: Ulid::from_parts(1, 99),
            device_name: "laptop".to_owned(),
            app_version: env!("CARGO_PKG_VERSION").to_owned(),
            host_for: Arc::new(host_for),
            options: nh_remote::ClientOptions::default(),
            ssh_program: Some(ssh_support::fake_ssh()),
            ssh_env: remote
                .env()
                .into_iter()
                .map(|(k, v)| (k.into(), v.into()))
                .collect(),
        },
        tokio::runtime::Handle::current(),
    )
}

/// The rows of the projection `machines` as a subscriber sees them, in the order of the
/// projection.
async fn projection(core: &Harness) -> Vec<Value> {
    let mut messages = Messages::subscribe(&core.handle, "machines");
    let snapshot = messages
        .wait_for("the snapshot", |m| m["type"] == "snapshot")
        .await;
    let items = &snapshot["data"]["items"];
    snapshot["data"]["order"]
        .as_array()
        .expect("an order")
        .iter()
        .map(|label| items[label.as_str().expect("a label")].clone())
        .collect()
}

async fn row(core: &Harness, label: &str, ready: impl Fn(&Value) -> bool) -> Value {
    let deadline = std::time::Instant::now() + WAIT;
    loop {
        let rows = projection(core).await;
        if let Some(found) = rows.iter().find(|r| r["label"] == label)
            && ready(found)
        {
            return found.clone();
        }
        assert!(
            std::time::Instant::now() < deadline,
            "waited for the row of {label}: {rows:?}"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

async fn add(core: &Harness, label: &str, session: &str, connect: &str) {
    core.call(
        "config.set",
        json!({
            "key": format!("machines.{label}"),
            "value": {"ssh": label, "session": session, "connect": connect,
                      "remote_nh": NH}
        }),
    )
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_core_without_links_has_the_row_local_only_and_refuses_to_connect() {
    let core = Harness::start();
    let rows = projection(&core).await;
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert_eq!(rows[0]["label"], "local");
    assert_eq!(rows[0]["state"], "local");
    assert_eq!(rows[0]["version"], env!("CARGO_PKG_VERSION"));
    // A machine in the file does not make a row without links.
    core.call(
        "config.set",
        json!({"key": "machines.studio", "value": {"ssh": "studio"}}),
    )
    .await;
    assert_eq!(projection(&core).await.len(), 1);
    for method in [
        "machine.connect",
        "machine.disconnect",
        "machine.open_window",
    ] {
        let error = core
            .handle
            .rpc(method, json!({"machine": "studio"}))
            .await
            .expect_err("refused");
        assert_eq!(error.code, ErrorCode::Unsupported, "{method}: {error}");
    }
    let listed = core.call("machine.list", json!({})).await;
    assert_eq!(listed["machines"].as_array().map(Vec::len), Some(1));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_launch_machine_connects_at_once_and_a_manual_one_at_machine_connect() {
    let remote = Remote::new().with_harnesses();
    remote.set_claude_login(true);
    let core = Harness::start();
    core.handle
        .set_machine_links(Arc::new(manager(&remote, &core)));
    assert_eq!(projection(&core).await.len(), 1, "local only at first");

    let (a, b) = (remote.session_for("a"), remote.session_for("b"));
    add(&core, "alpha", &a, "launch").await;
    add(&core, "beta", &b, "manual").await;

    // The order: local, then the order of the file.
    let labels: Vec<String> = projection(&core)
        .await
        .iter()
        .map(|r| r["label"].as_str().expect("label").to_owned())
        .collect();
    assert_eq!(labels, ["local", "alpha", "beta"]);

    let alpha = row(&core, "alpha", |r| r["state"] == "connected").await;
    assert_eq!(alpha["version"], env!("CARGO_PKG_VERSION"));
    let beta = row(&core, "beta", |_| true).await;
    assert_eq!(beta["state"], "disconnected", "{beta}");

    core.call("machine.connect", json!({"machine": "beta"}))
        .await;
    let beta = row(&core, "beta", |r| r["state"] == "connected").await;
    assert_eq!(beta["reason"], Value::Null);

    // The round-trip time comes with the first pings.
    row(&core, "alpha", |r| r["latency_ms"].is_number()).await;

    // The summary of the harnesses of the remote core.
    let alpha = row(&core, "alpha", |r| {
        r["harnesses"].as_array().is_some_and(|h| h.len() == 2)
    })
    .await;
    let claude = &alpha["harnesses"][0];
    assert_eq!(claude["id"], "claude");
    assert_eq!(claude["installed"], true);
    assert_eq!(claude["login"], "logged_in");
    assert_eq!(alpha["harnesses"][1]["id"], "codex");
    assert_eq!(alpha["harnesses"][1]["login"], "logged_out");

    // machine.list has the same rows as the projection.
    let listed = core.call("machine.list", json!({})).await;
    assert_eq!(
        listed["machines"].as_array().expect("rows").len(),
        3,
        "{listed}"
    );
    assert_eq!(listed["machines"][1]["label"], "alpha");

    // machine.disconnect ends the link; the row says why; the bridge is gone.
    core.call("machine.disconnect", json!({"machine": "alpha"}))
        .await;
    let alpha = row(&core, "alpha", |r| r["state"] == "disconnected").await;
    assert_eq!(alpha["reason"], "you disconnected");
    eventually("the bridge of alpha to end", || remote.bridges_of(&a) == 0).await;
    // The daemon of the machine keeps running: a disconnect is not a stop.
    assert_eq!(remote.bridges_of(&b), 1, "beta is still up");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_bad_label_is_refused_and_a_removed_machine_is_disconnected() {
    let remote = Remote::new().with_harnesses();
    let core = Harness::start();
    core.handle
        .set_machine_links(Arc::new(manager(&remote, &core)));
    for key in [
        "machines.local",
        "machines.Studio.ssh",
        "machines.a_b",
        "machines..ssh",
    ] {
        let error = core
            .handle
            .rpc("config.set", json!({"key": key, "value": {"ssh": "x"}}))
            .await
            .expect_err(key);
        assert_eq!(error.code, ErrorCode::InvalidParams, "{key}: {error}");
    }
    let long = format!("machines.{}", "a".repeat(33));
    let error = core
        .handle
        .rpc("config.set", json!({"key": long, "value": {"ssh": "x"}}))
        .await
        .expect_err("33 characters");
    assert_eq!(error.code, ErrorCode::InvalidParams);
    // An unknown field, and a machine without a target.
    for (key, value) in [
        ("machines.ok.nope", json!("x")),
        ("machines.ok", json!({"session": "default"})),
    ] {
        let error = core
            .handle
            .rpc("config.set", json!({"key": key, "value": value}))
            .await
            .expect_err(key);
        assert_eq!(error.code, ErrorCode::InvalidParams, "{key}: {error}");
    }
    assert_eq!(projection(&core).await.len(), 1, "nothing was written");

    // A machine that is up, and then removed with null.
    add(&core, "gone", &remote.session, "launch").await;
    row(&core, "gone", |r| r["state"] == "connected").await;
    core.call("config.set", json!({"key": "machines.gone", "value": null}))
        .await;
    let deadline = std::time::Instant::now() + WAIT;
    loop {
        let rows = core.call("machine.list", json!({})).await;
        if rows["machines"]
            .as_array()
            .is_some_and(|rows| rows.iter().all(|r| r["label"] != "gone"))
        {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the row of gone stays"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    eventually("the bridge of gone to end", || remote.bridges() == 0).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_call_over_the_link_of_a_machine_reaches_that_machine_and_no_other() {
    let remote = Remote::new().with_harnesses();
    let core = Harness::start();
    let manager = manager(&remote, &core);
    core.handle.set_machine_links(Arc::new(manager.clone()));
    let (a, b) = (remote.session_for("a"), remote.session_for("b"));
    add(&core, "alpha", &a, "manual").await;
    add(&core, "beta", &b, "manual").await;
    assert!(
        manager.client_of("alpha").is_none(),
        "no link before the connect"
    );
    core.call("machine.connect", json!({"machine": "alpha"}))
        .await;
    core.call("machine.connect", json!({"machine": "beta"}))
        .await;

    // The routing of the app asks the manager for the link of the machine of a window.
    for (label, session) in [("alpha", &a), ("beta", &b)] {
        let (client, _) = manager.client_of(label).expect("a link");
        let info = client
            .call(None, "session.info", json!({}))
            .await
            .expect("session.info");
        assert_eq!(info["name"], json!(session), "{label}");
    }
    // A new link is a new number, so a cache that belongs to a link starts again.
    let (_, first) = manager.client_of("alpha").expect("a link");
    core.call("machine.disconnect", json!({"machine": "alpha"}))
        .await;
    assert!(
        manager.client_of("alpha").is_none(),
        "no link after the disconnect"
    );
    core.call("machine.connect", json!({"machine": "alpha"}))
        .await;
    let (_, second) = manager.client_of("alpha").expect("a link");
    assert_ne!(first, second);

    // machine.open_window opens a window on the machine for this client, and the row names this
    // device as its owner.
    core.call("machine.open_window", json!({"machine": "beta"}))
        .await;
    let (client, _) = manager.client_of("beta").expect("a link");
    let mut owned = None;
    for _ in 0..100 {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        struct Sink(tokio::sync::mpsc::UnboundedSender<Value>);
        impl nh_core::MessageSink for Sink {
            fn send(&self, message: Value) -> bool {
                self.0.send(message).is_ok()
            }
        }
        let id = client
            .subscribe("windows", None, Box::new(Sink(tx)))
            .await
            .expect("windows");
        let snapshot = rx.recv().await.expect("a snapshot");
        client.unsubscribe(&id);
        let items = snapshot["data"]["items"]
            .as_object()
            .cloned()
            .unwrap_or_default();
        if let Some(row) = items.values().next() {
            owned = Some(row["client"].clone());
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert_eq!(owned, Some(json!(Ulid::from_parts(1, 99).to_string())));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_request_of_a_remote_core_goes_to_the_host_of_that_machine() {
    // Each remote core posts "Codex is not logged in" when it probes its fake `codex`.
    let remote = Remote::new().with_harnesses();
    let core = Harness::start();
    let hosts: Arc<
        std::sync::Mutex<std::collections::HashMap<String, Arc<nh_core::RecordingHost>>>,
    > = Arc::default();
    let by_label = Arc::clone(&hosts);
    let manager = manager_with_hosts(&remote, &core, move |label| {
        let host = host();
        by_label
            .lock()
            .expect("lock")
            .insert(label.to_owned(), Arc::clone(&host));
        host as Arc<dyn nh_core::Host>
    });
    core.handle.set_machine_links(Arc::new(manager));
    let (a, b) = (remote.session_for("a"), remote.session_for("b"));
    add(&core, "alpha", &a, "launch").await;
    add(&core, "beta", &b, "launch").await;
    row(&core, "alpha", |r| r["state"] == "connected").await;
    row(&core, "beta", |r| r["state"] == "connected").await;
    for label in ["alpha", "beta"] {
        let deadline = std::time::Instant::now() + WAIT;
        loop {
            let host = hosts.lock().expect("lock").get(label).cloned();
            let titles: Vec<String> = host
                .map(|h| {
                    h.calls()
                        .into_iter()
                        .filter_map(|call| match call {
                            nh_core::host::HostCall::Notify(_, n) => Some(n.title),
                            _ => None,
                        })
                        .collect()
                })
                .unwrap_or_default();
            if titles.iter().any(|t| t == "Codex is not logged in") {
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "{label} got no notification: {titles:?}"
            );
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }
    assert_eq!(
        hosts.lock().expect("lock").len(),
        2,
        "one host for each machine"
    );
}
