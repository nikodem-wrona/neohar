//! Records the real Claude Code and Codex binaries.
//!
//! These tests need the harnesses on the machine, so they run only when `NEOHAR_HARNESS=1`
//! (`mise run test:harness`). They show that the recorder works:
//!
//! - a "startup, prompt, text answer, exit" scenario records twice for each harness with the same
//!   screens, hook sequence and requests, ignoring ids and timestamps;
//! - the real `~/.claude`, `~/.claude.json` and `~/.codex` are the same after the recording;
//! - every other scenario of the harness records once and agrees with the committed recording of
//!   the same version of the harness. When the installed version has no committed recording, the
//!   test says so and skips only this part.
//!
//! A harness that is not installed fails its test.

#![cfg(target_os = "macos")]

mod common;

use std::path::PathBuf;

use common::{enabled, golden_root, record_once, save};
use nh_golden::{compare::compare, guard, recipe::Harness};

/// The names of the scenarios of a harness, without `startup`.
fn other_scenarios(harness: Harness) -> Vec<String> {
    let dir = golden_root().join("scenarios").join(harness.name());
    let mut names: Vec<String> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("read {}: {e}", dir.display()))
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let path = entry.path();
            (path.extension()? == "toml")
                .then(|| path.file_stem().map(|s| s.to_string_lossy().into_owned()))
                .flatten()
        })
        .filter(|name| name != "startup")
        .collect();
    names.sort();
    names
}

fn check(harness: Harness) {
    if !enabled() {
        return;
    }
    let home = PathBuf::from(std::env::var_os("HOME").expect("HOME"));
    let before = guard::take(harness, &home);
    let first = record_once(harness, "startup");
    let second = record_once(harness, "startup");
    // The real configuration is as it was.
    guard::verify(harness, &home, &before).expect("the real configuration is untouched");

    for recording in [&first, &second] {
        let meta = &recording.meta;
        assert_eq!(meta.exit, "code 0", "the harness exits by itself");
        assert_eq!(meta.llm_main_requests, 1, "one main request");
        assert_eq!(meta.turns_used, meta.turns_total, "every turn was used");
        assert_eq!(meta.unmatched_main, 0);
        assert_eq!(
            meta.hook_calls, 4,
            "SessionStart, UserPromptSubmit, Stop, SessionEnd"
        );
        assert_eq!(
            meta.hook_calls_api, meta.hook_calls,
            "every hook reached the API socket"
        );
    }

    let (a, b) = (
        save(&first, &format!("{}-a", harness.name())),
        save(&second, &format!("{}-b", harness.name())),
    );
    let differences = compare(&a, &b).expect("the recordings compare");
    assert!(
        differences.is_empty(),
        "two recordings of {} differ:\n{differences:#?}",
        harness.name()
    );

    let committed = golden_root()
        .join(harness.name())
        .join(&first.meta.harness_version)
        .join("startup");
    if committed.join("meta.toml").is_file() {
        let differences = compare(&committed, &a).expect("the recordings compare");
        assert!(
            differences.is_empty(),
            "{} {} no longer behaves like the committed recording:\n{differences:#?}",
            harness.name(),
            first.meta.harness_version
        );
    } else {
        eprintln!(
            "note: no committed recording for {} {}; record one with `mise run record:golden`",
            harness.name(),
            first.meta.harness_version
        );
    }
    let _ = std::fs::remove_dir_all(a);
    let _ = std::fs::remove_dir_all(b);
}

/// Records each scenario except `startup` once and compares it with the committed recording.
fn check_scenarios(harness: Harness) {
    if !enabled() {
        return;
    }
    let home = PathBuf::from(std::env::var_os("HOME").expect("HOME"));
    let before = guard::take(harness, &home);
    let names = other_scenarios(harness);
    if names.is_empty() {
        eprintln!("note: {} has no scenario besides startup", harness.name());
        return;
    }
    let mut failures = Vec::new();
    for name in &names {
        let recording = record_once(harness, name);
        let meta = &recording.meta;
        // A scenario can end with a failure code on purpose (an interrupted headless run exits
        // with 1), leave a turn unused, or run a server that the recorder stops (`still
        // running`). The comparison below checks the exit and the counts against the committed
        // recording.
        assert_eq!(meta.unmatched_main, 0, "{name}: every request got a turn");
        assert_eq!(
            meta.hook_calls_api, meta.hook_calls,
            "{name}: every hook reached the socket"
        );
        let dir = save(&recording, &format!("{}-{name}", harness.name()));
        let committed = golden_root()
            .join(harness.name())
            .join(&meta.harness_version)
            .join(name);
        if committed.join("meta.toml").is_file() {
            let differences = compare(&committed, &dir).expect("the recordings compare");
            if !differences.is_empty() {
                failures.push(format!("{name}:\n{differences:#?}"));
            }
        } else {
            eprintln!(
                "note: no committed recording for {} {} {name}; record one with `mise run record:golden`",
                harness.name(),
                meta.harness_version
            );
        }
        let _ = std::fs::remove_dir_all(dir);
    }
    guard::verify(harness, &home, &before).expect("the real configuration is untouched");
    assert!(
        failures.is_empty(),
        "{} no longer behaves like the committed recordings:\n{}",
        harness.name(),
        failures.join("\n")
    );
}

#[test]
fn claude_code_scenarios_match_the_committed_recordings() {
    check_scenarios(Harness::Claude);
}

#[test]
fn codex_scenarios_match_the_committed_recordings() {
    check_scenarios(Harness::Codex);
}

#[test]
fn claude_code_records_twice_the_same_and_leaves_the_real_config_alone() {
    check(Harness::Claude);
}

#[test]
fn codex_records_twice_the_same_and_leaves_the_real_config_alone() {
    check(Harness::Codex);
}
