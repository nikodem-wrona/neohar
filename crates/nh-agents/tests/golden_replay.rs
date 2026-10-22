//! The hooks of every recording go through the adapters, and the result is compared with a file
//! (`expected.jsonl`).
//!
//! The corpus of `tests/harness-golden/` has the hooks that the harnesses sent in each scenario
//! (`signals.jsonl`). This test plays them through `ClaudeHooks` and `CodexHooks`, and writes one
//! line for each hook: the event, what the shim gets back, and the signals that the adapter
//! reported. The file next to the recording holds the lines that were reviewed. A change of an
//! adapter that changes what it reports for any recording fails the test, and the diff shows which
//! recording and which hook. `NEOHAR_BLESS=1` writes the files again: read the change before you
//! commit it.

use std::{
    fs,
    path::{Path, PathBuf},
    sync::Arc,
};

use nh_agents::{
    adapter::{Adapter, AdapterCtx, AdapterSignal, Emitter, SignalReply},
    claude::ClaudeHooks,
    codex::CodexHooks,
    manifest::loader::builtin,
};
use nh_proto::api::{common::Harness, ids::RunId};
use serde_json::{Value, json};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/harness-golden")
}

fn run_id() -> RunId {
    "01M4F1W7EM2GN3EV6NHXZJEGGA".parse().expect("an id")
}

/// The recordings that have hooks: `(harness, version, scenario, dir)`.
fn recordings() -> Vec<(String, String, String, PathBuf)> {
    let mut found = Vec::new();
    for harness in ["claude", "codex"] {
        let Ok(versions) = fs::read_dir(root().join(harness)) else {
            continue;
        };
        for version in versions.flatten() {
            let Ok(scenarios) = fs::read_dir(version.path()) else {
                continue;
            };
            for scenario in scenarios.flatten() {
                if scenario.path().join("signals.jsonl").is_file() {
                    found.push((
                        harness.to_owned(),
                        version.file_name().to_string_lossy().into_owned(),
                        scenario.file_name().to_string_lossy().into_owned(),
                        scenario.path(),
                    ));
                }
            }
        }
    }
    found.sort();
    found
}

fn reply_name(reply: &SignalReply) -> String {
    match reply {
        SignalReply::Continue => "continue".to_owned(),
        SignalReply::Decision(decision) => format!("decision {decision:?}"),
        SignalReply::Wait(_) => "wait".to_owned(),
    }
}

/// Plays the hooks of one recording and gives one JSON line for each.
async fn play(harness: &str, dir: &Path) -> Vec<String> {
    let (emitter, receiver) = Emitter::channel();
    let kind = if harness == "claude" {
        Harness::Claude
    } else {
        Harness::Codex
    };
    let manifest = Arc::new(builtin().get(harness).expect("a manifest").clone());
    let ctx = AdapterCtx {
        run: run_id(),
        pane: None,
        harness: kind,
        manifest,
        emitter,
    };
    let mut adapter: Box<dyn Adapter> = if harness == "claude" {
        Box::new(ClaudeHooks::new())
    } else {
        Box::new(CodexHooks::new())
    };
    adapter.start(ctx).await.expect("the adapter starts");
    let mut lines = Vec::new();
    let text = fs::read_to_string(dir.join("signals.jsonl")).expect("signals");
    for row in text.lines().filter(|l| !l.trim().is_empty()) {
        let row: Value = serde_json::from_str(row).expect("a JSON line");
        let event = row["event"].as_str().expect("an event").to_owned();
        let reply = adapter
            .on_signal(AdapterSignal::Raw {
                harness_event: event.clone(),
                payload: row["payload"].clone(),
            })
            .await
            .expect("a reply");
        let signals: Vec<String> = receiver
            .try_iter()
            .filter(|signal| !matches!(signal, AdapterSignal::Raw { .. }))
            .map(|signal| format!("{signal:?}"))
            .collect();
        lines.push(
            json!({"event": event, "reply": reply_name(&reply), "signals": signals}).to_string(),
        );
    }
    lines
}

#[test]
fn every_recording_makes_the_signals_that_were_reviewed() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("a runtime");
    let bless = std::env::var_os("NEOHAR_BLESS").is_some();
    let all = recordings();
    assert!(all.len() >= 40, "only {} recordings have hooks", all.len());
    let mut wrong = Vec::new();
    for (harness, version, scenario, dir) in &all {
        let played = runtime.block_on(play(harness, dir));
        let text = played.join("\n") + "\n";
        let file = dir.join("expected.jsonl");
        if bless {
            fs::write(&file, &text).expect("the expected file is written");
            continue;
        }
        match fs::read_to_string(&file) {
            Err(_) => wrong.push(format!("{harness} {version} {scenario}: no expected.jsonl")),
            Ok(expected) if expected == text => {}
            Ok(expected) => {
                let before: Vec<&str> = expected.lines().collect();
                let at = played
                    .iter()
                    .zip(&before)
                    .position(|(now, then)| now != then)
                    .unwrap_or(played.len().min(before.len()));
                wrong.push(format!(
                    "{harness} {version} {scenario}: hook {at} differs\n  was: {}\n  now: {}",
                    before.get(at).copied().unwrap_or("(none)"),
                    played.get(at).map_or("(none)", String::as_str)
                ));
            }
        }
    }
    assert!(
        wrong.is_empty(),
        "{} recordings changed. Read the diff; NEOHAR_BLESS=1 cargo test -p nh-agents --test golden_replay writes the files again.\n{}",
        wrong.len(),
        wrong.join("\n")
    );
}

#[test]
fn a_replay_gives_the_same_lines_every_time() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("a runtime");
    let (harness, _, _, dir) = recordings()
        .into_iter()
        .find(|(h, _, s, _)| h == "claude" && s == "permission-allow")
        .expect("a recording");
    let first = runtime.block_on(play(&harness, &dir));
    let second = runtime.block_on(play(&harness, &dir));
    assert_eq!(first, second);
    assert!(first.len() >= 5);
    // The hook that asks for a permission says that the shim waits.
    assert!(
        first
            .iter()
            .any(|line| line.contains("\"event\":\"PermissionRequest\""))
    );
}
