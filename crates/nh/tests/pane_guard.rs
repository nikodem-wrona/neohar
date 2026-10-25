//! A program inside a pane may not weaken the approvals with the powers of the CLI.
//!
//! The pane runs a small shell program that calls `nh` with the socket of its environment, as a
//! tool of an agent could. The core reads the parents of the client, finds the holder of the
//! pane, and refuses the calls that answer interactions or weaken the approvals. A harmless call
//! goes on. The same calls from outside the pane (this test process) go on.

mod core_support;

use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use core_support::Harness;
use serde_json::json;

/// The program in the pane. Each call writes its output and its exit code to a file of its own.
const PROGRAM: &str = r#"#!/bin/sh
dir="$1"
"$NEOHAR_BIN" config set approvals.mode observe > "$dir/weaken.out" 2>&1
echo "exit $?" >> "$dir/weaken.out"
"$NEOHAR_BIN" interaction answer i1 allow > "$dir/answer.out" 2>&1
echo "exit $?" >> "$dir/answer.out"
"$NEOHAR_BIN" config set ui.font_size 15 > "$dir/harmless.out" 2>&1
echo "exit $?" >> "$dir/harmless.out"
touch "$dir/done"
sleep 600
"#;

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("nh-guard-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("a directory");
    dir
}

async fn wait_for(path: &Path) {
    let deadline = Instant::now() + Duration::from_secs(30);
    while !path.exists() {
        assert!(Instant::now() < deadline, "timeout: {}", path.display());
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

fn read(dir: &Path, name: &str) -> String {
    std::fs::read_to_string(dir.join(name)).unwrap_or_default()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_program_in_a_pane_may_not_weaken_the_approvals() {
    let dir = scratch("weaken");
    let script = dir.join("program.sh");
    std::fs::write(&script, PROGRAM).expect("the program");
    let core = Harness::start()
        .with_command_pane(&[
            "/bin/sh",
            script.to_str().expect("a path"),
            dir.to_str().expect("a path"),
        ])
        .await;
    wait_for(&dir.join("done")).await;

    let weaken = read(&dir, "weaken.out");
    assert!(!weaken.contains("exit 0"), "{weaken}");
    assert!(weaken.contains("outside a neohar pane"), "{weaken}");
    let answer = read(&dir, "answer.out");
    assert!(!answer.contains("exit 0"), "{answer}");
    assert!(answer.contains("outside a neohar pane"), "{answer}");
    let harmless = read(&dir, "harmless.out");
    assert!(harmless.contains("exit 0"), "{harmless}");

    // The mode did not change, and the harmless key did.
    let settings = core.call("config.get", json!({})).await;
    let sections = &settings["sections"];
    assert_ne!(
        sections["approvals"]["entries"]["mode"]["value"], "observe",
        "{sections}"
    );
    assert_eq!(
        sections["ui"]["entries"]["font_size"]["value"], 15,
        "{sections}"
    );

    // The same call from outside the pane goes on.
    let socket = core.api_socket().display().to_string();
    let ran = tokio::task::spawn_blocking(move || {
        std::process::Command::new(env!("CARGO_BIN_EXE_nh"))
            .args([
                "--socket",
                &socket,
                "config",
                "set",
                "approvals.mode",
                "observe",
            ])
            .output()
            .expect("nh runs")
    })
    .await
    .expect("the task");
    assert!(
        ran.status.success(),
        "{}",
        String::from_utf8_lossy(&ran.stderr)
    );
    let settings = core.call("config.get", json!({})).await;
    assert_eq!(
        settings["sections"]["approvals"]["entries"]["mode"]["value"],
        "observe"
    );
}
