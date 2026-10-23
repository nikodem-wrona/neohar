//! The harnesses of a machine against the real core:
//! a machine with no harness, a harness that is not logged in, and both logged in. The harness
//! programs are shell scripts on the `PATH` of the base environment, so the tests do not run the
//! Claude Code or the Codex of the machine.

mod core_support;

use std::{
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};

use core_support::{Harness, write_program};
use nh_core::{ErrorCode, RecordingHost, host::HostCall};
use serde_json::{Value, json};

const CODEX_OUT: &str = r#"#!/bin/sh
case "$1 $2" in
  "--version "*) echo 'codex-cli 0.162.1' ;;
  "login status") echo 'Not logged in' >&2; exit 1 ;;
  *) exit 2 ;;
esac
"#;

const CLAUDE_KEY: &str = r#"#!/bin/sh
case "$1 $2" in
  "--version "*) echo '2.1.292 (Claude Code)' ;;
  "auth status") echo '{"loggedIn": true, "authMethod": "api_key", "apiKeySource": "ANTHROPIC_API_KEY"}' ;;
  *) exit 2 ;;
esac
"#;

/// The data of the snapshot of a projection, as the API socket shows it.
async fn snapshot(core: &Harness, name: &str) -> Value {
    let mut client = core_support::client::Client::connect(&core.api_socket()).await;
    let subscribed = client
        .ok("projection.subscribe", json!({ "name": name }))
        .await;
    let id = subscribed["subscription"]
        .as_str()
        .expect("an id")
        .to_owned();
    loop {
        let line = client.recv().await;
        if line["method"] == "projection.message"
            && line["params"]["subscription"] == id.as_str()
            && line["params"]["message"]["type"] == "snapshot"
        {
            return line["params"]["message"]["data"].clone();
        }
    }
}

/// A snapshot of a projection once `ready` holds. The core makes one patch for the changes of
/// each 16 ms, so a snapshot just after a call can be the one before it.
async fn settled(core: &Harness, name: &str, ready: impl Fn(&Value) -> bool) -> Value {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let data = snapshot(core, name).await;
        if ready(&data) || Instant::now() >= deadline {
            return data;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// The row of a harness once it has the login state `login`.
async fn harness_row(core: &Harness, id: &str, login: &str) -> Value {
    settled(core, "harnesses", |data| {
        data["items"][id]["login"] == login
    })
    .await["items"][id]
        .clone()
}

fn warnings_of(data: &Value) -> Vec<String> {
    data["items"]["_"]["warnings"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|warning| {
            warning["id"]
                .as_str()
                .is_some_and(|id| id.starts_with("harnesses:"))
        })
        .filter_map(|warning| warning["text"].as_str().map(str::to_owned))
        .collect()
}

/// The warnings of `health` about the harnesses, once they are `expected`.
async fn warnings(core: &Harness, expected: &[&str]) -> Vec<String> {
    warnings_of(&settled(core, "health", |data| warnings_of(data) == expected).await)
}

fn bin(core: &Harness) -> PathBuf {
    core.dir.join("bin")
}

async fn first_pane(core: &Harness) -> String {
    let made = core
        .call("workspace.create", json!({ "name": "Work" }))
        .await;
    made["pane"].as_str().expect("a pane").to_owned()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_machine_with_no_harness_shows_it_and_a_start_fails() {
    let core = Harness::start_without_harnesses();
    core.call("harness.refresh", json!({})).await;
    for (id, name) in [("claude", "Claude Code"), ("codex", "Codex")] {
        let row = harness_row(&core, id, "unknown").await;
        assert_eq!(row["name"], name);
        assert_eq!(row["installed"], false, "{row}");
        assert_eq!(row["available"], false, "{row}");
        assert_eq!(row["login"], "unknown");
        assert!(
            row["hint"]
                .as_str()
                .is_some_and(|hint| hint.contains("is not on the PATH")),
            "{row}"
        );
    }
    let expected = ["Claude Code is not installed", "Codex is not installed"];
    assert_eq!(warnings(&core, &expected).await, expected);
    let menus = core
        .call("agent.choices", json!({"harness": "claude"}))
        .await;
    assert_eq!(menus["harnesses"][0]["installed"], false);
    assert_eq!(menus["harnesses"][0]["available"], false);
    assert!(menus["harnesses"][0]["hint"].is_string(), "{menus}");
    let pane = first_pane(&core).await;
    let refused = core
        .handle
        .rpc(
            "agent.start",
            json!({
                "harness": "claude",
                "placement": {"type": "split", "pane": pane, "direction": "right"},
            }),
        )
        .await
        .expect_err("no claude");
    assert_eq!(refused.code, ErrorCode::HarnessUnavailable);
    assert_eq!(refused.details["harness"], "claude");
    assert_eq!(refused.details["reason"], "not_installed");
    assert!(refused.details["hint"].is_string());
    // Nothing started.
    let panes = core.call("pane.list", json!({})).await;
    assert_eq!(panes["panes"].as_array().map(Vec::len), Some(1), "{panes}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_harness_that_is_not_logged_in_refuses_a_start() {
    let host = Arc::new(RecordingHost::new(env!("CARGO_BIN_EXE_nh").into(), false));
    let core = Harness::start_hosted(Arc::clone(&host) as Arc<dyn nh_core::Host>);
    write_program(&bin(&core).join("codex"), CODEX_OUT);
    core.call("harness.refresh", json!({"harness": "codex"}))
        .await;
    let row = harness_row(&core, "codex", "logged_out").await;
    assert_eq!(row["login"], "logged_out", "{row}");
    assert_eq!(row["login_source"], "probe");
    assert_eq!(row["installed"], true);
    assert_eq!(row["available"], false);
    assert_eq!(row["version"], "0.162.1");
    assert!(
        row["hint"]
            .as_str()
            .is_some_and(|hint| hint.contains("codex login")),
        "{row}"
    );
    let expected = ["Codex is not logged in"];
    assert_eq!(warnings(&core, &expected).await, expected);
    let posted = || {
        host.calls()
            .into_iter()
            .filter_map(|call| match call {
                HostCall::Notify(_, notification) => Some(notification.title),
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(posted(), ["Codex is not logged in"]);
    core.call("harness.refresh", json!({})).await;
    assert_eq!(
        posted().len(),
        1,
        "one notification until the state changes"
    );
    let menus = core
        .call("agent.choices", json!({"harness": "codex"}))
        .await;
    assert_eq!(menus["harnesses"][0]["login"], "logged_out");
    assert_eq!(menus["harnesses"][0]["available"], false);

    // A start with that harness fails, and nothing starts.
    let pane = first_pane(&core).await;
    let refused = core
        .handle
        .rpc(
            "agent.start",
            json!({
                "harness": "codex",
                "placement": {"type": "split", "pane": pane, "direction": "right"},
            }),
        )
        .await
        .expect_err("codex is logged out");
    assert_eq!(refused.code, ErrorCode::HarnessUnavailable);
    assert_eq!(refused.details["reason"], "logged_out");
    let panes = core.call("pane.list", json!({})).await;
    assert_eq!(panes["panes"].as_array().map(Vec::len), Some(1), "{panes}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn both_harnesses_logged_in_show_their_method_and_no_warning() {
    let core = Harness::start();
    write_program(&bin(&core).join("claude"), CLAUDE_KEY);
    core.call("harness.refresh", json!({})).await;
    let claude = settled(&core, "harnesses", |data| {
        data["items"]["claude"]["login_method"] == "api_key"
    })
    .await["items"]["claude"]
        .clone();
    assert_eq!(claude["login"], "logged_in", "{claude}");
    assert_eq!(claude["login_method"], "api_key");
    assert_eq!(claude["validated"], true);
    assert_eq!(claude["available"], true);
    assert!(claude["hint"].is_null(), "{claude}");
    assert!(
        claude["path"]
            .as_str()
            .is_some_and(|path| path.ends_with("/bin/claude"))
    );
    let codex = harness_row(&core, "codex", "logged_in").await;
    assert_eq!(codex["login"], "logged_in", "{codex}");
    assert_eq!(codex["login_method"], "subscription");
    // The recordings do not cover Codex 0.162.
    assert_eq!(codex["validated"], false);
    let order = snapshot(&core, "harnesses").await["order"].clone();
    assert_eq!(order, json!(["claude", "codex"]));
    assert!(warnings(&core, &[]).await.is_empty());
    let menus = core.call("agent.choices", json!({})).await;
    for menu in menus["harnesses"].as_array().expect("menus") {
        assert_eq!(menu["available"], true, "{menu}");
        assert_eq!(menu["login"], "logged_in", "{menu}");
    }
    let refused = core
        .handle
        .rpc("harness.refresh", json!({"harness": "gemini"}))
        .await
        .expect_err("no such harness");
    assert_eq!(refused.code, ErrorCode::NotFound);
}
